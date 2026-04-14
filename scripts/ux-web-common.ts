import { spawn } from 'node:child_process';
import { access, copyFile, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

export const repoRoot = process.cwd();
export const uxRoot = path.join(repoRoot, 'artifacts', 'ux');
export const currentRoot = path.join(uxRoot, 'current');
export const baselineRoot = path.join(uxRoot, 'baseline');
export const diffRoot = path.join(uxRoot, 'diff');
export const defaultDebugHostBaseUrl = 'http://127.0.0.1:5082';

const scenarioRoutes: Record<string, string> = {
  'debughost-home': '/',
  'debughost-engine': '/engine',
  'debughost-protobuf-lab': '/protobuf-lab',
  'debughost-lookup-workbench': '/lookup-workbench',
  'debughost-storage-workbench': '/storage-workbench',
  'debughost-logbook-interop': '/logbook-interop',
  'debughost-qso-viewer': '/qso-viewer',
  'debughost-commands': '/commands'
};

export interface WebCaptureOptions {
  scenario: string;
  url: string;
  selector?: string;
  waitForSelector?: string;
  outputPath: string;
  fullPage: boolean;
  launchDebugHost: boolean;
  theme: 'dark' | 'light';
  viewportWidth: number;
  viewportHeight: number;
}

export interface WebCaptureSummary {
  surface: 'web';
  scenario: string;
  url: string;
  selector?: string;
  waitForSelector?: string;
  outputPath: string;
  fullPage: boolean;
  launchDebugHost: boolean;
  theme: 'dark' | 'light';
  viewportWidth: number;
  viewportHeight: number;
  capturedAtUtc: string;
}

interface ManagedProcess {
  stop(): Promise<void>;
}

export async function ensureUxDirectories(): Promise<void> {
  await Promise.all([
    mkdir(currentRoot, { recursive: true }),
    mkdir(baselineRoot, { recursive: true }),
    mkdir(diffRoot, { recursive: true })
  ]);
}

export function parseCliArgs(argv: string[]): Map<string, string | boolean> {
  const options = new Map<string, string | boolean>();

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) {
      continue;
    }

    const next = argv[i + 1];
    if (!next || next.startsWith('--')) {
      options.set(arg, true);
      continue;
    }

    options.set(arg, next);
    i += 1;
  }

  return options;
}

export function getRequiredStringOption(
  options: Map<string, string | boolean>,
  name: string
): string {
  const value = options.get(name);
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`Missing required option ${name}.`);
  }

  return value.trim();
}

export function getStringOption(
  options: Map<string, string | boolean>,
  name: string,
  fallback?: string
): string | undefined {
  const value = options.get(name);
  if (typeof value === 'string' && value.trim().length > 0) {
    return value.trim();
  }

  return fallback;
}

export function getBooleanOption(options: Map<string, string | boolean>, name: string): boolean {
  return options.get(name) === true;
}

export function getNumberOption(
  options: Map<string, string | boolean>,
  name: string,
  fallback: number
): number {
  const value = options.get(name);
  if (typeof value !== 'string') {
    return fallback;
  }

  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`Option ${name} must be a positive integer.`);
  }

  return parsed;
}

export function currentImagePath(scenario: string): string {
  return path.join(currentRoot, `${scenario}.png`);
}

export function baselineImagePath(scenario: string): string {
  return path.join(baselineRoot, `${scenario}.png`);
}

export function diffImagePath(scenario: string): string {
  return path.join(diffRoot, `${scenario}.diff.png`);
}

export function jsonSidecarPath(imagePath: string): string {
  return imagePath.replace(/\.png$/i, '.json');
}

export function resolveUrl(
  scenario: string,
  baseUrl: string,
  explicitUrl?: string
): string {
  if (explicitUrl) {
    return explicitUrl;
  }

  const route = scenarioRoutes[scenario];
  if (!route) {
    throw new Error(`Scenario '${scenario}' does not have a built-in route. Use --url.`);
  }

  return new URL(route, ensureTrailingSlash(baseUrl)).toString();
}

