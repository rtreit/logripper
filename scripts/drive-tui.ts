import path from 'node:path';
import process from 'node:process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import * as nodePty from 'node-pty';
import { chromium, type Browser, type Page } from 'playwright';
import xtermHeadless from '@xterm/headless';
import addonSerialize from '@xterm/addon-serialize';

type MatchSource = 'screen' | 'transcript' | 'ansi' | 'any';

const { Terminal } = xtermHeadless as typeof import('@xterm/headless');
const { SerializeAddon } = addonSerialize as typeof import('@xterm/addon-serialize');
type HeadlessTerminal = InstanceType<typeof Terminal>;
type XtermSerializeAddon = InstanceType<typeof SerializeAddon>;

interface ActionScript {
  scenario?: string;
  fixture?: string;
  command?: string | CommandSpec;
  cwd?: string;
  env?: Record<string, string>;
  terminal?: TerminalSpec;
  actions: Action[];
}

interface CommandSpec {
  program?: string;
  args?: string[];
  commandLine?: string;
  cwd?: string;
  env?: Record<string, string>;
}

interface TerminalSpec {
  columns?: number;
  rows?: number;
  scrollback?: number;
}

type Action =
  | WaitAction
  | WaitForTextAction
  | WaitForIdleAction
  | SendKeysAction
  | SendTextAction
  | SnapshotAction
  | ResizeAction
  | WaitForExitAction;

interface BaseAction {
  type:
    | 'wait'
    | 'wait-for-text'
    | 'wait-for-idle'
    | 'send-keys'
    | 'send-text'
    | 'snapshot'
    | 'resize'
    | 'wait-for-exit';
  label?: string;
}

interface WaitAction extends BaseAction {
  type: 'wait';
  milliseconds: number;
}

interface WaitForTextAction extends BaseAction {
  type: 'wait-for-text';
  text?: string;
  pattern?: string;
  match?: 'includes' | 'regex';
  source?: MatchSource;
  timeoutMs?: number;
}

interface WaitForIdleAction extends BaseAction {
  type: 'wait-for-idle';
  idleMs?: number;
  timeoutMs?: number;
}

interface SendKeysAction extends BaseAction {
  type: 'send-keys';
  keys: string;
  delayMs?: number;
  settleMs?: number;
}

interface SendTextAction extends BaseAction {
  type: 'send-text';
  text: string;
  delayMsBetweenChars?: number;
  pressEnter?: boolean;
  settleMs?: number;
}

interface SnapshotAction extends BaseAction {
  type: 'snapshot';
  path: string;
}

interface ResizeAction extends BaseAction {
  type: 'resize';
  columns: number;
  rows: number;
  settleMs?: number;
}

interface WaitForExitAction extends BaseAction {
  type: 'wait-for-exit';
  timeoutMs?: number;
}

interface ResolvedCommand {
  program: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
  commandDescription: string;
}

interface SnapshotResult {
  name: string;
  txtPath: string;
  jsonPath: string;
  ansiPath: string;
  pngPath: string;
}

interface StepResult {
  index: number;
  type: Action['type'];
  label?: string;
  startedAtUtc: string;
  finishedAtUtc: string;
  details?: Record<string, unknown>;
}

interface DriveSummary {
  surface: 'terminal';
  scenario: string;
  actionScript: string;
  fixture?: string;
  command: {
    description: string;
    cwd: string;
    columns: number;
    rows: number;
  };
  outputRoot: string;
  transcriptPath: string;
  artifacts: string[];
  steps: StepResult[];
  exitCode: number | null;
  signal?: number;
  capturedAtUtc: string;
  durationMs: number;
}

interface ExitResult {
  exitCode: number;
  signal?: number;
}

interface CliOptions {
  actionScript: string;
  outputRoot?: string;
}

interface SnapshotRenderer {
  render(options: {
    ansiText: string;
    pngPath: string;
    columns: number;
    rows: number;
  }): Promise<void>;
  dispose(): Promise<void>;
}

