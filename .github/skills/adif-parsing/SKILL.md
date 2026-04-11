---
name: adif-parsing
description: >-
  Parse, validate, and generate ADIF (Amateur Data Interchange Format) files for QSO data interchange.
  Use when implementing ADIF import/export, mapping ADIF fields to proto QsoRecord, or debugging ADIF file issues.
---

# Skill: ADIF Parsing

> Parse, validate, and generate ADIF (Amateur Data Interchange Format) files for QSO data interchange.

## When to Use

- Implementing ADIF import/export in the Rust core engine
- Adding new ADIF fields to the QsoRecord proto mapping
- Debugging ADIF file parsing issues
- Validating ADIF file structure

## Reference Documents

- `docs/integrations/adif-specification.md` — Comprehensive ADIF 3.1.7 reference
- `proto/domain/qso_record.proto` — QsoRecord definition (ADIF maps to/from this)
- `proto/domain/station_snapshot.proto` — Historical local-station snapshot captured from ADIF/local context
- Full spec: https://www.adif.org/317/ADIF_317.htm
- Machine-readable data files: https://adif.org.uk/317/resources

## Key Design Rules

1. **ADIF is an edge concern** — it lives at the Rust import/export boundary, never crosses gRPC
2. **Normalize immediately** — convert ADIF tag-value pairs into proto QsoRecord fields
3. **Preserve unknown fields** — store unrecognized ADIF fields in an overflow map for round-trip fidelity
4. **Case-insensitive parsing** — field names, bands, modes, and tags are all case-insensitive
5. **Length-delimited** — `<FIELD:LENGTH>DATA` where LENGTH is the exact character count
6. **Forward compatible** — ignore unknown fields; accept deprecated fields on import but never emit them

## Parsing Workflow

```
ADI File → TagStream → RecordStream → Vec<AdifRecord>
  → normalize each field to proto QsoRecord
  → combine QSO_DATE + TIME_ON into Timestamp
  → map BAND/MODE strings to proto enums
  → convert FREQ (MHz) to frequency_khz (kHz)
  → preserve unmapped fields in extra_fields map
```

## Recommended Crates

| Crate | Status | Notes |
|---|---|---|
| `cammeresi/adif` | Evaluate | Async streaming, normalizers, most full-featured |
| `adif-rs` | Fallback | Simple ADI parser, no ADX |
| `ewpratten/adif` | Awareness | Basic, less maintained |

## Common Gotchas

- FREQ is in **MHz** in ADIF but **kHz** in proto QsoRecord
- If STATION_CALLSIGN is absent, OPERATOR serves as both callsign and operator
- Band strings include the unit suffix (e.g., `20M`, `70CM`) — strip for matching
- TIME_ON can be 4 digits (HHMM) or 6 digits (HHMMSS) — handle both
- IntlString fields (`_INTL` suffix) are ADX-only — never appear in ADI files
- Application-defined fields always start with `APP_` prefix
