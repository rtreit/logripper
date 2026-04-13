# Data Model Architecture

## Overview

QsoRipper uses **Protocol Buffers (proto3)** as the canonical schema definition for all shared domain types. Proto files are the single source of truth — Rust structs and C# classes are generated from them, ensuring zero drift between the two language runtimes.

## Why Protocol Buffers?

| Concern | How protobuf addresses it |
|---|---|
| Cross-language type safety | Code generation for Rust (`prost`) and C# (`Grpc.Tools`) from one schema |
| Wire format | Binary protobuf over gRPC for inter-process communication |
| Forward compatibility | Proto3 ignores unknown fields — matches QRZ API's own forward-compat model |
| Schema evolution | Adding optional fields is non-breaking; `buf breaking` enforces rules |
| Performance | Binary serialization is fast and compact — no JSON parsing overhead on the hot path |

## Architecture Alignment

The proto-first approach directly supports these architecture principles:

- **Principle #6 (Normalize Data Immediately)**: QRZ XML/ADIF responses are parsed and mapped into proto domain types at the provider edge. Internal communication always uses normalized types.
- **Principle #8 (Consumer-Driven Interfaces)**: Proto messages are designed around what the UI needs (for example, `LookupResult` wraps state + data + latency), not QRZ's XML structure.
- **Stable core, volatile edges**: Proto domain types are the stable core. QRZ XML parsing, ADIF parsing, and HTTP concerns are edge adapters that produce proto types.

## Proto Layout Rules

QsoRipper treats the protobuf 1-1-1 guidance as an architectural rule, not a style preference:

- **One top-level entity per file by default**: messages, enums, and services each get their own `.proto` file.
- **Service declaration files contain only the service**: request, response, stream item, enum, and support payload messages live in separate files under `proto/services/`.
- **Every RPC gets unique request/response envelopes**: unary and streaming methods both use method-specific `XxxRequest` / `XxxResponse` messages.
- **Reusable business payloads stay separate from envelopes**: shared models such as `LookupResult`, `QsoRecord`, `SetupStatus`, and `ActiveStationContext` are nested inside envelopes rather than returned directly as RPC shapes.
- **Exceptions are rare and explicit**: if QsoRipper ever deviates from 1-1-1, that decision must be documented and justified in the schema review. RPC envelopes are not the place for exceptions.

## Directory Structure

```
proto/
├── domain/
│   ├── callsign_record.proto
│   ├── dxcc_entity.proto
│   ├── lookup_result.proto
│   ├── lookup_state.proto
│   ├── qso_record.proto
│   ├── station_profile.proto
│   ├── station_snapshot.proto
│   └── ... one top-level reusable domain type per file
└── services/
    ├── lookup_service.proto
    ├── lookup_request.proto
    ├── lookup_response.proto
    ├── stream_lookup_request.proto
    ├── stream_lookup_response.proto
    ├── log_qso_request.proto
    ├── log_qso_response.proto
    └── ... one top-level service envelope/support type per file
```

## Language Split

| Component | Language | Rationale |
|---|---|---|
| Core engine (log storage, lookup coordinator, cache, ADIF parser) | Rust | Performance-critical hot path |
| TUI | Rust (ratatui) | Same-process access to core engine |
| QRZ providers (XML lookup, logbook API) | Rust | Same process as coordinator for cancellation/dedup |
| gRPC server | Rust (tonic) | Exposes core engine to other processes |
| GUI | C# / .NET (Avalonia) | Rich cross-platform UI framework |
| Reporting / contest analytics | C# / .NET | Strong library ecosystem |
| gRPC client | C# (Grpc.Net.Client) | GUI calls into Rust core |

**Key rule:** The Rust process is the **engine** — it owns the log database, QRZ integration, caching, and lookup orchestration. The .NET process is the **rich client** — it consumes the gRPC API.

## Core Domain Types

### CallsignRecord (`callsign_record.proto`)

Normalized representation of a ham radio operator/station. Derived from QRZ XML lookup data (40+ fields) but owned by QsoRipper. Field groups:

- **Identity**: callsign, aliases, previous_call, dxcc_entity_id
- **Name**: first_name, last_name, nickname, formatted_name
- **Address**: attention, addr1, addr2, state, zip, country, country_code
- **Location**: latitude, longitude, grid_square, county, fips, geo_source
- **License**: license_class, effective_date, expiration_date, license_codes
- **Contact**: email, web_url, qsl_manager
- **QSL preferences**: eqsl, lotw, paper_qsl (tri-state enum)
- **Zone**: cq_zone, itu_zone, iota
- **Metadata**: qrz_serial, last_modified, bio_length, image_url, etc.

### QsoRecord (`qso_record.proto`)

The core QSO (contact) entity. Every logged contact is a QsoRecord.

- **Identity**: local_id (UUID assigned by QsoRipper), qrz_logid (from QRZ sync)
- **Core**: station_callsign, worked_callsign, utc_timestamp, utc_end_timestamp, band, mode, submode, frequency_khz
- **Signal**: rst_sent, rst_received (structured RstReport), tx_power
- **QSL**: sent/received status for card, LoTW, eQSL
- **Enrichment**: worked_operator_name, worked_grid, worked_country, worked_dxcc, worked_continent
- **Contest**: contest_id, serial_sent/received, exchange_sent/received
- **Propagation**: prop_mode, sat_name, sat_mode
- **Sync**: sync_status (local_only → synced → modified → conflict)
- **ADIF overflow**: extra_fields map preserves unrecognized ADIF fields for lossless round-trip

### LookupResult (`lookup_result.proto`)

Wraps the async state machine for callsign lookups:

```
Loading → Found | NotFound | Error | Stale | Cancelled
```

Includes: state enum, optional CallsignRecord, cache_hit flag, lookup_latency_ms.

