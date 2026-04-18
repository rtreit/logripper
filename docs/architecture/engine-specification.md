# QsoRipper Engine Specification

> **Version 1.0** — The authoritative contract for implementing a QsoRipper engine in any language.
>
> This is a living document. When proto files, services, or behavioral contracts change, update this specification in the same change.

A QsoRipper engine is the core runtime that owns QSO logging, callsign lookup, rig control, space weather, station profiles, and external sync. Engines expose a gRPC API over HTTP/2 that any client — TUI, GUI, CLI, or web — can consume. The architecture is explicitly multi-engine: any conformant implementation, regardless of language, can serve as the engine behind any QsoRipper client.

This document is self-contained. A developer should be able to implement a fully conformant engine using only this specification and the `.proto` files under `proto/`.

---

## 1. Overview

QsoRipper is a high-performance ham radio logging system. Its architecture separates the **engine** (the server process that owns all data, integration, and business logic) from **clients** (TUI, GUI, CLI, DebugHost) that consume the engine over gRPC.

Key architectural properties:

- **Protocol Buffers are the single source of truth** for all shared types and service contracts. The `.proto` files under `proto/` define every message, enum, and RPC.
- **Engines are interchangeable.** Any implementation that passes the conformance harness is a valid engine.
- **Clients never own business logic.** They render state, capture input, and call RPCs.
- **ADIF is an edge concern.** Internal IPC uses protobuf exclusively. ADIF is only for external file interchange and QRZ API communication.
- **Offline-first.** Local logging must work without any network connectivity. External integrations degrade gracefully.

---

## 2. Architecture

### 2.1 Engine Role

The engine is a long-running server process responsible for:

| Responsibility | Description |
|---|---|
| QSO storage | Persistent CRUD for QSO records via a pluggable storage backend |
| Callsign lookup | QRZ XML lookups with caching, deduplication, and DXCC enrichment |
| QRZ logbook sync | Bidirectional synchronization with the QRZ logbook API |
| Rig control | Polling a rigctld daemon for frequency and mode |
| Space weather | Fetching and caching NOAA space weather indices |
| Station profiles | Managing station identity and per-session overrides |
| Setup/bootstrap | First-run wizard state, credential validation, and configuration persistence |
| Runtime config | Live developer-facing configuration overrides |

The engine does **not** own any UI rendering, keyboard handling, or display logic.

### 2.2 Client-Engine Separation

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│     TUI     │  │     GUI     │  │  DebugHost  │
│   (Rust)    │  │  (Avalonia) │  │   (Blazor)  │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       └────────┬───────┴────────┬───────┘
                │   gRPC/HTTP2   │
          ┌─────┴────────────────┴─────┐
          │         Engine             │
          │   (Rust or .NET or ...)    │
          └────────────────────────────┘
```

Clients connect to the engine via a single gRPC endpoint (default `http://[::1]:50051`). Clients may also connect through a gRPC-Web proxy for browser-based surfaces.

### 2.3 Protocol Buffers as Contract Core

All shared types live under `proto/`:

| Directory | Contents |
|---|---|
| `proto/domain/` | Domain model messages and enums (QsoRecord, CallsignRecord, Band, Mode, etc.) |
| `proto/services/` | Service definitions, RPC envelopes, and service-layer support types |

Engines generate language-specific bindings from these files. In Rust, `prost` and `tonic` generate types at build time. In C#, `Grpc.Tools` generates at build time. Never hand-write types that should come from proto generation.

The 1-1-1 rule applies: one top-level message, enum, or service per `.proto` file. Every RPC uses unique `XxxRequest`/`XxxResponse` envelopes. See `docs/architecture/data-model.md` for the full proto conventions.

### 2.4 Transport: gRPC over HTTP/2

- Native gRPC clients (CLI, TUI, GUI) connect directly over HTTP/2.
- Browser clients (DebugHost) connect through a gRPC-Web proxy that translates between gRPC-Web and native gRPC.
- The engine listens on a configurable address (default `[::1]:50051`, controlled by `QSORIPPER_SERVER_ADDR`).
- TLS is not required for local development. Production deployments should use TLS or a reverse proxy.

---

## 3. Required gRPC Services

An engine must implement all services in this section except where marked **optional**. Each subsection documents every RPC with its exact types, streaming mode, expected behavior, and error semantics.

For generated protobuf runtimes, absent optional scalar fields must be **omitted**, not assigned `null`. A successful handler must never fail while materializing a response just because an optional string/error field is not present.

### 3.1 EngineService

**Proto file:** `proto/services/engine_service.proto`

A stable handshake endpoint that identifies the engine implementation. Clients use this to verify connectivity and discover engine capabilities.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `GetEngineInfo` | `GetEngineInfoRequest` | `GetEngineInfoResponse` | Unary |

#### GetEngineInfo

Returns metadata about the running engine.

**Behavior:**
- Must always succeed if the engine is running.
- Returns the engine's name (e.g., `"qsoripper-server"`), version string, implementation language (e.g., `"rust"`, `"csharp"`), and a list of supported capability flags.
- The response must include the storage backend currently in use (see `StorageBackend` enum).

**Error semantics:**
- This RPC should never fail under normal operation.
- `UNAVAILABLE` — engine is shutting down.

### 3.2 LogbookService

**Proto file:** `proto/services/logbook_service.proto`

The primary QSO CRUD and sync surface. This is the most critical service in the engine.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `LogQso` | `LogQsoRequest` | `LogQsoResponse` | Unary |
| `UpdateQso` | `UpdateQsoRequest` | `UpdateQsoResponse` | Unary |
| `DeleteQso` | `DeleteQsoRequest` | `DeleteQsoResponse` | Unary |
| `GetQso` | `GetQsoRequest` | `GetQsoResponse` | Unary |
| `ListQsos` | `ListQsosRequest` | `stream ListQsosResponse` | Server-streaming |
| `SyncWithQrz` | `SyncWithQrzRequest` | `stream SyncWithQrzResponse` | Server-streaming |
| `GetSyncStatus` | `GetSyncStatusRequest` | `GetSyncStatusResponse` | Unary |
| `ImportAdif` | `stream ImportAdifRequest` | `ImportAdifResponse` | Client-streaming |
| `ExportAdif` | `ExportAdifRequest` | `stream ExportAdifResponse` | Server-streaming |

#### LogQso

Creates a new QSO record in the local logbook.

**Behavior:**
1. Generate a new `local_id` (UUID v4).
2. Normalize the `worked_callsign` (trim whitespace, convert to uppercase).
3. Validate required fields: `worked_callsign`, `band`, `mode`, `utc_timestamp` must be present and non-default.
4. Stamp `station_callsign` from the active station profile.
5. Capture a `StationSnapshot` from the active station context and attach it to the QSO.
6. Set `created_at` and `updated_at` to the current UTC time.
7. Set `sync_status` to `SYNC_STATUS_NOT_SYNCED`.
8. Persist the record via the storage backend.
9. Return the persisted `QsoRecord` in the response.

