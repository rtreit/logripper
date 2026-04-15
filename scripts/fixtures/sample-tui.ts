import process from 'node:process';

type FocusArea = 'tabs' | 'filter' | 'list';
type TabName = 'Recent' | 'Needed' | 'Review';

interface SampleItem {
  callsign: string;
  band: string;
  mode: string;
  note: string;
  tab: TabName;
}

const tabs: TabName[] = ['Recent', 'Needed', 'Review'];
const items: SampleItem[] = [
  { callsign: 'K1ABC', band: '20m', mode: 'FT8', note: 'Strong east coast signal.', tab: 'Recent' },
  { callsign: 'DL1XYZ', band: '15m', mode: 'CW', note: 'Needed for DXCC challenge slot.', tab: 'Recent' },
  { callsign: 'JA1AAA', band: '10m', mode: 'SSB', note: 'Morning opening from Tokyo.', tab: 'Recent' },
  { callsign: 'VK2QSO', band: '20m', mode: 'CW', note: 'Low-noise path after sunset.', tab: 'Needed' },
  { callsign: 'ZS6HAM', band: '17m', mode: 'FT8', note: 'Still needed on 17m digital.', tab: 'Needed' },
  { callsign: 'LU5DX', band: '40m', mode: 'SSB', note: 'Potential greyline check.', tab: 'Needed' },
  { callsign: 'EA8ZZ', band: '12m', mode: 'CW', note: 'Review lotw upload before sync.', tab: 'Review' },
  { callsign: 'VE3LOG', band: '6m', mode: 'FT8', note: 'Operator note mentions Es opening.', tab: 'Review' },
  { callsign: 'KH6PAC', band: '20m', mode: 'SSB', note: 'Review station profile before posting.', tab: 'Review' }
];

let activeTabIndex = 0;
let focus: FocusArea = 'tabs';
let filter = '';
let selectedIndex = 0;
let showDetails = false;
let shouldExit = false;

const stdin = process.stdin;
const stdout = process.stdout;

if (stdin.isTTY) {
  stdin.setEncoding('utf8');
  stdin.setRawMode(true);
}

stdin.resume();
stdin.on('data', (chunk: string) => {
  for (const key of splitKeys(chunk)) {
    handleKey(key);
    if (shouldExit) {
      cleanupAndExit();
      return;
    }
  }

  render();
});

process.on('SIGINT', () => cleanupAndExit());
process.on('SIGTERM', () => cleanupAndExit());
process.on('exit', () => {
  stdout.write('\x1b[?25h\x1b[?1049l');
});

stdout.write('\x1b[?1049h\x1b[?25l');
render();

function handleKey(key: string): void {
  if (key === '\u0003') {
    shouldExit = true;
    return;
  }

  if (key === '\t') {
    focus = focus === 'tabs'
      ? 'filter'
      : focus === 'filter'
        ? 'list'
        : 'tabs';
    return;
  }

  if (focus === 'tabs') {
    handleTabsKey(key);
    return;
  }

  if (focus === 'filter') {
    handleFilterKey(key);
    return;
  }

  handleListKey(key);
}

function handleTabsKey(key: string): void {
  if (key === '\x1b[C') {
    activeTabIndex = (activeTabIndex + 1) % tabs.length;
    selectedIndex = 0;
    showDetails = false;
    return;
  }

  if (key === '\x1b[D') {
    activeTabIndex = (activeTabIndex + tabs.length - 1) % tabs.length;
    selectedIndex = 0;
    showDetails = false;
    return;
  }

  if (key.toLowerCase() === 'q') {
    shouldExit = true;
  }
}

function handleFilterKey(key: string): void {
  if (key === '\x1b') {
    if (filter.length > 0) {
      filter = '';
      selectedIndex = 0;
    } else {
      focus = 'list';
    }
    return;
  }

  if (key === '\x7f' || key === '\b') {
    if (filter.length > 0) {
      filter = filter.slice(0, -1);
      selectedIndex = 0;
    }
    return;
  }

  if (key.length === 1 && key >= ' ') {
    filter += key;
    selectedIndex = 0;
    return;
  }

  if (key === '\r') {
    focus = 'list';
  }
}

function handleListKey(key: string): void {
  const visibleItems = getVisibleItems();
  if (key === '\x1b[A') {
    selectedIndex = Math.max(0, selectedIndex - 1);
    return;
  }

  if (key === '\x1b[B') {
    selectedIndex = Math.min(Math.max(visibleItems.length - 1, 0), selectedIndex + 1);
    return;
  }

  if (key === '\r') {
    showDetails = !showDetails;
    return;
  }

  if (key === '\x1b') {
    showDetails = false;
    return;
  }

  if (key.toLowerCase() === 'q') {
    shouldExit = true;
  }
}

