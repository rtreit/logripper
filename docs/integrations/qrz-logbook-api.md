# QRZ Logbook API - Integration Reference

Source specification: <https://www.qrz.com/docs/logbook/QRZLogbookAPI.html>

This document is a comprehensive development reference for QsoRipper's consumption of the QRZ Logbook REST API for inserting, fetching, deleting, and managing QSO records.

---

## Overview

The QRZ Logbook API provides an HTTP REST interface for external programs to interact with QRZ Logbook data. It is a combination free and paid subscription service. Some advanced features (INSERT, DELETE, STATUS, FETCH) require a valid subscription. All QRZ members can access, edit, update, and view their complete logs through the QRZ website regardless of subscription status.

---

## Logbook data model

### Core concepts

- Every QSO record has a unique integer **`logid`**.
- Every logbook has a unique integer **`bookid`** and belongs to a specific QRZ member.
- A logbook serves exactly **one callsign**. Every character in a callsign is significant, including portable/mobile identifiers. For example, `XX1XX` and `XX1XX/M` are separate callsigns requiring separate logbooks.
- When a user changes callsigns, a new logbook is opened. The user then has multiple logbooks (one per callsign).

### Required QSO fields for insertion

A QSO record requires these key attributes to be inserted:

1. The sending station's callsign (`station_callsign`)
2. The receiving station's callsign (`call`)
3. Date and time of the QSO (`qso_date`, `time_on`)
4. Frequency band (`band`)
5. Transmission mode (`mode`)

### Logbook date range

Each logbook has a configurable date range corresponding to when the callsign was/will be active (typically license effective through expiration date). **QSOs with dates outside this range will be rejected.**

### Access key model

- A QRZ member may have access to multiple logbooks.
- `bookid` values are never transmitted directly in the API.
- Instead, an opaque **API Access Key** provided by QRZ conveys both user identification and logbook routing.
- The access key for a given logbook is obtained through the QRZ website.

---

## Endpoint and protocol

| Property | Value |
|---|---|
| Endpoint | `https://logbook.qrz.com/api` |
| Method | HTTP `POST` |
| Request body | URL-encoded `name=value` pairs |
| Response body | URL-encoded `name=value` pairs |
| QSO data format | ADIF, sent in the `ADIF` parameter |

---

## Application identification requirements

All applications **must** provide an identifiable `User-Agent` HTTP header.

**Format guidance from QRZ:**

- Personal scripts: include your callsign and a unique script name, e.g. `QsoRipper/0.1.0 (AA7BQ)`
- Applications: `ApplicationName/version`, e.g. `QsoRipper/1.0.0`
- Maximum length: **128 characters**

Applications with missing or generic user agents (e.g. `node-fetch`, `python-requests`) may be subject to rate limiting or restrictions.

---

## Request parameters

Every API request must include `KEY` and `ACTION`. The server rejects requests containing unrecognized parameters.

### Request parameter types

| Parameter | Description |
|---|---|
| `KEY` | QRZ-supplied logbook access key |
| `ACTION` | Operation type: `INSERT`, `DELETE`, `STATUS`, `FETCH` |
| `ADIF` | ADIF-formatted QSO input data |
| `OPTION` | Action-specific options |
| `LOGIDS` | Comma-separated list of integer `logid` values |

### Response parameter types

| Parameter | Description |
|---|---|
| `RESULT` | `OK` on success, `FAIL` on failure, `AUTH` on insufficient privileges, or action-specific codes |
| `REASON` | Failure description (used with `RESULT=FAIL`) |
| `LOGIDS` | Comma-separated list of `logid` values affected by the action |
| `LOGID` | Single `logid` of inserted/replaced record (INSERT only, since it is a single-record operation) |
| `COUNT` | Number of QSO records affected by the action |
| `DATA` | Action-specific data payload (e.g. status reports) |

---

## API commands

### INSERT (subscription required)

Inserts one QSO record into the logbook selected by the API access key.

**Request:**

