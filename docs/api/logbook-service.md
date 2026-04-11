# LogbookService Reference

The `LogbookService` is the core QSO lifecycle interface. It covers local QSO CRUD and ADIF import/export today, with QRZ sync reserved for a later slice.

Proto definition: [`proto/services/logbook_service.proto`](../../proto/services/logbook_service.proto)

Domain payloads: [`proto/domain/qso_record.proto`](../../proto/domain/qso_record.proto), [`proto/domain/band.proto`](../../proto/domain/band.proto), [`proto/domain/mode.proto`](../../proto/domain/mode.proto), [`proto/domain/sync_status.proto`](../../proto/domain/sync_status.proto), [`proto/domain/station_snapshot.proto`](../../proto/domain/station_snapshot.proto)

Service envelopes and support types live in their own files under `proto/services/`. Every RPC uses a unique request/response envelope, including streamed items such as `ListQsosResponse` and `ExportAdifResponse`.

## Implementation Status

| RPC | Status | Notes |
|---|---|---|
| `LogQso` | ✅ Implemented | Saves locally through the configured backend; QRZ sync still reports unimplemented when requested |
| `UpdateQso` | ✅ Implemented | Updates local storage; QRZ sync still reports unimplemented when requested |
| `DeleteQso` | ✅ Implemented | Deletes from local storage; QRZ delete still reports unimplemented when requested |
| `GetQso` | ✅ Implemented | Loads a single local QSO by `local_id` |
| `ListQsos` | ✅ Implemented | Streams locally stored QSOs with filters, sorting, limit, and offset |
| `SyncWithQrz` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `GetSyncStatus` | ✅ Implemented | Returns live local counts from storage; QRZ-specific fields remain zero/absent until sync is implemented |
| `ImportAdif` | ✅ Implemented | Streams ADIF in, imports after client close, reports duplicates/fallback warnings |
| `ExportAdif` | ✅ Implemented | Streams filtered ADIF out in chronological order |

## RPCs

### LogQso

Log a new QSO (contact). Optionally syncs the new record to QRZ immediately.

```
rpc LogQso(LogQsoRequest) returns (LogQsoResponse)
```

> ✅ **Status:** Implemented for local storage.

**Request:** `LogQsoRequest`

| Field | Type | Description |
|---|---|---|
| `qso` | `QsoRecord` | The QSO to log. `local_id` should be empty; the engine assigns a UUID. |
| `sync_to_qrz` | `bool` | If `true`, also upload to QRZ logbook immediately after logging locally. |

**Response:** `LogQsoResponse`

| Field | Type | Description |
|---|---|---|
| `local_id` | `string` | Engine-assigned UUID for the new QSO |
| `qrz_logid` | `string` (optional) | QRZ logbook record ID, set only when `sync_to_qrz` was `true` and sync succeeded |
| `sync_success` | `bool` | `true` when the local save completed and no QRZ request failed; `false` when QRZ work was requested but is not yet implemented |
| `sync_error` | `string` (optional) | Human-readable sync error message when `sync_to_qrz == true` and the QRZ step failed |

**Behavior:**
- The engine always assigns a new `local_id` (UUID). Do not set `QsoRecord.local_id` in the request.
- Required user input is `worked_callsign`, `utc_timestamp`, `band`, and `mode`, plus `station_callsign` unless the effective active station context already supplies the local station identity.
- When active station context is available, the server materializes `station_snapshot` from that context and uses it as the source of default local-station values for the new record.
- If `sync_to_qrz == false`, the QSO is logged locally only. `sync_success` will be `true` and QRZ fields remain absent.
- A QRZ sync failure does not cause the local log to fail. The QSO is logged locally regardless. Check `sync_success` and `sync_error` to determine the QRZ outcome.

**Notable status codes:**
- `INVALID_ARGUMENT` — missing required fields or invalid enum values.

---

### UpdateQso

Update an existing QSO identified by `local_id`.

