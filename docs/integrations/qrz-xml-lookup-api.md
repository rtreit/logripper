# QRZ XML Lookup API - Integration Reference

Source specification: <https://www.qrz.com/docs/xml/current_spec.html> (v1.34, July 15, 2020)

This document is a comprehensive development reference for QsoRipper's consumption of the QRZ XML data service for callsign lookups, DXCC entity resolution, and operator enrichment.

---

## Overview

The QRZ XML service provides real-time access to callsign and DXCC data from the QRZ.COM database over standard HTTP. Responses are XML formatted.

**Subscription model:** Any QRZ user can authenticate, but an active QRZ Logbook Data subscription is required to receive full field data. Non-subscriber access returns a limited field set and is intended for testing only.

---

## Endpoint and versioning

### Service URL

Base: `https://xmldata.qrz.com/xml/`

The URL supports a version identifier segment:

```
https://xmldata.qrz.com/xml/<version_identifier>/?<query_parameters>
```

### Version identifier values

| Identifier | Behavior |
|---|---|
| _(none)_ | Legacy mode (v1.24) |
| `1.xx` | Use specific version (e.g. `1.34`; dot may be omitted: `134`) |
| `current` | Use the latest available version |

An invalid version identifier returns an error.

**QsoRipper policy:** Use `current` or a pinned version in configuration. A trailing slash after the version segment is strongly recommended by QRZ.

### Example URLs

```
https://xmldata.qrz.com/xml/?username=xx1xxx;password=abcdef          (legacy v1.24)
https://xmldata.qrz.com/xml/1.34/?username=xx1xxx;password=abcdef     (pinned v1.34)
https://xmldata.qrz.com/xml/current/?username=xx1xxx;password=abcdef  (latest)
```

---

## HTTP protocol notes

- Requests may use either HTTP `GET` or `POST`.
- Query parameter separators: either `&` or `;` are accepted.
- The interface does not use HTTP cookies, JavaScript, or HTML (except for the biography fetch, which returns HTML).

---

## Authentication and session management

### Login request

Send `username` and `password` to obtain a session key:

```
https://xmldata.qrz.com/xml/current/?username=xx1xxx;password=abcdef;agent=QsoRipper/0.1.0
```

**Login input fields:**

| Parameter | Required | Description |
|---|---|---|
| `username` | Yes | A valid QRZ.COM username |
| `password` | Yes | The correct password for the username |
| `agent` | Strongly recommended | Product name and version (e.g. `QsoRipper/0.1.0`). Assists QRZ support and troubleshooting. If omitted, QRZ falls back to the HTTP `User-Agent` header. |

### Successful login response

```xml
<?xml version="1.0" ?>
<QRZDatabase version="1.34">
  <Session>
    <Key>2331uf894c4bd29f3923f3bacf02c532d7bd9</Key>
    <Count>123</Count>
    <SubExp>Wed Jan 1 12:34:03 2013</SubExp>
    <GMTime>Sun Aug 16 03:51:47 2012</GMTime>
  </Session>
</QRZDatabase>
```

The `<QRZDatabase>` node includes `version` and `xmlns` attributes.

### Session key lifecycle

- All post-login requests must include the session key via the `s=` parameter.
- Session keys have **no guaranteed lifetime**. They are dynamically managed by the server.
- A key is valid for a single user and may be **immediately invalidated** if the user's IP address or other identifying information changes.
- Clients should perform **one login per session** and cache/reuse the key for all subsequent requests.
- When a response **omits the `<Key>` element**, no valid session exists and re-login is required.
- Monitor every response for session status and be prepared to re-authenticate at any time.

### Session node fields

| Field | Description |
|---|---|
| `Key` | A valid user session key. Absence means no active session. |
| `Count` | Number of lookups performed by this user in the current 24-hour period |
| `SubExp` | Subscription expiration date/time, or the string `"non-subscriber"` |
| `GMTime` | Server timestamp for this response |
| `Message` | Informational message for the user (e.g. subscription notices) |
| `Error` | Error message (e.g. "password incorrect", "session timeout", "callsign not found") |

Both `Error` and `Message` should be surfaced to the user when present. `Count`, `SubExp`, and `GMTime` are informational.

---

## XML response structure

### Top-level node

All responses are wrapped in `<QRZDatabase>`. Three child node types are defined:

