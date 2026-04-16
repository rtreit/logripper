---
name: rust-engine-workflow
description: >-
  Implement and review Rust engine changes for QsoRipper. Use when working in src/rust/,
  touching qsoripper-core, qsoripper-server, build.rs, ADIF/QRZ adapters, or validating
  current Rust behavior against official language guidance.
---

# Skill: Rust Engine Workflow

## When to Use

- Editing files under `src/rust/`
- Adding or changing engine behavior in `qsoripper-core`
- Changing tonic server startup or hosting in `qsoripper-server`
- Reviewing Rust bugs that may depend on current language semantics or edition behavior
- Working near `build.rs`, generated proto bindings, or the C DSP boundary

## Core Repo Rules

1. Keep reusable engine logic in `src/rust/qsoripper-core`.
2. Keep process startup and tonic host wiring in `src/rust/qsoripper-server`.
3. Do not hand-edit generated proto code; change `proto/` and regenerate.
4. Keep ADIF and provider-specific formats at the edge, normalized into project-owned types.
5. Favor current stable Rust semantics over workarounds for outdated compiler behavior.
6. When adding new RPCs, services, or behavioral changes, update `docs/architecture/engine-specification.md` in the same change so both engines stay aligned.

## Validation Loop

```powershell
cargo fmt --manifest-path src\rust\Cargo.toml --all -- --check
cargo test --manifest-path src\rust\Cargo.toml
cargo clippy --manifest-path src\rust\Cargo.toml --all-targets -- -D warnings
```

If the change affects gRPC or schema behavior:

```powershell
buf lint
cargo run --manifest-path src\rust\Cargo.toml -p qsoripper-server
dotnet run --project src\dotnet\QsoRipper.Cli -- status
```

## Current Reference Sources

- Rust Reference: https://doc.rust-lang.org/reference/
- Rust 2024 Edition Guide: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
- Rust release notes: https://blog.rust-lang.org/
- Rust Style Guide: https://doc.rust-lang.org/style-guide/

## QsoRipper-Specific Paths

- `src/rust/qsoripper-core/`
- `src/rust/qsoripper-server/`
- `src/rust/qsoripper-core/build.rs`
- `proto/`
- `src/c/qsoripper-dsp/`
