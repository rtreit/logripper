# Client Integration Guide

This guide covers everything you need to connect a client application to a QsoRipper engine host over gRPC, or to stand up another engine implementation that honors the same contracts.

## Endpoint Defaults

QsoRipper does not have a single privileged engine endpoint. The built-in local profiles are:

| Profile | Engine ID | Default endpoint |
|---|---|---|
| `local-rust` | `rust-tonic` | `http://127.0.0.1:50051` |
| `local-dotnet` | `dotnet-aspnet` | `http://127.0.0.1:50052` |

Recommended local startup:

```powershell
.\start-qsoripper.ps1 -Engine local-rust
.\start-qsoripper.ps1 -Engine local-dotnet
```

`-ForceRestart` is scoped to the requested profile, so restarting `local-rust` does not stop `local-dotnet` (and vice versa).

## Shared Selector and Runtime Switching

All .NET clients use the shared selector rules from `QsoRipper.EngineSelection`:

1. explicit runtime/profile choice (when supported by the client)
2. `QSORIPPER_ENGINE` (legacy `QSORIPPER_ENGINE_IMPLEMENTATION`)
3. `QSORIPPER_ENDPOINT`
4. built-in profile defaults

Local running-engine discovery is based on launcher state under `artifacts\run\` (`qsoripper-*.state.json` plus legacy `qsoripper-engine*.json`) and validates entries with PID + transport checks before presenting them as active.

Current client behavior:

- `QsoRipper.Cli`: per-invocation `--engine` / `--endpoint` plus shared env fallback.
- `QsoRipper.DebugHost`: runtime profile/endpoint picker with probe/apply flow.
- `QsoRipper.Gui`: runtime switching from **Tools → Use Rust Engine / Use .NET Engine** and **Refresh Engine Status**.

Direct engine-host startup is also available when you want to work on a specific implementation:

```
cd src/rust
cargo run -p qsoripper-server
```

```
dotnet run --project src/dotnet/QsoRipper.Engine.DotNet/QsoRipper.Engine.DotNet.csproj
```

The Rust host listen address can be overridden at startup:

```
cargo run -p qsoripper-server -- --listen 0.0.0.0:50051
```

Or via environment variable:

```
QSORIPPER_SERVER_ADDR=0.0.0.0:50051 cargo run -p qsoripper-server
```

## Generating Client Stubs

The proto files under `proto/` are the authoritative contract. Use standard protobuf/gRPC tooling for your language to generate stubs from that contract rather than hand-writing client or server shapes.

QsoRipper follows protobuf 1-1-1 by default: one top-level entity per file, service files that contain only the `service`, and method-specific `XxxRequest` / `XxxResponse` envelopes for every RPC. Your code generation step therefore needs to include the split service support files, not just the `*service.proto` declarations.

### Prerequisites

Install the Protocol Buffers compiler:

```
# Windows
winget install Google.Protobuf

# Linux (Debian/Ubuntu)
sudo apt install protobuf-compiler

# macOS
brew install protobuf
```

Install `buf` (recommended for schema quality validation):

```
# Windows
winget install Bufbuild.Buf

# Linux / macOS
# See https://buf.build/docs/installation
```

### C# (.NET)

The repository uses `Grpc.Tools` for automatic C# code generation at MSBuild time. Add the proto files as a `<Protobuf>` item group in your `.csproj`:

```xml
<ItemGroup>
  <Protobuf Include="..\..\proto\domain\*.proto"
            ProtoRoot="..\..\proto"
            GrpcServices="None" />
  <Protobuf Include="..\..\proto\services\*.proto"
            ProtoRoot="..\..\proto"
            GrpcServices="None" />
  <Protobuf Update="..\..\proto\services\*service.proto"
            GrpcServices="Client" />
</ItemGroup>
```

Required packages:

```xml
<PackageReference Include="Google.Protobuf" Version="..." />
<PackageReference Include="Grpc.Net.Client" Version="..." />
<PackageReference Include="Grpc.Tools" Version="..." PrivateAssets="All" />
```

The repository’s .NET clients (`QsoRipper.Cli`, `QsoRipper.Gui`, and `QsoRipper.DebugHost`) are working examples of this pattern.

### Rust

The repository engine uses `prost` + `tonic-build` with a `build.rs` script. See `src/rust/qsoripper-core/build.rs` for the generation setup.

For a standalone Rust client, prefer recursive discovery so new split contract files are picked up automatically:

```rust
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("../../proto");
    let mut protos = Vec::new();
    collect_proto_files(&proto_root.join("domain"), &mut protos)?;
    collect_proto_files(&proto_root.join("services"), &mut protos)?;
    protos.sort();

    tonic_build::configure().compile_protos(&protos, &[&proto_root])?;
    Ok(())
}

fn collect_proto_files(
    directory: &Path,
    protos: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_proto_files(path.as_path(), protos)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("proto") {
            protos.push(path);
        }
    }
    Ok(())
}
```

Required dependencies in `Cargo.toml`:

```toml
[dependencies]
tonic = "0.x"
prost = "0.x"
tokio = { version = "1", features = ["full"] }