const repoRoot = process.cwd();
const uxRoot = path.join(repoRoot, 'artifacts', 'ux');
const currentRoot = path.join(uxRoot, 'current');
const defaultColumns = 100;
const defaultRows = 30;
const defaultScrollback = 5_000;
const sampleFixtureName = 'sample-tui';
const xtermBrowserScriptPath = path.join(repoRoot, 'node_modules', '@xterm', 'xterm', 'lib', 'xterm.js');
const xtermBrowserCssPath = path.join(repoRoot, 'node_modules', '@xterm', 'xterm', 'css', 'xterm.css');
const terminalRenderTheme = {
  background: '#263238',
  foreground: '#eeffff',
  cursor: '#ffcc00',
  cursorAccent: '#263238',
  black: '#000000',
  red: '#ff5370',
  green: '#c3e88d',
  yellow: '#ffcb6b',
  blue: '#82aaff',
  magenta: '#c792ea',
  cyan: '#89ddff',
  white: '#ffffff',
  brightBlack: '#546e7a',
  brightRed: '#ff5370',
  brightGreen: '#c3e88d',
  brightYellow: '#ffcb6b',
  brightBlue: '#82aaff',
  brightMagenta: '#c792ea',
  brightCyan: '#89ddff',
  brightWhite: '#ffffff'
} as const;

async function main(): Promise<void> {
  const options = parseCliOptions(process.argv.slice(2));
  const actionScriptPath = path.resolve(options.actionScript);
  const actionScript = await loadActionScript(actionScriptPath);
  const scenario = actionScript.scenario?.trim() || path.basename(actionScriptPath, path.extname(actionScriptPath));
  const outputRoot = path.resolve(
    options.outputRoot ?? path.join(currentRoot, scenario)
  );
  const transcriptPath = path.join(outputRoot, 'transcript.txt');
  const reportPath = path.join(outputRoot, 'report.json');

  await Promise.all([
    mkdir(currentRoot, { recursive: true }),
    mkdir(path.join(uxRoot, 'baseline'), { recursive: true }),
    mkdir(path.join(uxRoot, 'diff'), { recursive: true }),
    mkdir(outputRoot, { recursive: true })
  ]);

  const columns = actionScript.terminal?.columns ?? defaultColumns;
  const rows = actionScript.terminal?.rows ?? defaultRows;
  const scrollback = actionScript.terminal?.scrollback ?? defaultScrollback;
  const resolvedCommand = resolveCommand(actionScript);
  const terminal = new Terminal({
    cols: columns,
    rows,
    scrollback,
    allowProposedApi: true
  });
  const serializeAddon = new SerializeAddon();
  terminal.loadAddon(serializeAddon);
  const snapshotRenderer = await createSnapshotRenderer();

  let transcript = '';
  let writeQueue = Promise.resolve();
  let lastOutputAt = Date.now();
  const artifacts = new Set<string>();
  const steps: StepResult[] = [];
  const startedAt = Date.now();

  const ptyProcess = nodePty.spawn(resolvedCommand.program, resolvedCommand.args, {
    name: 'xterm-color',
    cols: columns,
    rows,
    cwd: resolvedCommand.cwd,
    env: resolvedCommand.env
  });

  const exitPromise = new Promise<ExitResult>((resolve) => {
    ptyProcess.onExit((event) => {
      resolve({
        exitCode: event.exitCode,
        signal: event.signal
      });
    });
  });

  ptyProcess.onData((chunk) => {
    transcript += chunk;
    lastOutputAt = Date.now();
    writeQueue = writeQueue.then(
      () =>
        new Promise<void>((resolve) => {
          terminal.write(chunk, () => resolve());
        })
    );
  });

  let exitResult: ExitResult | undefined;

  try {
    for (let index = 0; index < actionScript.actions.length; index += 1) {
      const action = actionScript.actions[index]!;
      const stepStartedAtUtc = new Date().toISOString();
      const details = await runAction({
        action,
        index,
        outputRoot,
        ptyProcess,
        terminal,
        serializeAddon,
        snapshotRenderer,
        columnsRef: () => terminal.cols,
        rowsRef: () => terminal.rows,
        ensureFlushed: async () => {
          await writeQueue;
          await delay(10);
        },
        waitForCondition: async (predicate, timeoutMs) => {
          const deadline = Date.now() + timeoutMs;
          while (Date.now() < deadline) {
            await writeQueue;
            if (predicate()) {
              return;
            }

            await delay(25);
          }

          await writeQueue;
          if (!predicate()) {
            throw new Error(`Timed out after ${timeoutMs} ms.`);
          }
        },
        getTranscript: () => transcript,
        getAnsiScreen: () => serializeAddon.serialize(),
        getScreenText: () => captureVisibleScreen(terminal),
        getLastOutputAt: () => lastOutputAt,
        exitPromise,
        addArtifact: (artifactPath) => artifacts.add(artifactPath)
      });

      steps.push({
        index,
        type: action.type,
        label: action.label,
        startedAtUtc: stepStartedAtUtc,
        finishedAtUtc: new Date().toISOString(),
        details
      });
    }

    exitResult = await Promise.race([
      exitPromise,
      delay(50).then(() => undefined)
    ]);
  } finally {
    await writeQueue;
    await snapshotRenderer.dispose();

    if (!exitResult) {
      try {
        ptyProcess.kill();
      } catch {
        // Ignore shutdown errors in cleanup.
      }

      exitResult = await Promise.race([
        exitPromise,
        delay(1_000).then(() => undefined)
      ]);
    }

    await writeFile(transcriptPath, normalizeNewlines(transcript), 'utf8');
    artifacts.add(transcriptPath);

    const summary: DriveSummary = {
      surface: 'terminal',
      scenario,
      actionScript: actionScriptPath,
      fixture: actionScript.fixture,
      command: {
        description: resolvedCommand.commandDescription,
        cwd: resolvedCommand.cwd,
        columns: terminal.cols,
        rows: terminal.rows
      },
      outputRoot,
      transcriptPath,
      artifacts: Array.from(artifacts).sort(),
      steps,
      exitCode: exitResult?.exitCode ?? null,
      signal: exitResult?.signal,
      capturedAtUtc: new Date().toISOString(),
      durationMs: Date.now() - startedAt
    };

    await writeJson(reportPath, summary);
    console.log(`Saved terminal automation artifacts to ${outputRoot}`);
    console.log(`Summary: ${reportPath}`);
  }
}

