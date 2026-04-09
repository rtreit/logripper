# LogRipper

High-performance ham radio logging engine built for speed, clean workflows, and keyboard-first operation.

## Architecture

LogRipper is an **engine-first** project. The core engine handles logging, lookups, caching, and sync. It exposes all functionality through a gRPC API and has zero knowledge of any particular UI. Any number of UX implementations can be built on top of the engine: a terminal UI in Rust, a desktop GUI in .NET or Electron, a web frontend, a mobile app, or a voice-driven interface for accessibility. The engine doesn't know or care what's calling it.

```
┌─────────────────────────────────────────────┐
│  Engine (Rust)                              │
│                                             │
│  logripper-core:                            │
│    Log storage, QSO CRUD, QRZ lookups,      │
│    cache, ADIF parser, gRPC server (tonic)  │
└──────────────────┬──────────────────────────┘
                   │ gRPC (protobuf)
       ┌───────────┼───────────┐
       ▼           ▼           ▼
   ┌────────┐ ┌────────┐ ┌────────┐
   │ TUI    │ │Desktop │ │ Web /  │
   │        │ │  GUI   │ │ other  │
   └────────┘ └────────┘ └────────┘
```

**Rust** owns the core engine, QRZ providers, and gRPC server. UX implementations are independent consumers of the gRPC API and can be written in any language or framework. Nothing about the project requires any particular UI technology.

### UX Decoupling

The engine exposes a complete gRPC contract. Any UX implementation only needs a gRPC client to interact with the engine. Examples of possible frontends:

- A **terminal UI** built with ratatui, crossterm, or any TUI library in any language.
- A **native desktop GUI** using Avalonia, WPF, Win32, GTK, Qt, or similar.
- A **web UI**, **mobile app**, or **CLI tool**.
- Multiple UIs can run simultaneously against the same engine instance.

No UX implementation is privileged. The gRPC contract is the only interface.

### Protocol Buffers

Proto files under `proto/` are the **single source of truth** for all shared types (`QsoRecord`, `CallsignRecord`, `LookupResult`, bands, modes, etc.). Code can be generated for any consuming language -- zero hand-duplicated types:

- **Rust** (engine): `prost` + `tonic-build` generate structs and gRPC server stubs
- **Any client language**: standard protobuf/gRPC tooling generates client stubs (e.g., `Grpc.Tools` for C#, `protoc-gen-go` for Go, `grpc-web` for browsers)
- **Schema quality**: `buf lint` and `buf breaking` enforce conventions and backward compatibility

### gRPC Services

| Service | Purpose |
|---|---|
| **LookupService** | Callsign lookups -- single, streaming, batch, cached, DXCC |
| **LogbookService** | QSO CRUD, QRZ logbook sync, ADIF import/export |

### ADIF

ADIF (Amateur Data Interchange Format) is used **only at the edges** -- QRZ API calls and file I/O. Internal communication always uses protobuf. The Rust ADIF parser converts to/from proto types at the boundary, with an `extra_fields` map for lossless round-tripping.

## Project Structure

```
proto/                    Shared IDL (language-neutral)
  domain/                 CallsignRecord, QsoRecord, LookupResult, enums
  services/               LookupService, LogbookService gRPC definitions
src/
  rust/                   Rust workspace (Cargo.toml at this level)
    logripper-core/       Engine: storage, lookups, cache, ADIF, gRPC server
  c/                      Native C libraries called by the engine via FFI
    logripper-dsp/        Signal processing helpers (DSP, filtering, audio)
tests/
  fixtures/               Shared test data (ADIF files, etc.)
docs/
  architecture/           Data model docs, design decisions
  integrations/           ADIF spec reference, provider notes
```

## License

MIT
