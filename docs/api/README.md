# LogRipper Engine API

This is the canonical entry point for client authors integrating with the LogRipper engine over gRPC.

LogRipper is an **engine-first** project. The engine exposes everything through a gRPC API backed by Protocol Buffer contracts. Any UX implementation — a TUI, desktop GUI, web frontend, CLI tool, or mobile app — is an independent gRPC consumer. Nothing about the engine privileges any particular client technology.

## Services

| Service | Purpose | Reference |
|---|---|---|
| **SetupService** | First-run setup, persisted config status, bootstrap storage/station defaults | [setup-service.md](setup-service.md) |
| **StationProfileService** | Persisted station profile CRUD, active selection, bounded session override state | [station-profile-service.md](station-profile-service.md) |
| **LookupService** | Callsign lookups — single, streaming, batch, cached, DXCC | [lookup-service.md](lookup-service.md) |
| **LogbookService** | QSO CRUD, QRZ logbook sync, ADIF import/export | [logbook-service.md](logbook-service.md) |
| **DeveloperControlService** | Developer-only runtime config overrides and diagnostics | [`proto/services/developer_control_service.proto`](../../proto/services/developer_control_service.proto) |

## Contract Source of Truth

All service and domain types are defined in `proto/`. LogRipper treats protobuf 1-1-1 and per-RPC envelopes as an architectural rule:

```
proto/
├── domain/
│   ├── callsign_record.proto
│   ├── dxcc_entity.proto
│   ├── lookup_result.proto
│   ├── qso_record.proto
│   ├── station_profile.proto
│   └── ... one reusable domain type per file
└── services/
    ├── setup_service.proto                # service declaration only
    ├── station_profile_service.proto      # service declaration only
    ├── lookup_service.proto               # service declaration only
    ├── logbook_service.proto              # service declaration only
    ├── developer_control_service.proto    # service declaration only
    ├── lookup_request.proto               # per-RPC envelope
    ├── lookup_response.proto              # per-RPC envelope
    └── ... one envelope/support type per file
```

Rules of thumb:

- Every RPC uses a unique `XxxRequest` and `XxxResponse` envelope, including streaming RPCs.
- Shared business payloads live in dedicated domain or service support messages and are nested inside envelopes.
- Service files contain only the `service`; request/response/support messages live beside them as their own files.

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
- `SetupService` can report persisted setup status and save the initial storage/station bootstrap config.
- `LogbookService` local QSO CRUD, ADIF import/export, and local sync-status reporting are implemented against the active storage backend. QRZ sync remains planned, and `GetSyncStatus` still reports QRZ fields as zero/absent until remote sync lands.

The proto contract is considered stable for additive changes. Client code generated from the proto files will continue to compile as new fields and RPCs are added. See [client-integration.md](client-integration.md#schema-evolution-and-compatibility) for field tolerance guidance.

## Quick Links

- [Client Integration Guide](client-integration.md) — generating stubs, connecting, browser transport
- [Workflow Examples](workflows.md) — request/response shapes for common flows
- [SetupService Reference](setup-service.md)
- [StationProfileService Reference](station-profile-service.md)
- [LookupService Reference](lookup-service.md)
- [LogbookService Reference](logbook-service.md)
- [Data Model Architecture](../architecture/data-model.md) — architecture-oriented context for domain types
