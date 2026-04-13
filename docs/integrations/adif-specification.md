# ADIF Specification Reference (v3.1.7)

> Reference for LogRipper development based on the [ADIF 3.1.7 specification](https://www.adif.org/317/ADIF_317.htm).
> Machine-readable exports (CSV, JSON, XML) are available at https://adif.org.uk/317/resources

## Overview

ADIF (Amateur Data Interchange Format) is the standard for exchanging ham radio QSO data between applications. It defines precise text-based representations for amateur radio information. ADIF is **not** a database schema or UI specification — it only governs import/export interchange.

Key points:
- Applications choose which fields to support; not all fields are required
- Minimum suggested QSO: `QSO_DATE`, `TIME_ON`, `FREQ` and/or `BAND`, `CALL`, `MODE`
- Field names are case-insensitive
- Two file formats: **ADI** (tag-length-value, the primary format) and **ADX** (XML-based)
- Current version: 3.1.7 (released 2026-03-22)

---

## Data Types

| Type | Indicator | Description |
|---|---|---|
| Boolean | B | Single char: `Y`/`y` (true) or `N`/`n` (false) |
| Character | — | ASCII 32-126 |
| Date | D | 8 digits `YYYYMMDD` in UTC (1930 ≤ YYYY) |
| Digit | — | ASCII 48-57 |
| Enumeration | E | Case-insensitive value from a defined set |
| GridSquare | — | 2/4/6/8-char Maidenhead locator (case-insensitive) |
| GridSquareExt | — | Characters 9-10 (or 9-12) of extended Maidenhead locator |
| Integer | — | Decimal integer, optional leading minus |
| IntlString | I | Unicode/UTF-8 string (**ADX files only**) |
| IOTARefNo | — | Format `CC-XXX` (continent + 3-digit island group) |
| Location | L | 11 chars: `XDDD MM.MMM` (X=N/S/E/W, DDD=degrees, MM.MMM=minutes) |
| MultilineString | M | Characters + CR/LF line breaks |
| Number | N | Decimal number, optional minus, optional decimal point |
| PositiveInteger | — | Unsigned decimal integer > 0 |
| POTARef | — | Parks on the Air reference: `xxxx-nnnnn[@yyyyyy]` |
| SOTARef | — | SOTA reference: `prefix/REF-NNN` |
| String | S | Sequence of Characters (ASCII 32-126) |
| Time | T | 4 digits `HHMM` or 6 digits `HHMMSS` in UTC |
| WWFFRef | — | WWFF reference: `xxFF-nnnn` (8-11 chars) |

---

## Enumerations

### Band Enumeration

| Band | Lower Freq (MHz) | Upper Freq (MHz) |
|---|---|---|
| 2190m | 0.1357 | 0.1378 |
| 630m | 0.472 | 0.479 |
| 560m | 0.501 | 0.504 |
| 160m | 1.8 | 2.0 |
| 80m | 3.5 | 4.0 |
| 60m | 5.06 | 5.45 |
| 40m | 7.0 | 7.3 |
| 30m | 10.1 | 10.15 |
| 20m | 14.0 | 14.35 |
| 17m | 18.068 | 18.168 |
| 15m | 21.0 | 21.45 |
| 12m | 24.890 | 24.99 |
| 10m | 28.0 | 29.7 |
| 8m | 40 | 45 |
| 6m | 50 | 54 |
| 5m | 54.000001 | 69.9 |
| 4m | 70 | 71 |
| 2m | 144 | 148 |
| 1.25m | 222 | 225 |
| 70cm | 420 | 450 |
| 33cm | 902 | 928 |
| 23cm | 1240 | 1300 |
| 13cm | 2300 | 2450 |
| 9cm | 3300 | 3500 |
| 6cm | 5650 | 5925 |
| 3cm | 10000 | 10500 |
| 1.25cm | 24000 | 24250 |
| 6mm | 47000 | 47200 |
| 4mm | 75500 | 81000 |
| 2.5mm | 119980 | 123000 |
| 2mm | 134000 | 149000 |
| 1mm | 241000 | 250000 |
| submm | 300000 | 7500000 |

### Mode Enumeration

| Mode | Submodes (selected) |
|---|---|
| AM | — |
| ARDOP | — |
| ATV | — |
| C4FM | *(import-only, use DIGITALVOICE + submode C4FM)* |
| CHIP | CHIP64, CHIP128 |
| CLO | — |
| CONTESTI | — |
| CW | PCW |
| DIGITALVOICE | C4FM, DMR, DSTAR, FREEDV, M17 |
| DOMINO | DOM-M, DOM4, DOM5, DOM8, DOM11, DOM16, DOM22, DOM44, DOM88, DOMINOEX, DOMINOF |
| DSTAR | *(import-only, use DIGITALVOICE + submode DSTAR)* |
| DYNAMIC | VARA HF, VARA SATELLITE, VARA FM 1200, VARA FM 9600, FREEDATA |
| FAX | — |
| FM | — |
| FSK | SCAMP_FAST, SCAMP_SLOW, SCAMP_VSLOW |
| FT8 | — |
| HELL | FMHELL, FSKHELL, FSKH105, FSKH245, HELL80, HELLX5, HELLX9, HFSK, PSKHELL, SLOWHELL |
| ISCAT | ISCAT-A, ISCAT-B |
| JT4 | JT4A through JT4G |
| JT9 | JT9-1, JT9-2, JT9-5, JT9-10, JT9-30, JT9A-H (+ FAST variants) |
| JT44 | — |
| JT65 | JT65A, JT65B, JT65B2, JT65C, JT65C2 |
| MFSK | FST4, FST4W, FT2, FT4, JS8, JTMS, MFSK4-128(L), MSK144, Q65, FSQCALL |
| MTONE | SCAMP_OO, SCAMP_OO_SLW |
| MSK144 | — |
| OFDM | RIBBIT_PIX, RIBBIT_SMS |
| OLIVIA | OLIVIA 4/125, 4/250, 8/250, 8/500, 16/500, 16/1000, 32/1000 |
| OPERA | OPERA-BEACON, OPERA-QSO |
| PAC | PAC2, PAC3, PAC4 |
| PAX | PAX2 |
| PKT | — |
| PSK | PSK10-1000 variants, QPSK31-500, FSK31, 8PSK variants, SIM31, PSKAM/PSKFEC variants |
| Q15 | — |
| QRA64 | QRA64A through QRA64E |
| ROS | ROS-EME, ROS-HF, ROS-MF |
| RTTY | ASCI |
| RTTYM | — |
| SSB | USB, LSB |
| SSTV | — |
| T10 | — |
| THOR | THOR-M, THOR4-100, THOR25X4, THOR50X1, THOR50X2 |
| THRB | THRBX, THRBX1-4, THROB1-4 |
| TOR | AMTORFEC, GTOR, NAVTEX, SITORB |
| V4 | — |
| VOI | — |
| WINMOR | — |
| WSPR | — |

### Continent Enumeration

| Code | Continent |
|---|---|
| NA | North America |
| SA | South America |
| EU | Europe |
| AF | Africa |
| OC | Oceania |
| AS | Asia |
| AN | Antarctica |

### QSL Sent Enumeration

| Value | Meaning |
|---|---|
| Y | Yes (QSL sent) |
| N | No (QSL not sent) |
| R | Requested |
| Q | Queued |
| I | Ignore/invalid |

### QSL Received Enumeration

| Value | Meaning |
|---|---|
| Y | Yes (QSL received) |
| N | No (QSL not received) |
| R | Requested |
| I | Ignore/invalid |
| V | Verified *(import-only)* |

### QSL Via Enumeration

| Value | Meaning |
|---|---|
| B | Bureau |
| D | Direct |
| E | Electronic |
| M | Manager *(import-only)* |

### QSO Upload Status Enumeration

| Value | Meaning |
|---|---|
| Y | Uploaded |
| N | Do not upload |
| M | Modified after upload |

### QSO Download Status Enumeration

| Value | Meaning |
|---|---|
| Y | Downloaded |
| N | Not downloaded |
| I | Ignore/invalid |

### QSO Complete Enumeration

| Value | Meaning |
|---|---|
| Y | Yes |
| N | No |
| NIL | Not heard |
| ? | Uncertain |

### Propagation Mode Enumeration

| Code | Description |
|---|---|
| AS | Aircraft Scatter |
| AUR | Aurora |
| AUE | Aurora-E |
| BS | Back scatter |
| ECH | EchoLink |
| EME | Earth-Moon-Earth |
| ES | Sporadic E |
| F2 | F2 Reflection |
| FAI | Field Aligned Irregularities |
| GWAVE | Ground Wave |
| INTERNET | Internet-assisted |
| ION | Ionospheric Scatter |
| IRL | IRLP |
| LOS | Line of Sight |
| MS | Meteor Scatter |
| RPT | Terrestrial or atmospheric repeater or transponder |
| RS | Rain Scatter |
| SAT | Satellite |
| TEP | Trans-equatorial |
| TR | Tropospheric ducting |

### Ant Path Enumeration

| Code | Meaning |
|---|---|
| G | Grayline |
| O | Other |
| S | Short path |
| L | Long path |

### EQSL_AG Enumeration

| Value | Meaning |
|---|---|
| Y | Yes (Authenticity Guaranteed) |
| N | No |
| U | Unknown |

### Morse Key Type Enumeration

| Code | Description |
|---|---|
| SK | Straight Key |
| BG | Bug (semi-automatic) |
| SP | Single-lever Paddle |
| DP | Dual-lever Paddle |
| HK | Hand Key (other) |
| KP | Keyboard/Processor |
| OT | Other |

---

## QSO Fields — Complete Reference

> All fields are optional. Applications decide which to support. Field names are case-insensitive.

### Contacted Station Fields

| Field | Type | Enum | Description |
|---|---|---|---|
| CALL | String | — | Contacted station's callsign |
| CONTACTED_OP | String | — | Callsign of the individual operating the contacted station |
| NAME | String | — | Contacted operator's name |
| AGE | Number | — | Contacted operator's age (0-120) |
| ADDRESS | MultilineString | — | Contacted station's full mailing address |
| QTH | String | — | Contacted station's city |
| EMAIL | String | — | Contacted station's email |
| WEB | String | — | Contacted station's URL |
| SILENT_KEY | Boolean | — | Contacted operator is now a Silent Key |
| EQ_CALL | String | — | Contacted station's owner's callsign |
| PFX | String | — | Contacted station's WPX prefix |

### Core QSO Fields

| Field | Type | Enum | Description |
|---|---|---|---|
| QSO_DATE | Date | — | Date QSO started (YYYYMMDD, UTC) |
| QSO_DATE_OFF | Date | — | Date QSO ended |
| TIME_ON | Time | — | QSO start time (HHMM or HHMMSS, UTC) |
| TIME_OFF | Time | — | QSO end time |
| BAND | Enum | Band | QSO band |
| BAND_RX | Enum | Band | Logging station's receiving band (split QSO) |
| MODE | Enum | Mode | QSO mode |
| SUBMODE | String | Submode | QSO submode |
| FREQ | Number | — | QSO frequency in MHz |
| FREQ_RX | Number | — | Logging station's receiving frequency in MHz (split QSO) |
| QSO_COMPLETE | Enum | QSO Complete | Whether QSO was complete (Y/N/NIL/?) |
| QSO_RANDOM | Boolean | — | Whether QSO was random or scheduled |

### Signal Reports

| Field | Type | Description |
|---|---|---|
| RST_SENT | String | Signal report sent to contacted station |
| RST_RCVD | String | Signal report received from contacted station |
| TX_PWR | Number | Logging station's transmitter power in Watts (≥0) |
| RX_PWR | Number | Contacted station's transmitter power in Watts (≥0) |

### Geographic / Location Fields

| Field | Type | Enum | Description |
|---|---|---|---|
| GRIDSQUARE | GridSquare | — | Contacted station's Maidenhead grid (2/4/6/8 chars) |
| GRIDSQUARE_EXT | GridSquareExt | — | Characters 9-12 of contacted station's extended grid |
| LAT | Location | — | Contacted station's latitude |
| LON | Location | — | Contacted station's longitude |
| DXCC | Enum | DXCC Entity Code | Contacted station's DXCC entity code |
| COUNTRY | String | — | Contacted station's DXCC entity name |
| STATE | Enum | Primary Admin Subdiv | Contacted station's primary admin subdivision (US state, etc.) |
| CNTY | Enum | Secondary Admin Subdiv | Contacted station's secondary admin subdivision (US county, etc.) |
| CNTY_ALT | SecondaryAdminSubdivListAlt | — | Alternate secondary admin subdivision codes |
| CONT | Enum | Continent | Contacted station's continent |
| CQZ | PositiveInteger | — | Contacted station's CQ zone (1-40) |
| ITUZ | PositiveInteger | — | Contacted station's ITU zone (1-90) |
| IOTA | IOTARefNo | — | Contacted station's IOTA designator (CC-XXX) |
| IOTA_ISLAND_ID | PositiveInteger | — | Contacted station's IOTA island identifier |
| DISTANCE | Number | — | Distance in km between stations (≥0) |
| REGION | Enum | Region | Contacted station's WAE/CQ entity within DXCC |

### Logging Station Fields (MY_ prefix)

| Field | Type | Description |
|---|---|---|
| STATION_CALLSIGN | String | Logging station's callsign (used over the air) |
| OPERATOR | String | Logging operator's callsign |
| OWNER_CALLSIGN | String | Callsign of the owner of the logging station |
| MY_GRIDSQUARE | GridSquare | Logging station's grid (2/4/6/8 chars) |
| MY_GRIDSQUARE_EXT | GridSquareExt | Extended grid chars 9-12 |
| MY_LAT | Location | Logging station's latitude |
| MY_LON | Location | Logging station's longitude |
| MY_ALTITUDE | Number | Logging station's height in meters (MSL) |
| MY_CITY | String | Logging station's city |
| MY_CNTY | Enum | Logging station's county |
| MY_CNTY_ALT | SecondaryAdminSubdivListAlt | Alternate county codes |
| MY_COUNTRY | String | Logging station's DXCC entity name |
| MY_CQ_ZONE | PositiveInteger | Logging station's CQ zone (1-40) |
| MY_DXCC | Enum | Logging station's DXCC entity code |
| MY_FISTS | PositiveInteger | FISTS CW Club member number |
| MY_IOTA | IOTARefNo | Logging station's IOTA designator |
| MY_IOTA_ISLAND_ID | PositiveInteger | Logging station's IOTA island ID |
| MY_ITU_ZONE | PositiveInteger | Logging station's ITU zone (1-90) |
| MY_NAME | String | Logging operator's name |
| MY_POSTAL_CODE | String | Logging station's postal code |
| MY_RIG | String | Description of logging station's equipment |
| MY_SIG | String | Special interest activity or event |
| MY_SIG_INFO | String | Special interest activity info |
| MY_SOTA_REF | SOTARef | Logging station's SOTA reference |
| MY_STATE | Enum | Logging station's primary admin subdivision |
| MY_STREET | String | Logging station's street |
| MY_ANTENNA | String | Logging station's antenna |
| MY_ARRL_SECT | Enum | Logging station's ARRL section |
| MY_DARC_DOK | Enum | Logging station's DARC DOK |
| MY_POTA_REF | POTARefList | Logging station's POTA references |
| MY_VUCC_GRIDS | GridSquareList | Logging station's VUCC grids |
| MY_WWFF_REF | WWFFRef | Logging station's WWFF reference |
| MY_MORSE_KEY_INFO | String | Logging station's Morse key details |
| MY_MORSE_KEY_TYPE | Enum | Logging station's Morse key type |
| MY_USACA_COUNTIES | SecondarySubdivList | USACA border county pair |

### QSL / Confirmation Fields

| Field | Type | Enum | Description |
|---|---|---|---|
| QSL_SENT | Enum | QSL Sent | Paper QSL sent status (default: N) |
| QSL_SENT_VIA | Enum | QSL Via | Means by which QSL was sent |
| QSL_RCVD | Enum | QSL Rcvd | Paper QSL received status (default: N) |
| QSL_RCVD_VIA | Enum | QSL Via | Means by which QSL was received |
| QSLSDATE | Date | — | QSL sent date |
| QSLRDATE | Date | — | QSL received date |
| QSL_VIA | String | — | Contacted station's QSL route |
| QSLMSG | MultilineString | — | Message for paper/electronic QSL |
| QSLMSG_RCVD | MultilineString | — | Message received on QSL |
| LOTW_QSL_SENT | Enum | QSL Sent | LoTW QSL sent status (default: N) |
| LOTW_QSL_RCVD | Enum | QSL Rcvd | LoTW QSL received status (default: N) |
| LOTW_QSLSDATE | Date | — | Date QSL sent to LoTW |
| LOTW_QSLRDATE | Date | — | Date QSL received from LoTW |
| EQSL_QSL_SENT | Enum | QSL Sent | eQSL sent status (default: N) |
| EQSL_QSL_RCVD | Enum | QSL Rcvd | eQSL received status (default: N) |
| EQSL_QSLSDATE | Date | — | Date QSL sent to eQSL |
| EQSL_QSLRDATE | Date | — | Date QSL received from eQSL |
| EQSL_AG | Enum | EQSL_AG | eQSL Authenticity Guaranteed status (default: U) |
| DCL_QSL_SENT | Enum | QSL Sent | DARC Community Logbook sent status (default: N) |
| DCL_QSL_RCVD | Enum | QSL Rcvd | DARC Community Logbook received status (default: N) |
| DCL_QSLSDATE | Date | — | Date QSL sent to DCL |
| DCL_QSLRDATE | Date | — | Date QSL received from DCL |
| CREDIT_SUBMITTED | CreditList | Credit | Credits sought for this QSO |
| CREDIT_GRANTED | CreditList | Credit | Credits granted for this QSO |

### Online Service Upload/Download Fields

| Field | Type | Description |
|---|---|---|
| QRZCOM_QSO_UPLOAD_DATE | Date | Date last uploaded to QRZ.com |
| QRZCOM_QSO_UPLOAD_STATUS | Enum (QSO Upload) | Upload status on QRZ.com |
| QRZCOM_QSO_DOWNLOAD_DATE | Date | Date downloaded from QRZ.com |
| QRZCOM_QSO_DOWNLOAD_STATUS | Enum (QSO Download) | Download status from QRZ.com |
| CLUBLOG_QSO_UPLOAD_DATE | Date | Date last uploaded to Club Log |
| CLUBLOG_QSO_UPLOAD_STATUS | Enum (QSO Upload) | Upload status on Club Log |
| HRDLOG_QSO_UPLOAD_DATE | Date | Date last uploaded to HRDLog.net |
| HRDLOG_QSO_UPLOAD_STATUS | Enum (QSO Upload) | Upload status on HRDLog.net |
| HAMLOGEU_QSO_UPLOAD_DATE | Date | Date last uploaded to HAMLOG.EU |
| HAMLOGEU_QSO_UPLOAD_STATUS | Enum (QSO Upload) | Upload status on HAMLOG.EU |
| HAMQTH_QSO_UPLOAD_DATE | Date | Date last uploaded to HamQTH.com |
| HAMQTH_QSO_UPLOAD_STATUS | Enum (QSO Upload) | Upload status on HamQTH.com |

### Contest Fields

| Field | Type | Description |
|---|---|---|
| CONTEST_ID | String (Contest ID enum) | Contest identifier |
| SRX | Integer | Contest serial number received (≥0) |
| SRX_STRING | String | Contest info received (Cabrillo format) |
| STX | Integer | Contest serial number transmitted (≥0) |
| STX_STRING | String | Contest info transmitted (Cabrillo format) |
| CHECK | String | Contest check (e.g. ARRL Sweepstakes) |
| CLASS | String | Contest class (e.g. ARRL Field Day) |
| PRECEDENCE | String | Contest precedence |
| ARRL_SECT | Enum (ARRL Section) | Contacted station's ARRL section |

### Propagation / Conditions Fields

| Field | Type | Description |
|---|---|---|
| PROP_MODE | Enum (Propagation Mode) | Propagation mode |
| ANT_AZ | Number | Logging station's antenna azimuth (0-360°) |
| ANT_EL | Number | Logging station's antenna elevation (-90 to 90°) |
| ANT_PATH | Enum (Ant Path) | Signal path (grayline/short/long/other) |
| A_INDEX | Number | Geomagnetic A index (0-400) |
| K_INDEX | Integer | Geomagnetic K index (0-9) |
| SFI | Integer | Solar flux index (0-300) |
| MAX_BURSTS | Number | Max meteor scatter burst length in seconds (≥0) |
| NR_BURSTS | Integer | Number of meteor scatter bursts (≥0) |
| NR_PINGS | Integer | Number of meteor scatter pings (≥0) |
| MS_SHOWER | String | Meteor shower name |
| FORCE_INIT | Boolean | New EME "initial" |
| SAT_MODE | String | Satellite mode code (uplink/downlink bands) |
| SAT_NAME | String | Satellite name |

### Special Activity Fields

| Field | Type | Description |
|---|---|---|
| SIG | String | Contacted station's special activity/interest group |
| SIG_INFO | String | Info associated with contacted station's activity |
| DARC_DOK | Enum | Contacted station's DARC DOK |
| FISTS | PositiveInteger | Contacted station's FISTS member number |
| FISTS_CC | PositiveInteger | Contacted station's FISTS Century Certificate number |
| SKCC | String | Contacted station's SKCC member info |
| TEN_TEN | PositiveInteger | Contacted station's Ten-Ten number |
| UKSMG | PositiveInteger | Contacted station's UKSMG member number |
| POTA_REF | POTARefList | Contacted station's POTA references |
| SOTA_REF | SOTARef | Contacted station's SOTA reference |
| WWFF_REF | WWFFRef | Contacted station's WWFF reference |
| USACA_COUNTIES | SecondarySubdivList | USACA border county pair |
| VUCC_GRIDS | GridSquareList | Contacted station's VUCC grids |
| IOTA | IOTARefNo | Contacted station's IOTA designator |
| MORSE_KEY_INFO | String | Contacted station's Morse key details |
| MORSE_KEY_TYPE | Enum (Morse Key Type) | Contacted station's Morse key type |
| ALTITUDE | Number | Contacted station's altitude in meters (MSL) |

### Miscellaneous Fields

| Field | Type | Description |
|---|---|---|
| COMMENT | String | QSO comment (for contacted operator) |
| NOTES | MultilineString | QSO notes (for logging operator) |
| PUBLIC_KEY | String | Public encryption key |
| RIG | MultilineString | Description of contacted station's equipment |
| SWL | Boolean | QSO pertains to an SWL report |

---

## ADI File Format

The primary interchange format. Tag-length-value syntax.

### Data Specifier Syntax

```
<FIELDNAME:LENGTH[:TYPE]>DATA
```

- `FIELDNAME`: case-insensitive field name
- `LENGTH`: unsigned decimal integer = number of characters in DATA
- `TYPE`: optional data type indicator (e.g., `S` for String, `D` for Date, `N` for Number)
- `DATA`: field value (exactly LENGTH characters)

### File Structure

```
[Optional Header]
<EOH>
Record1 <EOR>
Record2 <EOR>
...
```

- If the file starts with `<`, there is no header (first `<` begins first record)
- Header can contain arbitrary text + header data specifiers
- Characters between data specifiers and outside tags are ignored (allows formatting)

### Header Fields

| Field | Description |
|---|---|
| ADIF_VER | ADIF version number (e.g., `3.1.7`) |
| CREATED_TIMESTAMP | File creation timestamp |
| PROGRAMID | Name of the generating application |
| PROGRAMVERSION | Version of the generating application |
| USERDEFn | User-defined field definition |

### Sample ADI File

```
Generated by LogRipper v0.1.0

<adif_ver:5>3.1.7
<programid:9>LogRipper
<EOH>

<qso_date:8>20260115
<time_on:4>1523
<call:5>VK9NS
<band:3>20M
<mode:4>RTTY
<freq:8>14.08500
<rst_sent:2>59
<rst_rcvd:2>57
<station_callsign:5>AA7BQ
<my_gridsquare:6>DM43an
<eor>

<qso_date:8>20260115
<time_on:4>1545
<call:5>ON4UN
<band:3>40M
<mode:3>SSB
<submode:3>USB
<freq:5>7.180
<rst_sent:2>59
<rst_rcvd:2>59
<contest_id:10>CQ-WW-SSB
<srx:3>142
<stx:3>033
<eor>
```

### Parsing Rules

1. **Case-insensitive**: field names, mode/band values, EOH/EOR tags
2. **Ignore unknown fields**: forward compatibility — skip fields your app doesn't recognize
3. **Ignore text outside specifiers**: allows comments, whitespace, formatting between records
4. **Length-delimited**: the LENGTH value determines exactly how many characters to read for DATA
5. **No maximum field length**: importing apps may truncate, exporting apps may write any length
6. **Field order arbitrary**: fields within a record can appear in any order
7. **No duplicate fields per record**: each field name may appear at most once per record

### Application-Defined Fields

Format: `APP_PROGRAMID_FIELDNAME`

```
<APP_LOGRIPPER_SYNC_STATUS:6>synced
```

### User-Defined Fields

Defined in the header with USERDEFn, referenced in records by field name:

```
<USERDEF1:8:N>QRP_ARCI
<EOH>
<qrp_arci:5>12345
<eor>
```

---

## ADX File Format (XML)

XML-based format with UTF-8 encoding. Uses `.adx` extension. Supports international characters.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<ADX>
  <HEADER>
    <ADIF_VER>3.1.7</ADIF_VER>
    <PROGRAMID>LogRipper</PROGRAMID>
  </HEADER>
  <RECORDS>
    <RECORD>
      <QSO_DATE>20260115</QSO_DATE>
      <TIME_ON>1523</TIME_ON>
      <CALL>VK9NS</CALL>
      <BAND>20M</BAND>
      <MODE>RTTY</MODE>
    </RECORD>
  </RECORDS>
</ADX>
```

ADX support is optional — all ADIF-compliant apps must support ADI, but ADX is not required.

---

## Mapping to LogRipper Proto QsoRecord

The ADIF parser (Rust-side edge adapter) converts ADIF fields to/from proto `QsoRecord`:

| ADIF Field | Proto QsoRecord Field | Notes |
|---|---|---|
| STATION_CALLSIGN | station_callsign | Falls back to OPERATOR if absent |
| CALL | worked_callsign | — |
| QSO_DATE + TIME_ON | utc_timestamp | Combined into Timestamp |
| QSO_DATE_OFF + TIME_OFF | utc_end_timestamp | End date falls back to QSO_DATE when ADIF omits QSO_DATE_OFF |
| BAND | band | Map string to Band enum |
| MODE + SUBMODE | mode | Map to Mode enum; store submode separately if needed |
| FREQ | frequency_khz | Convert MHz → kHz (multiply by 1000) |
| RST_SENT | rst_sent | Parse into RstReport |
| RST_RCVD | rst_received | Parse into RstReport |
| TX_PWR | tx_power | — |
| CONTACTED_OP | worked_operator_callsign | — |
| GRIDSQUARE | worked_grid | — |
| COUNTRY | worked_country | — |
| DXCC | worked_dxcc | — |
| STATE | worked_state | — |
| CONT | worked_continent | — |
| CQZ | worked_cq_zone | — |
| ITUZ | worked_itu_zone | — |
| CNTY | worked_county | — |
| IOTA | worked_iota | — |
| ARRL_SECT | worked_arrl_section | — |
| NAME | worked_operator_name | — |
| CONTEST_ID | contest_id | — |
| STX | serial_sent | — |
| SRX | serial_received | — |
| STX_STRING | exchange_sent | — |
| SRX_STRING | exchange_received | — |
| COMMENT | comment | — |
| NOTES | notes | — |
| QSL_SENT | qsl_sent_status | Map Y/N/R/Q/I to QslStatus enum |
| QSL_RCVD | qsl_received_status | Map Y/N/R/I to QslStatus enum |
| QSLSDATE | qsl_sent_date | Stored as a Timestamp at 00:00:00 UTC |
| QSLRDATE | qsl_received_date | Stored as a Timestamp at 00:00:00 UTC |
| LOTW_QSL_SENT | lotw_sent | Map to bool |
| LOTW_QSL_RCVD | lotw_received | Map to bool |
| EQSL_QSL_SENT | eqsl_sent | Map to bool |
| EQSL_QSL_RCVD | eqsl_received | Map to bool |
| MY_NAME | station_snapshot.operator_name | Logging-station snapshot field |
| MY_ARRL_SECT | station_snapshot.arrl_section | Logging-station snapshot field |

**Fields not in QsoRecord or its nested station snapshot** (such as unsupported `MY_` fields, propagation data, satellite info, etc.) are preserved in an overflow map for round-trip fidelity during ADIF import/export.

---

## Parser Strategy

### Recommended Rust Crate

Evaluate [`cammeresi/adif`](https://github.com/cammeresi/adif) for the Rust-side parser:
- Async streaming parser (tokio-based)
- TagStream + RecordStream layered abstraction
- Data normalizers for field cleaning
- Panic-free, memory-efficient

Alternatives: [`adif-rs`](https://github.com/macopacabana/adif-rs) (simpler), [`ewpratten/adif`](https://github.com/ewpratten/adif) (basic).

### Recommended .NET Package

The .NET GUI does not parse ADIF directly (gRPC provides normalized proto types), but for any direct ADIF file handling:
- [`AdifNet`](https://www.nuget.org/packages/AdifNet/) — full ADI+ADX, .NET 8, SQL adapter
- [`AdifLib`](https://www.nuget.org/packages/AdifLib/) — lightweight, .NET Standard 2.0

### Design Considerations

1. **Round-trip fidelity**: preserve all ADIF fields during import, even those not in QsoRecord, so exports don't lose data
2. **Forward compatibility**: ignore unknown fields (matching ADIF's upward compatibility policy)
3. **Band/Mode normalization**: convert string values to proto enums; handle submodes
4. **Date/time combining**: merge QSO_DATE + TIME_ON into a single UTC timestamp
5. **Frequency vs Band**: FREQ is optional when BAND is present; derive one from the other when possible
6. **Import-only fields**: accept deprecated fields on import but never emit them on export

---

## Key Spec References

- Full specification: https://www.adif.org/317/ADIF_317.htm
- Machine-readable data exports: https://adif.org.uk/317/resources (CSV, JSON, XML, TSV, XLSX)
- Test QSO files: available in the resources ZIP (ADI + ADX formats)
- ADX XML schemas: https://adif.org.uk/317/resources
- ADIF Developers Group: https://groups.io/g/adifdev