```
rpc UpdateQso(UpdateQsoRequest) returns (UpdateQsoResponse)
```

> ✅ **Status:** Implemented for local storage.

**Request:** `UpdateQsoRequest`

| Field | Type | Description |
|---|---|---|
| `qso` | `QsoRecord` | Updated QSO. `local_id` must be set to identify the record. |
| `sync_to_qrz` | `bool` | If `true`, also update the record in QRZ logbook. |

**Response:** `UpdateQsoResponse`

| Field | Type | Description |
|---|---|---|
| `success` | `bool` | Whether the local update succeeded |
| `error` | `string` (optional) | Error message when `success == false` |
| `sync_success` | `bool` | Whether the optional QRZ sync succeeded |
| `sync_error` | `string` (optional) | Sync error message |

**Notable status codes:**
- `NOT_FOUND` — `local_id` does not exist in the local logbook.
- `INVALID_ARGUMENT` — the request is missing `local_id` or other required fields.

---

### DeleteQso

Delete a QSO from the local logbook. Optionally also deletes it from QRZ logbook.

```
rpc DeleteQso(DeleteQsoRequest) returns (DeleteQsoResponse)
```

> ✅ **Status:** Implemented for local storage.

**Request:** `DeleteQsoRequest`

| Field | Type | Description |
|---|---|---|
| `local_id` | `string` | UUID of the QSO to delete |
| `delete_from_qrz` | `bool` | If `true`, also delete the record from QRZ logbook (**permanent**, cannot be undone) |

**Response:** `DeleteQsoResponse`

| Field | Type | Description |
|---|---|---|
| `success` | `bool` | Whether the local delete succeeded |
| `error` | `string` (optional) | Error message when `success == false` |
| `qrz_delete_success` | `bool` | Whether the optional QRZ delete succeeded |
| `qrz_delete_error` | `string` (optional) | QRZ delete error message |

> ⚠️ **Warning:** Setting `delete_from_qrz = true` is **permanent and irreversible** on the QRZ side. Prompt the user to confirm before calling this with `delete_from_qrz = true`.

**Notable status codes:**
- `NOT_FOUND` — `local_id` does not exist.
- `INVALID_ARGUMENT` — `local_id` is blank.

---

### GetQso

Retrieve a single QSO by its local UUID.

```
rpc GetQso(GetQsoRequest) returns (GetQsoResponse)
```

> ✅ **Status:** Implemented for local storage.

**Request:** `GetQsoRequest`

| Field | Type | Description |
|---|---|---|
| `local_id` | `string` | UUID of the QSO to retrieve |

**Response:** `GetQsoResponse`

| Field | Type | Description |
|---|---|---|
| `qso` | `QsoRecord` | The retrieved QSO record |

**Notable status codes:**
- `NOT_FOUND` — `local_id` does not exist.
- `INVALID_ARGUMENT` — `local_id` is blank.

---

### ListQsos

List QSOs with optional filters, returning results as a server-streaming response.

```
rpc ListQsos(ListQsosRequest) returns (stream ListQsosResponse)
```

> ✅ **Status:** Implemented for local storage.

**Request:** `ListQsosRequest`

| Field | Type | Description |
|---|---|---|
| `after` | `Timestamp` (optional) | Include only QSOs with `utc_timestamp` after this time |
| `before` | `Timestamp` (optional) | Include only QSOs with `utc_timestamp` before this time |
| `callsign_filter` | `string` (optional) | Filter by `worked_callsign` (exact match) |
| `band_filter` | `Band` (optional) | Filter by band |
| `mode_filter` | `Mode` (optional) | Filter by mode |
| `contest_id` | `string` (optional) | Filter by contest ID |
| `limit` | `uint32` | Maximum records to return; `0` means no limit |
| `offset` | `uint32` | Skip this many records (for pagination) |
| `sort` | `QsoSortOrder` | `QSO_SORT_ORDER_NEWEST_FIRST` (default) or `QSO_SORT_ORDER_OLDEST_FIRST` |

