# Web capture

## When to consult this note

- Capturing the DebugHost or another local browser surface
- Taking a screenshot before editing a web page
- Producing a baseline or diff for a browser page
- Reducing screenshot flake from animations or unstable layout

## QsoRipper rules

- Use Playwright for web capture.
- Prefer scenario names over raw URLs when a built-in route alias exists.
- Default to deterministic browser settings: fixed viewport, reduced motion, and disabled animations.
- Prefer element screenshots when the defect is local to one region.
- Keep baselines environment-specific; browser rendering changes across hosts.

## Built-in scenario expectations

- `debughost-home` -> `/`
- `debughost-engine` -> `/engine`
- `debughost-protobuf-lab` -> `/protobuf-lab`
- `debughost-lookup-workbench` -> `/lookup-workbench`
- `debughost-storage-workbench` -> `/storage-workbench`
- `debughost-logbook-interop` -> `/logbook-interop`
- `debughost-qso-viewer` -> `/qso-viewer`
- `debughost-commands` -> `/commands`

## Capture flow

1. Launch the local web host if needed.
2. Navigate to the resolved URL.
3. Wait for the page and any requested selector to stabilize.
4. Apply deterministic screenshot settings.
5. Save PNG under `artifacts/ux/current/`.
6. Write JSON summary alongside the image.

## Diff flow

1. Capture current output for the same scenario and viewport.
2. Compare against `artifacts/ux/baseline/<scenario>.png`.
3. Save diff output under `artifacts/ux/diff/` when pixels differ.
4. Fail explicitly on missing baselines unless baseline creation was requested.

## Common failure modes

- **Unstyled / no CSS**: DebugHost must run in `Development` environment.
  `MapStaticAssets` in Production mode serves pre-compressed `.gz` files that
  only exist after `dotnet publish`. When using `dotnet run`, set
  `ASPNETCORE_ENVIRONMENT=Development` or the page renders with zero styling.
  The `--launch-debughost` flag handles this automatically.
- **Blazor not interactive**: The page prerender may look correct structurally
  but interactive components won't respond. Wait for `blazor.web.js` to
  establish the SignalR circuit before capturing interactive state.
- **Stale fingerprinted URLs**: After rebuilding, the fingerprint hashes in CSS
  `<link>` tags change. Restart the DebugHost after rebuild to pick up new
  static asset manifests.

## Check this first

- Is the capture using the expected route and viewport?
- Is the DebugHost running in `Development` environment? (check for unstyled output)
- Is the page still animating or loading late content?
- Is the defect page-wide or local to one selector?
- Are you comparing against a baseline produced on the same platform/browser setup?

## Minimal commands

```powershell
npm run ux:capture:web -- --scenario debughost-home --launch-debughost
npm run ux:diff:web -- --scenario debughost-home --launch-debughost
```