| Parameter | Value |
|---|---|
| `ACTION` | `INSERT` |
| `ADIF` | The ADIF data to be inserted |
| `OPTION` | _(optional)_ `REPLACE` to automatically overwrite any existing duplicate QSOs |

**Response:**

| Parameter | Values |
|---|---|
| `RESULT` | `OK` (inserted), `FAIL` (not inserted), `REPLACE` (duplicate overwritten) |
| `COUNT` | Number of records inserted or replaced (always 1 or 0) |
| `LOGID` | The `logid` of the inserted or replaced record |
| `REASON` | Error description (when `RESULT=FAIL`) |

**Example request body (URL-encoded in practice):**

```
KEY=ABCD-0A0B-1C1D-2E2F&ACTION=INSERT&ADIF=<band:3>80m<mode:3>SSB<call:4>XX1X<qso_date:8>20140121<station_callsign:5>AA7BQ<time_on:4>0346<eor>
```

**Example response:**

```
RESULT=OK&LOGID=130877825&COUNT=1
```

**Implementation warnings:**

- The `REPLACE` option **will overwrite confirmed QSOs** with the supplied unconfirmed QSO data until QRZ re-verifies the match. Treat this as a high-risk operation requiring explicit user intent.

---

### DELETE (subscription required)

Deletes one or more QSO records from the logbook selected by the API access key.

**Request:**

| Parameter | Value |
|---|---|
| `ACTION` | `DELETE` |
| `LOGIDS` | Comma-separated list of `logid` values to delete |

**Response:**

| Parameter | Values |
|---|---|
| `RESULT` | `OK` (all deleted), `PARTIAL` (some not found), `FAIL` (none found) |
| `LOGIDS` | Comma-separated list of `logid` values that were **not found** (only when `RESULT=PARTIAL`) |
| `COUNT` | Number of QSO records actually deleted |

**Critical warning:** This command **permanently deletes** records. There is **no undo**. Deleted records cannot be recovered. QsoRipper should require explicit user confirmation before executing DELETE operations.

---

### STATUS (subscription required)

Returns a status report for the logbook selected by the API access key.

**Request:**

| Parameter | Value |
|---|---|
| `ACTION` | `STATUS` |

**Response:**

| Parameter | Values |
|---|---|
| `RESULT` | `OK` (success), `FAIL` (invalid access key) |
| `DATA` | `&`-separated list of `name=value` pairs containing logbook status |

**DATA payload may include:** total QSOs in book, total confirmed, DXCC total, USA states total, start date, end date, book owner, bookid, book name, authorized users.

---

### FETCH (subscription required)

Fetches one or more QSO records from the logbook matching specified criteria.

**Request:**

| Parameter | Value |
|---|---|
| `ACTION` | `FETCH` |
| `OPTION` | Comma-separated filter options (see below) |

**FETCH option parameters:**

Options are sent as a comma-separated list of colon-separated `name:value` pairs with **no spaces**. Example: `BAND:80m,MODE:SSB,MAX:400`

| Option | Description |
|---|---|
| `ALL` | Fetch the entire logbook (default). When used, only `TYPE` and `STATUS` may also be specified. |
| `DXCC:nnn` | Fetch records with DXCC=nnn |
| `BETWEEN:2014-01-01+2014-01-31` | Fetch records between start and end dates (inclusive) |
| `MODSINCE:2023-01-01` | Only return records modified since this date |
| `AFTERLOGID:123123123` | Only return records with `app_qrzlog_logid` >= the given value |
| `BAND:xxx` | Fetch QSOs on the given band |
| `MODE:xxx` | Fetch QSOs with the given mode |
| `CALL:XX1XX` | Fetch QSOs with the indicated callsign |
| `LOGIDS:nnn+nnn+nnn` | Fetch specific records by logid list (plus-separated) |
| `MAX:nnnn` | Maximum number of records to return (0 = count only; unspecified = unlimited) |
| `TYPE:ADIF\|LOGIDS` | Response format: ADIF data (default) or logid list |
| `STATUS:CONFIRMED\|ALL` | Filter: confirmed records only, or all records (default: ALL) |

