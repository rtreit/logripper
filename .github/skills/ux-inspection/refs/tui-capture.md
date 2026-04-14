# Terminal capture

## When to consult this note

- Recording a CLI or terminal workflow for review
- Producing a reviewable GIF plus transcript
- Comparing terminal output changes before and after a fix
- Wiring a future TUI binary into the same workflow

## QsoRipper rules

- Use `scripts\capture-tui.ps1` as the entry point; it is currently Windows-only and can bootstrap a compatible repo-local Terminalizer runtime when needed.
- Use `scripts\drive-tui.ts` when you need a live PTY session with JSON action scripts, wait/assert steps, and rendered screen snapshots under `artifacts\ux\current\<scenario>\`.
- Be honest about scope: today this path targets terminal workflows and the CLI, not a full ratatui UI.
- Keep terminal size and theme fixed so the rendered artifact is repeatable.
- Always save the raw transcript as well as the rendered artifact.
- Structure scenarios so they can later point at a dedicated TUI binary without changing the artifact contract.
- Keep `capture-tui.ps1` for rendered GIFs; the PTY driver complements it rather than replacing it.

## Default scenario posture

- Prefer deterministic help or scripted command flows over live-engine commands when you only need a stable visual artifact.
- Use explicit `-Command` overrides for custom or future TUI scenarios.

## Capture flow

1. Resolve a scenario to a terminal command.
2. Create deterministic Terminalizer config (rows, columns, theme).
3. Record the session to YAML.
4. Render the YAML to GIF.
5. Save the transcript and JSON summary beside the artifact.

## Live PTY automation flow

1. Resolve a JSON action script to a PTY command or built-in fixture.
2. Launch the terminal with fixed rows/columns.
3. Send scripted key/text input.
4. Wait for expected screen or transcript text.
5. Save visible screen snapshots (`*.screen.png`, `*.screen.txt`, `*.screen.json`, `*.ansi.txt`) plus `transcript.txt` and `report.json`.

## Check this first

- Did the script finish bootstrapping the local Terminalizer runtime successfully on this machine?
- Is the local Terminalizer install compatible with the Node runtime on this machine?
- Is the scenario deterministic enough for visual review?
- Does the transcript capture the same command/output the GIF shows?
- Would a plain transcript be more useful than a GIF for this defect?
- Is Playwright Chromium installed so the PTY driver can render PNG snapshots?

## Minimal command

```powershell
.\scripts\capture-tui.ps1 -Scenario cli-help
.\scripts\capture-tui.ps1 -Scenario custom -Command "dotnet run --project src\dotnet\QsoRipper.Cli -- --help"
npm run ux:drive:tui -- --action-script .\scripts\automation\tui-sample-smoke.json
```
