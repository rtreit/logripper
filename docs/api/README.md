# LogRipper Engine API

This is the canonical entry point for client authors integrating with the LogRipper engine over gRPC.

LogRipper is an **engine-first** project. The engine exposes everything through a gRPC API backed by Protocol Buffer contracts. Any UX implementation — a TUI, desktop GUI, web frontend, CLI tool, or mobile app — is an independent gRPC consumer. Nothing about the engine privileges any particular client technology.

## Services

| Service | Purpose | Reference |
|---|---|---|
| **LookupService** | Callsign lookups — single, streaming, batch, cached, DXCC | [lookup-service.md](lookup-service.md) |
| **LogbookService** | QSO CRUD, QRZ logbook sync, ADIF import/export | [logbook-service.md](logbook-service.md) |

## Contract Source of Truth

All service and domain types are defined in `proto/`:

```
proto/
├── domain/
│   ├── callsign.proto   # CallsignRecord, DxccEntity, GeoSource, QslPreference
│   ├── qso.proto        # QsoRecord, Band, Mode, RstReport, SyncStatus, QslStatus
│   └── lookup.proto     # LookupResult, LookupState, LookupRequest, BatchLookup
└── services/
    ├── lookup_service.proto   # LookupService gRPC definitions
    └── logbook_service.proto  # LogbookService gRPC definitions
```

The `.proto` files are the durable reference source. Comments inside them document individual field and RPC semantics. The reference docs in this directory provide higher-level integration guidance and implementation-status tables on top of those definitions.

## Transport

The engine speaks native gRPC (HTTP/2 + binary protobuf). Default listen address is `http://127.0.0.1:50051`.

| Client type | Recommended transport | Notes |
|---|---|---|
| **Native desktop / TUI / CLI** | Native gRPC (HTTP/2) | Any gRPC client library works directly |
| **Browser / web** | gRPC-Web via proxy | Browsers cannot issue raw HTTP/2 gRPC frames — see [client-integration.md](client-integration.md#browser-and-web-clients) |

> **Browser clients** require an intermediate proxy or gateway (e.g., Envoy with the gRPC-Web filter, or a gRPC-Web-aware reverse proxy). Direct raw gRPC from a browser is not supported without this layer. See the integration guide for details.

## Implementation Status

Not all contract entries in the proto files are fully implemented in the current Rust server. The reference docs for each service include a status table marking each RPC as **implemented**, **partial**, or **planned** (unimplemented stub).

In general:

- `LookupService` callsign lookups (unary, streaming, cached) are implemented.
- `LogbookService` QSO CRUD, sync, and ADIF flows are contract-complete but currently return `UNIMPLEMENTED` from the server. `GetSyncStatus` returns placeholder zeroed values.

The proto contract is considered stable for additive changes. Client code generated from the proto files will continue to compile as new fields and RPCs are added. See [client-integration.md](client-integration.md#schema-evolution-and-compatibility) for field tolerance guidance.

## Quick Links

- [Client Integration Guide](client-integration.md) — generating stubs, connecting, browser transport
- [Workflow Examples](workflows.md) — request/response shapes for common flows
- [LookupService Reference](lookup-service.md)
- [LogbookService Reference](logbook-service.md)
- [Data Model Architecture](../architecture/data-model.md) — architecture-oriented context for domain types
