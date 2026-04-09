# Integrations Instructions

External services should enrich local workflows without becoming hard dependencies.

## Integration Rules

- Isolate each provider behind an adapter interface.
- Implement retries and timeouts with explicit limits.
- Normalize provider-specific fields into internal models.
- Keep provider auth/session lifecycle out of UI code.
- Log actionable errors without leaking credentials.

## QRZ Notes

- Handle login/session refresh explicitly.
- Do not block QSO save paths on lookup failure.