**Response:**

| Parameter | Values |
|---|---|
| `RESULT` | `OK` (matches found), `FAIL` (parameter or other problem) |
| `COUNT` | Total number of records matching selection criteria |
| `LOGIDS` | Comma-separated list of matching `logid` values (limited by `MAX`) |
| `ADIF` | ADIF data for matching QSOs (limited by `MAX`; returned when `TYPE` is `ADIF` or default) |

**Usage notes:**

- Multiple options may be combined, separated by `&` or `;` characters.
- `COUNT` always reflects the total match count for the given criteria regardless of `MAX`.
- To fetch **only the count**, set `MAX:0`.
- When `ALL` is specified, only `TYPE` and `STATUS` may accompany it.

### Recommended paging strategy

Large logbooks can cause timeouts if fetched in one request. Use bounded fetches:

1. Start with `MAX:250,AFTERLOGID:0`
2. If 250 records are returned, make another request with `AFTERLOGID` set to the highest `app_qrzlog_logid` value returned + 1
3. Repeat until fewer than 250 (or your `MAX`) records are returned

---

## Error handling policy for QsoRipper

| Scenario | Detection | QsoRipper behavior |
|---|---|---|
| Auth / privilege failure | `RESULT=AUTH` | Treat as credential/config issue; do not retry; surface to user |
| Validation failure | `RESULT=FAIL` with `REASON` | Surface clear reason to user; keep local state unchanged |
| Partial delete | `RESULT=PARTIAL` | Log which logids were not found; surface to user for review |
| Date range rejection | `RESULT=FAIL`, reason mentions date range | Surface as data validation error with the logbook's configured range |
| Network / transient failure | No response or HTTP error | Bounded retries with timeout and jitter |
| Rate limiting | HTTP 429 or similar | Back off with exponential delay |

**Core rule:** Never block local QSO logging on QRZ API availability. QRZ sync is an enrichment/upload path, not a prerequisite for local log operations.

---

## ADIF format notes

QSO data is exchanged using ADIF (Amateur Data Interchange Format). Each field uses the format `<fieldname:length>value` and records end with `<eor>`.

**Example ADIF record:**

```
<band:3>80m<mode:3>SSB<call:4>XX1X<qso_date:8>20140121<station_callsign:5>AA7BQ<time_on:4>0346<eor>
```

The QsoRipper adapter should include a robust ADIF parser/serializer that handles:

- Variable field lengths and ordering
- Optional fields present or absent per record
- Multi-record payloads from FETCH responses

---

## Mapping into QsoRipper domain

The adapter should parse ADIF and map into internal QSO structures, then expose normalized domain records to the app layer.

### Minimum field mapping

| ADIF field | QsoRipper domain |
|---|---|
| `station_callsign` | Local station identity |
| `call` | Worked callsign |
| `qso_date` + `time_on` | UTC timestamp (combined) |
| `band` | Band enum/value |
| `mode` | Mode enum/value |
| `rst_sent` | RST sent (optional) |
| `rst_rcvd` | RST received (optional) |
| `freq` | Frequency (optional) |
| `comment` / `notes` | Operator notes (optional) |
| `gridsquare` | Locator (optional) |

Additional ADIF fields should be preserved as extension data to support round-tripping with QRZ.

---

## Configuration keys

Use these environment variables (see `.env.example`):

| Variable | Purpose |
|---|---|
| `QSORIPPER_QRZ_LOGBOOK_BASE_URL` | Logbook API endpoint (default: `https://logbook.qrz.com/api`) |
| `QSORIPPER_QRZ_LOGBOOK_API_KEY` | QRZ-issued logbook access key |
| `QSORIPPER_QRZ_USER_AGENT` | User-Agent header value (e.g. `QsoRipper/0.1.0 (YOURCALL)`) |
| `QSORIPPER_QRZ_HTTP_TIMEOUT_SECONDS` | HTTP request timeout |
| `QSORIPPER_QRZ_MAX_RETRIES` | Maximum retry count for transient failures |