function getVisibleItems(): SampleItem[] {
  const activeTab = tabs[activeTabIndex]!;
  const normalizedFilter = filter.trim().toLowerCase();
  const filtered = items.filter((item) => item.tab === activeTab);
  if (!normalizedFilter) {
    return filtered;
  }

  return filtered.filter((item) =>
    `${item.callsign} ${item.band} ${item.mode} ${item.note}`.toLowerCase().includes(normalizedFilter)
  );
}

function render(): void {
  const columns = stdout.columns || 100;
  const rows = stdout.rows || 30;
  const visibleItems = getVisibleItems();
  selectedIndex = Math.min(selectedIndex, Math.max(visibleItems.length - 1, 0));
  const selected = visibleItems[selectedIndex];
  const divider = '-'.repeat(Math.max(1, columns));
  const lines: string[] = [];

  lines.push('QsoRipper Sample TUI');
  lines.push(renderTabsLine(columns));
  lines.push(divider);
  lines.push(padRight(`Filter: ${filter || '(type to filter)'}`, columns));
  lines.push(padRight(`Focus: ${focus} | Visible QSOs: ${visibleItems.length}`, columns));
  lines.push(divider);
  lines.push(padRight('Callsign    Band  Mode  Notes', columns));

  const listHeight = Math.max(6, rows - 16);
  for (let index = 0; index < listHeight; index += 1) {
    const item = visibleItems[index];
    if (!item) {
      lines.push(padRight('', columns));
      continue;
    }

    const prefix = focus === 'list' && index === selectedIndex ? '>' : ' ';
    const detailMarker = showDetails && index === selectedIndex ? '*' : ' ';
    const row = `${prefix}${detailMarker} ${item.callsign.padEnd(9)} ${item.band.padEnd(4)} ${item.mode.padEnd(4)} ${item.note}`;
    lines.push(padRight(row.slice(0, columns), columns));
  }

  lines.push(divider);
  if (selected) {
    lines.push(padRight(`Selected: ${selected.callsign} on ${selected.band} ${selected.mode}`, columns));
    if (showDetails) {
      for (const line of wrapText(`Details: ${selected.note}`, columns)) {
        lines.push(padRight(line, columns));
      }
    } else {
      lines.push(padRight('Details: press Enter to expand the selected item.', columns));
    }
  } else {
    lines.push(padRight('No rows match the current filter.', columns));
    lines.push(padRight('Details: clear or change the filter to restore the list.', columns));
  }

  lines.push(divider);
  lines.push(padRight('Keys: Left/Right tabs | Tab focus | Type filter | Up/Down select | Enter details | Esc clear/collapse | q quit', columns));

  stdout.write(`\x1b[2J\x1b[H${lines.slice(0, rows).join('\n')}`);
}

function renderTabsLine(columns: number): string {
  const parts = tabs.map((tab, index) => {
    const active = index === activeTabIndex ? `[${tab}]` : ` ${tab} `;
    if (focus === 'tabs' && index === activeTabIndex) {
      return `>${active}<`;
    }

    return ` ${active} `;
  });

  return padRight(parts.join(' '), columns);
}

function wrapText(text: string, width: number): string[] {
  const safeWidth = Math.max(20, width);
  const words = text.split(/\s+/u);
  const lines: string[] = [];
  let current = '';

  for (const word of words) {
    const next = current.length === 0 ? word : `${current} ${word}`;
    if (next.length > safeWidth) {
      if (current.length > 0) {
        lines.push(current);
      }
      current = word;
    } else {
      current = next;
    }
  }

  if (current.length > 0) {
    lines.push(current);
  }

  return lines;
}

function padRight(value: string, width: number): string {
  if (value.length >= width) {
    return value.slice(0, width);
  }

  return `${value}${' '.repeat(width - value.length)}`;
}

function splitKeys(input: string): string[] {
  const keys: string[] = [];
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index]!;
    if (character !== '\x1b') {
      keys.push(character);
      continue;
    }

    const next = input[index + 1];
    const third = input[index + 2];
    if (next === '[' && third) {
      keys.push(input.slice(index, index + 3));
      index += 2;
      continue;
    }

    keys.push(character);
  }

  return keys;
}

function cleanupAndExit(): void {
  stdout.write('\x1b[2J\x1b[H');
  stdout.write('\x1b[?25h\x1b[?1049l');
  process.exit(0);
}
