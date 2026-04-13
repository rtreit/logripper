# Proto Contract Instructions

## Purpose

These instructions govern schema and gRPC contract work in `proto/` and any Rust or .NET code generated from those files.

## Source of Truth

- `proto/` is the only source of truth for shared service and domain contracts.
- Never hand-edit generated Rust or C# code to "fix" a schema issue. Change the `.proto` file and regenerate through the existing build flow.
- ADIF is an external interchange format only. Internal IPC and cross-process contracts stay on protobuf + gRPC.

## 1-1-1 and Envelope Rules

- Follow protobuf 1-1-1 by default: one top-level message, enum, or service per `.proto` file.
- Service declaration files contain only the `service`; request, response, stream item, enum, and support payload types live in their own files.
- Every RPC gets a unique `XxxRequest` and `XxxResponse` envelope, including streaming RPCs.
- Transport-only RPC messages belong in `proto/services/`, not `proto/domain/`.
- If multiple RPCs need the same business payload, extract a separate reusable message and wrap it from each response; do not reuse one RPC response envelope as another RPC's nested model.
- Exceptions are rare, must be explicit and documented, and never justify skipping per-RPC envelopes.

## Compatibility Rules

- Prefer additive changes over breaking changes.
- Preserve existing field numbers and enum numeric values.
- Keep zero-value enum members as the unspecified/default state.
- When introducing new fields, think through both Rust (`prost` / `tonic`) and C# (`Grpc.Tools`) consumers.
- Keep service behavior aligned across the Rust server, .NET CLI, and Debug Workbench.

## Rust/.NET Contract Rules

- Rust server traits and generated message types come from `src/rust/qsoripper-core/build.rs` generation.
- .NET clients consume the same contracts through generated gRPC client code under `src/dotnet/`.
- If a proto change affects logbook or lookup semantics, update both Rust server-side handling and the .NET debugging/client surfaces in the same change when practical.
- Shared contract visibility in `src/dotnet/QsoRipper.DebugHost` is part of the .NET client surface. Keep `/protobuf-lab` able to inspect generated message shapes for shared proto messages, and prefer automatic discovery over hand-maintained message lists.
- When a new shared message needs richer example data than the default constructor provides, extend the custom builder path in `src/dotnet/QsoRipper.DebugHost/Services/SampleProtoFactory.cs` instead of adding another manual UI registry.

## Validation

- For schema or service changes, run:
  - `buf lint`
  - `cargo test --manifest-path src/rust/Cargo.toml`
  - `dotnet build src/dotnet/QsoRipper.slnx`
- For changes that affect generated .NET message surfaces or Debug Host inspection workflows, also keep the Debug Host sample-catalog tests green so new contracts remain visible in `/protobuf-lab`.
- If the contract change introduces or changes runtime behavior in Rust or .NET, add coverage for the affected paths and rerun the relevant local quality gates before pushing:
  - Rust: `cargo llvm-cov --manifest-path src/rust/Cargo.toml --all --lcov --output-path rust-coverage.lcov`
  - .NET: `dotnet test src/dotnet/QsoRipper.slnx --collect:"XPlat Code Coverage" --settings src/dotnet/CodeCoverage.runsettings --results-directory coverage`
- Do not push proto/service changes that you already know will fail the corresponding quality or coverage gates in CI.
- If the change affects runtime behavior, also smoke-test the live server with:
  - `cargo run --manifest-path src/rust/Cargo.toml -p qsoripper-server`
  - `dotnet run --project src/dotnet/QsoRipper.Cli -- status`

## Current References

- Protocol Buffers language guide: https://protobuf.dev/programming-guides/proto3/
- Protocol Buffers best practices (1-1-1): https://protobuf.dev/best-practices/1-1-1/
- Buf lint/breaking overview: https://buf.build/docs/
- tonic docs: https://docs.rs/tonic/latest/tonic/
- prost docs: https://docs.rs/prost/latest/prost/
