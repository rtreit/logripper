# Security Instructions

Apply secure-by-default behavior to configuration, data handling, and integrations.

## Core Rules

- Never commit secrets or tokens.
- Load credentials from environment variables or secure stores.
- Treat user and external data as untrusted input.
- Fail explicitly on authorization or credential errors.
- Redact sensitive values from logs and diagnostics.

## Review Checklist

1. No hardcoded credentials.
2. No sensitive data in structured logs.
3. Clear timeout and retry boundaries for network calls.
4. Explicit error paths for auth failures.
