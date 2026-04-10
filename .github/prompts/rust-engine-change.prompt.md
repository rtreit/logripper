---
name: rust-engine-change
description: Implement a Rust engine or tonic/proto change in LogRipper with current-stable Rust guidance and regression-first validation.
---

# Rust Engine Change

Use this workflow when implementing or reviewing a Rust engine change in LogRipper.

1. Identify whether the change belongs in `logripper-core`, `logripper-server`, or `proto/`.
2. If the issue depends on Rust semantics, lints, or undefined behavior claims, verify against current official Rust sources before assuming older guidance still applies.
3. Add or update a failing regression test first when fixing a bug.
4. Implement the smallest correct fix that preserves engine/UI boundaries.
5. Run the Rust validation loop:
   - `cargo fmt --manifest-path src/rust/Cargo.toml --all -- --check`
   - `cargo test --manifest-path src/rust/Cargo.toml`
   - `cargo clippy --manifest-path src/rust/Cargo.toml --all-targets -- -D warnings`
6. If proto or gRPC behavior changed, also run:
   - `buf lint`
   - `dotnet build src/dotnet/LogRipper.slnx`
   - live smoke path through `logripper-server` and `LogRipper.Cli`
7. Summarize impact, validation, and any remaining contract or migration considerations.