async function runAction(context: {
  action: Action;
  index: number;
  outputRoot: string;
  ptyProcess: nodePty.IPty;
  terminal: HeadlessTerminal;
  serializeAddon: XtermSerializeAddon;
  snapshotRenderer: SnapshotRenderer;
  columnsRef(): number;
  rowsRef(): number;
  ensureFlushed(): Promise<void>;
  waitForCondition(predicate: () => boolean, timeoutMs: number): Promise<void>;
  getTranscript(): string;
  getAnsiScreen(): string;
  getScreenText(): string;
  getLastOutputAt(): number;
  exitPromise: Promise<ExitResult>;
  addArtifact(artifactPath: string): void;
}): Promise<Record<string, unknown> | undefined> {
  const { action } = context;

  switch (action.type) {
    case 'wait':
      await delay(action.milliseconds);
      return { milliseconds: action.milliseconds };

    case 'wait-for-text': {
      const source = action.source ?? 'any';
      const timeoutMs = action.timeoutMs ?? 10_000;
      const matcher = createMatcher(action);
      await context.waitForCondition(() => matcher(matchesFromSource(source, context)), timeoutMs);
      return {
        source,
        timeoutMs,
        match: action.match ?? (action.pattern ? 'regex' : 'includes'),
        text: action.text,
        pattern: action.pattern
      };
    }

    case 'wait-for-idle': {
      const idleMs = action.idleMs ?? 250;
      const timeoutMs = action.timeoutMs ?? 10_000;
      await context.waitForCondition(
        () => Date.now() - context.getLastOutputAt() >= idleMs,
        timeoutMs
      );
      return {
        idleMs,
        timeoutMs
      };
    }

    case 'send-keys': {
      const sequence = parseKeys(action.keys);
      const delayMs = action.delayMs ?? 0;
      for (const chunk of sequence) {
        context.ptyProcess.write(chunk);
        if (delayMs > 0) {
          await delay(delayMs);
        }
      }

      if ((action.settleMs ?? 0) > 0) {
        await delay(action.settleMs!);
      }

      return {
        keys: action.keys,
        chunks: sequence.length
      };
    }

    case 'send-text': {
      const delayMsBetweenChars = action.delayMsBetweenChars ?? 0;
      for (const character of action.text) {
        context.ptyProcess.write(character);
        if (delayMsBetweenChars > 0) {
          await delay(delayMsBetweenChars);
        }
      }

      if (action.pressEnter) {
        context.ptyProcess.write('\r');
      }

      if ((action.settleMs ?? 0) > 0) {
        await delay(action.settleMs!);
      }

      return {
        textLength: action.text.length,
        pressEnter: action.pressEnter ?? false
      };
    }

    case 'snapshot': {
      await context.ensureFlushed();
      const snapshot = await writeSnapshot(
        context.outputRoot,
        action.path,
        context.getScreenText(),
        context.getAnsiScreen(),
        context.columnsRef(),
        context.rowsRef(),
        context.snapshotRenderer
      );
      context.addArtifact(snapshot.txtPath);
      context.addArtifact(snapshot.jsonPath);
      context.addArtifact(snapshot.ansiPath);
      context.addArtifact(snapshot.pngPath);
      return {
        name: snapshot.name,
        txtPath: snapshot.txtPath,
        jsonPath: snapshot.jsonPath,
        ansiPath: snapshot.ansiPath,
        pngPath: snapshot.pngPath
      };
    }

    case 'resize':
      context.ptyProcess.resize(action.columns, action.rows);
      if ((action.settleMs ?? 0) > 0) {
        await delay(action.settleMs!);
      }

      return {
        columns: action.columns,
        rows: action.rows
      };

    case 'wait-for-exit': {
      const timeoutMs = action.timeoutMs ?? 10_000;
      const exitResult = await waitForExit(context.exitPromise, timeoutMs);
      return {
        timeoutMs,
        exitCode: exitResult.exitCode,
        signal: exitResult.signal
      };
    }
  }
}

