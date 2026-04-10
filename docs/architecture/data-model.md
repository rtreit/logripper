# Data Model Architecture

## Overview

LogRipper uses **Protocol Buffers (proto3)** as the canonical schema definition for all shared domain types. Proto files are the single source of truth — Rust structs and C# classes are generated from them, ensuring zero drift between the two language runtimes.

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
- **Principle #8 (Consumer-Driven Interfaces)**: Proto messages are designed around what the UI needs (e.g., `LookupResult` wraps state + data + latency), not QRZ's XML structure.
- **Stable core, volatile edges**: Proto domain types are the stable core. QRZ XML parsing, ADIF parsing, and HTTP concerns are edge adapters that produce proto types.

## Directory Structure

```
proto/
├── domain/
│   ├── callsign.proto       # CallsignRecord, DxccEntity, GeoSource, QslPreference
│   ├── qso.proto            # QsoRecord, Band, Mode, RstReport, SyncStatus, QslStatus
│   └── lookup.proto         # LookupResult, LookupState, LookupRequest, BatchLookup
└── services/
    ├── lookup_service.proto  # LookupService gRPC (Lookup, StreamLookup, BatchLookup, DXCC)
    └── logbook_service.proto # LogbookService gRPC (CRUD, sync, ADIF import/export)
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

### CallsignRecord (`callsign.proto`)

Normalized representation of a ham radio operator/station. Derived from QRZ XML lookup data (40+ fields) but owned by LogRipper. Field groups:

- **Identity**: callsign, aliases, previous_call, dxcc_entity_id
- **Name**: first_name, last_name, nickname, formatted_name
- **Address**: attention, addr1, addr2, state, zip, country, country_code
- **Location**: latitude, longitude, grid_square, county, fips, geo_source
- **License**: license_class, effective_date, expiration_date, license_codes
- **Contact**: email, web_url, qsl_manager
- **QSL preferences**: eqsl, lotw, paper_qsl (tri-state enum)
- **Zone**: cq_zone, itu_zone, iota
- **Metadata**: qrz_serial, last_modified, bio_length, image_url, etc.

### QsoRecord (`qso.proto`)

The core QSO (contact) entity. Every logged contact is a QsoRecord.

- **Identity**: local_id (UUID assigned by LogRipper), qrz_logid (from QRZ sync)
- **Core**: station_callsign, worked_callsign, utc_timestamp, band, mode, submode, frequency_khz
- **Signal**: rst_sent, rst_received (structured RstReport), tx_power
- **QSL**: sent/received status for card, LoTW, eQSL
- **Enrichment**: worked_operator_name, worked_grid, worked_country, worked_dxcc, worked_continent
- **Contest**: contest_id, serial_sent/received, exchange_sent/received
- **Propagation**: prop_mode, sat_name, sat_mode
- **Sync**: sync_status (local_only → synced → modified → conflict)
- **ADIF overflow**: extra_fields map preserves unrecognized ADIF fields for lossless round-trip

### LookupResult (`lookup.proto`)

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

### LogbookService

QSO lifecycle management:

- `LogQso` / `UpdateQso` / `DeleteQso` — CRUD with optional immediate QRZ sync
- `ListQsos` — filtered/paginated query with server-streaming response
- `SyncWithQrz` — full or incremental sync, streams progress updates
- `ImportAdif` / `ExportAdif` — client-streaming import, server-streaming export

## ADIF as External Format

ADIF (Amateur Data Interchange Format) is used exclusively for:

1. **QRZ Logbook API** — INSERT/FETCH use ADIF-encoded QSO data
2. **File import/export** — standard `.adi` files from other logging programs
3. **Contest log submission** — Cabrillo/ADIF export

ADIF is **never** used for internal IPC. The Rust-side ADIF parser converts to/from proto QsoRecord at the edge.

### ADIF Round-Trip Strategy

QsoRecord includes an `extra_fields` map (`map<string, string>`) to preserve ADIF fields that don't have dedicated proto fields (e.g., MY_ station fields, satellite info, propagation conditions). During import:

1. Recognized fields → mapped to dedicated QsoRecord fields
2. Unrecognized fields → stored in `extra_fields` (keyed by uppercase ADIF field name)
3. During export → dedicated fields are emitted first, then `extra_fields` are appended

This ensures no data loss when round-tripping ADIF files through LogRipper.

See `docs/integrations/adif-specification.md` for the complete ADIF 3.1.7 reference including all 150+ QSO fields, data types, enumerations, and field-to-proto mapping table.

## Code Generation

### Tooling

- **buf** — schema linting (`buf lint`) and breaking change detection (`buf breaking`)
- **prost + tonic-build** — Rust struct/gRPC generation
- **Grpc.Tools** — C# class/gRPC generation

### Build integration

```bash
# Lint proto files
buf lint

# Check for breaking changes against main branch
buf breaking --against '.git#branch=main'

# Generate code (configured in buf.gen.yaml)
buf generate
```

### Generated output locations

| Language | Output path | Notes |
|---|---|---|
| Rust | `src/generated/rust/` | Checked into repo or generated at build time via `build.rs` |
| C# | `src/generated/csharp/` | Generated at build time by `Grpc.Tools` NuGet package |

## Adding a New Field

1. Add the field to the appropriate `.proto` file with the next available field number
2. Run `buf lint` to verify naming conventions
3. Run `buf breaking` to verify backward compatibility
4. Regenerate code: `buf generate`
5. Update the provider adapter (e.g., QRZ XML parser) to populate the new field
6. Update the UI components that should display the field

**Important:** Never reuse or reassign proto field numbers. Deleted fields should be marked with `reserved`.

## Conventions

- **Field numbering**: Group related fields in ranges (identity: 1-9, name: 10-19, address: 20-29, etc.)
- **Field naming**: snake_case in proto files (auto-converted to PascalCase in C#, snake_case in Rust)
- **Optional fields**: Use `optional` keyword for fields that may not be present from the provider
- **Enums**: Prefer `_UNSPECIFIED = 0` when the schema has a neutral default; operational defaults may intentionally keep a domain-specific zero value
- **Timestamps**: Use `google.protobuf.Timestamp` for all date/time fields
- **C# namespace**: Set via `option csharp_namespace = "LogRipper.Domain"` or `"LogRipper.Services"`
- **Packages**: Keep the current `proto/domain` and `proto/services` layout with `logripper.domain` / `logripper.services` packages until the project deliberately introduces versioned external contracts
- **RPC message shapes**: Reuse shared domain messages like `LookupResult` and `QsoRecord` directly when they are already the app-facing contract; add method-specific wrapper messages only when they carry distinct semantics