**Error semantics:**
- `INVALID_ARGUMENT` — missing or invalid required fields.
- `FAILED_PRECONDITION` — no active station profile set (station context unavailable).
- `INTERNAL` — storage write failure.

#### UpdateQso

Updates an existing QSO record by `local_id`.

**Behavior:**
1. Look up the existing record by `local_id`.
2. Apply provided field updates. Fields not included in the request are not modified.
3. Set `updated_at` to the current UTC time.
4. If the QSO was previously synced, set `sync_status` to `SYNC_STATUS_MODIFIED`.
5. Persist the updated record.
6. Return the updated `QsoRecord`.

**Error semantics:**
- `NOT_FOUND` — no QSO with the given `local_id`.
- `INVALID_ARGUMENT` — invalid field values.
- `INTERNAL` — storage write failure.

#### DeleteQso

Deletes a QSO record by `local_id`.

**Behavior:**
1. Look up the existing record by `local_id`.
2. Remove the record from storage.
3. Return success with the deleted `local_id`.

**Error semantics:**
- `NOT_FOUND` — no QSO with the given `local_id`.
- `INTERNAL` — storage delete failure.

#### GetQso

Retrieves a single QSO record by `local_id`.

**Behavior:**
- Return the full `QsoRecord` if found.

**Error semantics:**
- `NOT_FOUND` — no QSO with the given `local_id`.

#### ListQsos

Streams QSO records matching optional filter criteria.

**Behavior:**
- Apply filters from the request: time range (`after`/`before`), `callsign_filter`, `band_filter`, `mode_filter`, `contest_id`, `limit`, `offset`.
- Sort by `QsoSortOrder` (default: newest first).
- Stream one `ListQsosResponse` per matching QSO record.
- An empty logbook produces zero stream messages (not an error).

**Error semantics:**
- `INVALID_ARGUMENT` — malformed filter values.

#### SyncWithQrz

Initiates a bidirectional sync with the QRZ logbook API.

**Behavior:**

The sync follows a three-phase lifecycle:

1. **Download phase** — Fetch all QSOs from the QRZ logbook API via ADIF. Parse the ADIF response. For each remote QSO, attempt to match it against local records using fuzzy matching on callsign + timestamp + band + mode. Filter out ghost/duplicate records. Insert new remote-only records and update local records that have newer remote data (per the configured `ConflictPolicy`).

2. **Upload phase** — Find all local QSOs with `sync_status` of `SYNC_STATUS_NOT_SYNCED` or `SYNC_STATUS_MODIFIED`. For each, serialize to ADIF and upload via the QRZ logbook API. On success, update `sync_status` to `SYNC_STATUS_SYNCED` and record the `qrz_logid` returned by QRZ.

3. **Metadata phase** — Update the `sync_metadata` record with the current QRZ QSO count, last sync timestamp, and logbook owner callsign.

Stream progress messages throughout all phases so clients can display real-time sync state.

**Error semantics:**
- `FAILED_PRECONDITION` — QRZ logbook credentials not configured.
- `UNAVAILABLE` — QRZ API unreachable.
- `INTERNAL` — storage or parsing failure.
- Partial failures during sync should not abort the entire operation. Report per-QSO errors in the stream and continue.

#### GetSyncStatus

Returns the current sync metadata state.

**Behavior:**
- Return the current `sync_metadata` values: QRZ QSO count, last sync timestamp, logbook owner callsign.
- If no sync has ever occurred, return zero counts and no timestamp.

**Error semantics:**
- `INTERNAL` — storage read failure.

#### ImportAdif

Imports QSO records from a client-streamed ADIF payload.

**Behavior:**
1. Receive `ImportAdifRequest` messages, each containing an `AdifChunk` (a fragment of ADIF text).
2. Concatenate all chunks into a complete ADIF document.
3. Parse the ADIF document into individual QSO records.
4. For each parsed QSO, generate a `local_id`, normalize fields, and insert into storage.
5. Return a summary: total records parsed, records imported, records skipped (duplicates or validation failures), and any error messages.

**Duplicate handling:** The engine should detect duplicates by matching on callsign + UTC timestamp + band + mode and skip them rather than creating duplicate entries.

**Error semantics:**
- `INVALID_ARGUMENT` — ADIF content is malformed or unparseable.
- `INTERNAL` — storage write failure.

#### ExportAdif

Streams the logbook as an ADIF document.

**Behavior:**
1. Query all QSO records (optionally filtered by the request parameters).
2. Serialize each QSO to ADIF format.
3. Stream `ExportAdifResponse` messages, each containing an `AdifChunk`.
4. The first chunk should contain the ADIF header. Subsequent chunks contain QSO records.
5. Preserve `extra_fields` from imported QSOs for lossless round-trip.

**Error semantics:**
- `INTERNAL` — storage read or serialization failure.

### 3.3 LookupService

**Proto file:** `proto/services/lookup_service.proto`

Callsign lookup and DXCC enrichment.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `Lookup` | `LookupRequest` | `LookupResponse` | Unary |
| `StreamLookup` | `StreamLookupRequest` | `stream StreamLookupResponse` | Server-streaming |
| `GetCachedCallsign` | `GetCachedCallsignRequest` | `GetCachedCallsignResponse` | Unary |
| `GetDxccEntity` | `GetDxccEntityRequest` | `GetDxccEntityResponse` | Unary |
| `BatchLookup` | `BatchLookupRequest` | `BatchLookupResponse` | Unary |

#### Lookup

Performs a single callsign lookup.

**Behavior:**
1. Check the local lookup cache first. If a fresh, non-expired result exists, return it immediately with `cache_hit = true`.
2. If no cache hit, query the QRZ XML API.
3. Normalize the QRZ response into a `CallsignRecord`.
4. Cache the result in the `lookup_snapshots` store with an expiry timestamp.
5. Enrich with DXCC entity data if available.
6. Return a `LookupResult` containing the `CallsignRecord`, lookup state, latency, and cache hit status.

**Slash-call fallback:** If the callsign contains a `/` modifier (e.g., `W1AW/7`), and the full lookup fails, strip the modifier and retry with the base callsign. Populate `base_callsign`, `modifier_text`, and `modifier_kind` on the result.

**In-flight deduplication:** If a lookup for the same callsign is already in progress, coalesce the request rather than firing a duplicate QRZ query.

**Error semantics:**
- `NOT_FOUND` — callsign not found in QRZ (this is a valid result state, not a gRPC error; return `LookupState.LOOKUP_STATE_NOT_FOUND`).
- `UNAVAILABLE` — QRZ API unreachable (return `LookupState.LOOKUP_STATE_ERROR` in the result).
- `FAILED_PRECONDITION` — QRZ credentials not configured (return `LookupState.LOOKUP_STATE_ERROR`).

#### StreamLookup

Performs a callsign lookup with streaming progress updates.

**Behavior:**
- Same lookup logic as `Lookup`, but streams intermediate state changes (e.g., `LOOKUP_STATE_IN_PROGRESS`, cache check result, final result).
- Useful for UIs that want to show real-time lookup progress.

