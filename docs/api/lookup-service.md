# LookupService Reference

The `LookupService` is the app-facing callsign lookup interface. It maps to the lookup architecture:

```
Client → LookupService → LookupCoordinator → CallsignProvider → QrzProvider
```

Proto definition: [`proto/services/lookup_service.proto`](../../proto/services/lookup_service.proto)

Domain types: [`proto/domain/lookup.proto`](../../proto/domain/lookup.proto), [`proto/domain/callsign.proto`](../../proto/domain/callsign.proto)

## Implementation Status

| RPC | Status | Notes |
|---|---|---|
| `Lookup` | ✅ Implemented | Unary callsign lookup via coordinator |
| `StreamLookup` | ✅ Implemented | Server-streaming with `Loading → Found/Error` state transitions |
| `GetCachedCallsign` | ✅ Implemented | L1 in-memory cache check only, no network call |
| `GetDxccEntity` | ⚠️ Planned | Returns `UNIMPLEMENTED` — deferred from first lookup slice |
| `BatchLookup` | ⚠️ Planned | Returns `UNIMPLEMENTED` — deferred from first lookup slice |

## RPCs

### Lookup

Single unary callsign lookup. Resolves through the cache then provider.

```
rpc Lookup(LookupRequest) returns (LookupResult)
```

**Request:** `LookupRequest`

| Field | Type | Description |
|---|---|---|
| `callsign` | `string` | Callsign to look up (e.g., `"W1AW"`) |
| `skip_cache` | `bool` | If `true`, bypasses the L1 in-memory cache and forces a fresh provider fetch |

**Response:** `LookupResult`

| Field | Type | Description |
|---|---|---|
| `state` | `LookupState` | Terminal state of the lookup (`FOUND`, `NOT_FOUND`, `ERROR`) |
| `record` | `CallsignRecord` (optional) | Populated when `state == FOUND` |
| `error_message` | `string` (optional) | Human-readable error when `state == ERROR` |
| `cache_hit` | `bool` | Whether the result was served from cache |
| `lookup_latency_ms` | `uint32` | Round-trip latency in milliseconds |
| `queried_callsign` | `string` | Echo of the original requested callsign |
| `debug_http_exchanges` | `DebugHttpExchange[]` | Redacted provider request/response capture for debugging provider-backed lookups |

**Behavior:**
- Always returns a single `LookupResult`. The state field signals the outcome.
- If the provider is not configured (no QRZ credentials), returns `state == ERROR` with a configuration error message.
- If the callsign is in the L1 cache and `skip_cache` is false, serves the cached result with `cache_hit == true`.
- Provider-backed results may include redacted `debug_http_exchanges` entries for login and lookup HTTP calls. Cache-only responses leave this list empty.

**Debug capture payload:**

Each `DebugHttpExchange` is an additive, provider-agnostic transport capture with:

- `provider_name`
- `operation`
- `started_at_utc`
- `duration_ms`
- `attempt`
- `method`
- `url`
- `request_headers`
- `request_body` (optional)
- `response_status_code` (optional)
- `response_headers`
- `response_body` (optional)
- `error_message` (optional)

Sensitive values are redacted before the exchange is returned to clients. For QRZ XML this includes session keys, passwords, tokens, and auth/cookie-style headers.

**Notable status codes:**
- `OK` — returned in all cases (including not-found); the `state` field carries the semantic outcome.
- `INTERNAL` — unexpected server-side error (rare; most errors are expressed in `LookupResult.state`).

---

### StreamLookup

Server-streaming lookup that emits progressive state updates as the lookup progresses.

```
rpc StreamLookup(LookupRequest) returns (stream LookupResult)
```

**Request:** `LookupRequest` — same as `Lookup`.

**Response stream:** One or more `LookupResult` messages, terminated by the server.

**State transition sequence:**

```
LOADING → (STALE)? → FOUND | NOT_FOUND | ERROR
```

1. The server always emits a `LOADING` result first, so the client can show an in-progress indicator immediately.
2. If a stale cached entry exists while a fresh fetch is in progress, the server may emit a `STALE` result with the cached `record` before the final result arrives.
3. The stream closes after the terminal result (`FOUND`, `NOT_FOUND`, or `ERROR`).

**Typical stream for a fresh lookup (no cache):**
```
{ state: LOADING, queried_callsign: "W1AW" }
{ state: FOUND,   queried_callsign: "W1AW", record: { ... }, lookup_latency_ms: 240 }
```

**Typical stream for a cache hit:**
```
{ state: LOADING, queried_callsign: "W1AW" }
{ state: FOUND,   queried_callsign: "W1AW", record: { ... }, cache_hit: true, lookup_latency_ms: 1 }
```

