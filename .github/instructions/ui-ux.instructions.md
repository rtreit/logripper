# UI and UX Instructions

Design for keyboard-first operation with fast operator feedback.

## Interaction Rules

- All high-frequency actions must have shortcuts.
- Keep command latency and visual churn low.
- Make form navigation efficient for rapid QSO entry.
- Use consistent keymaps across TUI and GUI where practical.
- Surface validation errors clearly and immediately.
- For labels sourced from shared proto enums (for example band/mode in DebugHost), use shared display helpers rather than page-local string munging of generated enum names.
- When asked to visually inspect or interact with the Avalonia GUI, use the repo-backed UX inspection tooling first:
  - `.\scripts\capture-avalonia.ps1` for deterministic screenshots
  - `.\scripts\drive-avalonia.ps1` for live interaction, screenshots, UI tree dumps, and smoke scenarios
- Do not substitute `dotnet run --project src\dotnet\QsoRipper.Gui\QsoRipper.Gui.csproj` log scraping for visual inspection. A plain app launch is only for startup diagnostics such as crashes, binding warnings, or resource-load errors.

## Workflow Focus

- Rapid QSO entry
- Fast correction/edit flow
- Minimal friction during high-rate contest operation