**Error semantics:** Same as `Lookup`.

#### GetCachedCallsign

Returns a cached lookup result without querying the external provider.

**Behavior:**
- Query the `lookup_snapshots` store for the requested callsign.
- If found and not expired, return the cached `CallsignRecord`.
- If not found or expired, return an empty result (not an error).

**Error semantics:**
- `INTERNAL` — storage read failure.

#### GetDxccEntity

Returns DXCC entity information for a given DXCC code.

**Behavior:**
- Look up the `DxccEntity` by numeric DXCC code from the engine's DXCC reference data.
- Return country name, continent, zones, and geographic data.

**Error semantics:**
- `NOT_FOUND` — unknown DXCC code.

> **Note:** This RPC may be marked as unimplemented in early engine versions. Engines should return `UNIMPLEMENTED` if not yet supported.

#### BatchLookup

Performs lookups for multiple callsigns in a single request.

**Behavior:**
- Accept a list of callsigns.
- Perform lookups for each (cache-first, then external).
- Return a list of `LookupResult` entries, one per input callsign.
- Order of results matches order of input callsigns.

**Error semantics:**
- Per-callsign errors are reported in individual `LookupResult` entries, not as top-level gRPC errors.

> **Note:** This RPC may be marked as unimplemented in early engine versions.

### 3.4 RigControlService

**Proto file:** `proto/services/rig_control_service.proto`

Rig integration via the rigctld protocol.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `GetRigStatus` | `GetRigStatusRequest` | `GetRigStatusResponse` | Unary |
| `GetRigSnapshot` | `GetRigSnapshotRequest` | `GetRigSnapshotResponse` | Unary |
| `TestRigConnection` | `TestRigConnectionRequest` | `TestRigConnectionResponse` | Unary |

#### GetRigStatus

Returns the current rig connection status.

**Behavior:**
- Return a `RigConnectionStatus` value: `Connected`, `Disconnected`, `Error`, or `Disabled`.
- If rig control is disabled via configuration, return `Disabled`.

**Error semantics:**
- This RPC should always succeed. Connection problems are reported in the status value, not as gRPC errors.

#### GetRigSnapshot

Returns the most recent frequency/mode snapshot from the rig.

**Behavior:**
- Return a `RigSnapshot` containing `frequency_hz`, `band`, `mode`, `submode`, `raw_mode`, `status`, and `sampled_at`.
- If the rig is disconnected or disabled, return a snapshot with appropriate status and no frequency/mode data.
- If the last snapshot is older than `QSORIPPER_RIGCTLD_STALE_THRESHOLD_MS`, mark it as stale.

**Error semantics:**
- This RPC should always succeed. Rig errors are reported in the snapshot's `status` and `error_message` fields.

#### TestRigConnection

Tests TCP connectivity to the configured rigctld instance.

**Behavior:**
1. Attempt a TCP connection to `QSORIPPER_RIGCTLD_HOST`:`QSORIPPER_RIGCTLD_PORT`.
2. If the connection succeeds, send a basic command (e.g., `f\n`) and verify a response.
3. Return success/failure with diagnostics.

**Error semantics:**
- Connection and protocol errors are reported in the response, not as gRPC errors.

### 3.5 SpaceWeatherService

**Proto file:** `proto/services/space_weather_service.proto`

Cached space weather data from NOAA SWPC.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `GetCurrentSpaceWeather` | `GetCurrentSpaceWeatherRequest` | `GetCurrentSpaceWeatherResponse` | Unary |
| `RefreshSpaceWeather` | `RefreshSpaceWeatherRequest` | `RefreshSpaceWeatherResponse` | Unary |

#### GetCurrentSpaceWeather

Returns the most recently cached space weather snapshot.

**Behavior:**
- Return a `SpaceWeatherSnapshot` with K-index, A-index, solar flux, sunspot number, geomagnetic storm scale, and fetch timestamps.
- If space weather is disabled or no data has been fetched, return a snapshot with `SpaceWeatherStatus.SPACE_WEATHER_STATUS_ERROR` or `SPACE_WEATHER_STATUS_DISABLED`.
- Do not trigger a remote fetch. Return whatever is cached.

**Error semantics:**
- This RPC should always succeed. Data unavailability is reported in the snapshot status.

#### RefreshSpaceWeather

Forces an immediate refresh from the NOAA APIs.

**Behavior:**
1. Fetch fresh data from NOAA SWPC endpoints (K-index JSON and solar indices text).
2. Parse and update the cached snapshot.
3. Return the new snapshot.

**Error semantics:**
- `UNAVAILABLE` — NOAA endpoints unreachable.
- `FAILED_PRECONDITION` — space weather integration is disabled.

### 3.6 SetupService

**Proto file:** `proto/services/setup_service.proto`

First-run bootstrap and credential validation.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `GetSetupStatus` | `GetSetupStatusRequest` | `GetSetupStatusResponse` | Unary |
| `SaveSetup` | `SaveSetupRequest` | `SaveSetupResponse` | Unary |
| `GetSetupWizardState` | `GetSetupWizardStateRequest` | `GetSetupWizardStateResponse` | Unary |
| `ValidateSetupStep` | `ValidateSetupStepRequest` | `ValidateSetupStepResponse` | Unary |
| `TestQrzCredentials` | `TestQrzCredentialsRequest` | `TestQrzCredentialsResponse` | Unary |
| `TestQrzLogbookCredentials` | `TestQrzLogbookCredentialsRequest` | `TestQrzLogbookCredentialsResponse` | Unary |

#### GetSetupStatus

Returns whether initial setup has been completed.

**Behavior:**
- Check if a valid configuration and station profile exist.
- Return a `SetupStatus` indicating `complete` or `incomplete` with details about what is missing.

#### SaveSetup

Persists setup configuration and station profile.

**Behavior:**
1. Validate all provided fields.
2. Persist configuration (QRZ credentials, station profile, storage settings) to the config path.
3. Apply the configuration to the running engine (activate the station profile, enable integrations).
4. Mark setup as complete.

**Error semantics:**
- `INVALID_ARGUMENT` — invalid or missing required setup fields.
- `INTERNAL` — failed to persist configuration.

#### GetSetupWizardState

Returns the current state of the setup wizard for multi-step UIs.

**Behavior:**
- Return the list of `SetupWizardStep` values with their completion status (`SetupWizardStepStatus`).
- Steps include: station profile, QRZ XML credentials, QRZ logbook credentials, storage backend, rig control, space weather.

#### ValidateSetupStep

Validates a single step of the setup wizard without persisting.

**Behavior:**
- Accept a `SetupWizardStep` identifier and field values.
- Validate the fields for that step.
- Return validation results per field (`SetupFieldValidation`).

**Error semantics:**
- `INVALID_ARGUMENT` — unknown step identifier.

#### TestQrzCredentials

Tests QRZ XML API credentials by attempting a login.

**Behavior:**
1. Send a login request to the QRZ XML API with the provided username and password.
2. Return success if a session key is obtained.
3. Return failure with a descriptive message if authentication fails.