- `<Session>` - always present
- `<Callsign>` - present on successful callsign lookup
- `<DXCC>` - present on successful DXCC lookup

### Forward compatibility requirement

QRZ may add new XML nodes or attributes at any time. The parser **must**:

- Parse in an "object=attribute" manner
- Ignore unknown nodes and attributes without error
- Make no assumptions about node count, order, or presence of previously undefined elements

---

## Callsign lookup

### Request

```
https://xmldata.qrz.com/xml/current/?s=<session_key>;callsign=<target>
```

### Example response

```xml
<?xml version="1.0" ?>
<QRZDatabase version="1.34">
  <Callsign>
    <call>AA7BQ</call>
    <aliases>N6UFT,KJ6RK,DL/AA7BQ</aliases>
    <dxcc>291</dxcc>
    <fname>FRED L</fname>
    <name>LLOYD</name>
    <addr1>8711 E PINNACLE PEAK RD 193</addr1>
    <addr2>SCOTTSDALE</addr2>
    <state>AZ</state>
    <zip>85255</zip>
    <country>United States</country>
    <ccode>291</ccode>
    <lat>34.23456</lat>
    <lon>-112.34356</lon>
    <grid>DM32af</grid>
    <county>Maricopa</county>
    <fips>04013</fips>
    <land>USA</land>
    <efdate>2000-01-20</efdate>
    <expdate>2010-01-20</expdate>
    <p_call>KJ6RK</p_call>
    <class>E</class>
    <codes>HAI</codes>
    <qslmgr>NONE</qslmgr>
    <email>flloyd@qrz.com</email>
    <url>https://www.qrz.com/db/aa7bq</url>
    <u_views>115336</u_views>
    <bio>3937/2003-11-04</bio>
    <image>https://files.qrz.com/q/aa7bq/aa7bq.jpg</image>
    <serial>3626</serial>
    <moddate>2003-11-04 19:37:02</moddate>
    <MSA>6200</MSA>
    <AreaCode>602</AreaCode>
    <TimeZone>Mountain</TimeZone>
    <GMTOffset>-7</GMTOffset>
    <DST>N</DST>
    <eqsl>Y</eqsl>
    <mqsl>Y</mqsl>
    <cqzone>3</cqzone>
    <ituzone>2</ituzone>
    <geoloc>user</geoloc>
    <attn>c/o QRZ LLC</attn>
    <nickname>The Boss</nickname>
    <name_fmt>FRED "The Boss" LLOYD</name_fmt>
    <born>1953</born>
  </Callsign>
  <Session>
    <Key>2331uf894c4bd29f3923f3bacf02c532d7bd9</Key>
    <Count>123</Count>
    <SubExp>Wed Jan 1 12:34:03 2013</SubExp>
    <GMTime>Sun Nov 16 04:13:46 2012</GMTime>
  </Session>
</QRZDatabase>
```

### Complete callsign node fields

Not all fields are returned with every request. Field ordering is arbitrary and subject to change.

| Field | Description |
|---|---|
| `call` | Callsign |
| `xref` | Cross reference: the query callsign that returned this record |
| `aliases` | Comma-separated list of other callsigns that resolve to this record |
| `dxcc` | DXCC entity ID (country code) for the callsign |
| `fname` | First name |
| `name` | Last name |
| `addr1` | Address line 1 (house number and street) |
| `addr2` | Address line 2 (city name) |
| `state` | State (USA only) |
| `zip` | Zip/postal code |
| `country` | Country name for the QSL mailing address |
| `ccode` | DXCC entity code for the mailing address country |
| `lat` | Latitude of address (signed decimal, S < 0 > N) |
| `lon` | Longitude of address (signed decimal, W < 0 > E) |
| `grid` | Maidenhead grid locator |
| `county` | County name (USA) |
| `fips` | FIPS county identifier (USA) |
| `land` | DXCC country name of the callsign |
| `efdate` | License effective date (USA) |
| `expdate` | License expiration date (USA) |
| `p_call` | Previous callsign |
| `class` | License class |
| `codes` | License type codes (USA) |
| `qslmgr` | QSL manager info |
| `email` | Email address |
| `url` | Web page address |
| `u_views` | QRZ web page views |
| `bio` | Approximate length of the bio HTML in bytes (present only if a biography exists) |
| `biodate` | Date of the last bio update |
| `image` | Full URL of the callsign's primary image |
| `imageinfo` | `height:width:size` in bytes, of the image file |
| `serial` | QRZ database serial number |
| `moddate` | QRZ callsign last modified date |
| `MSA` | Metro Service Area (USPS) |
| `AreaCode` | Telephone area code (USA) |
| `TimeZone` | Time zone (USA) |
| `GMTOffset` | GMT time offset |
| `DST` | Daylight Saving Time observed (Y/N) |
| `eqsl` | Will accept e-QSL (0/1 or blank if unknown) |
| `mqsl` | Will return paper QSL (0/1 or blank if unknown) |
| `cqzone` | CQ Zone identifier |
| `ituzone` | ITU Zone identifier |
| `born` | Operator's year of birth |
| `user` | User who manages this callsign on QRZ |
| `lotw` | Will accept LOTW (0/1 or blank if unknown) |
| `iota` | IOTA designator (blank if unknown) |
| `geoloc` | Source of lat/long data (see geolocation section) |
| `attn` | Attention address line; prepend to address (v1.34+) |
| `nickname` | A different or shortened name used on the air (v1.34+) |
| `name_fmt` | Combined full name and nickname in QRZ display format (v1.34+, format subject to change) |

