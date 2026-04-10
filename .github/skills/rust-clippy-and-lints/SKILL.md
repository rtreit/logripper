---
name: rust-clippy-and-lints
description: >-
  Enforce current Rust lint, formatting, and dependency-audit practices in LogRipper. Use
  when enabling or interpreting clippy lints, deciding on lint suppressions, or aligning
  Rust code with rustfmt, current stable lints, and modern supply-chain checks.
---

# Skill: Rust Clippy and lints

## When to Use

- Running or fixing `cargo clippy`
- Running `cargo fmt --check`
- Working on `src/rust/rustfmt.toml`, `src/rust/deny.toml`, or workspace lint settings in `src/rust/Cargo.toml`
- Adding dependency-audit checks like `cargo deny` or planning a manual `cargo audit` pass
- Deciding whether to suppress or address a lint
- Cleaning up Rust style and consistency issues
- Reviewing unsafe code and modern lint expectations

## Key Rules

1. Use `rustfmt` defaults unless the repository has a strong reason to diverge.
2. Treat Clippy as a best-practice signal, not just noise to suppress.
3. Prefer `#[expect(...)]` for narrow, intentional exceptions.
4. Avoid crate-wide or module-wide `allow` attributes unless there is a strong, documented reason.
5. Recheck unsafe-related guidance against current stable Rust and edition guidance.
6. Use `cargo deny` for routine dependency risk checks, and reserve `cargo audit` for occasional manual review instead of every CI run.

## Standard Commands

```powershell
cargo fmt --manifest-path src\rust\Cargo.toml --all -- --check
cargo clippy --manifest-path src\rust\Cargo.toml --all-targets -- -D warnings
Push-Location src\rust
cargo deny check --config deny.toml
# Optional manual review
cargo audit
Pop-Location
```

## Current References

- Clippy docs: https://rust-lang.github.io/rust-clippy/master/
- Rust Style Guide: https://doc.rust-lang.org/style-guide/
- Rust 2024 Edition Guide: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
- RustSec: https://rustsec.org/