**Error semantics:**
- Authentication failures are reported in the response, not as gRPC errors.
- `UNAVAILABLE` — QRZ API unreachable.

#### TestQrzLogbookCredentials

Tests QRZ logbook API credentials.

**Behavior:**
1. Send a `STATUS` request to the QRZ logbook API with the provided API key.
2. Return success if the API responds with valid logbook metadata.
3. Return failure with a descriptive message otherwise.

**Error semantics:**
- Same pattern as `TestQrzCredentials`.

### 3.7 StationProfileService

**Proto file:** `proto/services/station_profile_service.proto`

Manages station identity profiles and session overrides.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `ListStationProfiles` | `ListStationProfilesRequest` | `ListStationProfilesResponse` | Unary |
| `GetStationProfile` | `GetStationProfileRequest` | `GetStationProfileResponse` | Unary |
| `SaveStationProfile` | `SaveStationProfileRequest` | `SaveStationProfileResponse` | Unary |
| `DeleteStationProfile` | `DeleteStationProfileRequest` | `DeleteStationProfileResponse` | Unary |
| `SetActiveStationProfile` | `SetActiveStationProfileRequest` | `SetActiveStationProfileResponse` | Unary |
| `GetActiveStationContext` | `GetActiveStationContextRequest` | `GetActiveStationContextResponse` | Unary |
| `SetSessionStationProfileOverride` | `SetSessionStationProfileOverrideRequest` | `SetSessionStationProfileOverrideResponse` | Unary |
| `ClearSessionStationProfileOverride` | `ClearSessionStationProfileOverrideRequest` | `ClearSessionStationProfileOverrideResponse` | Unary |

#### ListStationProfiles

Returns all saved station profiles.

**Behavior:**
- Return a list of `StationProfileRecord` entries with their profile names and data.

#### GetStationProfile

Returns a single station profile by name.

**Error semantics:**
- `NOT_FOUND` — no profile with the given name.

#### SaveStationProfile

Creates or updates a station profile.

**Behavior:**
1. Validate required fields: profile name, station callsign.
2. Persist the profile.
3. If this is the first profile and no active profile is set, automatically activate it.

**Error semantics:**
- `INVALID_ARGUMENT` — missing or invalid fields.

#### DeleteStationProfile

Deletes a station profile by name.

**Error semantics:**
- `NOT_FOUND` — no profile with the given name.
- `FAILED_PRECONDITION` — cannot delete the active profile while it is active.

#### SetActiveStationProfile

Activates a saved profile as the current station context.

**Behavior:**
- Load the named profile and set it as the active station context.
- All subsequent `LogQso` calls will stamp QSOs with this profile's station data.

**Error semantics:**
- `NOT_FOUND` — no profile with the given name.

#### GetActiveStationContext

Returns the currently active station context.

**Behavior:**
- Return an `ActiveStationContext` containing the resolved station profile (accounting for any session override) and the profile name.
- If no active profile is set, return an empty context (not an error).

#### SetSessionStationProfileOverride

Temporarily overrides the active station profile for the current session.

**Behavior:**
- Accept individual field overrides (e.g., operator callsign, grid square).
- The override is applied on top of the active profile; it does not replace it.
- The override persists until explicitly cleared or the engine restarts.

#### ClearSessionStationProfileOverride

Removes the session override, reverting to the base active profile.

### 3.8 DeveloperControlService

**Proto file:** `proto/services/developer_control_service.proto`

Developer-only live configuration overrides. Not intended for end-user UIs.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `GetRuntimeConfig` | `GetRuntimeConfigRequest` | `GetRuntimeConfigResponse` | Unary |
| `ApplyRuntimeConfig` | `ApplyRuntimeConfigRequest` | `ApplyRuntimeConfigResponse` | Unary |
| `ResetRuntimeConfig` | `ResetRuntimeConfigRequest` | `ResetRuntimeConfigResponse` | Unary |

#### GetRuntimeConfig

Returns the full runtime configuration snapshot.

**Behavior:**
- Return a `RuntimeConfigSnapshot` containing all configuration fields with their current values, defaults, descriptions, types, and whether they are secret.
- Secret values (e.g., API keys) must be redacted in the response.

#### ApplyRuntimeConfig

Applies one or more runtime configuration mutations.

**Behavior:**
1. Accept a list of `RuntimeConfigMutation` entries (field name + new value + mutation kind).
2. Validate each mutation against the field's allowed values and type.
3. Apply the mutations to the running engine state.
4. Return the updated configuration snapshot.

**Error semantics:**
- `INVALID_ARGUMENT` — unknown field name, invalid value, or type mismatch.

#### ResetRuntimeConfig

Resets all runtime configuration to environment/default values.

**Behavior:**
- Discard all applied mutations.
- Reload configuration from environment variables and defaults.
- Return the reset configuration snapshot.

### 3.9 StressControlService (Optional)

**Proto file:** `proto/services/stress_control_service.proto`

Load testing control plane. Implementation is optional; engines that do not support stress testing should return `UNIMPLEMENTED` for all RPCs.

#### RPCs

| RPC | Request | Response | Mode |
|---|---|---|---|
| `StartStressRun` | `StartStressRunRequest` | `StartStressRunResponse` | Unary |
| `StopStressRun` | `StopStressRunRequest` | `StopStressRunResponse` | Unary |
| `GetStressRunStatus` | `GetStressRunStatusRequest` | `GetStressRunStatusResponse` | Unary |
| `StreamStressRunEvents` | `StreamStressRunEventsRequest` | `stream StreamStressRunEventsResponse` | Server-streaming |
| `ListStressProfiles` | `ListStressProfilesRequest` | `ListStressProfilesResponse` | Unary |

#### StartStressRun

Starts a load test run with the specified profile and configuration.

#### StopStressRun

Stops a running stress test.

#### GetStressRunStatus

Returns the current state of a stress run (idle, running, completed, failed).

#### StreamStressRunEvents

Streams real-time events (log entries, metrics, vector state changes) from a running stress test.

#### ListStressProfiles

Returns available stress test profiles.

---

## 4. Storage Contract

### 4.1 Backend Selection

The engine must support at least two storage backends, selectable at startup:

| Backend | Env Value | Description |
|---|---|---|
| **Memory** | `memory` | In-process, non-persistent. Default. |
| **SQLite** | `sqlite` | File-backed, persistent across restarts. |

Selection is controlled by the `QSORIPPER_STORAGE_BACKEND` environment variable. If unset, the engine defaults to `memory`.

### 4.2 In-Memory Backend

- All data is stored in-process data structures (maps, vectors).
- Data is lost on engine restart.
- Suitable for testing, development, and conformance runs.
- Must implement the full `EngineStorage` trait (logbook + lookup snapshots).

### 4.3 SQLite Backend

- Data is persisted to a SQLite file at the path specified by `QSORIPPER_SQLITE_PATH` (or `QSORIPPER_STORAGE_PATH`).
- Must use WAL journal mode for concurrent read/write performance.
- Must set `busy_timeout = 5000` (5 seconds) to handle transient lock contention.
- Must enable `foreign_keys = ON`.
- Must implement the full `EngineStorage` trait.

