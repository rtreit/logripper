# Performance Instructions

Favor low-latency and predictable behavior in both interactive and batch workflows.

## Performance Rules

- Prefer O(1) and O(log n) access paths for hot operations.
- Minimize allocations in tight loops and high-frequency handlers.
- Avoid blocking network calls on the primary logging path.
- Cache external lookup results where appropriate.
- Measure before and after when changing critical paths.

## Language Guidance

- Use Rust or C# for core performance-sensitive components.
- Treat Python as non-primary for runtime-critical logic.