---

## Lat/long and grid data

### Derivation hierarchy

QRZ derives geographic coordinates through a priority chain:

**For USA callsigns:**

1. User-supplied lat/long (if available, also used to compute grid)
2. User-supplied grid square (lat/long set to grid center)
3. Geocoding from postal address (US Census Tiger/Line dataset)
4. Zip code centroid
5. State approximate center

**For non-USA callsigns:**

1. User-supplied lat/long or grid
2. Approximate center of the DXCC entity (country)

Nearly every callsign will include geographic coordinates, but accuracy varies significantly.

### `geoloc` field values

| Value | Source |
|---|---|
| `user` | Input by the user directly |
| `geocode` | Derived from USA geocoding data |
| `grid` | Derived from a user-supplied grid square |
| `zip` | Derived from the callsign's USA zip code |
| `state` | Derived from the callsign's USA state |
| `dxcc` | Derived from the callsign's DXCC entity (country center) |
| `none` | No value could be determined |

**QsoRipper implementation note:** Surface the `geoloc` value as metadata so the UI or consumer can convey coordinate confidence. Do not treat all coordinates as equally precise.

---

## Biography data

The `<bio>` field appears in the callsign record **only if** a biography exists on the server. It indicates the approximate size of the bio HTML in bytes, and `<biodate>` gives the last update date.

### Fetching biography HTML

```
https://xmldata.qrz.com/xml/current/?s=<session_key>;html=<callsign>
```

This endpoint is **unique** in that it does **not** return XML. It returns regular HTML with embedded CSS, matching the QRZ page presentation.

---

## DXCC / prefix lookups

The `dxcc=` parameter provides three lookup modes:

| Input | Behavior |
|---|---|
| `dxcc=291` (numeric) | Return the DXCC entity record for entity 291 |
| `dxcc=XX1XX` (callsign) | Reduce to 4, then 3, then 2-letter prefix; return first matching DXCC entity |
| `dxcc=all` (keyword) | Return the entire list of 380+ DXCC entities. **Use sparingly.** |

### Example request

```
https://xmldata.qrz.com/xml/current/?s=<session_key>;dxcc=291
```

### Example response

```xml
<?xml version="1.0" ?>
<QRZDatabase version="1.34">
  <DXCC>
    <dxcc>291</dxcc>
    <cc>US</cc>
    <ccc>USA</ccc>
    <name>United States</name>
    <continent>NA</continent>
    <ituzone>6</ituzone>
    <cqzone>3</cqzone>
    <timezone>-5</timezone>
    <lat>37.788081</lat>
    <lon>-97.470703</lon>
  </DXCC>
  <Session>
    <Key>d0cf9d7b3b937ed5f5de28ddf5a0122d</Key>
    <Count>12</Count>
    <SubExp>Wed Jan 13 13:59:00 2013</SubExp>
    <GMTime>Mon Oct 12 22:33:56 2012</GMTime>
  </Session>
</QRZDatabase>
```

### DXCC node fields

