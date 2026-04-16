# Integrations Instructions

External services should enrich local workflows without becoming hard dependencies.

## Integration Rules

- Isolate each provider behind an adapter interface.
- Implement retries and timeouts with explicit limits.
- Normalize provider-specific fields into internal models.
- Keep provider auth/session lifecycle out of UI code.
- Log actionable errors without leaking credentials.
- New integrations must be implemented in both the Rust and .NET engines for full parity.
- When adding or changing an integration, update `docs/architecture/engine-specification.md` in the same change.

## QRZ Notes

- Handle login/session refresh explicitly.
- Do not block QSO save paths on lookup failure.

