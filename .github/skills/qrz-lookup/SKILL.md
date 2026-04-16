---
name: qrz-lookup
description: Implementing or extending QRZ lookup flows, including auth/session handling and data normalization.
---

# QRZ Lookup Skill

## When to Use

- Adding QRZ operator/station lookup features
- Debugging QRZ auth or session token issues
- Mapping QRZ fields into internal station/operator models

## Expectations

1. Keep QRZ logic in integration adapters, not UI layers.
2. Handle auth/session lifecycle explicitly.
3. Use bounded retries and timeouts.
4. Degrade gracefully when QRZ is unavailable.
5. When changing lookup behavior or adding new lookup features, update `docs/architecture/engine-specification.md` to keep the integration contract current.

## Project references

- `docs/integrations/qrz-xml-lookup-api.md`
- `docs/integrations/qrz-logbook-api.md`

