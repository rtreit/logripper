---
name: rust-api-and-idioms
description: >-
  Apply current Rust API design and idioms in LogRipper. Use when shaping Rust public APIs,
  choosing signatures, modeling errors, or checking whether older Rust-era advice still
  applies to current stable Rust.
---

# Skill: Rust API and idioms

## When to Use

- Designing new Rust modules or public functions
- Refactoring Rust APIs for clarity or ergonomics
- Reviewing error handling and ownership patterns
- Checking whether an old Rust pattern is still recommended on current stable Rust

## Design Rules

1. Prefer simple, explicit APIs over clever abstractions.
2. Model domain state with enums and structs rather than stringly flags.
3. Use typed errors for meaningful failure surfaces; prefer `thiserror` over ad hoc strings.
4. Avoid unnecessary clones and allocations on hot paths.
5. Do not keep outdated workarounds just because they were needed in older Rust versions.
6. Prefer current idioms documented by the Rust API Guidelines and Clippy.

## Review Checklist

- Is this API readable from the caller side?
- Does ownership/borrowing match actual lifetime needs?
- Is a clone/allocation happening on a hot path without justification?
- Is a lint suppression overly broad where `#[expect]` or a redesign would be better?
- Does the implementation rely on an old-edition limitation that should be rechecked?

## Current References

- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Clippy docs: https://rust-lang.github.io/rust-clippy/master/
- Rust Reference: https://doc.rust-lang.org/reference/
- Rust 2024 Edition Guide: https://doc.rust-lang.org/edition-guide/rust-2024/index.html