export async function captureWeb(options: WebCaptureOptions): Promise<WebCaptureSummary> {
  await ensureUxDirectories();
  const managedProcess = await maybeStartDebugHost(options.launchDebugHost, new URL(options.url).origin);
  const browser = await chromium.launch({ headless: true });

  try {
    const context = await browser.newContext({
      viewport: {
        width: options.viewportWidth,
        height: options.viewportHeight
      },
      colorScheme: options.theme,
      reducedMotion: 'reduce'
    });
    const page = await context.newPage();

    await page.goto(options.url, { waitUntil: 'networkidle' });
    if (options.waitForSelector) {
      await page.locator(options.waitForSelector).waitFor({ state: 'visible' });
    }

    await page.addStyleTag({
      content: [
        '* { animation: none !important; transition: none !important; caret-color: transparent !important; }',
        '*::before, *::after { animation: none !important; transition: none !important; }',
        'html { scroll-behavior: auto !important; }'
      ].join('\n')
    });
    await page.evaluate(async () => {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    });
    await mkdir(path.dirname(options.outputPath), { recursive: true });

    if (options.selector) {
      const locator = page.locator(options.selector);
      await locator.waitFor({ state: 'visible' });
      await locator.screenshot({
        path: options.outputPath,
        animations: 'disabled',
        caret: 'hide'
      });
    } else {
      await page.screenshot({
        path: options.outputPath,
        animations: 'disabled',
        caret: 'hide',
        fullPage: options.fullPage
      });
    }

    const summary: WebCaptureSummary = {
      surface: 'web',
      scenario: options.scenario,
      url: options.url,
      selector: options.selector,
      waitForSelector: options.waitForSelector,
      outputPath: options.outputPath,
      fullPage: options.fullPage,
      launchDebugHost: options.launchDebugHost,
      theme: options.theme,
      viewportWidth: options.viewportWidth,
      viewportHeight: options.viewportHeight,
      capturedAtUtc: new Date().toISOString()
    };

    await writeSummary(summary, jsonSidecarPath(options.outputPath));
    await writeSummary(summary, currentReportPath());
    return summary;
  } finally {
    await browser.close();
    await managedProcess?.stop();
  }
}

export async function writeSummary(summary: unknown, outputPath?: string): Promise<void> {
  const targetPath = outputPath ?? currentReportPath();
  await mkdir(path.dirname(targetPath), { recursive: true });
  await writeFile(targetPath, `${JSON.stringify(summary, null, 2)}\n`, 'utf8');
}

export async function fileExists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

export async function removeIfExists(filePath: string): Promise<void> {
  if (await fileExists(filePath)) {
    await rm(filePath, { force: true });
  }
}

export async function readBuffer(filePath: string): Promise<Buffer> {
  return readFile(filePath);
}

export async function copyFileTo(sourcePath: string, destinationPath: string): Promise<void> {
  await mkdir(path.dirname(destinationPath), { recursive: true });
  await copyFile(sourcePath, destinationPath);
}

export function currentReportPath(): string {
  return path.join(currentRoot, 'report.json');
}

function ensureTrailingSlash(url: string): string {
  return url.endsWith('/') ? url : `${url}/`;
}

async function maybeStartDebugHost(
  shouldLaunch: boolean,
  baseUrl: string
): Promise<ManagedProcess | undefined> {
  if (!shouldLaunch) {
    return undefined;
  }

  const debugHostProject = path.join('src', 'dotnet', 'QsoRipper.DebugHost', 'QsoRipper.DebugHost.csproj');
  const debugHostOutputRoot = path.join(
    repoRoot,
    'artifacts',
    'ux',
    'web-debughost-bin',
    `${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`
  );
  await mkdir(debugHostOutputRoot, { recursive: true });

  const output: string[] = [];
  const child = spawn(
    'dotnet',
    [
      'run',
      '--project',
      debugHostProject,
      `-p:OutputPath=${debugHostOutputRoot}${path.sep}`,
      '--no-launch-profile',
      '--urls',
      baseUrl
    ],
    {
      cwd: repoRoot,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        // MapStaticAssets in Production mode serves pre-compressed .gz files
        // that only exist after `dotnet publish`. Development mode falls back
        // to serving source files from wwwroot, which is what we need for
        // `dotnet run` based captures.
        ASPNETCORE_ENVIRONMENT: 'Development'
      }
    }
  );

  child.stdout?.on('data', (buffer: Buffer) => {
    output.push(buffer.toString('utf8'));
    trimCapturedOutput(output);
  });
  child.stderr?.on('data', (buffer: Buffer) => {
    output.push(buffer.toString('utf8'));
    trimCapturedOutput(output);
  });

  await waitForHttpReady(baseUrl, output);

  return {
    async stop() {
      if (child.exitCode !== null) {
        return;
      }

      child.kill();
      await delay(250);
      if (child.exitCode === null) {
        child.kill('SIGKILL');
      }
    }
  };
}

async function waitForHttpReady(baseUrl: string, output: string[]): Promise<void> {
  const deadline = Date.now() + 60_000;

  while (Date.now() < deadline) {
    try {
      const response = await fetch(baseUrl);
      if (response.ok) {
        return;
      }
    } catch {
      // Ignore probe errors until timeout.
    }

    await delay(500);
  }

  throw new Error(
    [
      `Timed out waiting for DebugHost at ${baseUrl}.`,
      output.length > 0 ? 'Process output:' : '',
      output.join('').trim()
    ]
      .filter((line) => line.length > 0)
      .join('\n')
  );
}

function trimCapturedOutput(output: string[]): void {
  if (output.length > 20) {
    output.splice(0, output.length - 20);
  }
}
