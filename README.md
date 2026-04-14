# QsoRipper

High-performance ham radio logging engine built for speed, clean workflows, and keyboard-first operation.

## Architecture

QsoRipper is an **engine-first** project. The core engine handles logging, lookups, caching, and sync. It exposes all functionality through a gRPC API and has zero knowledge of any particular UI. Any number of UX implementations can be built on top of the engine: a terminal UI in Rust, a desktop GUI in .NET or Electron, a web frontend, a mobile app, or a voice-driven interface for accessibility. The engine doesn't know or care what's calling it.

```
┌─────────────────────────────────────────────┐
│  Engine (Rust)                              │
│                                             │
│  qsoripper-core:                            │
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
- **Contract shape**: protobuf 1-1-1 is the default — one top-level entity per file, service files that contain only the `service`, and unique `XxxRequest` / `XxxResponse` envelopes for every RPC

### gRPC Services

| Service | Purpose |
|---|---|
| **SetupService** | First-run setup, persisted config status, bootstrap storage/station defaults |
| **StationProfileService** | Persisted station profile CRUD, active profile selection, bounded session overrides |
| **LookupService** | Callsign lookups -- single, streaming, batch, cached, DXCC |
| **LogbookService** | QSO CRUD, QRZ logbook sync, ADIF import/export |

**Building a client?** See the [Engine API Documentation](docs/api/README.md) for a client-facing reference covering service contracts, implementation status, stub generation, transport options, and workflow examples.

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

**Repository build script:**

```powershell
.\build.ps1
.\build.ps1 -Configuration Debug
.\build.ps1 check
```

By default, `.\build.ps1` builds the Rust workspace in **Release**, publishes the Native AOT CLI to `artifacts\publish\QsoRipper.Cli\Release\`, and publishes the desktop GUI to `artifacts\publish\QsoRipper.Gui\Release\`. Use `-Configuration Debug` to switch the Rust build and both .NET publish outputs to `Debug`.

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
cargo run -p qsoripper-server
```

This starts the developer gRPC server on `127.0.0.1:50051` by default so the .NET CLI and debug workbench can validate transport and service wiring against a live Rust host.

The server can now swap storage implementations at startup:

```powershell
cd src\rust
cargo run -p qsoripper-server -- --storage memory
cargo run -p qsoripper-server -- --storage sqlite --sqlite-path .\data\qsoripper.db
cargo run -p qsoripper-server -- --config .\config\qsoripper.toml
```

Equivalent environment variables are also supported:

```powershell
$env:QSORIPPER_STORAGE_BACKEND = "sqlite"
$env:QSORIPPER_SQLITE_PATH = ".\data\qsoripper.db"
$env:QSORIPPER_CONFIG_PATH = ".\config\qsoripper.toml"
cargo run -p qsoripper-server
```

When no explicit config override is provided, the server uses a persisted setup file at:

- **Windows:** `%APPDATA%\qsoripper\config.toml`
- **Linux:** `~/.config/qsoripper/config.toml` (or `XDG_CONFIG_HOME`)

If you want the engine to stay running in the background while you log QSOs from another terminal, use the repo-root helper scripts:

```powershell
.\start-qsoripper.ps1
.\artifacts\publish\QsoRipper.Cli\Release\QsoRipper.Cli.exe log W1AW 20m FT8
.\stop-qsoripper.ps1
```

`start-qsoripper.ps1` builds `qsoripper-server`, imports `.env`, starts the engine in the background from the repository root, respects `QSORIPPER_CONFIG_PATH` and `QSORIPPER_SQLITE_PATH` unless you override them with parameters, and writes process state plus stdout/stderr logs under `artifacts\run\`.

### Local engine configuration

Use `.env.example` as the local template for QRZ settings and optional local-station defaults:

```
Copy-Item .env.example .env
```

Common local overrides include:

```powershell
QSORIPPER_CONFIG_PATH=C:\Users\yourname\OneDrive\qsoripper\config.toml
QSORIPPER_SQLITE_PATH=C:\Users\yourname\OneDrive\qsoripper\qsoripper.db
```

The QRZ credentials are easy to mix up, so keep this split in mind:

| Setting | What it must contain |
|---|---|
| `QSORIPPER_QRZ_XML_USERNAME` | Your QRZ account username |
| `QSORIPPER_QRZ_XML_PASSWORD` | Your **actual QRZ account password** for the XML lookup service |
| `QSORIPPER_QRZ_LOGBOOK_API_KEY` | Your separate **QRZ Logbook API access key** from the QRZ website |

**Important:** `QSORIPPER_QRZ_XML_PASSWORD` and `QSORIPPER_QRZ_LOGBOOK_API_KEY` are **not** the same value and are **not** interchangeable. Using the logbook API key as the XML password will cause QRZ XML login failures and may trigger a temporary lockout.

For lockout-safe debugging, you can temporarily set:

```
QSORIPPER_QRZ_XML_CAPTURE_ONLY=true
```

In capture mode, QsoRipper builds the outgoing QRZ XML request and returns redacted request diagnostics without sending any HTTP traffic to QRZ.

You can also set `QSORIPPER_STATION_*` values in `.env` to define the active station profile that the Rust engine snapshots into newly logged QSOs.

For the new first-run bootstrap surface, `SetupService` persists the engine's log file path, initial station profile, and optional QRZ XML credentials to `config.toml`, then hot-applies those persisted values to the running engine. After setup, `StationProfileService` manages additional station profiles, persisted active-profile selection, and bounded in-memory session overrides for portable or event operation. The Debug Host `/engine` page now exposes setup and station-profile editor forms for these contract surfaces, so local bootstrap/profile lifecycle testing no longer requires `grpcurl`.

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
   Set-Location C:\path\to\qsoripper
   env
   Set-Location src\rust
   cargo run -p qsoripper-server
   ```

   The developer engine listens on `http://localhost:50051` by default.

