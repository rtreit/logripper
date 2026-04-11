# Workflow Examples

This page provides example request/response shapes and state transitions for common LogRipper engine workflows. These are intended to make integration concrete rather than requiring you to derive behavior from raw proto names.

All examples show field values in a language-neutral pseudo-JSON format. Actual wire encoding is binary protobuf.

## LookupService Workflows

### Unary Callsign Lookup

The simplest lookup: send one `LookupRequest`, receive one `LookupResult`.

**Request:**
```json
{
  "callsign": "W1AW",
  "skip_cache": false
}
```

**Response (found):**
```json
{
  "state": "LOOKUP_STATE_FOUND",
  "record": {
    "callsign": "W1AW",
    "first_name": "Hiram",
    "last_name": "Maxim",
    "country": "United States",
    "grid_square": "FN31pr",
    "cq_zone": 5,
    "itu_zone": 8
  },
  "cache_hit": false,
  "lookup_latency_ms": 230,
  "queried_callsign": "W1AW"
}
```

**Response (not found):**
```json
{
  "state": "LOOKUP_STATE_NOT_FOUND",
  "cache_hit": false,
  "lookup_latency_ms": 180,
  "queried_callsign": "NOTACALL"
}
```

**Response (provider error):**
```json
{
  "state": "LOOKUP_STATE_ERROR",
  "error_message": "Provider configuration error: QRZ credentials not set",
  "cache_hit": false,
  "lookup_latency_ms": 0,
  "queried_callsign": "W1AW"
}
```

---

### Streaming Lookup

The streaming variant is ideal for TUI/GUI clients that want to show a loading indicator while the lookup is in flight.

**Request:**
```json
{
  "callsign": "K7ABC",
  "skip_cache": false
}
```

**Stream (fresh lookup, no prior cache entry):**

Message 1 — emitted immediately:
```json
{
  "state": "LOOKUP_STATE_LOADING",
  "queried_callsign": "K7ABC"
}
```

Message 2 — emitted after provider responds:
```json
{
  "state": "LOOKUP_STATE_FOUND",
  "record": { "callsign": "K7ABC", ... },
  "cache_hit": false,
  "lookup_latency_ms": 310,
  "queried_callsign": "K7ABC"
}
```
*Stream closes after message 2.*

**Stream (stale cache hit — returns cached data while refreshing):**

Message 1:
```json
{ "state": "LOOKUP_STATE_LOADING", "queried_callsign": "K7ABC" }
```

Message 2 — stale entry returned immediately:
```json
{
  "state": "LOOKUP_STATE_STALE",
  "record": { "callsign": "K7ABC", ... },
  "cache_hit": true,
  "queried_callsign": "K7ABC"
}
```

Message 3 — fresh result replaces stale entry:
```json
{
  "state": "LOOKUP_STATE_FOUND",
  "record": { "callsign": "K7ABC", ... },
  "cache_hit": false,
  "lookup_latency_ms": 290,
  "queried_callsign": "K7ABC"
}
```
*Stream closes after message 3.*

**UI integration pattern:**

```
on(LOADING)    → show spinner
on(STALE)      → show cached data + "refreshing..." indicator
on(FOUND)      → show result, hide spinner
on(NOT_FOUND)  → show "not found" feedback, hide spinner
on(ERROR)      → show error message, hide spinner
```

---

### Cache-Only Lookup

Use `GetCachedCallsign` when you want a zero-latency check without triggering a network call.

**Request:**
```json
{ "callsign": "W1AW" }
```

**Response (cache hit):**
```json
{
  "state": "LOOKUP_STATE_FOUND",
  "record": { "callsign": "W1AW", ... },
  "cache_hit": true,
  "lookup_latency_ms": 0,
  "queried_callsign": "W1AW"
}
```

**Response (not cached):**
```json
{
  "state": "LOOKUP_STATE_NOT_FOUND",
  "cache_hit": false,
  "lookup_latency_ms": 0,
  "queried_callsign": "W1AW"
}
```

**Recommended type-ahead pattern:**

1. On each keystroke, call `GetCachedCallsign` for a zero-latency cache check.
2. If the result is `FOUND` or `STALE`, display it immediately — no network call needed.
3. After a short debounce (typing has stabilized), call `StreamLookup` to fetch a fresh result.
4. Update the display when the stream emits `FOUND`, `NOT_FOUND`, or `ERROR`.

Calling `StreamLookup` on every keystroke without debounce would generate unnecessary network traffic. Wait for typing to stabilize before firing a provider lookup, and cancel in-flight streams when a newer request supersedes them.

---

## LogbookService Workflows

> **Note:** Local `LogbookService` CRUD, status, and ADIF import/export flows are implemented today. QRZ sync remains unimplemented; see the [LogbookService Reference](logbook-service.md) for the current status table.

---

### Log a New QSO