| Field | Description |
|---|---|
| `dxcc` | DXCC entity number |
| `cc` | 2-letter country code (ISO-3166) |
| `ccc` | 3-letter country code (ISO-3166) |
| `name` | Long country/entity name |
| `continent` | 2-letter continent designator |
| `ituzone` | ITU Zone |
| `cqzone` | CQ Zone |
| `timezone` | UTC timezone offset (+/-). Odd values like `0545` mean "5 hours, 45 minutes". Plus sign is implied. |
| `lat` | Latitude (approximate center of entity) |
| `lon` | Longitude (approximate center of entity) |
| `notes` | Special notes and/or exceptions |

### DXCC implementation notes

- Entities with IDs greater than 900 are unique to QRZ and not part of the standard DXCC list.
- Lat/lon values represent the approximate geographic center of the entity.
- Non-match failures return: `No DXCC information for: xxxx`
- Cache the `dxcc=all` result locally and refresh infrequently; do not call it on every session.

---

## Error conditions

There are two general error types:

### 1) Data errors

Typically "item not found" responses. The session remains valid and `<Key>` is still returned.

```xml
<?xml version="1.0" ?>
<QRZDatabase version="1.34">
  <Session>
    <Error>Not found: g1srdd</Error>
    <Key>1232u4eaf13b8336d61982c1fd1099c9a38ac</Key>
    <GMTime>Sun Nov 16 05:07:14 2003</GMTime>
  </Session>
</QRZDatabase>
```

### 2) Session errors

The session has expired or been invalidated. The `<Key>` field is **not returned**.

```xml
<?xml version="1.0" ?>
<QRZDatabase version="1.34">
  <Session>
    <Error>Session Timeout</Error>
    <GMTime>Sun Nov 16 05:11:58 2003</GMTime>
  </Session>
</QRZDatabase>
```

### Special error: "Connection refused"

This error indicates service is refused for the user. **Successful login will not be possible for at least 24 hours.** No further details are provided.

### Error handling strategy for QsoRipper

| Error class | Detection | QsoRipper behavior |
|---|---|---|
| Session timeout / invalid key | `<Error>` present AND `<Key>` absent | Re-authenticate immediately; retry the original request once |
| Callsign not found | `<Error>` contains "Not found" AND `<Key>` present | Negative-cache with short TTL; do not retry |
| Connection refused | `<Error>` is "Connection refused" | Back off for 24 hours; surface to user as config/account issue |
| Password incorrect | `<Error>` contains auth failure text | Stop retries; surface as credential error |
| Network / HTTP failure | No XML response at all | Bounded retries with timeout and jitter |

In all cases, **never block the local QSO logging path** on lookup availability.

---

## Normalization into QsoRipper domain

Map QRZ XML fields into an internal `CallsignRecord` model. Do not expose raw XML structures above the adapter boundary.

### Recommended field groupings

**Identity:**
`call`, `xref`, `aliases`, `p_call`

**Operator info:**
`fname`, `name`, `nickname`, `name_fmt`, `email`, `born`

**Address / mailing:**
`attn`, `addr1`, `addr2`, `state`, `zip`, `country`, `ccode`

**Geography:**
`lat`, `lon`, `grid`, `geoloc`, `county`, `fips`, `land`, `dxcc`

**Zone info:**
`cqzone`, `ituzone`, `continent` (from DXCC)

**License info (USA-centric):**
`class`, `codes`, `efdate`, `expdate`

**QSL preferences:**
`eqsl`, `mqsl`, `lotw`, `qslmgr`, `iota`

**Metadata:**
`image`, `imageinfo`, `url`, `u_views`, `bio`, `biodate`, `serial`, `moddate`, `user`

**USA-specific (optional):**
`MSA`, `AreaCode`, `TimeZone`, `GMTOffset`, `DST`

---

## Configuration keys

Use these environment variables (see `.env.example`):

| Variable | Purpose |
|---|---|
| `QSORIPPER_QRZ_XML_BASE_URL` | XML service base URL (default: `https://xmldata.qrz.com/xml/current/`) |
| `QSORIPPER_QRZ_XML_USERNAME` | QRZ username for XML API login |
| `QSORIPPER_QRZ_XML_PASSWORD` | QRZ password for XML API login |
| `QSORIPPER_QRZ_USER_AGENT` | Agent string sent with requests (e.g. `QsoRipper/0.1.0`) |
| `QSORIPPER_QRZ_HTTP_TIMEOUT_SECONDS` | HTTP request timeout |
| `QSORIPPER_QRZ_MAX_RETRIES` | Maximum retry count for transient failures |

