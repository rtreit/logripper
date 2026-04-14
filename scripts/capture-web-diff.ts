import path from 'node:path';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';
import {
  baselineImagePath,
  captureWeb,
  copyFileTo,
  currentImagePath,
  currentReportPath,
  defaultDebugHostBaseUrl,
  diffImagePath,
  fileExists,
  getBooleanOption,
  getNumberOption,
  getRequiredStringOption,
  getStringOption,
  jsonSidecarPath,
  parseCliArgs,
  readBuffer,
  removeIfExists,
  resolveUrl,
  writeSummary
} from './ux-web-common.js';

interface WebDiffSummary {
  surface: 'web';
  scenario: string;
  url: string;
  outputPath: string;
  baselinePath: string;
  diffPath?: string;
  diffPixels: number;
  status: 'matched' | 'diff-detected' | 'baseline-created' | 'baseline-updated';
  selector?: string;
  waitForSelector?: string;
  fullPage: boolean;
  launchDebugHost: boolean;
  theme: 'dark' | 'light';
  viewportWidth: number;
  viewportHeight: number;
  capturedAtUtc: string;
}

async function main(): Promise<void> {
  const options = parseCliArgs(process.argv.slice(2));
  const scenario = getRequiredStringOption(options, '--scenario');
  const baseUrl = getStringOption(options, '--base-url', defaultDebugHostBaseUrl)!;
  const explicitUrl = getStringOption(options, '--url');
  const theme = normalizeTheme(getStringOption(options, '--theme', 'dark')!);
  const selector = getStringOption(options, '--selector');
  const waitForSelector = getStringOption(options, '--wait-for-selector');
  const outputPath = path.resolve(getStringOption(options, '--output', currentImagePath(scenario))!);
  const baselinePath = path.resolve(getStringOption(options, '--baseline', baselineImagePath(scenario))!);
  const generatedDiffPath = path.resolve(getStringOption(options, '--diff', diffImagePath(scenario))!);
  const viewportWidth = getNumberOption(options, '--viewport-width', 1440);
  const viewportHeight = getNumberOption(options, '--viewport-height', 900);
  const fullPage = getBooleanOption(options, '--full-page');
  const launchDebugHost = getBooleanOption(options, '--launch-debughost');
  const updateBaseline = getBooleanOption(options, '--update-baseline');
  const url = resolveUrl(scenario, baseUrl, explicitUrl);

  const captureSummary = await captureWeb({
    scenario,
    url,
    selector,
    waitForSelector,
    outputPath,
    fullPage,
    launchDebugHost,
    theme,
    viewportWidth,
    viewportHeight
  });

  if (!(await fileExists(baselinePath))) {
    if (!updateBaseline) {
      throw new Error(`Baseline is missing at ${baselinePath}. Re-run with --update-baseline to create it.`);
    }

    await copyFileTo(outputPath, baselinePath);
    const summary: WebDiffSummary = {
      surface: 'web',
      scenario,
      url,
      outputPath,
      baselinePath,
      diffPixels: 0,
      status: 'baseline-created',
      selector,
      waitForSelector,
      fullPage,
      launchDebugHost,
      theme,
      viewportWidth,
      viewportHeight,
      capturedAtUtc: captureSummary.capturedAtUtc
    };
    await writeDiffSummary(outputPath, summary);
    console.log(`Created baseline ${baselinePath}`);
    return;
  }

  const baselinePng = PNG.sync.read(await readBuffer(baselinePath));
  const currentPng = PNG.sync.read(await readBuffer(outputPath));
  const width = Math.max(baselinePng.width, currentPng.width);
  const height = Math.max(baselinePng.height, currentPng.height);

  const normalizedBaseline = normalizePng(baselinePng, width, height);
  const normalizedCurrent = normalizePng(currentPng, width, height);
  const diffPng = new PNG({ width, height });
  const diffPixels = pixelmatch(
    normalizedBaseline.data,
    normalizedCurrent.data,
    diffPng.data,
    width,
    height,
    { threshold: 0.1 }
  );

  let diffPath: string | undefined;
  if (diffPixels > 0) {
    diffPath = generatedDiffPath;
    await removeIfExists(diffPath);
    await writeBinaryFile(diffPath, PNG.sync.write(diffPng));
  } else {
    await removeIfExists(generatedDiffPath);
  }

  let status: WebDiffSummary['status'] = diffPixels === 0 ? 'matched' : 'diff-detected';
  if (updateBaseline) {
    await copyFileTo(outputPath, baselinePath);
    status = 'baseline-updated';
  }

  const summary: WebDiffSummary = {
    surface: 'web',
    scenario,
    url,
    outputPath,
    baselinePath,
    diffPath,
    diffPixels,
    status,
    selector,
    waitForSelector,
    fullPage,
    launchDebugHost,
    theme,
    viewportWidth,
    viewportHeight,
    capturedAtUtc: captureSummary.capturedAtUtc
  };

  await writeDiffSummary(outputPath, summary);

  if (diffPixels > 0 && !updateBaseline) {
    process.exitCode = 1;
  }

  console.log(`Compared ${outputPath} against ${baselinePath}`);
  if (diffPixels > 0) {
    console.log(`Detected ${diffPixels} different pixels.`);
  }
}

function normalizeTheme(value: string): 'dark' | 'light' {
  const normalized = value.trim().toLowerCase();
  if (normalized === 'dark' || normalized === 'light') {
    return normalized;
  }

  throw new Error(`Unsupported theme '${value}'. Use dark or light.`);
}

function normalizePng(source: PNG, width: number, height: number): PNG {
  if (source.width === width && source.height === height) {
    return source;
  }

  const target = new PNG({ width, height });
  PNG.bitblt(source, target, 0, 0, source.width, source.height, 0, 0);
  return target;
}

async function writeDiffSummary(outputPath: string, summary: WebDiffSummary): Promise<void> {
  await writeSummary(summary, jsonSidecarPath(outputPath));
  await writeSummary(summary, currentReportPath());
}

async function writeBinaryFile(targetPath: string, content: Buffer): Promise<void> {
  const { mkdir, writeFile } = await import('node:fs/promises');
  await mkdir(path.dirname(targetPath), { recursive: true });
  await writeFile(targetPath, content);
}

await main();
