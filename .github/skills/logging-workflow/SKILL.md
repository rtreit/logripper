---
name: logging-workflow
description: Designing and implementing efficient QSO logging flows for rapid operator input and correction.
---

# Logging Workflow Skill

## When to Use

- Implementing QSO entry and edit workflows
- Adding validation or dupe-check behavior
- Improving operator throughput in contest scenarios

## Expectations

1. Optimize for fast keyboard-driven entry.
2. Keep validation immediate and low-friction.
3. Protect data integrity without interrupting flow.
4. Keep interaction behavior consistent across TUI and GUI.
5. When changing QSO logging behavior or adding new logging features, update `docs/architecture/engine-specification.md` to keep the behavioral contract current.

