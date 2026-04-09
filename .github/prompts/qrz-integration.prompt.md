---
name: qrz-integration
description: Implement or update QRZ lookup functionality with safe auth handling and resilient behavior.
---

# QRZ Integration Prompt

Implement QRZ integration behavior with these constraints:

1. Keep QRZ API logic in a dedicated adapter/service layer.
2. Use secure credential loading and avoid hardcoded values.
3. Implement session handling, timeouts, and bounded retries.
4. Normalize QRZ response data into internal models.
5. Do not block local logging if QRZ is unavailable.
6. Add clear observability for errors without leaking sensitive details.
