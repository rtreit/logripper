# Copilot Instructions

## Project Overview

QsoRipper is a high-performance ham radio logging system focused on speed, clean workflows, and keyboard-first operation.

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
- Follow protobuf 1-1-1 by default: one top-level message, enum, or service per `.proto` file.
- Every RPC must use unique `XxxRequest` and `XxxResponse` envelopes. Streaming RPCs also get unique streamed response envelopes.
- Keep transport-only RPC messages in `proto/services/`, not `proto/domain/`.
- If multiple RPCs need the same payload, extract a separate reusable message and wrap it from each response instead of reusing one RPC response envelope as another RPC's payload.
- Exceptions to 1-1-1 are rare, must be explicit and documented, and never justify skipping per-RPC envelopes.
- Use `buf lint` to validate proto files. Use `buf breaking` to guard against incompatible schema changes.
- ADIF is for external interchange (QRZ API, file I/O) only — internal IPC uses protobuf.
- Keep shared proto messages discoverable in the Debug Host Protobuf Lab; prefer auto-discovered message catalogs over hand-maintained UI enums or lists.
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

## Quality and Coverage Gates

- Treat the existing CI quality and coverage thresholds as local pre-push requirements, not something to discover only after opening a PR.
- When implementing a new feature, behavior change, or new error path, add or expand automated tests in the same change so the new code is directly covered.
- Do not rely on existing coverage headroom to carry new code. If a feature adds meaningful logic, it should add meaningful test coverage too.
- For Rust changes, keep `cargo fmt`, `cargo clippy`, `cargo test`, `cargo llvm-cov`, `buf lint`, and `cargo deny` green when those gates apply.
- For .NET changes, keep `dotnet format`, `dotnet build`, and `dotnet test` with coverage green when those gates apply.
- Do not push code that you already know will fail an existing quality or coverage gate.

## Markdown Code Fences

When writing markdown that will be rendered on GitHub, such as PR descriptions, issue bodies, review comments, or other repository comments:

- Never use `bash` as the code fence language for Windows commands.
- Backslash path separators like `src\dotnet\QsoRipper.slnx` can render incorrectly in GitHub-flavored markdown when labeled as `bash`.
- Use a plain fenced code block with no language tag, or use `powershell` / `cmd` instead.
- Prefer Windows-style paths in those examples when the command is intended to run on Windows.

## Cross-Platform

All code must work on both Windows and Linux. The engine is developed on Windows but runs in Linux Docker containers in production.

- Use `std::path::Path` and `PathBuf` in Rust for all filesystem operations. Never hardcode path separators.
- In C code, use only portable POSIX/C standard headers (`<stdint.h>`, `<stddef.h>`, `<string.h>`, etc.). Avoid Windows-specific headers like `<windows.h>` unless behind a platform guard.
- Use `#[cfg(target_os = "...")]` in Rust or `#ifdef _WIN32` / `#ifdef __linux__` in C only when platform-specific behavior is genuinely unavoidable. Prefer portable abstractions.
- Do not assume a specific shell. Build and test commands should work with `cargo build` / `cargo test` on any platform.
- Test on both Windows and Linux before merging platform-sensitive changes.
