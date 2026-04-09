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
- The engine exposes a gRPC API; UX implementations are independent consumers.
- No specific UI technology is required or privileged.
- Keep third-party integrations isolated behind interfaces.
- Make offline logging resilient, even when network integrations fail.
- Rust owns the core engine, QRZ providers, and gRPC server.
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

## Cross-Platform

All code must work on both Windows and Linux. The engine is developed on Windows but runs in Linux Docker containers in production.

- Use `std::path::Path` and `PathBuf` in Rust for all filesystem operations. Never hardcode path separators.
- In C code, use only portable POSIX/C standard headers (`<stdint.h>`, `<stddef.h>`, `<string.h>`, etc.). Avoid Windows-specific headers like `<windows.h>` unless behind a platform guard.
- Use `#[cfg(target_os = "...")]` in Rust or `#ifdef _WIN32` / `#ifdef __linux__` in C only when platform-specific behavior is genuinely unavoidable. Prefer portable abstractions.
- Do not assume a specific shell. Build and test commands should work with `cargo build` / `cargo test` on any platform.
- Test on both Windows and Linux before merging platform-sensitive changes.
