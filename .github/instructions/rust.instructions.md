# Rust Instructions

## Purpose

These instructions govern Rust work in LogRipper, especially under `src/rust/`, `proto/`, and the C FFI boundary under `src/c/`.

## Architecture Rules

- `src/rust/logripper-core` owns reusable engine logic, domain mapping, generated proto bindings, and adapter seams.
- `src/rust/logripper-server` owns runtime hosting and tonic server startup; keep process/bootstrap logic there rather than pushing it into `logripper-core`.
- Keep ADIF, QRZ, and other external formats/services at the Rust edge. Normalize into project-owned proto/domain types immediately.
- Do not move engine behavior into .NET debug surfaces; .NET remains a client/inspection layer.

## Current Rust Semantics

- Distinguish the crate **edition** from the installed **compiler version**. `edition = "2021"` does not mean the repo is using an old compiler.
- This repository should be evaluated against the pinned stable toolchain in `rust-toolchain.toml`, not against stale assumptions from older Rust blog posts or pre-2024 guidance.
- When a bug report or review comment depends on Rust language semantics, lint behavior, or undefined behavior claims, verify against current official sources before acting.
- Treat the Rust 2024 edition guide and current stable release notes as the canonical source for edition-era behavior changes.

## Implementation Rules

- Prefer idiomatic, current-stable Rust patterns over bespoke workarounds for language limitations that may no longer apply.
- Keep filesystem code portable with `Path` and `PathBuf`; do not hardcode separators in Rust source.
- Keep unsafe usage explicit and narrow. If unsafe behavior is required, document the invariant locally and avoid broad unsafe regions.
- Prefer narrow lint suppressions with justification. Use `#[expect(...)]` instead of broad `#[allow(...)]` when the lint should stay visible if the underlying issue disappears.
- Use `thiserror` for typed Rust error surfaces rather than ad hoc stringly errors.
- New Rust features, behavior changes, and non-trivial new branches must include tests in the same change. If coverage is hard to reach, refactor the seams for testability rather than leaving the logic effectively untested.

## Validation

- Standard Rust validation starts with:
  - `cargo fmt --manifest-path src/rust/Cargo.toml --all -- --check`
  - `cargo test --manifest-path src/rust/Cargo.toml`
  - `cargo clippy --manifest-path src/rust/Cargo.toml --all-targets -- -D warnings`
- For new Rust behavior and substantial Rust changes, also run the coverage gate locally:
  - `cargo llvm-cov --manifest-path src/rust/Cargo.toml --all --lcov --output-path rust-coverage.lcov`
  - `cargo llvm-cov report --manifest-path src/rust/Cargo.toml --summary-only`
- Keep Rust line coverage at or above the current CI threshold (80%) and do not push Rust changes that already fail formatting, lint, test, coverage, `buf lint`, or `cargo deny` gates that apply to the change.
- Rust formatting rules live in `src/rust/rustfmt.toml`; prefer changing that file over arguing style ad hoc.
- Workspace lint policy lives in `src/rust/Cargo.toml`; keep shared Clippy and rustc lint defaults centralized there.
- When proto or service contracts change, also run:
  - `buf lint`
  - `cargo run --manifest-path src/rust/Cargo.toml -p logripper-server`
  - `dotnet run --project src/dotnet/LogRipper.Cli -- status`
- When changing Rust dependencies or supply-chain-sensitive infrastructure, also run:
  - `Push-Location src\rust; cargo deny check --config deny.toml; Pop-Location`
- Treat `cargo audit` as a manual/occasional vulnerability review rather than a per-PR CI requirement unless the team explicitly changes that policy.

## Authoritative Sources

- Rust Reference: https://doc.rust-lang.org/reference/
- Rust 2024 Edition Guide: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
- Rust release notes: https://blog.rust-lang.org/
- Rust Style Guide: https://doc.rust-lang.org/style-guide/
- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Clippy docs: https://rust-lang.github.io/rust-clippy/master/
- RustSec / cargo-audit: https://rustsec.org/