4. Start the developer debug workbench in a second terminal:

   ```powershell
   Set-Location C:\path\to\qsoripper
   env
   Set-Location src\dotnet
   dotnet run --project QsoRipper.DebugHost
   ```

5. Open the workbench in a browser:

   ```
   http://localhost:5082/lookup-workbench
   ```

6. Exercise the lookup flow:

   - **Live lookup** calls the engine's unary lookup.
   - **Stream lookup** shows the state transition flow.
   - **Cache lookup** checks only the engine cache.

If you want to inspect request shape without touching QRZ, set `QSORIPPER_QRZ_XML_CAPTURE_ONLY=true` in `.env`, run `env` again in the engine shell, and restart `qsoripper-server`.

**.NET workspace:**

```
cd src/dotnet
dotnet build QsoRipper.slnx
dotnet test QsoRipper.slnx
```

This builds the shared .NET workspace, including the developer debug host and the CLI project used for engine validation over gRPC. To publish the Native AOT CLI from the repository root, use `.\build.ps1` or `.\build.ps1 dotnet`.

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
dotnet test QsoRipper.slnx --collect:"XPlat Code Coverage" --results-directory coverage
reportgenerator -reports:"coverage/**/coverage.cobertura.xml" -targetdir:"coverage/report" -reporttypes:"Html"
```

Then open `src/dotnet/coverage/report/index.html` in a browser.

### Developer Debug Workbench

The repository now includes a **developer-only Blazor Server debug host** under `src/dotnet/QsoRipper.DebugHost`. This is not the product logger UX. It is an internal workbench for:

- configuring and probing a local Rust engine endpoint
- inspecting generated protobuf payloads and sample data
- exercising the callsign lookup flow as live gRPC services come online
- running curated Rust/.NET validation commands from a safe allowlist

Build, test, and run it with:

```
cd src/dotnet
dotnet build QsoRipper.Debug.sln
dotnet test QsoRipper.Debug.sln
dotnet run --project QsoRipper.DebugHost
```

### Engine Validation CLI

The repository also includes a minimal **.NET 10 CLI tool** under `src/dotnet/QsoRipper.Cli` for validating connectivity to the Rust engine over gRPC.

Run it directly from source with:

```
cd src/dotnet
dotnet run --project QsoRipper.Cli -- status
dotnet run --project QsoRipper.Cli -- --endpoint http://localhost:50051 status
```

Or publish the Native AOT build and run the produced executable:

```powershell
.\build.ps1 dotnet
.\artifacts\publish\QsoRipper.Cli\Release\QsoRipper.Cli.exe status
```

The CLI generates client stubs from the shared proto contracts at build time and currently includes commands for status, lookup, and local logbook operations over gRPC.

## Project Structure

```
proto/                    Shared IDL (language-neutral)
  domain/                 Reusable domain messages/enums (one top-level entity per file)
  services/               Service declarations plus per-RPC envelopes/support types (one top-level entity per file)
src/
  rust/                   Rust workspace (Cargo.toml at this level)
    qsoripper-core/       Engine: storage, lookups, cache, ADIF, gRPC server
  dotnet/
    QsoRipper.slnx        Root .NET workspace solution
    QsoRipper.Cli/        Minimal CLI for engine validation over gRPC
    QsoRipper.DebugHost/  Developer-only Blazor Server debug workbench
    QsoRipper.DebugHost.Tests/  Tests for debug-host services and payload builders
  c/                      Native C libraries called by the engine via FFI
    qsoripper-dsp/        Signal processing helpers (DSP, filtering, audio)
tests/
  fixtures/               Shared test data (ADIF files, etc.)
docs/
  architecture/           Data model docs, design decisions
  integrations/           ADIF spec reference, provider notes
```

## License

MIT
