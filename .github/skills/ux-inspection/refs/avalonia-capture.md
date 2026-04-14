# Avalonia capture

## When to consult this note

- Capturing the desktop GUI before a layout or styling change
- Inspecting the main log window with deterministic data
- Exporting a named control such as `RecentQsoGrid`
- Avoiding flaky OS-level screenshots

## QsoRipper rules

- Use the app's built-in screenshot mode; do not rely on generic desktop screenshots.
- Load fixture data so capture does not depend on a live engine or local persisted state.
- Prefer `RecentQsoGrid` capture when only the hot-path grid matters.
- Keep capture deterministic by avoiding environment-specific layout persistence.
- Save PNG output under `artifacts/ux/current/` and emit JSON summary beside it.

## Supported targets

- `MainWindow`
- Any named control reachable from the main window, for example `RecentQsoGrid`
- Live inspection surfaces exposed through `drive-avalonia.ps1`: `MainWindow`, `Settings`, `Wizard`

## Fixture expectations

- Fixture data should define recent QSO rows and any visible search/filter context needed for the scenario.
- Keep timestamps, sync state, station/profile labels, and long-text cases stable.
- Prefer fixture data that stresses dense rows, truncation, and column balance.

## Capture flow

1. Launch `QsoRipper.Gui` in screenshot mode.
2. Load the requested fixture or the built-in default sample.
3. Apply deterministic startup state without touching local persisted layout.
4. Render the window or named control to PNG using `RenderTargetBitmap`.
5. Exit cleanly after saving the artifact and JSON summary.

## Check this first

- Are you targeting the correct control name?
- Is a local persisted grid layout being bypassed for deterministic capture?
- Does the fixture contain enough rows and long text to expose the defect?
- Are you capturing the whole window when a control-only capture would be clearer?

## Minimal command

```powershell
.\scripts\capture-avalonia.ps1 -Scenario main-window
.\scripts\capture-avalonia.ps1 -Scenario recent-qso-grid -Target RecentQsoGrid
.\scripts\drive-avalonia.ps1 -ActionScript .\scripts\automation\avalonia-main-window-smoke.json
.\scripts\drive-avalonia.ps1 -ActionScript .\scripts\automation\avalonia-settings-smoke.json
.\scripts\drive-avalonia.ps1 -Fixture .\scripts\fixtures\ux-setup-wizard.fixture.json -ActionScript .\scripts\automation\avalonia-setup-wizard-smoke.json
```