function matchesFromSource(source: MatchSource, context: {
  getTranscript(): string;
  getAnsiScreen(): string;
  getScreenText(): string;
}): string {
  switch (source) {
    case 'screen':
      return context.getScreenText();
    case 'transcript':
      return context.getTranscript();
    case 'ansi':
      return context.getAnsiScreen();
    case 'any':
      return [
        context.getScreenText(),
        context.getTranscript(),
        context.getAnsiScreen()
      ].join('\n');
  }
}

function createMatcher(action: WaitForTextAction): (content: string) => boolean {
  const matchKind = action.match ?? (action.pattern ? 'regex' : 'includes');
  if (matchKind === 'regex') {
    const regex = new RegExp(action.pattern ?? action.text ?? '');
    return (content) => regex.test(content);
  }

  const expected = action.text;
  if (!expected) {
    throw new Error('wait-for-text requires text when match is includes.');
  }

  return (content) => content.includes(expected);
}

function captureVisibleScreen(terminal: HeadlessTerminal): string {
  const buffer = terminal.buffer.active;
  const lines: string[] = [];
  const start = buffer.viewportY;
  for (let row = 0; row < terminal.rows; row += 1) {
    const line = buffer.getLine(start + row);
    lines.push((line?.translateToString(false) ?? '').replace(/\s+$/u, ''));
  }

  return normalizeNewlines(lines.join('\n'));
}

