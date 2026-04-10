# Proto Contract Instructions

## Purpose

These instructions govern schema and gRPC contract work in `proto/` and any Rust or .NET code generated from those files.

## Source of Truth

- `proto/` is the only source of truth for shared service and domain contracts.
- Never hand-edit generated Rust or C# code to "fix" a schema issue. Change the `.proto` file and regenerate through the existing build flow.
- ADIF is an external interchange format only. Internal IPC and cross-process contracts stay on protobuf + gRPC.

## Compatibility Rules

- Prefer additive changes over breaking changes.
- Preserve existing field numbers and enum numeric values.
- Keep zero-value enum members as the unspecified/default state.
- When introducing new fields, think through both Rust (`prost` / `tonic`) and C# (`Grpc.Tools`) consumers.
- Keep service behavior aligned across the Rust server, .NET CLI, and Debug Workbench.

## Rust/.NET Contract Rules

- Rust server traits and generated message types come from `src/rust/logripper-core/build.rs` generation.
- .NET clients consume the same contracts through generated gRPC client code under `src/dotnet/`.
- If a proto change affects logbook or lookup semantics, update both Rust server-side handling and the .NET debugging/client surfaces in the same change when practical.

## Validation

- For schema or service changes, run:
  - `buf lint`
  - `cargo test --manifest-path src/rust/Cargo.toml`
  - `dotnet build src/dotnet/LogRipper.slnx`
- If the contract change introduces or changes runtime behavior in Rust or .NET, add coverage for the affected paths and rerun the relevant local quality gates before pushing:
  - Rust: `cargo llvm-cov --manifest-path src/rust/Cargo.toml --all --lcov --output-path rust-coverage.lcov`
  - .NET: `dotnet test src/dotnet/LogRipper.slnx --collect:"XPlat Code Coverage" --settings src/dotnet/CodeCoverage.runsettings --results-directory coverage`
- Do not push proto/service changes that you already know will fail the corresponding quality or coverage gates in CI.
- If the change affects runtime behavior, also smoke-test the live server with:
  - `cargo run --manifest-path src/rust/Cargo.toml -p logripper-server`
  - `dotnet run --project src/dotnet/LogRipper.Cli -- status`

## Current References

- Protocol Buffers language guide: https://protobuf.dev/programming-guides/proto3/
- Buf lint/breaking overview: https://buf.build/docs/
- tonic docs: https://docs.rs/tonic/latest/tonic/
- prost docs: https://docs.rs/prost/latest/prost/
