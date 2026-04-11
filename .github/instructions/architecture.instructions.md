# Architecture Instructions

## Purpose

This document defines the architectural principles for **Logripper**, with a focus on:

- Extreme performance, especially perceived latency in the TUI
- Clean, minimal abstractions
- Long-term extensibility without over-engineering

The guiding goal:

> Typing a callsign should feel instant. Everything else is background enrichment.

## Core Principles

### 1. Optimize for Time-to-First-Useful-Data

- UI must never block on network calls.
- Cached data should render immediately.
- Remote data should update the UI asynchronously.
- Always prefer progressive enhancement over waiting.

### 2. Keep the TUI Thin

The TUI is responsible for:

- Capturing user input
- Rendering state
- Managing interaction such as focus and navigation

The TUI must not:

- Call external APIs directly
- Handle retries, caching, or parsing
- Know about QRZ or other provider specifics

### 3. Use a Single App-Facing Interface

All UX interactions should go through a single interface, such as `CallsignLookup`.

This interface represents user intent, not provider behavior.

Conceptual flow:

```text
TUI
  -> CallsignLookup
      -> LookupCoordinator
          -> CallsignProvider
              -> QrzProvider
```

### 4. One Component Owns Lookup Orchestration

`LookupCoordinator` owns:

- Debounce logic
- Cancellation of stale requests
- In-flight deduplication
- Cache policy
- Background refresh
- Mapping results into UI state

This is where performance lives.

### 5. Keep Providers at the Edge

Each external system such as QRZ should:

- Be isolated behind `CallsignProvider`
- Handle its own authentication
- Handle its own session lifecycle
- Handle parsing and provider-specific error behavior

No provider-specific logic should leak outside the adapter.

### 6. Normalize Data Immediately

Never expose raw provider responses.

Convert provider data into a project-owned domain model such as `CallsignRecord`.

This ensures:

- Stability across providers
- Clean internal contracts
- Easier future expansion

### 7. Avoid Abstraction Chains

Only introduce abstractions when they:

- Own real logic or policy
- Decouple unstable components

Do not create forwarding chains like:

```text
Service -> Manager -> Repository -> Client
```

unless each layer has a distinct responsibility.

Rule: if a layer mostly forwards calls, delete it.

### 8. Prefer Consumer-Driven Interfaces

The UI defines what it needs.

Providers adapt to that interface.

Do not let XML schemas, HTTP responses, or provider quirks dictate internal design.

### 9. Treat Caching as a First-Class Feature

Caching is part of the performance architecture:

- L1 in-memory cache
- Negative cache for not-found callsigns
- In-flight request deduplication
- Optional persistence later if measurement proves it matters

Goal:

> The system should feel like it already knows common callsigns.

### 10. Trigger Lookups Intelligently

Do not fire remote lookups on every keystroke without policy.

Use meaningful triggers such as:

- A short debounce after typing stabilizes
- Enter or submit
- Focus change to a candidate row
- Explicit or predictive prefetch where it helps

### 11. Use Structured Concurrency and Cancellation

If the user types `K`, then `K7`, then `K7A`, stale work should be cancelled or ignored deterministically.

Only the newest result should win.

### 12. Instrument Before You Micro-Optimize

Measure:

- Cache hit rate
- Lookup latency
- QRZ auth refresh count
- Cancelled lookup count
- Duplicate request suppression
- Render and update latency

Performance work should be evidence-driven.

### 13. Test the Seams

Focus tests on:

- Domain logic and state transitions
- Provider adapters and QRZ session behavior
- TUI and app flow with fake providers

This keeps the core stable while letting the adapter evolve.

### 14. Build Vertically First

The first milestone should be one thin end-to-end slice:

```text
typed callsign
  -> app-facing lookup interface
  -> cache check
  -> background QRZ fetch
  -> normalized result
  -> UI state update
  -> metrics emitted
```

## Team Rule of Thumb

**Stable core, volatile edges.**

Core types, lookup state, and UX flow should belong to Logripper.

QRZ, XML, HTTP, auth, and future providers are edge concerns.

## Data Model Layer

### Schema-First with Protocol Buffers

All shared domain types are defined in `.proto` files under `proto/`. These are the single source of truth:

- `proto/domain/callsign_record.proto` — CallsignRecord
- `proto/domain/dxcc_entity.proto` — DxccEntity
- `proto/domain/qso_record.proto` — QsoRecord
- `proto/domain/lookup_result.proto` — LookupResult
- `proto/domain/lookup_state.proto` — LookupState
- `proto/services/lookup_service.proto` — gRPC LookupService declaration
- `proto/services/logbook_service.proto` — gRPC LogbookService declaration

Code is generated for both Rust (`prost`/`tonic`) and C# (`Grpc.Tools`). Never hand-write types that should come from proto generation.

Treat protobuf 1-1-1 as an architectural rule:

- one top-level message, enum, or service per `.proto` file by default
- service declaration files contain only the `service`
- every RPC gets unique `XxxRequest` and `XxxResponse` envelopes
- reusable payloads are extracted into dedicated domain or service support messages and nested inside envelopes
- exceptions are rare, must be explicit and documented, and never apply to per-RPC envelopes

### Language Split

- **Rust** = Core engine, TUI, QRZ providers, gRPC server (tonic)
- **C# / .NET** = GUI (Avalonia), reporting, analytics, gRPC client

The Rust process is the engine. The .NET process is the rich client. They communicate via gRPC.

### ADIF Is an Edge Concern

ADIF is the external interchange format for QRZ logbook API and file import/export. It is **not** used for internal IPC. The Rust-side ADIF parser converts to/from proto domain types at the provider edge.

See `docs/architecture/data-model.md` for full details, conventions, and how to add fields.

## One-Sentence Project Philosophy

**Build for instant UX with the fewest abstractions that preserve clean boundaries.**
