---
name: tonic-proto-contracts
description: >-
  Change or review protobuf and tonic contracts for LogRipper. Use when editing proto/,
  gRPC services, generated Rust bindings, or .NET clients that depend on shared lookup and
  logbook contracts.
---

# Skill: tonic/proto contracts

## When to Use

- Editing files under `proto/`
- Adding or changing `LookupService` or `LogbookService`
- Reviewing Rust/.NET interoperability risks
- Updating generated client/server behavior tied to `prost`, `tonic`, or `Grpc.Tools`

## Key Rules

1. `proto/` is the single source of truth for shared contracts.
2. Preserve field numbers and enum values.
3. Keep zero-value enums as unspecified/default states.
4. Prefer additive schema changes over breaking changes.
5. ADIF is not an internal IPC format; protobuf remains the internal contract.

## Repo-Specific Contract Surfaces

- `proto/domain/*.proto`
- `proto/services/*.proto`
- `src/rust/logripper-core/build.rs`
- `src/rust/logripper-server/`
- `src/dotnet/LogRipper.Cli/`
- `src/dotnet/LogRipper.DebugHost/`

## Validation

```powershell
buf lint
cargo test --manifest-path src\rust\Cargo.toml
dotnet build src\dotnet\LogRipper.slnx
```

Runtime smoke path:

```powershell
cargo run --manifest-path src\rust\Cargo.toml -p logripper-server
dotnet run --project src\dotnet\LogRipper.Cli -- status
```

## Current References

- Protocol Buffers: https://protobuf.dev/programming-guides/proto3/
- Buf docs: https://buf.build/docs/
- tonic docs: https://docs.rs/tonic/latest/tonic/
- prost docs: https://docs.rs/prost/latest/prost/