### 4.4 Schema

The SQLite backend uses the following schema (defined in `src/rust/qsoripper-storage-sqlite/src/migrations/0001_initial.sql`):

#### `qsos` table

QSO records are stored as protobuf binary blobs in a `record` column, with indexed extraction columns for efficient querying.

| Column | Type | Constraints | Description |
|---|---|---|---|
| `local_id` | `TEXT` | `PRIMARY KEY NOT NULL` | UUID v4 identifier |
| `qrz_logid` | `TEXT` | | QRZ log ID after sync |
| `qrz_bookid` | `TEXT` | | QRZ book ID after sync |
| `station_callsign` | `TEXT` | `NOT NULL` | Station callsign (indexed) |
| `worked_callsign` | `TEXT` | `NOT NULL` | Worked callsign (indexed) |
| `utc_timestamp_ms` | `INTEGER` | | UTC timestamp in milliseconds (indexed) |
| `band` | `INTEGER` | `NOT NULL` | Proto Band enum value (indexed) |
| `mode` | `INTEGER` | `NOT NULL` | Proto Mode enum value (indexed) |
| `contest_id` | `TEXT` | | Contest identifier (indexed) |
| `created_at_ms` | `INTEGER` | | Creation timestamp in ms |
| `updated_at_ms` | `INTEGER` | | Last update timestamp in ms |
| `sync_status` | `INTEGER` | `NOT NULL` | Proto SyncStatus enum value (indexed) |
| `record` | `BLOB` | `NOT NULL` | Full QsoRecord serialized as protobuf |

**Design rationale:** The `record` BLOB stores the complete proto-serialized `QsoRecord`. Extraction columns duplicate key fields for efficient SQL-level filtering and indexing. When reading, the engine deserializes from the `record` BLOB to get the full domain object.

#### `sync_metadata` table

Singleton row tracking QRZ logbook sync state.

| Column | Type | Constraints | Description |
|---|---|---|---|
| `id` | `INTEGER` | `PRIMARY KEY CHECK (id = 1)` | Always 1 (singleton) |
| `qrz_qso_count` | `INTEGER` | `NOT NULL DEFAULT 0` | QSO count reported by QRZ |
| `last_sync_ms` | `INTEGER` | | Last sync timestamp in ms |
| `qrz_logbook_owner` | `TEXT` | | QRZ logbook owner callsign |

A seed row `(1, 0)` is inserted on creation.

#### `lookup_snapshots` table

Cached callsign lookup results.

| Column | Type | Constraints | Description |
|---|---|---|---|
| `callsign` | `TEXT` | `PRIMARY KEY NOT NULL` | Normalized callsign |
| `result` | `BLOB` | `NOT NULL` | Proto-serialized `LookupResult` |
| `stored_at_ms` | `INTEGER` | `NOT NULL` | Cache timestamp in ms |
| `expires_at_ms` | `INTEGER` | | Cache expiry timestamp in ms |

### 4.5 Migration Strategy

- Migrations are embedded in the engine binary and applied at startup.
- Each migration is a numbered SQL file (e.g., `0001_initial.sql`).
- The engine must track which migrations have been applied and only run new ones.
- Migrations must be idempotent where possible.
- Schema changes must be backward-compatible: add columns with defaults, never remove columns in active use.

### 4.6 Storage Trait

All backends must implement the `EngineStorage` trait, which decomposes into:

- **`LogbookStore`** — `insert_qso`, `update_qso`, `delete_qso`, `get_qso`, `list_qsos`, `qso_counts`, `get_sync_metadata`, `upsert_sync_metadata`
- **`LookupSnapshotStore`** — `get_lookup_snapshot`, `upsert_lookup_snapshot`, `delete_lookup_snapshot`
- **`backend_name()`** — returns the backend identifier string (e.g., `"memory"`, `"sqlite"`)

---

## 5. Integration Contracts

### 5.1 QRZ XML Lookup

**API endpoint:** `https://xmldata.qrz.com/xml/current/`

**Authentication:** Session-key based. The engine must:
1. Send a login request with `username` and `password` parameters.
2. Extract the `<Key>` element from the XML response.
3. Use the session key for subsequent lookup requests.
4. Handle session expiry by re-authenticating when the API returns an auth error.
5. Retry with a fresh session key on the first failure before reporting an error.

**Request format:** HTTP GET with query parameters:
- Login: `?username=<user>&password=<pass>&agent=<user_agent>`
- Lookup: `?s=<callsign>&callsign=<callsign>&agent=<user_agent>`

**Response format:** XML with namespace `http://xmldata.qrz.com`. The engine must use namespace-aware XML parsing. Key elements:
- `<Callsign>` — contains all station data fields
- `<Session>` — contains session key, error messages, subscription status

**Normalization:** Map QRZ XML fields to `CallsignRecord` proto fields immediately at the provider edge. Never expose raw XML structures beyond the QRZ adapter.

**Rate limiting:** Respect QRZ's rate limits. Implement exponential backoff on HTTP 429 or repeated failures.

**Credential env vars:**
- `QSORIPPER_QRZ_XML_USERNAME`
- `QSORIPPER_QRZ_XML_PASSWORD`
- `QSORIPPER_QRZ_USER_AGENT`
- `QSORIPPER_QRZ_XML_BASE_URL` (override for testing)

### 5.2 QRZ Logbook Sync

**API endpoint:** `https://logbook.qrz.com/api`

**Authentication:** API key passed as a `KEY` parameter in every request.

**Request format:** HTTP POST, form-encoded body.

**Operations:**

| Action | Parameters | Description |
|---|---|---|
| `STATUS` | `KEY`, `ACTION=STATUS` | Returns logbook metadata (QSO count, owner) |
| `FETCH` | `KEY`, `ACTION=FETCH`, `OPTION=ALL` | Downloads all QSOs as ADIF |
| `INSERT` | `KEY`, `ACTION=INSERT`, `ADIF=<record>` | Uploads a single QSO |
| `DELETE` | `KEY`, `ACTION=DELETE`, `LOGID=<id>` | Deletes a QSO by logid |

**Response format:** Ampersand-delimited key-value pairs. Check `RESULT` field for success/failure.

**ADIF interchange:** The logbook API uses ADIF format for QSO data. The engine must serialize/deserialize ADIF at this boundary.

**Credential env vars:**
- `QSORIPPER_QRZ_LOGBOOK_API_KEY`
- `QSORIPPER_QRZ_LOGBOOK_BASE_URL` (override for testing)

### 5.3 Rig Control (rigctld)

**Protocol:** TCP text-based protocol (Hamlib rigctld).

**Connection:** TCP socket to `QSORIPPER_RIGCTLD_HOST`:`QSORIPPER_RIGCTLD_PORT` (default `localhost:4532`).

**Commands:**

| Command | Response | Description |
|---|---|---|
| `f\n` | Frequency in Hz (e.g., `14074000`) | Get current frequency |
| `m\n` | Mode and passband (e.g., `USB\n2400`) | Get current mode |