**Response stream:** Zero or more `ListQsosResponse` messages, then stream close.

| Field | Type | Description |
|---|---|---|
| `qso` | `QsoRecord` | One matched QSO per streamed envelope |

**Behavior:**
- Results are streamed as they are produced, rather than buffered and returned in a single message. Clients should consume incrementally.
- All filter fields are optional; omitting all filters returns all QSOs (subject to `limit`/`offset`).

**Notable status codes:**
- `OK` — zero or more `ListQsosResponse` envelopes streamed back.

---

### SyncWithQrz

Trigger a full or incremental sync with the QRZ logbook. Progress is streamed back to the client.

```
rpc SyncWithQrz(SyncWithQrzRequest) returns (stream SyncWithQrzResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `SyncWithQrzRequest`

| Field | Type | Description |
|---|---|---|
| `full_sync` | `bool` | `true` = re-fetch all records from QRZ; `false` = incremental (changes since last sync) |

**Response stream:** One or more `SyncWithQrzResponse` messages, terminated by a message with `complete == true`.

**`SyncWithQrzResponse` fields:**

| Field | Type | Description |
|---|---|---|
| `total_records` | `uint32` | Total records to process |
| `processed_records` | `uint32` | Records processed so far |
| `uploaded_records` | `uint32` | Records pushed to QRZ |
| `downloaded_records` | `uint32` | Records fetched from QRZ |
| `conflict_records` | `uint32` | Records with local/remote divergence |
| `current_action` | `string` (optional) | Human-readable status message |
| `complete` | `bool` | `true` on the final message — stream ends after this |
| `error` | `string` (optional) | Error message if sync failed |

**Behavior:**
- The server closes the stream after sending a message with `complete == true`.
- Clients should update progress UI on each received message.
- A QRZ credentials error will produce an early terminal message with `complete == true` and `error` set.

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.
- `UNAUTHENTICATED` — future: QRZ credentials not configured or invalid.

---

### GetSyncStatus

Get the current sync state and logbook statistics.

```
rpc GetSyncStatus(GetSyncStatusRequest) returns (GetSyncStatusResponse)
```

> ✅ **Status:** Implemented for local storage counts. QRZ metadata remains zero/absent until QRZ sync is implemented.

**Request:** `GetSyncStatusRequest` — empty message, no fields.

**Response:** `GetSyncStatusResponse`

| Field | Type | Description |
|---|---|---|
| `local_qso_count` | `uint32` | Number of QSOs in the local logbook |
| `qrz_qso_count` | `uint32` | Number of QSOs reported by QRZ (from `STATUS` command) |
| `pending_upload` | `uint32` | Local QSOs not yet synced to QRZ |
| `last_sync` | `Timestamp` (optional) | Timestamp of the most recent successful sync |
| `qrz_logbook_owner` | `string` (optional) | QRZ logbook owner callsign |

**Current behavior:** `local_qso_count` and `pending_upload` are derived from current storage contents. Until QRZ sync exists, `qrz_qso_count` remains `0`, `last_sync` is absent, and `qrz_logbook_owner` is absent.

**Notable status codes:**
- `OK` — always returned; check field values for substantive data.

---

### ImportAdif

Import QSOs from ADIF data. The client streams chunks of raw ADIF bytes; the server parses and imports them.

```
rpc ImportAdif(stream ImportAdifRequest) returns (ImportAdifResponse)
```

> ✅ **Status:** Implemented for local ADIF migration import.

**Request stream:** One or more `ImportAdifRequest` messages, each containing one `AdifChunk`.

| Field | Type | Description |
|---|---|---|
| `chunk` | `AdifChunk` | Wrapper envelope for one raw ADIF byte slice |

**Behavior:**
- Clients may split large ADIF files into multiple chunks to avoid large single messages.
- The server accumulates chunks and parses the complete ADIF payload after the client closes the send side.
- Imported `STATION_CALLSIGN`, `OPERATOR`, and `MY_*` fields are preserved through `station_snapshot`; the current active station profile does **not** overwrite imported local-station history.
- If an ADIF record has no local-station context at all, the server uses the current active station profile as an explicit fallback and adds a warning describing that fallback.
- Duplicate policy: a record is skipped when it matches an existing QSO on `station_callsign`, `worked_callsign`, `utc_timestamp`, `band`, `mode`, and compatible `submode` / `frequency_khz`.
- Invalid core ADIF values such as unknown `BAND`, unknown `MODE`, or invalid `QSO_DATE` / `TIME_ON` combinations are skipped with warnings. Raw ADIF values are still retained in `extra_fields` so later exports stay predictable.

**Response:** `ImportAdifResponse`

| Field | Type | Description |
|---|---|---|
| `records_imported` | `uint32` | Number of QSOs successfully imported |
| `records_skipped` | `uint32` | Number of records skipped (duplicates or parse errors) |
| `warnings` | `repeated string` | Human-readable warnings for individual record issues |

**Notable status codes:**
- `OK` — import completed; inspect counts and warnings for duplicates or skipped records.
- `INVALID_ARGUMENT` — malformed ADIF payload that could not be parsed.
- `INTERNAL` — storage failure during import.

---

### ExportAdif

Export QSOs to ADIF format. The server streams chunks of raw ADIF bytes back to the client.

```
rpc ExportAdif(ExportAdifRequest) returns (stream ExportAdifResponse)
```

> ✅ **Status:** Implemented for local ADIF export.

**Request:** `ExportAdifRequest`

| Field | Type | Description |
|---|---|---|
| `after` | `Timestamp` (optional) | Export only QSOs after this time |
| `before` | `Timestamp` (optional) | Export only QSOs before this time |
| `contest_id` | `string` (optional) | Export only QSOs for a specific contest |
| `include_header` | `bool` | Whether to include the ADIF file header with version/program info |

**Response stream:** One or more `ExportAdifResponse` messages containing one `AdifChunk`, then stream close.

| Field | Type | Description |
|---|---|---|
| `chunk` | `AdifChunk` | Wrapper envelope for one exported ADIF byte slice |

**Behavior:**
- Clients should concatenate received chunks to reconstruct the full ADIF payload.
- Omitting all filters exports all QSOs.
- Export order is chronological (`oldest first`) after applying the filters.
- `include_header=true` prepends an ADIF header with version/program metadata.

**Notable status codes:**
- `OK` — export stream opened successfully.
- `INTERNAL` — storage failure while enumerating records for export.

---

## QsoRecord Key Fields

| Field | Required? | Description |
|---|---|---|
| `local_id` | Assigned by engine | UUID — do not set in `LogQso` requests |
| `station_callsign` | Required | Local operator's callsign |
| `worked_callsign` | Required | Remote station's callsign |
| `utc_timestamp` | Required | UTC time of the contact |
| `band` | Required | Frequency band (see `Band` enum) |
| `mode` | Required | Operating mode (see `Mode` enum) |
| `frequency_khz` | Optional | Precise frequency in kHz |
| `submode` | Optional | ADIF submode string (e.g., `"USB"`, `"PSK31"`) |
| `rst_sent` / `rst_received` | Optional | RST signal reports |
| `sync_status` | Set by engine | `LOCAL_ONLY → SYNCED → MODIFIED → CONFLICT` |
| `station_snapshot` | Optional | Immutable local-station metadata captured when the QSO was logged |
| `extra_fields` | Optional | ADIF fields with no dedicated proto field — preserved for lossless round-trip |

## QsoSortOrder Values

| Value | Description |
|---|---|
| `QSO_SORT_ORDER_NEWEST_FIRST` | Default (zero value) — most recent QSOs first |
| `QSO_SORT_ORDER_OLDEST_FIRST` | Oldest QSOs first |
