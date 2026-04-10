# LogbookService Reference

The `LogbookService` is the core QSO lifecycle interface. It covers logging new contacts, editing or deleting existing ones, syncing with the QRZ logbook, and ADIF import/export.

Proto definition: [`proto/services/logbook_service.proto`](../../proto/services/logbook_service.proto)

Domain types: [`proto/domain/qso.proto`](../../proto/domain/qso.proto)

## Implementation Status

| RPC | Status | Notes |
|---|---|---|
| `LogQso` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `UpdateQso` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `DeleteQso` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `GetQso` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `ListQsos` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `SyncWithQrz` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `GetSyncStatus` | ✅ Partial | Returns zeroed placeholder values (storage not yet wired) |
| `ImportAdif` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |
| `ExportAdif` | ⚠️ Planned | Contract defined; returns `UNIMPLEMENTED` |

## RPCs

### LogQso

Log a new QSO (contact). Optionally syncs the new record to QRZ immediately.

```
rpc LogQso(LogQsoRequest) returns (LogQsoResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

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
| `sync_success` | `bool` | Whether the optional QRZ sync succeeded (always `false` when `sync_to_qrz` was `false`) |
| `sync_error` | `string` (optional) | Human-readable sync error message when `sync_success == false` and sync was requested |

**Behavior:**
- The engine always assigns a new `local_id` (UUID). Do not set `QsoRecord.local_id` in the request.
- If `sync_to_qrz == false`, the QSO is logged locally only. `sync_success` will be `false` and `qrz_logid` will be absent.
- A QRZ sync failure does not cause the local log to fail. The QSO is logged locally regardless. Check `sync_success` and `sync_error` to determine the sync outcome.

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.
- `INVALID_ARGUMENT` — future: missing required fields (`station_callsign`, `worked_callsign`, `utc_timestamp`, `band`, `mode`).

---

### UpdateQso

Update an existing QSO identified by `local_id`.

```
rpc UpdateQso(UpdateQsoRequest) returns (UpdateQsoResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

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
- `UNIMPLEMENTED` — current server response.
- `NOT_FOUND` — future: `local_id` does not exist in the local logbook.

---

### DeleteQso

Delete a QSO from the local logbook. Optionally also deletes it from QRZ logbook.

```
rpc DeleteQso(DeleteQsoRequest) returns (DeleteQsoResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

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
- `UNIMPLEMENTED` — current server response.
- `NOT_FOUND` — future: `local_id` does not exist.

---

### GetQso

Retrieve a single QSO by its local UUID.

```
rpc GetQso(GetQsoRequest) returns (GetQsoResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `GetQsoRequest`

| Field | Type | Description |
|---|---|---|
| `local_id` | `string` | UUID of the QSO to retrieve |

**Response:** `GetQsoResponse`

| Field | Type | Description |
|---|---|---|
| `qso` | `QsoRecord` | The retrieved QSO record |

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.
- `NOT_FOUND` — future: `local_id` does not exist.

---

### ListQsos

List QSOs with optional filters, returning results as a server-streaming response.

```
rpc ListQsos(ListQsosRequest) returns (stream QsoRecord)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

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
| `sort` | `SortOrder` | `SORT_ORDER_NEWEST_FIRST` (default) or `SORT_ORDER_OLDEST_FIRST` |

**Response stream:** Zero or more `QsoRecord` messages, then stream close.

**Behavior:**
- Results are streamed as they are produced, rather than buffered and returned in a single message. Clients should consume incrementally.
- All filter fields are optional; omitting all filters returns all QSOs (subject to `limit`/`offset`).

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.

---

### SyncWithQrz

Trigger a full or incremental sync with the QRZ logbook. Progress is streamed back to the client.

```
rpc SyncWithQrz(SyncRequest) returns (stream SyncProgress)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `SyncRequest`

| Field | Type | Description |
|---|---|---|
| `full_sync` | `bool` | `true` = re-fetch all records from QRZ; `false` = incremental (changes since last sync) |

**Response stream:** One or more `SyncProgress` messages, terminated by a message with `complete == true`.

**`SyncProgress` fields:**

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
rpc GetSyncStatus(SyncStatusRequest) returns (SyncStatusResponse)
```

> ✅ **Status:** Partial. Returns zeroed placeholder values — storage is not yet wired to persistent state.

**Request:** `SyncStatusRequest` — empty message, no fields.

**Response:** `SyncStatusResponse`

| Field | Type | Description |
|---|---|---|
| `local_qso_count` | `uint32` | Number of QSOs in the local logbook |
| `qrz_qso_count` | `uint32` | Number of QSOs reported by QRZ (from `STATUS` command) |
| `pending_upload` | `uint32` | Local QSOs not yet synced to QRZ |
| `last_sync` | `Timestamp` (optional) | Timestamp of the most recent successful sync |
| `qrz_logbook_owner` | `string` (optional) | QRZ logbook owner callsign |

**Current behavior:** All fields return `0` or absent — the engine server is not yet wired to a persistent storage backend. Use `GetSyncStatus` to verify transport connectivity; do not rely on field values for application logic yet.

**Notable status codes:**
- `OK` — always returned; check field values for substantive data.

---

### ImportAdif

Import QSOs from ADIF data. The client streams chunks of raw ADIF bytes; the server parses and imports them.

```
rpc ImportAdif(stream AdifChunk) returns (ImportResult)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request stream:** One or more `AdifChunk` messages, each containing a slice of raw ADIF bytes.

| Field | Type | Description |
|---|---|---|
| `data` | `bytes` | Raw ADIF text bytes for this chunk |

**Behavior:**
- Clients may split large ADIF files into multiple chunks to avoid large single messages.
- The server accumulates chunks and parses the complete ADIF payload after the client closes the send side.

**Response:** `ImportResult`

| Field | Type | Description |
|---|---|---|
| `records_imported` | `uint32` | Number of QSOs successfully imported |
| `records_skipped` | `uint32` | Number of records skipped (duplicates or parse errors) |
| `warnings` | `repeated string` | Human-readable warnings for individual record issues |

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.
- `INVALID_ARGUMENT` — future: malformed ADIF data.

---

### ExportAdif

Export QSOs to ADIF format. The server streams chunks of raw ADIF bytes back to the client.

```
rpc ExportAdif(ExportRequest) returns (stream AdifChunk)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `ExportRequest`

| Field | Type | Description |
|---|---|---|
| `after` | `Timestamp` (optional) | Export only QSOs after this time |
| `before` | `Timestamp` (optional) | Export only QSOs before this time |
| `contest_id` | `string` (optional) | Export only QSOs for a specific contest |
| `include_header` | `bool` | Whether to include the ADIF file header with version/program info |

**Response stream:** One or more `AdifChunk` messages containing raw ADIF bytes, then stream close.

**Behavior:**
- Clients should concatenate received chunks to reconstruct the full ADIF payload.
- Omitting all filters exports all QSOs.

**Notable status codes:**
- `UNIMPLEMENTED` — current server response.

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
| `extra_fields` | Optional | ADIF fields with no dedicated proto field — preserved for lossless round-trip |

## SortOrder Values

| Value | Description |
|---|---|
| `SORT_ORDER_NEWEST_FIRST` | Default (zero value) — most recent QSOs first |
| `SORT_ORDER_OLDEST_FIRST` | Oldest QSOs first |
