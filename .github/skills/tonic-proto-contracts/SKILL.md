---
name: tonic-proto-contracts
description: >-
  Change or review protobuf and tonic contracts for QsoRipper. Use when editing proto/,
  gRPC services, generated Rust bindings, or .NET clients that depend on shared lookup and
  logbook contracts. Enforce protobuf 1-1-1, unique per-RPC envelopes, and clean domain vs
  service message boundaries.
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
4. Follow protobuf 1-1-1 by default: one top-level message, enum, or service per `.proto` file.
5. Every RPC must use unique `XxxRequest` and `XxxResponse` envelopes; streamed items get unique response envelopes too.
6. Keep transport-only RPC messages in `proto/services/`; reusable business entities belong in `proto/domain` or dedicated service support messages.
7. If multiple RPCs need the same payload, extract a separate message and wrap it instead of reusing an RPC response envelope as nested data.
8. Prefer additive schema changes over breaking changes.
9. ADIF is not an internal IPC format; protobuf remains the internal contract.
10. When adding or changing RPCs, services, or contract behavior, update `docs/architecture/engine-specification.md` in the same change.

## Repo-Specific Contract Surfaces

- `proto/domain/*.proto`
- `proto/services/*.proto`
- `src/rust/qsoripper-core/build.rs`
- `src/rust/qsoripper-server/`
- `src/dotnet/QsoRipper.Engine.DotNet/`
- `src/dotnet/QsoRipper.Cli/`
- `src/dotnet/QsoRipper.DebugHost/`
- `docs/architecture/engine-specification.md`

## Validation

```powershell
buf lint
cargo test --manifest-path src\rust\Cargo.toml
dotnet build src\dotnet\QsoRipper.slnx
```

Runtime smoke path:

```powershell
cargo run --manifest-path src\rust\Cargo.toml -p qsoripper-server
dotnet run --project src\dotnet\QsoRipper.Cli -- status
```

## Current References

- Protocol Buffers: https://protobuf.dev/programming-guides/proto3/
- Protocol Buffers best practices (1-1-1): https://protobuf.dev/best-practices/1-1-1/
- Buf docs: https://buf.build/docs/
- tonic docs: https://docs.rs/tonic/latest/tonic/
- prost docs: https://docs.rs/prost/latest/prost/
