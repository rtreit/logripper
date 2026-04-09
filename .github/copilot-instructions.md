# Copilot Instructions

## Project Overview

LogRipper is a high-performance ham radio logging system focused on speed, clean workflows, and keyboard-first operation.

Primary goals:
- Fast TUI experience for operators during active radio operation
- Clean graphical interface for richer workflows
- Rich operator and station enrichment through QRZ lookups

## Engineering Principles

- Prefer Rust or C# for core runtime and performance-critical paths.
- Avoid Python for hot paths and primary services.
- Keep startup and interaction latency low.
- Favor small, composable modules over monoliths.

## Architecture Direction

- Keep the log engine independent from any specific UI.
- Share domain logic between TUI and GUI surfaces.
- Keep third-party integrations isolated behind interfaces.
- Make offline logging resilient, even when network integrations fail.
- Rust owns the core engine, TUI, and QRZ providers. .NET owns the GUI and reporting.
- Components communicate via gRPC with Protocol Buffer messages.

## Data Model Conventions

- All shared domain types are defined in `proto/` and generated for both Rust and C#.
- Proto files are the single source of truth. Never hand-write types that should come from proto generation.
- Use `buf lint` to validate proto files. Use `buf breaking` to guard against incompatible schema changes.
- ADIF is for external interchange (QRZ API, file I/O) only — internal IPC uses protobuf.
- See `docs/architecture/data-model.md` for full conventions.

## Domain Guidance

- The core entity is the QSO record.
- Standardize canonical fields early: callsign, UTC timestamp, band, mode, RST sent/received, operator, locator, notes.
- Preserve edit history and traceability for log corrections.

## Integration Guidance

- QRZ integration should be isolated from UI code.
- Never hardcode credentials or API keys.
- Use environment variables or secure configuration providers for secrets.
- Integration failures must degrade gracefully and never block local logging.

## UX Rules

- Keyboard-first by default for all high-frequency actions.
- Keep TUI and GUI behavior aligned where practical.
- Prioritize uninterrupted operator flow during contest and pileup scenarios.

## Tooling Notes

- Use PowerShell for Windows shell scripting.
- Use `rg` for text search operations.
- Keep build and test loops fast to support tight iteration.