**Polling model:**
- The engine polls rigctld at a configurable interval.
- Each poll reads frequency and mode, constructs a `RigSnapshot`, and caches it.
- If the TCP connection fails, the rig status transitions to `Disconnected` or `Error`.
- If the snapshot is older than `QSORIPPER_RIGCTLD_STALE_THRESHOLD_MS`, it is marked stale.

**Read timeout:** `QSORIPPER_RIGCTLD_READ_TIMEOUT_MS` controls the per-command TCP read timeout.

### 5.4 Space Weather (NOAA SWPC)

**Data sources:**

| Data | URL | Format |
|---|---|---|
| K-index (planetary) | `https://services.swpc.noaa.gov/json/planetary_k_index_1m.json` | JSON array |
| Solar indices | `https://services.swpc.noaa.gov/text/daily-solar-indices.txt` | Fixed-width text |

**Refresh model:**
- Background refresh at `QSORIPPER_NOAA_REFRESH_INTERVAL_SECONDS` (default: 900 seconds / 15 minutes).
- Cached snapshot expires after `QSORIPPER_NOAA_STALE_AFTER_SECONDS`.
- HTTP timeout controlled by `QSORIPPER_NOAA_TIMEOUT_SECONDS`.
- If refresh fails, the engine retains the last known good snapshot and reports the error in the snapshot status.

**Parsed fields:** K-index, A-index, solar flux (SFI), sunspot number, geomagnetic storm scale.

---

## 6. Configuration

### 6.1 Environment Variables

All configuration is driven by environment variables prefixed with `QSORIPPER_`. The engine should also support loading from a `.env` file at the config path.

#### Global

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_SERVER_ADDR` | String | `[::1]:50051` | gRPC listen address |
| `QSORIPPER_CONFIG_PATH` | Path | Platform-dependent | Configuration file directory |

#### Storage

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_STORAGE_BACKEND` | Enum | `memory` | `memory` or `sqlite` |
| `QSORIPPER_STORAGE_PATH` | Path | | SQLite file directory |
| `QSORIPPER_SQLITE_PATH` | Path | | Full SQLite file path (overrides `STORAGE_PATH`) |

#### QRZ XML Lookup

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_QRZ_XML_USERNAME` | String | | QRZ.com username |
| `QSORIPPER_QRZ_XML_PASSWORD` | String | | QRZ.com password (secret) |
| `QSORIPPER_QRZ_USER_AGENT` | String | | HTTP User-Agent for QRZ requests |
| `QSORIPPER_QRZ_XML_BASE_URL` | URL | `https://xmldata.qrz.com/xml/current/` | QRZ XML API base URL |

#### QRZ Logbook

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_QRZ_LOGBOOK_API_KEY` | String | | QRZ logbook API key (secret) |
| `QSORIPPER_QRZ_LOGBOOK_BASE_URL` | URL | `https://logbook.qrz.com/api` | QRZ logbook API base URL |

#### Sync

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_SYNC_AUTO_ENABLED` | Bool | `false` | Enable automatic background sync |
| `QSORIPPER_SYNC_INTERVAL_SECONDS` | Integer | `300` | Auto-sync interval in seconds |
| `QSORIPPER_SYNC_CONFLICT_POLICY` | Enum | `local_wins` | `local_wins`, `remote_wins`, or `newest_wins` |

#### Rig Control

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_RIGCTLD_ENABLED` | Bool | `false` | Enable rigctld integration |
| `QSORIPPER_RIGCTLD_HOST` | String | `localhost` | rigctld TCP host |
| `QSORIPPER_RIGCTLD_PORT` | Integer | `4532` | rigctld TCP port |
| `QSORIPPER_RIGCTLD_READ_TIMEOUT_MS` | Integer | `2000` | Per-command read timeout |
| `QSORIPPER_RIGCTLD_STALE_THRESHOLD_MS` | Integer | `5000` | Snapshot staleness threshold |

