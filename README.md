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

## Getting Started

### Prerequisites

**Rust toolchain** -- install via [rustup](https://rustup.rs/):

```
# Windows
winget install Rustlang.Rustup

# Linux (Debian/Ubuntu)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**.NET SDK 10** -- required for the .NET workspace under `src/dotnet/`, including the developer debug workbench and engine validation CLI:

```
# Windows
winget install Microsoft.DotNet.SDK.10

# Linux (Debian/Ubuntu)
sudo apt install dotnet-sdk-10.0
```

The repository pins SDK `10.0.201` in `global.json`.

**Protocol Buffers compiler** -- needed to generate gRPC code from proto files:

```
# Windows
winget install Google.Protobuf

# Linux (Debian/Ubuntu)
sudo apt install protobuf-compiler

# Linux (Fedora)
sudo dnf install protobuf-compiler
```

**C compiler** -- required for the native FFI libraries under `src/c/`. On Windows, install the "Desktop development with C++" workload in Visual Studio or the [Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/). On Linux, `gcc` or `clang` is typically already available; install with `sudo apt install build-essential` if needed. The `cc` crate finds the compiler automatically on both platforms.

**buf** (optional) -- for linting and breaking change detection on proto files:

```
# Windows
winget install Bufbuild.Buf

# Linux
# See https://buf.build/docs/installation
```

### Build and Test

**Rust engine:**

```
cd src/rust
cargo build
cargo test
```

This compiles the C libraries via FFI, generates Rust types from the proto files, and builds the engine. All tests (unit + integration) run with `cargo test`.

**Runnable gRPC server host:**

```
cd src/rust
cargo run -p logripper-server
```

This starts the developer gRPC server on `127.0.0.1:50051` by default so the .NET CLI and debug workbench can validate transport and service wiring against a live Rust host.

### QRZ local configuration

Use `.env.example` as the local template for QRZ settings:

```
Copy-Item .env.example .env
```

The QRZ credentials are easy to mix up, so keep this split in mind:

| Setting | What it must contain |
|---|---|
| `LOGRIPPER_QRZ_XML_USERNAME` | Your QRZ account username |
| `LOGRIPPER_QRZ_XML_PASSWORD` | Your **actual QRZ account password** for the XML lookup service |
| `LOGRIPPER_QRZ_LOGBOOK_API_KEY` | Your separate **QRZ Logbook API access key** from the QRZ website |

**Important:** `LOGRIPPER_QRZ_XML_PASSWORD` and `LOGRIPPER_QRZ_LOGBOOK_API_KEY` are **not** the same value and are **not** interchangeable. Using the logbook API key as the XML password will cause QRZ XML login failures and may trigger a temporary lockout.

For lockout-safe debugging, you can temporarily set:

```
LOGRIPPER_QRZ_XML_CAPTURE_ONLY=true
```

In capture mode, LogRipper builds the outgoing QRZ XML request and returns redacted request diagnostics without sending any HTTP traffic to QRZ.

### Local lookup debug workflow

For local QRZ lookup debugging, use two terminals: one for the Rust engine and one for the .NET debug workbench.

1. Copy the local template and fill in your real QRZ values:

   ```powershell
   Copy-Item .env.example .env
   notepad .env
   ```

2. Optional PowerShell profile helper for loading `.env` into the current terminal session:

   ```powershell
   # Load .env file into current terminal session
   function loadenv {
       Get-Content .env | ForEach-Object {
           if ($_ -notmatch '^\s*#' -and $_ -match '=') {
               $name, $value = $_ -split '=', 2
               Set-Item -Path "Env:$($name.Trim())" -Value $value.Trim()
           }
       }
   }

   Set-Alias -Name env -Value loadenv
   ```

   After adding that to your PowerShell profile, run `env` from the repository root whenever you want to load `.env` into the current shell.

3. Start the Rust engine in the first terminal:

   ```powershell
   Set-Location C:\path\to\logripper
   env
   Set-Location src\rust
   cargo run -p logripper-server
   ```

   The developer engine listens on `http://localhost:50051` by default.

4. Start the developer debug workbench in a second terminal:

   ```powershell
   Set-Location C:\path\to\logripper
   env
   Set-Location src\dotnet
   dotnet run --project LogRipper.DebugHost
   ```

5. Open the workbench in a browser:

   ```
   http://localhost:5082/lookup-workbench
   ```

6. Exercise the lookup flow:

   - **Live lookup** calls the engine's unary lookup.
   - **Stream lookup** shows the state transition flow.
   - **Cache lookup** checks only the engine cache.

If you want to inspect request shape without touching QRZ, set `LOGRIPPER_QRZ_XML_CAPTURE_ONLY=true` in `.env`, run `env` again in the engine shell, and restart `logripper-server`.

**.NET workspace:**

```
cd src/dotnet
dotnet build LogRipper.slnx
dotnet test LogRipper.slnx
```

This builds the shared .NET workspace, including the developer debug host and the CLI tool that validates engine connectivity over gRPC.

### Code Coverage

Both the Rust engine and .NET components are instrumented for coverage on every CI run. Coverage reports are uploaded as workflow artifacts.

**Thresholds:**

| Surface  | Tool             | Threshold | Measured baseline |
|----------|------------------|-----------|-------------------|
| Rust     | cargo-llvm-cov   | 80% lines | ~86% lines        |
| .NET     | Coverlet         | 8% lines  | ~10% lines        |

> **Note:** The .NET threshold is intentionally low because coverage is currently skewed by auto-generated protobuf/gRPC stubs which have no direct unit tests. The hand-written service and model code has significantly higher coverage. Ratchet the threshold up incrementally as tests are added and generated code is excluded.

**Run Rust coverage locally** (requires `llvm-tools-preview` component and `cargo-llvm-cov`):

```
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cd src/rust
cargo llvm-cov --all --open
```

To check the threshold locally:

```
cd src/rust
cargo llvm-cov --all --fail-under-lines 80
```

**Run .NET coverage locally** (requires `dotnet-reportgenerator-globaltool`):

```
dotnet tool install -g dotnet-reportgenerator-globaltool
cd src/dotnet
dotnet test LogRipper.slnx --collect:"XPlat Code Coverage" --results-directory coverage
reportgenerator -reports:"coverage/**/coverage.cobertura.xml" -targetdir:"coverage/report" -reporttypes:"Html"
```

Then open `src/dotnet/coverage/report/index.html` in a browser.

### Developer Debug Workbench

The repository now includes a **developer-only Blazor Server debug host** under `src/dotnet/LogRipper.DebugHost`. This is not the product logger UX. It is an internal workbench for:

- configuring and probing a local Rust engine endpoint
- inspecting generated protobuf payloads and sample data
- exercising the callsign lookup flow as live gRPC services come online
- running curated Rust/.NET validation commands from a safe allowlist

Build, test, and run it with:

```
cd src/dotnet
dotnet build LogRipper.Debug.sln
dotnet test LogRipper.Debug.sln
dotnet run --project LogRipper.DebugHost
```

### Engine Validation CLI

The repository also includes a minimal **.NET 10 CLI tool** under `src/dotnet/LogRipper.Cli` for validating connectivity to the Rust engine over gRPC.

Run it with:

```
cd src/dotnet
dotnet run --project LogRipper.Cli -- status
dotnet run --project LogRipper.Cli -- --endpoint http://localhost:50051 status
```

The CLI generates client stubs from the shared proto contracts at build time and currently includes a `status` command for `LogbookService.GetSyncStatus`.

## Project Structure

```
proto/                    Shared IDL (language-neutral)
  domain/                 CallsignRecord, QsoRecord, LookupResult, enums
  services/               LookupService, LogbookService gRPC definitions
src/
  rust/                   Rust workspace (Cargo.toml at this level)
    logripper-core/       Engine: storage, lookups, cache, ADIF, gRPC server
  dotnet/
    LogRipper.slnx        Root .NET workspace solution
    LogRipper.Cli/        Minimal CLI for engine validation over gRPC
    LogRipper.DebugHost/  Developer-only Blazor Server debug workbench
    LogRipper.DebugHost.Tests/  Tests for debug-host services and payload builders
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