### Supporting Enums

- **Band**: 2190m through submm (33 values, full ADIF 3.1.7 enumeration with frequency ranges)
- **Mode**: 45 modes matching the complete ADIF 3.1.7 Mode enumeration; submodes are stored as a string field
- **GeoSource**: user, geocode, grid, zip, state, dxcc, none (maps to QRZ geoloc values)
- **SyncStatus**: local_only, synced, modified, conflict
- **QslStatus**: no, yes, requested, queued, ignore (aligned with ADIF QSL Sent/Rcvd enums)
- **QslPreference**: unknown, yes, no (tri-state for QRZ's 0/1/blank)

## gRPC Services

### LookupService

The app-facing lookup interface from the architecture diagram:

```
TUI/GUI → LookupService → LookupCoordinator → CallsignProvider → QrzProvider
```

Key RPCs:
- `Lookup` — single request/response
- `StreamLookup` — server-streaming progressive updates (Loading → Stale → Found)
- `GetCachedCallsign` — L1 cache-only check
- `GetDxccEntity` — DXCC entity lookup
- `BatchLookup` — contest prefetch

Each RPC returns a unique service envelope such as `LookupResponse`, `StreamLookupResponse`, or `GetCachedCallsignResponse`. Shared payloads like `LookupResult` stay nested inside those envelopes so each RPC can evolve independently.

### LogbookService

QSO lifecycle management:

- `LogQso` / `UpdateQso` / `DeleteQso` — CRUD with optional immediate QRZ sync
- `ListQsos` — filtered/paginated query with server-streaming response
- `SyncWithQrz` — full or incremental sync, streams progress updates
- `ImportAdif` / `ExportAdif` — client-streaming import, server-streaming export

Logbook RPCs follow the same rule: `ListQsos` streams `ListQsosResponse` envelopes that carry `QsoRecord`, `ExportAdif` streams `ExportAdifResponse` envelopes that carry `AdifChunk`, and unary RPCs use method-specific envelopes even when the payload is a single shared domain type.

## ADIF as External Format

ADIF (Amateur Data Interchange Format) is used exclusively for:

1. **QRZ Logbook API** — INSERT/FETCH use ADIF-encoded QSO data
2. **File import/export** — standard `.adi` files from other logging programs
3. **Contest log submission** — Cabrillo/ADIF export

ADIF is **never** used for internal IPC. The Rust-side ADIF parser converts to/from proto QsoRecord at the edge.

### ADIF Round-Trip Strategy

QsoRecord includes an `extra_fields` map (`map<string, string>`) to preserve ADIF fields that don't have dedicated proto fields (e.g., satellite info, propagation conditions, application-defined fields). Core local-station ADIF fields now flow through `station_snapshot` instead of `extra_fields`. During import:

1. Recognized fields → mapped to dedicated QsoRecord fields
2. Unrecognized fields → stored in `extra_fields` (keyed by uppercase ADIF field name)
3. During export → dedicated fields are emitted first, then `extra_fields` are appended

This ensures no data loss when round-tripping ADIF files through QsoRipper.

See `docs/integrations/adif-specification.md` for the complete ADIF 3.1.7 reference including all 150+ QSO fields, data types, enumerations, and field-to-proto mapping table.

## Code Generation

### Tooling

- **buf** — schema linting (`buf lint`) and breaking change detection (`buf breaking`)
- **prost + tonic-build** — Rust struct/gRPC generation during Cargo builds
- **Grpc.Tools** — C# class/gRPC generation during MSBuild

### Build integration

```
# Lint proto files
buf lint

# Check for breaking changes against main branch
buf breaking --against '.git#branch=main'

# Regenerate Rust bindings
cargo build --manifest-path src/rust/Cargo.toml

# Regenerate C# bindings
dotnet build src/dotnet/QsoRipper.slnx
```

### Generated output locations

| Language | Output path | Notes |
|---|---|---|
| Rust | Cargo `OUT_DIR` under `src/rust/target/` | Generated at build time by `src/rust/qsoripper-core/build.rs`; not checked in |
| C# | MSBuild intermediate output under `src/dotnet/**/obj/` | Generated at build time by `Grpc.Tools`; not checked in |

## Adding a New Field

1. Add the field to the appropriate `.proto` file with the next available field number
2. Run `buf lint` to verify naming conventions
3. Run `buf breaking` to verify backward compatibility
4. Rebuild the Rust workspace and .NET consumers so generated bindings refresh
5. Update the provider adapter (e.g., QRZ XML parser) to populate the new field
6. Update the UI components that should display the field

**Important:** Never reuse or reassign proto field numbers. Deleted fields should be marked with `reserved`.

## Conventions

- **Field numbering**: Group related fields in ranges (identity: 1-9, name: 10-19, address: 20-29, etc.)
- **Field naming**: snake_case in proto files (auto-converted to PascalCase in C#, snake_case in Rust)
- **Optional fields**: Use `optional` keyword for fields that may not be present from the provider
- **Enums**: Prefer `_UNSPECIFIED = 0` when the schema has a neutral default; operational defaults may intentionally keep a domain-specific zero value
- **Timestamps**: Use `google.protobuf.Timestamp` for all date/time fields
- **C# namespace**: Set via `option csharp_namespace = "QsoRipper.Domain"` or `"QsoRipper.Services"`
- **Packages**: Keep the current `proto/domain` and `proto/services` layout with `qsoripper.domain` / `qsoripper.services` packages until the project deliberately introduces versioned external contracts
- **1-1-1 layout**: Default to one top-level message, enum, or service per `.proto` file
- **RPC message shapes**: Every RPC gets unique `XxxRequest` and `XxxResponse` envelopes; shared payloads are nested inside those envelopes rather than used as the top-level RPC contract