#### Space Weather

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_NOAA_SPACE_WEATHER_ENABLED` | Bool | `false` | Enable NOAA space weather |
| `QSORIPPER_NOAA_REFRESH_INTERVAL_SECONDS` | Integer | `900` | Background refresh interval |
| `QSORIPPER_NOAA_STALE_AFTER_SECONDS` | Integer | `3600` | Snapshot expiry |
| `QSORIPPER_NOAA_TIMEOUT_SECONDS` | Integer | `10` | HTTP request timeout |

#### Station Profile

| Variable | Type | Default | Description |
|---|---|---|---|
| `QSORIPPER_STATION_PROFILE_NAME` | String | | Default profile name |
| `QSORIPPER_STATION_CALLSIGN` | String | | Station callsign |
| `QSORIPPER_STATION_OPERATOR_CALLSIGN` | String | | Operator callsign (if different) |

### 6.2 Graceful Degradation Rules

The engine must start and function even when external integrations are unavailable. Degradation follows these rules:

| Missing Configuration | Behavior |
|---|---|
| QRZ XML credentials | QRZ lookups disabled. `Lookup` returns `LookupState.LOOKUP_STATE_NOT_FOUND`. |
| QRZ logbook API key | Logbook sync disabled. `SyncWithQrz` returns `FAILED_PRECONDITION`. |
| rigctld host/port | Rig control disabled. `GetRigStatus` returns `RIG_CONNECTION_STATUS_DISABLED`. |
| NOAA weather disabled | Space weather disabled. `GetCurrentSpaceWeather` returns `SPACE_WEATHER_STATUS_DISABLED`. |
| No station profile | QSO logging requires a profile. `LogQso` returns `FAILED_PRECONDITION` until a profile is set. |

**Core invariant:** Local QSO storage and CRUD always work, regardless of external integration state. The engine must never fail to start because an external service is unavailable.

### 6.3 Configuration Persistence

- Configuration is persisted as a shared TOML file in `QSORIPPER_CONFIG_PATH`.
- The `SaveSetup` RPC writes configuration to this path.
- On startup, the engine loads persisted configuration and overlays environment variable overrides (env vars take precedence).
- Runtime config mutations (via `DeveloperControlService`) are ephemeral and do not persist across restarts unless explicitly saved.

---

## 7. Behavioral Requirements

### 7.1 Station Context

Every logged QSO must carry station identity data. The station context system works as follows:

1. **Station profiles** are named, persisted sets of station defaults: callsign, operator callsign, grid square, county, state, country, DXCC, CQ/ITU zones, latitude/longitude, and ARRL section.

2. **Active profile** — exactly one profile is active at any time. The engine resolves the active profile from (highest priority first):
   - Session override fields (set via `SetSessionStationProfileOverride`)
   - Active profile (set via `SetActiveStationProfile`)

3. **Station snapshot** — when a QSO is logged, the engine captures the current station context as an immutable `StationSnapshot` and attaches it to the `QsoRecord`. This snapshot is never retroactively updated if the profile changes.

4. **Materialization** — the engine must implement a `station_snapshot_from_profile` function that converts a `StationProfile` (plus any session overrides) into a `StationSnapshot` suitable for embedding in a QSO.

### 7.2 QSO Lifecycle

#### Creating a QSO

1. Client calls `LogQso` with the worked callsign, band, mode, signal reports, and optional fields.
2. Engine generates `local_id` as UUID v4.
3. Engine normalizes `worked_callsign`: `trim().to_uppercase()`.
4. Engine validates required fields: `worked_callsign` must be non-empty, `band` must be non-default, `mode` must be non-default, `utc_timestamp` must be present.
5. Engine stamps `station_callsign` and `station_snapshot` from the active station context.
6. Engine sets `created_at` = `updated_at` = now (UTC), `sync_status` = `NOT_SYNCED`.
7. Engine persists the `QsoRecord` via the storage backend.
8. Engine returns the persisted record to the client.

#### Updating a QSO

1. Client calls `UpdateQso` with `local_id` and changed fields.
2. Engine loads existing record, applies changes, sets `updated_at` = now.
3. If previously synced, engine sets `sync_status` = `MODIFIED`.
4. Engine persists and returns updated record.

#### Deleting a QSO

1. Client calls `DeleteQso` with `local_id`.
2. Engine removes the record from storage.
3. If the QSO had been synced to QRZ, the engine may optionally queue a remote delete (implementation-dependent).

### 7.3 Sync Lifecycle

The QRZ logbook sync is a three-phase operation:

#### Phase 1: Download

1. Call QRZ logbook API `FETCH` with `OPTION=ALL`.
2. Parse the ADIF response into QSO records.
3. For each remote QSO:
   a. Attempt to match against local records using fuzzy matching: callsign (case-insensitive) + UTC timestamp (within a tolerance window) + band + mode.
   b. If matched, compare and update per the `ConflictPolicy`:
      - `LOCAL_WINS` — keep local version, only update `qrz_logid`.
      - `REMOTE_WINS` — overwrite local fields with remote data.
      - `NEWEST_WINS` — keep the version with the later `updated_at`.
   c. If unmatched, insert as a new local record with `sync_status = SYNCED`.
4. Filter ghost records: QSOs that appear in the remote data but are clearly duplicates or artifacts.

#### Phase 2: Upload

1. Query local QSOs with `sync_status` in (`NOT_SYNCED`, `MODIFIED`).
2. For each, serialize to ADIF and call QRZ logbook API `INSERT`.
3. On success, update `sync_status = SYNCED` and store the returned `qrz_logid`.
4. On per-QSO failure, log the error and continue with remaining QSOs.

#### Phase 3: Metadata

1. Call QRZ logbook API `STATUS` to get the current QSO count and owner.
2. Update `sync_metadata` with the count, timestamp, and owner callsign.

**Resilience:** A failure in any phase should not prevent other phases from executing. The engine should report partial success/failure in the stream.

### 7.4 Lookup Lifecycle

#### Single Lookup Flow

```
Client calls Lookup("W1AW")
  → Engine checks lookup_snapshots cache
    → Cache HIT (not expired) → return cached result (cache_hit=true)
    → Cache MISS or expired:
      → Check in-flight dedup map
        → Already in flight → wait for existing result
        → Not in flight → register in-flight
          → Call QRZ XML API
            → Parse XML response
            → Normalize to CallsignRecord
            → Enrich with DXCC entity data
            → Cache in lookup_snapshots
            → Remove from in-flight map
          → Return result (cache_hit=false)