**Use case:** TUI/GUI clients that want to show an in-progress spinner while the lookup is running. Subscribe to the stream and update the UI on each received `LookupResult`.

**Notable status codes:**
- `OK` — stream completed normally.
- `CANCELLED` — client cancelled the stream (expected for type-ahead debounce scenarios).

---

### GetCachedCallsign

Returns the cached `LookupResult` for a callsign without making any network call.

```
rpc GetCachedCallsign(CachedCallsignRequest) returns (LookupResult)
```

**Request:** `CachedCallsignRequest`

| Field | Type | Description |
|---|---|---|
| `callsign` | `string` | Callsign to check in the L1 cache |

**Response:** `LookupResult`

- If the callsign is in the L1 cache: returns `state == FOUND` (or the cached state), `cache_hit == true`.
- If the callsign is not cached: returns `state == NOT_FOUND`, `cache_hit == false`.

**No network calls are made.** This RPC is safe to call speculatively and at high frequency.

**Use case:** Type-ahead display that first tries the cache for a zero-latency response, then optionally falls through to `StreamLookup` for a fresh result.

**Notable status codes:**
- `OK` — always returned; outcome is in `LookupResult.state`.

---

### GetDxccEntity

Look up a DXCC (DX Century Club) entity by numeric code or callsign prefix.

```
rpc GetDxccEntity(DxccRequest) returns (DxccEntity)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `DxccRequest` (oneof)

| Field | Type | Description |
|---|---|---|
| `dxcc_code` | `uint32` | Numeric DXCC entity code |
| `prefix` | `string` | Callsign prefix — QRZ performs a 4→3→2 letter reduction to find the entity |

**Response:** `DxccEntity`

| Field | Type | Description |
|---|---|---|
| `dxcc_code` | `uint32` | DXCC entity code |
| `country_name` | `string` | Full country name |
| `continent` | `string` | 2-letter continent code (NA, SA, EU, AF, AS, OC, AN) |
| `itu_zone` | `uint32` (optional) | ITU zone |
| `cq_zone` | `uint32` (optional) | CQ zone |
| `utc_offset` | `double` (optional) | UTC offset in hours |
| `latitude` | `double` (optional) | Entity reference latitude |
| `longitude` | `double` (optional) | Entity reference longitude |
| `notes` | `string` (optional) | Additional notes |

**Notable status codes:**
- `UNIMPLEMENTED` — current server response (planned feature).
- `NOT_FOUND` — expected future status code when the entity does not exist.

---

### BatchLookup

Look up multiple callsigns in a single request. Intended for contest prefetch scenarios.

```
rpc BatchLookup(BatchLookupRequest) returns (BatchLookupResponse)
```

> ⚠️ **Status:** Planned. Currently returns `UNIMPLEMENTED`.

**Request:** `BatchLookupRequest`

| Field | Type | Description |
|---|---|---|
| `callsigns` | `repeated string` | List of callsigns to look up |

**Response:** `BatchLookupResponse`

| Field | Type | Description |
|---|---|---|
| `results` | `repeated LookupResult` | One result per requested callsign, in request order |

**Use case:** Pre-populate the cache before a contest session begins by looking up a list of expected callsigns in one call.

**Notable status codes:**
- `UNIMPLEMENTED` — current server response (planned feature).

---

## LookupState Values

| Value | Meaning |
|---|---|
| `LOOKUP_STATE_UNSPECIFIED` | Default/zero value — should not appear in normal responses |
| `LOOKUP_STATE_LOADING` | Request is in flight; used as the initial `StreamLookup` emission |
| `LOOKUP_STATE_FOUND` | Callsign resolved successfully; `record` field is populated |
| `LOOKUP_STATE_NOT_FOUND` | Callsign does not exist in the provider |
| `LOOKUP_STATE_ERROR` | Provider error (network failure, auth failure, rate limit) |
| `LOOKUP_STATE_STALE` | Returning cached data while a background refresh is pending |
| `LOOKUP_STATE_CANCELLED` | Lookup was superseded by a newer request |

## Error Handling Notes

- `LookupService` returns gRPC `OK` for most responses; the semantic outcome is always in `LookupResult.state`.
- Treat `LOOKUP_STATE_ERROR` as a soft error — log the `error_message`, show feedback to the user, but do not crash. The provider may recover on the next request.
- `LOOKUP_STATE_STALE` means stale data is being returned while the cache refreshes. Display the stale data and update when the next result arrives.
- Clients should handle `CANCELLED` responses gracefully — they are expected during type-ahead debounce scenarios where older requests are abandoned.
