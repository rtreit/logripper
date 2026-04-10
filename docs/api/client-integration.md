# Client Integration Guide

This guide covers everything you need to connect a client application to the LogRipper engine over gRPC.

## Endpoint Defaults

The engine listens on `http://127.0.0.1:50051` by default when started with:

```
cd src/rust
cargo run -p logripper-server
```

The listen address can be overridden at startup:

```
cargo run -p logripper-server -- --listen 0.0.0.0:50051
```

Or via environment variable:

```
LOGRIPPER_SERVER_ADDR=0.0.0.0:50051 cargo run -p logripper-server
```

## Generating Client Stubs

The proto files under `proto/` are the authoritative contract. Use standard protobuf/gRPC tooling for your language to generate client stubs.

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
  <Protobuf Include="..\..\proto\services\lookup_service.proto" GrpcServices="Client" />
  <Protobuf Include="..\..\proto\services\logbook_service.proto" GrpcServices="Client" />
  <Protobuf Include="..\..\proto\domain\callsign.proto" GrpcServices="None" />
  <Protobuf Include="..\..\proto\domain\lookup.proto" GrpcServices="None" />
  <Protobuf Include="..\..\proto\domain\qso.proto" GrpcServices="None" />
</ItemGroup>
```

Required packages:

```xml
<PackageReference Include="Google.Protobuf" Version="..." />
<PackageReference Include="Grpc.Net.Client" Version="..." />
<PackageReference Include="Grpc.Tools" Version="..." PrivateAssets="All" />
```

The debug workbench under `src/dotnet/LogRipper.DebugHost/` is a working example of this pattern.

### Rust

The repository engine uses `prost` + `tonic-build` with a `build.rs` script. See `src/rust/logripper-core/build.rs` for the generation setup.

For a standalone client (not using the engine crate), add to `build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile_protos(
            &[
                "../../proto/services/lookup_service.proto",
                "../../proto/services/logbook_service.proto",
            ],
            &["../../proto"],
        )?;
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
  proto/services/lookup_service.proto \
  proto/services/logbook_service.proto \
  proto/domain/*.proto
```

### JavaScript / TypeScript (Node.js, not browser)

Use `@grpc/proto-loader` and `@grpc/grpc-js`:

```
npx grpc_tools_node_protoc \
  --proto_path=proto \
  --js_out=import_style=commonjs,binary:gen \
  --grpc_out=grpc_js:gen \
  proto/services/lookup_service.proto \
  proto/services/logbook_service.proto \
  proto/domain/*.proto
```

Or use `ts-proto` for TypeScript with full type generation:

```
npx protoc \
  --plugin=protoc-gen-ts_proto=./node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out=./gen \
  --proto_path=proto \
  proto/services/lookup_service.proto \
  proto/services/logbook_service.proto \
  proto/domain/*.proto
```

### Other Languages

Standard `protoc` invocations work for any language with a gRPC plugin. The proto files use proto3 syntax with no non-standard extensions.

## Native Clients (Desktop, TUI, CLI)

Native clients connect directly to the engine with a standard gRPC channel over HTTP/2.

**C# example:**

```csharp
using Grpc.Net.Client;
using LogRipper.Services;
using LogRipper.Domain;

var channel = GrpcChannel.ForAddress("http://localhost:50051");
var client = new LookupService.LookupServiceClient(channel);

var result = await client.LookupAsync(new LookupRequest
{
    Callsign = "W1AW",
    SkipCache = false,
});

Console.WriteLine($"State: {result.State}, Callsign: {result.QueriedCallsign}");
```

**Rust example (tonic client):**

```rust
use logripper::services::lookup_service_client::LookupServiceClient;
use logripper::domain::LookupRequest;

let mut client = LookupServiceClient::connect("http://127.0.0.1:50051").await?;
let response = client.lookup(LookupRequest {
    callsign: "W1AW".into(),
    skip_cache: false,
}).await?;
println!("State: {:?}", response.into_inner().state);
```

## Browser and Web Clients

Browsers cannot issue native gRPC (HTTP/2 + binary framing) requests due to browser networking constraints. Web clients must use **gRPC-Web**, which is a modified protocol that works over standard HTTP/1.1 or HTTP/2 in a way browsers can handle.

The LogRipper engine exposes native gRPC only. To connect a browser or web client, you need an intermediate proxy or gateway.

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
  proto/services/lookup_service.proto \
  proto/services/logbook_service.proto \
  proto/domain/*.proto
```

Or use `@connectrpc/protoc-gen-connect-es` with `@bufbuild/protoc-gen-es` if you choose the Connect protocol.

### Important: No Native Browser gRPC

Do not attempt to connect a browser-side JavaScript client directly to `http://localhost:50051` with a standard gRPC client library — it will fail. The browser HTTP stack does not support the HTTP/2 framing that native gRPC requires. Always route browser traffic through a gRPC-Web–aware proxy.

## Schema Evolution and Compatibility

The LogRipper proto contract follows standard proto3 additive evolution rules:

- **New optional fields** may be added to any message in future releases without breaking existing clients.
- **New RPCs** may be added to existing services. Old clients will not call them.
- **Enum values** may be added. Clients should handle unknown enum integer values gracefully (proto3 preserves unknown enum values as their integer form).
- **Field numbers and types** are never changed. `buf breaking` enforces this in CI.

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