async function writeSnapshot(
  outputRoot: string,
  requestedPath: string,
  screenText: string,
  ansiText: string,
  columns: number,
  rows: number,
  snapshotRenderer: SnapshotRenderer
): Promise<SnapshotResult> {
  const sanitizedBaseName = stripKnownExtension(requestedPath);
  const resolvedBase = path.resolve(outputRoot, sanitizedBaseName);
  const txtPath = `${resolvedBase}.screen.txt`;
  const jsonPath = `${resolvedBase}.screen.json`;
  const ansiPath = `${resolvedBase}.ansi.txt`;
  const pngPath = `${resolvedBase}.screen.png`;

  await Promise.all([
    writeTextFile(txtPath, screenText),
    writeTextFile(ansiPath, ansiText),
    writeJson(jsonPath, {
      name: path.basename(sanitizedBaseName),
      columns,
      rows,
      screenText,
      lines: screenText.split('\n')
    })
  ]);
  await snapshotRenderer.render({
    ansiText,
    pngPath,
    columns,
    rows
  });

  return {
    name: path.basename(sanitizedBaseName),
    txtPath,
    jsonPath,
    ansiPath,
    pngPath
  };
}

async function createSnapshotRenderer(): Promise<SnapshotRenderer> {
  let browser: Browser | undefined;
  let page: Page | undefined;

  try {
    browser = await chromium.launch({ headless: true });
    page = await browser.newPage({
      deviceScaleFactor: 1,
      viewport: { width: 1440, height: 960 }
    });
    await page.setContent(`
      <!DOCTYPE html>
      <html lang="en">
        <head>
          <meta charset="utf-8" />
          <title>QsoRipper TUI Snapshot Renderer</title>
        </head>
        <body>
          <div id="terminal-shell">
            <div id="terminal-host"></div>
          </div>
        </body>
      </html>
    `);
    await page.addStyleTag({ path: xtermBrowserCssPath });
    await page.addStyleTag({
      content: [
        'html, body { margin: 0; padding: 0; background: #263238; }',
        'body { display: inline-block; }',
        '#terminal-shell { display: inline-block; padding: 12px; background: #263238; }',
        '#terminal-host { display: inline-block; }',
        '.xterm { padding: 0; }',
        '.xterm .xterm-viewport { overflow: hidden !important; background: #263238 !important; }',
        '.xterm .xterm-screen canvas { image-rendering: pixelated; }',
        '.xterm .xterm-helper-textarea, .xterm .xterm-accessibility { display: none !important; }',
        '.xterm .xterm-cursor-layer { visibility: hidden !important; }'
      ].join('\n')
    });
    await page.addScriptTag({ path: xtermBrowserScriptPath });
    await page.evaluate((theme) => {
      const globalTerminal = (globalThis as {
        Terminal?: new (options?: Record<string, unknown>) => {
          open(element: Element): void;
          write(data: string, callback?: () => void): void;
          dispose(): void;
          refresh(start: number, end: number): void;
        };
        __qsoripperRenderer?: {
          render(snapshot: {
            ansiText: string;
            columns: number;
            rows: number;
          }): Promise<void>;
        };
      });

      if (!globalTerminal.Terminal) {
        throw new Error('Browser xterm bundle did not expose a Terminal constructor.');
      }

      const TerminalCtor = globalTerminal.Terminal;
      let terminalInstance: {
        open(element: Element): void;
        write(data: string, callback?: () => void): void;
        dispose(): void;
        refresh(start: number, end: number): void;
      } | undefined;

      globalTerminal.__qsoripperRenderer = {
        async render(snapshot): Promise<void> {
          const host = document.getElementById('terminal-host');
          if (!host) {
            throw new Error('Missing terminal host element.');
          }

          terminalInstance?.dispose();
          host.replaceChildren();

          terminalInstance = new TerminalCtor({
            cols: snapshot.columns,
            rows: snapshot.rows,
            convertEol: true,
            cursorBlink: false,
            cursorStyle: 'block',
            fontFamily: 'Cascadia Mono, Consolas, Monaco, Lucida Console, monospace',
            fontSize: 14,
            lineHeight: 1,
            allowTransparency: false,
            theme
          });

          terminalInstance.open(host);
          await new Promise<void>((resolve) => {
            terminalInstance!.write(snapshot.ansiText, () => resolve());
          });
          terminalInstance.refresh(0, snapshot.rows - 1);
          await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
        }
      };
    }, terminalRenderTheme);
  } catch (error) {
    await page?.close().catch(() => undefined);
    await browser?.close().catch(() => undefined);
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Unable to initialize browser-backed TUI snapshot rendering. Ensure Playwright Chromium is installed with 'npx playwright install chromium'. ${message}`
    );
  }

  return {
    async render(options): Promise<void> {
      const estimatedWidth = Math.max(640, options.columns * 12 + 48);
      const estimatedHeight = Math.max(320, options.rows * 24 + 48);
      await page!.setViewportSize({
        width: estimatedWidth,
        height: estimatedHeight
      });
      await page!.evaluate((snapshot) => {
        const renderer = (globalThis as {
          __qsoripperRenderer?: {
            render(config: {
              ansiText: string;
              columns: number;
              rows: number;
            }): Promise<void>;
          };
        }).__qsoripperRenderer;

        if (!renderer) {
          throw new Error('QsoRipper TUI snapshot renderer is not available.');
        }

        return renderer.render(snapshot);
      }, {
        ansiText: options.ansiText,
        columns: options.columns,
        rows: options.rows
      });
      await mkdir(path.dirname(options.pngPath), { recursive: true });
      await page!.locator('#terminal-shell').screenshot({
        path: options.pngPath
      });
    },
    async dispose(): Promise<void> {
      await page?.close().catch(() => undefined);
      await browser?.close().catch(() => undefined);
    }
  };
}

function parseCliOptions(argv: string[]): CliOptions {
  let actionScript = '';
  let outputRoot: string | undefined;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]!;
    const next = argv[index + 1];

    switch (argument) {
      case '--action-script':
        actionScript = next ?? '';
        index += 1;
        break;
      case '--output-root':
        outputRoot = next;
        index += 1;
        break;
      default:
        break;
    }
  }

  if (!actionScript.trim()) {
    throw new Error('Missing required option --action-script.');
  }

  return {
    actionScript,
    outputRoot
  };
}

async function loadActionScript(actionScriptPath: string): Promise<ActionScript> {
  const raw = await readFile(actionScriptPath, 'utf8');
  const parsed = JSON.parse(raw) as ActionScript;
  if (!Array.isArray(parsed.actions) || parsed.actions.length === 0) {
    throw new Error(`Action script '${actionScriptPath}' must contain a non-empty actions array.`);
  }

  return parsed;
}

function resolveCommand(actionScript: ActionScript): ResolvedCommand {
  if (actionScript.fixture?.trim()) {
    return resolveFixtureCommand(actionScript.fixture.trim(), actionScript);
  }

  if (typeof actionScript.command === 'string') {
    return resolveShellCommand(actionScript.command, actionScript.cwd, actionScript.env);
  }

  if (actionScript.command) {
    const command = actionScript.command;
    if (command.commandLine?.trim()) {
      return resolveShellCommand(command.commandLine.trim(), command.cwd ?? actionScript.cwd, mergeEnv(actionScript.env, command.env));
    }

    if (!command.program?.trim()) {
      throw new Error('Command objects require program or commandLine.');
    }

    return {
      program: command.program,
      args: command.args ?? [],
      cwd: resolveWorkingDirectory(command.cwd ?? actionScript.cwd),
      env: mergeEnv(actionScript.env, command.env),
      commandDescription: [command.program, ...(command.args ?? [])].join(' ')
    };
  }

  throw new Error('Action script must define fixture or command.');
}

function resolveFixtureCommand(fixtureName: string, actionScript: ActionScript): ResolvedCommand {
  if (fixtureName !== sampleFixtureName) {
    throw new Error(`Unknown built-in fixture '${fixtureName}'.`);
  }

  const tsxCliPath = path.join(repoRoot, 'node_modules', 'tsx', 'dist', 'cli.mjs');
  const fixturePath = path.join(repoRoot, 'scripts', 'fixtures', 'sample-tui.ts');
  return {
    program: process.execPath,
    args: [tsxCliPath, fixturePath],
    cwd: resolveWorkingDirectory(actionScript.cwd),
    env: mergeEnv(actionScript.env),
    commandDescription: `fixture:${fixtureName}`
  };
}

function resolveShellCommand(
  commandLine: string,
  cwd: string | undefined,
  envOverrides: Record<string, string> | undefined
): ResolvedCommand {
  const program = process.platform === 'win32'
    ? process.env.ComSpec ?? 'cmd.exe'
    : process.env.SHELL ?? '/bin/bash';
  const args = process.platform === 'win32'
    ? ['/d', '/s', '/c', commandLine]
    : ['-lc', commandLine];

  return {
    program,
    args,
    cwd: resolveWorkingDirectory(cwd),
    env: mergeEnv(envOverrides),
    commandDescription: commandLine
  };
}

function mergeEnv(...overrides: Array<Record<string, string> | undefined>): Record<string, string> {
  return Object.assign({}, process.env, ...overrides);
}

function resolveWorkingDirectory(cwd: string | undefined): string {
  if (!cwd?.trim()) {
    return repoRoot;
  }

  return path.resolve(repoRoot, cwd);
}

function parseKeys(input: string): string[] {
  const result: string[] = [];
  let current = '';

  for (let index = 0; index < input.length; index += 1) {
    const character = input[index]!;
    if (character !== '{') {
      current += character;
      continue;
    }

    const closingIndex = input.indexOf('}', index);
    if (closingIndex < 0) {
      current += character;
      continue;
    }

    if (current.length > 0) {
      result.push(current);
      current = '';
    }

    const token = input.slice(index + 1, closingIndex).trim().toLowerCase();
    result.push(resolveKeyToken(token));
    index = closingIndex;
  }

  if (current.length > 0) {
    result.push(current);
  }

  return result;
}

function resolveKeyToken(token: string): string {
  const directMap: Record<string, string> = {
    enter: '\r',
    tab: '\t',
    escape: '\x1b',
    esc: '\x1b',
    up: '\x1b[A',
    down: '\x1b[B',
    right: '\x1b[C',
    left: '\x1b[D',
    home: '\x1b[H',
    end: '\x1b[F',
    pagedown: '\x1b[6~',
    pageup: '\x1b[5~',
    delete: '\x1b[3~',
    backspace: '\x7f',
    space: ' '
  };

  const direct = directMap[token];
  if (direct) {
    return direct;
  }

  const ctrlMatch = /^ctrl\+([a-z])$/u.exec(token);
  if (ctrlMatch) {
    return String.fromCharCode(ctrlMatch[1]!.toUpperCase().charCodeAt(0) - 64);
  }

  throw new Error(`Unsupported key token '{${token}}'.`);
}

async function waitForExit(exitPromise: Promise<ExitResult>, timeoutMs: number): Promise<ExitResult> {
  const timed = await Promise.race([
    exitPromise,
    delay(timeoutMs).then(() => undefined)
  ]);

  if (!timed) {
    throw new Error(`Timed out waiting ${timeoutMs} ms for process exit.`);
  }

  return timed;
}

function stripKnownExtension(filePath: string): string {
  return filePath.replace(/\.(json|txt|ansi|screen)$/iu, '');
}

async function writeTextFile(targetPath: string, content: string): Promise<void> {
  await mkdir(path.dirname(targetPath), { recursive: true });
  await writeFile(targetPath, normalizeNewlines(content), 'utf8');
}

async function writeJson(targetPath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(targetPath), { recursive: true });
  await writeFile(targetPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function normalizeNewlines(value: string): string {
  return value.replace(/\r\n/gu, '\n');
}

await main();
process.exit(0);
