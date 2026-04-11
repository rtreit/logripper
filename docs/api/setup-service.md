# SetupService Reference

`SetupService` is the engine-facing bootstrap surface for first-run configuration.

Proto definition: [`proto/services/setup_service.proto`](../../proto/services/setup_service.proto)

## Purpose

This service owns **persisted engine setup**, not developer-only runtime overrides.

Use it to:

- detect whether a real config file already exists
- discover the config path the engine will use
- discover the suggested default SQLite path
- inspect the currently persisted storage/station bootstrap
- save the initial storage choice, station profile, and optional QRZ XML credentials

## Current behavior

| RPC | Status | Notes |
|---|---|---|
| `GetSetupStatus` | ✅ Implemented | Returns persisted setup status, config path, suggested SQLite path, and validation warnings |
| `SaveSetup` | ✅ Implemented | Validates and writes `config.toml`, then hot-applies the new persisted config to the running engine |

## RPCs

### GetSetupStatus

Read the current persisted setup status.

```
rpc GetSetupStatus(GetSetupStatusRequest) returns (SetupStatusResponse)
```

**Response highlights**

| Field | Type | Meaning |
|---|---|---|
| `config_file_exists` | `bool` | Whether the persisted setup file exists |
| `setup_complete` | `bool` | Whether the saved setup is sufficient for the current first-run slice |
| `config_path` | `string` | Path to the config file the engine is using |
| `storage_backend` | `StorageBackend` | Persisted storage choice |
| `sqlite_path` | `string` (optional) | Persisted SQLite path when SQLite is selected |
| `station_profile` | `StationProfile` (optional) | Persisted bootstrap station profile |
| `active_station_profile_id` | `string` (optional) | Persisted active station-profile id when profile lifecycle is configured |
| `station_profile_count` | `uint32` | Count of persisted station profiles currently stored |
| `qrz_xml_username` | `string` (optional) | Persisted QRZ XML username |
| `has_qrz_xml_password` | `bool` | Whether a QRZ XML password is stored |
| `suggested_sqlite_path` | `string` | Recommended SQLite path when the user has not picked one yet |
| `warnings` | `repeated string` | Human-readable setup gaps or validation warnings |

### SaveSetup

Validate and persist the initial engine setup.

```
rpc SaveSetup(SaveSetupRequest) returns (SaveSetupResponse)
```

**Request highlights**

| Field | Type | Meaning |
|---|---|---|
| `storage_backend` | `StorageBackend` | Required. `MEMORY` or `SQLITE` |
| `sqlite_path` | `string` (optional) | Optional explicit SQLite path. When omitted for SQLite, the engine uses `suggested_sqlite_path` |
| `station_profile` | `StationProfile` | Required bootstrap station profile |
| `qrz_xml_username` | `string` (optional) | Optional QRZ XML username |
| `qrz_xml_password` | `string` (optional) | Optional QRZ XML password |

**Validation rules**

- `station_profile.station_callsign` is required
- QRZ XML username/password must either both be set or both be omitted
- `dxcc`, `cq_zone`, and `itu_zone` must be greater than zero when present
- `latitude` / `longitude` must be finite and within valid bounds

## Persistence model

- Default config path:
  - Windows: `%APPDATA%\logripper\config.toml`
  - Linux: `~/.config/logripper/config.toml` (or `XDG_CONFIG_HOME`)
- The server also supports overriding the config path with:
  - environment: `LOGRIPPER_CONFIG_PATH`
  - CLI: `--config path\to\config.toml`

Saved setup is hot-applied to the running engine for **new** requests. Existing saved QSOs remain unchanged because they already carry their own `station_snapshot`.
