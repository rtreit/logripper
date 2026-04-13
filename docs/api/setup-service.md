# SetupService Reference

`SetupService` is the engine-facing bootstrap surface for first-run configuration.

Proto definition: [`proto/services/setup_service.proto`](../../proto/services/setup_service.proto)

## Purpose

This service owns **persisted engine setup**, not developer-only runtime overrides.

Use it to:

- detect whether a real config file already exists
- discover the config path the engine will use
- discover the suggested default log file path
- inspect the currently persisted logbook/station bootstrap
- save the initial log file path, station profile, and optional QRZ XML credentials

## Current behavior

| RPC | Status | Notes |
|---|---|---|
| `GetSetupStatus` | ✅ Implemented | Returns persisted setup status, config path, suggested log file path, and validation warnings |
| `SaveSetup` | ✅ Implemented | Validates and writes `config.toml`, then hot-applies the new persisted config to the running engine |

Both RPCs use unique request/response envelopes and share a reusable `SetupStatus` payload. `GetSetupStatusResponse.status` and `SaveSetupResponse.status` each carry that payload instead of reusing an RPC response as a nested model.

## RPCs

### GetSetupStatus

Read the current persisted setup status.

```
rpc GetSetupStatus(GetSetupStatusRequest) returns (GetSetupStatusResponse)
```

**Response envelope:** `GetSetupStatusResponse`

| Field | Type | Meaning |
|---|---|---|
| `status` | `SetupStatus` | Reusable setup payload |

**`SetupStatus` highlights**

| Field | Type | Meaning |
|---|---|---|
| `config_file_exists` | `bool` | Whether the persisted setup file exists |
| `setup_complete` | `bool` | Whether the saved setup is sufficient for the current first-run slice |
| `config_path` | `string` | Path to the config file the engine is using |
| `log_file_path` | `string` (optional) | Persisted log file path for the durable logbook |
| `station_profile` | `StationProfile` (optional) | Persisted bootstrap station profile |
| `active_station_profile_id` | `string` (optional) | Persisted active station-profile id when profile lifecycle is configured |
| `station_profile_count` | `uint32` | Count of persisted station profiles currently stored |
| `qrz_xml_username` | `string` (optional) | Persisted QRZ XML username |
| `has_qrz_xml_password` | `bool` | Whether a QRZ XML password is stored |
| `suggested_log_file_path` | `string` | Recommended log file path when the user has not picked one yet |
| `warnings` | `repeated string` | Human-readable setup gaps or validation warnings |

### SaveSetup

Validate and persist the initial engine setup.

```
rpc SaveSetup(SaveSetupRequest) returns (SaveSetupResponse)
```

**Request highlights**

| Field | Type | Meaning |
|---|---|---|
| `log_file_path` | `string` (optional) | Preferred durable log file path for the operator-facing setup flow |
| `station_profile` | `StationProfile` | Required bootstrap station profile |
| `qrz_xml_username` | `string` (optional) | Optional QRZ XML username |
| `qrz_xml_password` | `string` (optional) | Optional QRZ XML password |

**Validation rules**

- `station_profile.station_callsign` is required
- `log_file_path` is required for the current operator-facing setup flow
- QRZ XML username/password must either both be set or both be omitted
- `dxcc`, `cq_zone`, and `itu_zone` must be greater than zero when present
- `latitude` / `longitude` must be finite and within valid bounds

**Response envelope:** `SaveSetupResponse`

| Field | Type | Meaning |
|---|---|---|
| `status` | `SetupStatus` | The persisted setup state after validation/save completes |

Legacy compatibility note: the proto still carries `storage_backend`, `sqlite_path`, and `suggested_sqlite_path` for older clients, but new callers should use `log_file_path` and `suggested_log_file_path`.

## Persistence model

- Default config path:
  - Windows: `%APPDATA%\qsoripper\config.toml`
  - Linux: `~/.config/qsoripper/config.toml` (or `XDG_CONFIG_HOME`)
- The server also supports overriding the config path with:
  - environment: `QSORIPPER_CONFIG_PATH`
  - CLI: `--config path\to\config.toml`

Saved setup is hot-applied to the running engine for **new** requests. Existing saved QSOs remain unchanged because they already carry their own `station_snapshot`.