```

#### Slash-Call Fallback

For callsigns with modifiers (e.g., `W1AW/7`, `VE3/W1AW`):

1. Attempt lookup with the full callsign.
2. If not found, extract the base callsign (strip the modifier).
3. Retry lookup with the base callsign.
4. On the result, populate:
   - `base_callsign` — the callsign used for the successful lookup
   - `modifier_text` — the modifier portion (e.g., `/7`)
   - `modifier_kind` — the type of modifier (`ModifierKind` enum)
   - `callsign_ambiguity` — flags if the callsign interpretation is ambiguous

#### Zone Cascade

When DXCC data is available, cascade zone information onto the lookup result if the source record lacks it:
- CQ zone from DXCC entity if not on the callsign record
- ITU zone from DXCC entity if not on the callsign record

### 7.5 ADIF Import/Export

**ADIF is the Amateur Data Interchange Format**, used exclusively for external file interchange and QRZ API communication. Internal engine IPC always uses protobuf.

#### Import

1. Parse the ADI-format input (header + records delimited by `<eor>`).
2. Map ADIF field names to `QsoRecord` proto fields.
3. Preserve unrecognized ADIF fields in the `extra_fields` map for lossless round-trip.
4. Generate a `local_id` for each imported record.
5. Normalize callsigns and validate required fields.
6. Insert into storage with `sync_status = NOT_SYNCED`.

#### Export

1. Generate an ADIF header with program name and version.
2. For each QSO, serialize proto fields back to ADIF field names.
3. Include `extra_fields` to preserve data from previous imports.
4. Output records delimited by `<eor>`.

### 7.6 Error Handling

#### General Principles

- Use standard gRPC status codes (see individual RPC documentation).
- Include descriptive error messages in the gRPC status detail.
- Never leak credentials, API keys, or session tokens in error messages.
- Log actionable errors server-side with enough context to diagnose issues.
- External integration failures must never crash the engine or prevent local operations.

#### Standard gRPC Status Code Usage

| Code | Usage |
|---|---|
| `OK` | Success |
| `INVALID_ARGUMENT` | Malformed request, missing required fields, invalid values |
| `NOT_FOUND` | Requested entity does not exist |
| `FAILED_PRECONDITION` | Operation cannot proceed due to system state (e.g., no credentials, no active profile) |
| `UNAVAILABLE` | External service unreachable |
| `UNIMPLEMENTED` | RPC is defined but not yet implemented |
| `INTERNAL` | Unexpected server error (storage failure, serialization bug) |

---

## 8. Capability Reporting

### 8.1 GetEngineInfo Contract

Every engine must implement `GetEngineInfo` to report its identity and capabilities. This is the first RPC a client calls after connecting.

**Required response fields:**

| Field | Example (Rust) | Example (.NET) |
|---|---|---|
| Engine name | `qsoripper-server` | `qsoripper-engine-dotnet` |
| Version | `0.1.0` | `0.1.0` |
| Language | `rust` | `csharp` |
| Storage backend | `sqlite` | `memory` |
| Capabilities | List of supported feature flags | List of supported feature flags |

**Capability flags** indicate which optional features the engine supports. Clients use these to enable or disable UI features. Examples:

- `logbook` — core QSO CRUD
- `lookup` — callsign lookup
- `sync` — QRZ logbook sync
- `rig_control` — rigctld integration
- `space_weather` — NOAA space weather
- `stress` — stress testing control plane
- `adif_import` — ADIF file import
- `adif_export` — ADIF file export

Engines should report capabilities accurately based on their current configuration. An engine with no QRZ credentials should not report `lookup` as a capability.

---

## 9. Conformance Testing

### 9.1 Conformance Harness

The conformance harness lives at `tests/Run-EngineConformance.ps1`. It is a PowerShell script that:

1. Starts an engine process (configurable: Rust with SQLite, .NET with memory, etc.).
2. Runs the QsoRipper CLI against the engine to exercise the full RPC surface.
3. Compares results across engine implementations for field-level parity.
4. Writes a structured JSON summary to `artifacts/conformance/<run-id>/`.

The harness is the authoritative definition of "conformant." If the spec and the harness disagree, update the spec.

### 9.2 Required Test Scenarios

A conformant engine must pass all of the following scenarios:

#### Setup and Status

1. `setup --from-env` succeeds and reports `setupComplete = true`.
2. `status` reports the correct engine identity and storage backend.
3. Station callsign is correctly persisted and reported.

#### QSO CRUD

4. `LogQso` creates a QSO with a generated `local_id` and correct station stamping.
5. `GetQso` returns the logged QSO with all fields intact.
6. `ListQsos` returns exactly the expected QSOs with correct ordering.
7. `UpdateQso` modifies the specified fields and updates `updated_at`.
8. `DeleteQso` removes the QSO; subsequent `GetQso` returns `NOT_FOUND`.
9. Unary success and failure responses with optional scalar fields serialize cleanly at the service boundary without handler exceptions.

#### ADIF Round-Trip

10. `ExportAdif` produces valid ADIF output containing all logged QSOs.
11. `ImportAdif` with previously exported ADIF creates equivalent records.
12. `extra_fields` survive a full import → export → import round-trip.

#### Cross-Engine Parity

13. Given the same sequence of operations, the Rust and .NET engines produce field-identical `GetQso`, `ListQsos`, and `ExportAdif` results.
14. Both engines report `localQsoCount == 1` after logging one QSO.

#### Lookup (if credentials available)

14. `Lookup` for a known callsign returns a populated `CallsignRecord`.
15. `GetCachedCallsign` returns the cached result after a successful lookup.
16. `Lookup` for an unknown callsign returns `LOOKUP_STATE_NOT_FOUND`.

#### Degradation

17. Engine starts successfully with no QRZ credentials configured.
18. Engine starts successfully with no rigctld configured.
19. `LogQso` works when external integrations are unavailable.

---

## 10. Reference Implementations

### 10.1 Rust Engine (qsoripper-server)

| Property | Value |
|---|---|
| **Location** | `src/rust/qsoripper-server/` |
| **Core library** | `src/rust/qsoripper-core/` |
| **Language** | Rust |
| **gRPC framework** | tonic + prost |
| **Storage backends** | `qsoripper-storage-memory`, `qsoripper-storage-sqlite` |
| **Build** | `cargo build --manifest-path src/rust/Cargo.toml -p qsoripper-server` |
| **Run** | `cargo run --manifest-path src/rust/Cargo.toml -p qsoripper-server` |
| **Test** | `cargo test --manifest-path src/rust/Cargo.toml` |

**Architecture notes:**
- `qsoripper-core` owns reusable engine logic: domain mapping, proto bindings, storage traits, QRZ adapters, rig control, space weather, and ADIF parsing.
- `qsoripper-server` owns the tonic server bootstrap, runtime configuration registry, and gRPC service implementations.
- Storage backends are separate crates (`qsoripper-storage-memory`, `qsoripper-storage-sqlite`) that implement the `EngineStorage` trait from `qsoripper-core`.
- Proto generation happens in `qsoripper-core/build.rs`.

### 10.2 .NET Engine (QsoRipper.Engine.DotNet)

| Property | Value |
|---|---|
| **Location** | `src/dotnet/QsoRipper.Engine.DotNet/` |
| **Language** | C# |
| **gRPC framework** | Grpc.Tools + ASP.NET Core |
| **Storage backend** | In-memory (managed state) |
| **Build** | `dotnet build src/dotnet/QsoRipper.Engine.DotNet/QsoRipper.Engine.DotNet.csproj` |
| **Run** | `dotnet run --project src/dotnet/QsoRipper.Engine.DotNet/QsoRipper.Engine.DotNet.csproj` |
| **Test** | `dotnet test src/dotnet/QsoRipper.Engine.DotNet.Tests/` |

**Architecture notes:**
- `GrpcServices.cs` maps gRPC service interfaces to the managed engine state.
- `ManagedEngineState.cs` implements core engine logic: QSO CRUD, station context, lookup orchestration.
- `ManagedAdifCodec.cs` handles ADIF serialization/deserialization.
- `ManagedQsoParity.cs` ensures QSO normalization and station stamping matches the Rust engine.
- Proto generation uses `Grpc.Tools` configured in the `.csproj`.

---

## Appendix A: Key Domain Types Quick Reference

| Proto File | Type | Description |
|---|---|---|
| `proto/domain/qso_record.proto` | `QsoRecord` | The core logged-contact entity |
| `proto/domain/callsign_record.proto` | `CallsignRecord` | Normalized callsign lookup result |
| `proto/domain/dxcc_entity.proto` | `DxccEntity` | DXCC entity reference data |
| `proto/domain/lookup_result.proto` | `LookupResult` | Lookup outcome with metadata |
| `proto/domain/station_profile.proto` | `StationProfile` | Durable station defaults |
| `proto/domain/station_snapshot.proto` | `StationSnapshot` | Immutable per-QSO station capture |
| `proto/domain/rig_snapshot.proto` | `RigSnapshot` | Rig frequency/mode snapshot |
| `proto/domain/space_weather_snapshot.proto` | `SpaceWeatherSnapshot` | Space weather indices |
| `proto/domain/sync_config.proto` | `SyncConfig` | Sync policy configuration |
| `proto/domain/band.proto` | `Band` | Band enumeration (ADIF-aligned) |
| `proto/domain/mode.proto` | `Mode` | Mode enumeration (ADIF-aligned) |
| `proto/domain/sync_status.proto` | `SyncStatus` | QSO sync state |
| `proto/domain/lookup_state.proto` | `LookupState` | Lookup result state |
| `proto/domain/conflict_policy.proto` | `ConflictPolicy` | Sync conflict resolution policy |
| `proto/domain/rig_connection_status.proto` | `RigConnectionStatus` | Rig connection state |
| `proto/domain/space_weather_status.proto` | `SpaceWeatherStatus` | Space weather data state |

## Appendix B: Proto File Conventions

- **1-1-1 rule:** One top-level message, enum, or service per `.proto` file.
- **Per-RPC envelopes:** Every RPC gets unique `XxxRequest` and `XxxResponse` messages.
- **Service declarations** contain only the `service` block; all message types live in separate files.
- **Domain types** live in `proto/domain/`; transport/service support types live in `proto/services/`.
- **Reusable payloads** are extracted into dedicated messages and wrapped from each response — never reuse one RPC's response as another's.
- Run `buf lint` to validate proto files. Run `buf breaking` to guard against incompatible schema changes.

See `docs/architecture/data-model.md` for the complete proto conventions and field-addition guide.
