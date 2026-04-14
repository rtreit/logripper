import path from 'node:path';
import {
  captureWeb,
  currentImagePath,
  defaultDebugHostBaseUrl,
  getBooleanOption,
  getNumberOption,
  getRequiredStringOption,
  getStringOption,
  jsonSidecarPath,
  parseCliArgs,
  resolveUrl
} from './ux-web-common.js';

async function main(): Promise<void> {
  const options = parseCliArgs(process.argv.slice(2));
  const scenario = getRequiredStringOption(options, '--scenario');
  const baseUrl = getStringOption(options, '--base-url', defaultDebugHostBaseUrl)!;
  const explicitUrl = getStringOption(options, '--url');
  const theme = normalizeTheme(getStringOption(options, '--theme', 'dark')!);
  const selector = getStringOption(options, '--selector');
  const waitForSelector = getStringOption(options, '--wait-for-selector');
  const outputPath = getStringOption(options, '--output', currentImagePath(scenario))!;
  const viewportWidth = getNumberOption(options, '--viewport-width', 1440);
  const viewportHeight = getNumberOption(options, '--viewport-height', 900);
  const url = resolveUrl(scenario, baseUrl, explicitUrl);

  const summary = await captureWeb({
    scenario,
    url,
    selector,
    waitForSelector,
    outputPath: path.resolve(outputPath),
    fullPage: getBooleanOption(options, '--full-page'),
    launchDebugHost: getBooleanOption(options, '--launch-debughost'),
    theme,
    viewportWidth,
    viewportHeight
  });

  console.log(`Saved web capture to ${summary.outputPath}`);
  console.log(`Summary: ${jsonSidecarPath(summary.outputPath)}`);
}

function normalizeTheme(value: string): 'dark' | 'light' {
  const normalized = value.trim().toLowerCase();
  if (normalized === 'dark' || normalized === 'light') {
    return normalized;
  }

  throw new Error(`Unsupported theme '${value}'. Use dark or light.`);
}

await main();
