---
name: ux-inspection
description: >-
  Capture and inspect web UI, Avalonia desktop UI, and terminal UX artifacts for QsoRipper.
  Use when diagnosing visual defects, clipping, density regressions, layout glitches, screenshot
  diffs, or before/after UI changes that need repeatable artifacts.
---

# Skill: UX Inspection

## When to Use

- A UI looks clipped, misaligned, sparse, unstable, or visually noisy.
- You want an artifact before editing so the defect is concrete.
- You want a baseline/diff loop for a web page, Avalonia window, or terminal workflow.
- You need a repeatable capture path Copilot CLI can run without ad hoc screenshot steps.

## Capture Targets

### Web UI

- Script: `scripts/capture-web.ts`
- Diff script: `scripts/capture-web-diff.ts`
- Best for DebugHost pages, browser flows, selector-level screenshots, and baseline diffs.

### Avalonia desktop UI

- Script: `scripts/capture-avalonia.ps1`
- Interactive script: `scripts/drive-avalonia.ps1`
- Backed by a first-class screenshot mode inside `src/dotnet/QsoRipper.Gui`
- Best for deterministic `MainWindow` and named-control capture such as `RecentQsoGrid`
- Use the interactive driver when you need real clicks, text entry, menu traversal, column drags, or modal dialog inspection against a fixture-backed live window

### Terminal / TUI workflows

- Script: `scripts/capture-tui.ps1` (Windows-only today)
- Interactive script: `scripts/drive-tui.ts`
- Today this targets scripted CLI/terminal workflows honestly; point it at a future TUI binary later
- Best for terminal review artifacts, rendered PNG snapshots, transcripts, GIF capture through Terminalizer, and PTY-driven screen-state automation

## Required Workflow

1. Capture the current artifact into `artifacts/ux/current/`.
2. If a baseline exists, compare against `artifacts/ux/baseline/`.
3. Describe the visible defects before editing.
4. Make the smallest viable patch.
5. Re-capture the same scenario.
6. Report what changed and what still looks wrong.

## Artifact Conventions

- Current captures live under `artifacts/ux/current/`
- Baselines live under `artifacts/ux/baseline/`
- Diffs live under `artifacts/ux/diff/`
- Prefer scenario-based names such as:
  - `debughost-home.png`
  - `main-window.png`
  - `recent-qso-grid.png`
  - `cli-help.gif`

Each capture script should also emit a JSON sidecar or summary so later steps can inspect the latest artifact without guessing paths.

## Review Checklist

- blank or broken regions
- clipped text or controls
- oversized chrome
- alignment drift
- unstable spacing
- column width problems
- visual noise
- density regressions
- focus and selection visibility
- rendering artifacts

## QsoRipper Rules

- Capture before editing when the defect is visual.
- Prefer deterministic fixture-backed capture over manual screenshots.
- Prefer named control capture on Avalonia hot-path surfaces when the whole window is not needed.
- Keep hot-path review focused on density, scan speed, and keyboard-first affordances.
- Do not recommend a new UI container/control without capturing the current problem first.

## Advanced MCP Layer

The pragmatic layer is scripts plus this skill. When a uniform tool interface becomes worth the maintenance cost, wrap the same capture flows behind MCP tools such as:

- `capture_web`
- `capture_avalonia`
- `capture_terminal`
- `compare_artifacts`
- `list_latest_artifacts`

The repo already includes Playwright MCP configuration in `.mcp.json`; keep custom MCP work incremental and script-backed.

## Local References

- `refs/web-capture.md`
- `refs/avalonia-capture.md`
- `refs/tui-capture.md`
- `refs/visual-review-checklist.md`