**Request:**
```json
{
  "qso": {
    "worked_callsign": "K7ABC",
    "utc_timestamp": "2025-06-15T18:32:00Z",
    "band": "BAND_20M",
    "mode": "MODE_SSB",
    "submode": "USB",
    "rst_sent": { "readability": 5, "strength": 9, "raw": "59" },
    "rst_received": { "readability": 5, "strength": 8, "raw": "58" },
    "notes": "Nice signal on 14.225"
  },
  "sync_to_qrz": false
}
```

> When an effective active station context exists, `station_callsign` can be omitted from `LogQso` and will be derived from that active context. If no active context exists, the request must still provide `station_callsign` explicitly.

**Response:**
```json
{
  "local_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "sync_success": false
}
```

**Response (with immediate QRZ sync):**
```json
{
  "local_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "qrz_logid": "1234567890",
  "sync_success": true
}
```

---

### Update a QSO

**Request:**
```json
{
  "qso": {
    "local_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "station_callsign": "W1AW",
    "worked_callsign": "K7ABC",
    "utc_timestamp": "2025-06-15T18:32:00Z",
    "band": "BAND_20M",
    "mode": "MODE_SSB",
    "submode": "USB",
    "notes": "Updated note after QSO confirmed"
  },
  "sync_to_qrz": false
}
```

**Response:**
```json
{
  "success": true,
  "sync_success": false
}
```

---

### List QSOs

Stream all 20m SSB QSOs from the past week, newest first:

**Request:**
```json
{
  "after": "2025-06-08T00:00:00Z",
  "band_filter": "BAND_20M",
  "mode_filter": "MODE_SSB",
  "sort": "SORT_ORDER_NEWEST_FIRST",
  "limit": 100,
  "offset": 0
}
```

**Response stream:** Zero or more `QsoRecord` messages, one per matched QSO, then stream close.

---

### QRZ Sync Flow

Trigger a full sync and monitor progress:

**Request:**
```json
{ "full_sync": true }
```

**Progress stream:**

```json
{ "total_records": 0, "processed_records": 0, "current_action": "Connecting to QRZ..." }
{ "total_records": 500, "processed_records": 0, "current_action": "Fetching QRZ logbook..." }
{ "total_records": 500, "processed_records": 100, "downloaded_records": 100 }
{ "total_records": 500, "processed_records": 250, "downloaded_records": 250 }
{ "total_records": 500, "processed_records": 500, "downloaded_records": 498, "conflict_records": 2, "complete": true }
```

The stream closes after the message with `complete: true`.

**Error case:**
```json
{ "complete": true, "error": "QRZ authentication failed: invalid logbook API key" }
```

---

### ADIF Import

Stream an ADIF file in chunks (client-streaming):

**Request stream:**

Chunk 1:
```json
{ "data": "<ADIF header bytes>" }
```

Chunk 2:
```json
{ "data": "<QSO records bytes>" }
```

Client closes the send side after all chunks are sent.

**Response (after client closes stream):**
```json
{
  "records_imported": 42,
  "records_skipped": 2,
  "warnings": [
    "Record 17: local-station history was absent in ADIF; applied active station profile 'Home'.",
    "Record 38: duplicate skipped; matched an existing QSO on station_callsign, worked_callsign, utc_timestamp, band, mode, and compatible submode/frequency."
  ]
}
```

Notes:
- The server buffers all incoming chunks, parses only after the client closes the stream, then imports records.
- Imported `STATION_CALLSIGN`, `OPERATOR`, and `MY_*` fields are preserved as historical `station_snapshot` data.
- The active station profile is only used when the ADIF record has no local-station context at all, and that fallback is reported in `warnings`.
- Records with unrecognized core ADIF values such as `BAND`, `MODE`, or invalid `QSO_DATE`/`TIME_ON` are skipped with warnings; the raw ADIF values remain in `extra_fields` for round-trip export fidelity.

---

### ADIF Export

Export all QSOs between two dates:

**Request:**
```json
{
  "after": "2025-01-01T00:00:00Z",
  "before": "2025-12-31T23:59:59Z",
  "include_header": true
}
```

**Response stream:** One or more `AdifChunk` messages containing raw ADIF bytes, then stream close.

Clients should concatenate chunk data in order to reconstruct the complete ADIF file.

Notes:
- Filters already present in `ExportRequest` (`after`, `before`, `contest_id`) are applied before serialization.
- Export order is chronological (`oldest first`) for predictable migration output.
- When `include_header` is `true`, the payload starts with an ADIF header containing the LogRipper program metadata.

---

## Sync Status Check

Use `GetSyncStatus` to verify engine connectivity and check logbook statistics:

**Request:** *(empty)*

**Response:**
```json
{
  "local_qso_count": 1234,
  "qrz_qso_count": 1230,
  "pending_upload": 4,
  "last_sync": "2025-06-15T12:00:00Z",
  "qrz_logbook_owner": "W1AW"
}
```

> Current server reports live local counts from the configured backend. Until QRZ sync is implemented, QRZ-specific fields remain `0` or absent.