[build-dependencies]
tonic-build = "0.x"
```

### Go

```
protoc \
  --proto_path=proto \
  --go_out=gen \
  --go-grpc_out=gen \
  proto/domain/*.proto \
  proto/services/*.proto
```

### JavaScript / TypeScript (Node.js, not browser)

Use `@grpc/proto-loader` and `@grpc/grpc-js`:

```
npx grpc_tools_node_protoc \
  --proto_path=proto \
  --js_out=import_style=commonjs,binary:gen \
  --grpc_out=grpc_js:gen \
  proto/domain/*.proto \
  proto/services/*.proto
```

Or use `ts-proto` for TypeScript with full type generation:

```
npx protoc \
  --plugin=protoc-gen-ts_proto=./node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out=./gen \
  --proto_path=proto \
  proto/domain/*.proto \
  proto/services/*.proto
```

### Other Languages

Standard `protoc` invocations work for any language with a gRPC plugin. The proto files use proto3 syntax with no non-standard extensions.

## Native Clients (Desktop, TUI, CLI)

Native clients connect directly to whichever engine host they target with a standard gRPC channel over HTTP/2.

**C# example:**

```csharp
using Grpc.Net.Client;
using QsoRipper.Services;
using QsoRipper.Domain;

var channel = GrpcChannel.ForAddress("http://localhost:50051");
var client = new LookupService.LookupServiceClient(channel);

var response = await client.LookupAsync(new LookupRequest
{
    Callsign = "W1AW",
    SkipCache = false,
});

Console.WriteLine($"State: {response.Result.State}, Callsign: {response.Result.QueriedCallsign}");
```

**Rust example (tonic client):**

```rust
use my_client::qsoripper::services::lookup_service_client::LookupServiceClient;
use my_client::qsoripper::services::LookupRequest;

let mut client = LookupServiceClient::connect("http://127.0.0.1:50051").await?;
let response = client.lookup(LookupRequest {
    callsign: "W1AW".into(),
    skip_cache: false,
}).await?;
let result = response.into_inner().result.expect("lookup payload");
println!("State: {:?}", result.state);
```

## Browser and Web Clients

Browsers cannot issue native gRPC (HTTP/2 + binary framing) requests due to browser networking constraints. Web clients must use **gRPC-Web**, which is a modified protocol that works over standard HTTP/1.1 or HTTP/2 in a way browsers can handle.

The QsoRipper engine exposes native gRPC only. To connect a browser or web client, you need an intermediate proxy or gateway.

### gRPC-Web Options

**Option 1: Envoy proxy** (recommended for production)

Envoy supports the `grpc_web` HTTP filter that translates between gRPC-Web (browser) and native gRPC (engine):

```yaml
# Simplified Envoy config excerpt
http_filters:
  - name: envoy.filters.http.grpc_web
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.http.grpc_web.v3.GrpcWeb
  - name: envoy.filters.http.router
```

**Option 2: grpcwebproxy** (simpler, for development)

```
go install github.com/improbable-eng/grpc-web/go/grpcwebproxy@latest

grpcwebproxy \
  --backend_addr=localhost:50051 \
  --run_tls_server=false \
  --allow_all_origins
```

**Option 3: connect-go / connect-web**

The [Connect protocol](https://connectrpc.com/) is compatible with gRPC and adds HTTP/JSON support for browser clients without a separate proxy. This would require the engine server to add Connect support (not currently implemented).

### Generating Browser Client Stubs

Use `protoc-gen-grpc-web` for JavaScript/TypeScript browser clients:

```
protoc \
  --proto_path=proto \
  --js_out=import_style=commonjs:gen \
  --grpc-web_out=import_style=commonjs+dts,mode=grpcwebtext:gen \
  proto/domain/*.proto \
  proto/services/*.proto
```

Or use `@connectrpc/protoc-gen-connect-es` with `@bufbuild/protoc-gen-es` if you choose the Connect protocol.

### Important: No Native Browser gRPC

Do not attempt to connect a browser-side JavaScript client directly to `http://localhost:50051` with a standard gRPC client library — it will fail. The browser HTTP stack does not support the HTTP/2 framing that native gRPC requires. Always route browser traffic through a gRPC-Web–aware proxy.

## Schema Evolution and Compatibility

The current QsoRipper proto contract follows standard proto3 additive evolution rules **from the current 1-1-1 envelope baseline forward**.

> PR [#74](https://github.com/rtreit/qsoripper/pull/74) was a deliberate breaking-contract cleanup performed while the project is still early. Clients pinned to older pre-1-1-1 revisions must regenerate against the current `proto/` surface rather than assuming wire compatibility across that cutover.

- **New optional fields** may be added to any message in future releases without breaking existing clients.
- **New RPCs** may be added to existing services. Old clients will not call them.
- **Enum values** may be added. Clients should handle unknown enum integer values gracefully (proto3 preserves unknown enum values as their integer form).
- **Field numbers and types** should not be changed within the current published baseline. `buf breaking` is the guardrail for future changes against that baseline.

**Client tolerance recommendations:**
- Ignore unknown fields in responses — proto3 decoders do this by default.
- Use a `default` or fallback branch when switching on enum values so new values are handled gracefully.
- Do not rely on zero values to mean "absent" for `optional` fields in proto3 — use `has_*` checks (in languages that support them) or check for explicit presence.

## Buf for Schema Validation

To lint the proto files and check for breaking changes:

```
# Lint proto definitions
buf lint

# Check for incompatible changes against main
buf breaking --against '.git#branch=main'
```

The `buf.yaml` configuration lives at the repository root.
