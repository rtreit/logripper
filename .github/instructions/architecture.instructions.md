# Architecture Instructions

Use a layered architecture that keeps domain logic independent from UI and external services.

## Preferred Boundaries

- `core/domain`: QSO entities, validation, business rules
- `core/app`: use cases and orchestration
- `adapters/storage`: persistence implementations
- `adapters/integrations`: QRZ and other external APIs
- `ui/tui` and `ui/gui`: presentation and interaction logic

## Rules

- Shared logic belongs in core, not duplicated across interfaces.
- Integrations are replaceable adapters behind explicit interfaces.
- Keep mutation rules centralized to protect log consistency.

