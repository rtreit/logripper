---
name: debug-investigation
description: Investigate and resolve a QsoRipper bug with a root-cause-first process.
---

# Debug Investigation Prompt

Investigate the issue using this workflow:

1. Reproduce and document expected vs actual behavior.
2. Identify likely fault domain (core, storage, integration, TUI, GUI).
3. Isolate the root cause with concrete evidence.
4. Apply the smallest correct fix that addresses the cause.
5. Add regression coverage for the failing scenario.
6. Summarize impact and why the fix is safe.

