---
name: run-tests
description: >-
  Runs .NET tests with dotnet test. Use when asked to run tests, execute tests,
  apply test filters, detect test platform (VSTest or Microsoft.Testing.Platform),
  identify test framework, or troubleshoot test execution failures. Covers MSTest,
  xUnit, NUnit, and TUnit across both VSTest and MTP platforms.
---

# Run .NET Tests

Detect the test platform and framework, run tests, and apply filters using `dotnet test`.

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## When to Use

- User wants to run tests in a .NET project
- User needs to run a subset of tests using filters
- User needs help detecting which test platform (VSTest vs MTP) or framework is in use
- User wants to understand the correct filter syntax for their setup

## When Not to Use

- User needs to write or generate test code (use `code-testing`)
- User wants to iterate on failing tests without rebuilding
- User needs CI/CD pipeline configuration
- User needs to debug a test

## Inputs

| Input | Required | Description |
|-------|----------|-------------|
| Project or solution path | No | Path to test project (.csproj) or solution (.sln). Defaults to current directory. |
| Filter expression | No | Filter expression to select specific tests |
| Target framework | No | Target framework moniker to run against (e.g., `net8.0`) |

## Workflow

### Quick Reference

| Platform | SDK | Command pattern |
|----------|-----|----------------|
| VSTest | Any | `dotnet test [<path>] [--filter <expr>] [--logger trx]` |
| MTP | 8 or 9 | `dotnet test [<path>] -- <MTP_ARGS>` |
| MTP | 10+ | `dotnet test --project <path> <MTP_ARGS>` |

**Detection files to always check** (in order): `global.json` -> `.csproj` -> `Directory.Build.props` -> `Directory.Packages.props`

### Step 1: Detect the test platform and framework

1. Read `global.json` first — on .NET SDK 10+, `"test": { "runner": "Microsoft.Testing.Platform" }` is the **authoritative MTP signal**.
2. Read `.csproj`, `Directory.Build.props`, and `Directory.Packages.props` for framework packages and MTP properties.

**Quick detection summary:**

| Signal | Means |
|--------|-------|
| `global.json` has `"test": { "runner": "Microsoft.Testing.Platform" }` | **MTP on SDK 10+** — pass args directly, no `--` |
| `<TestingPlatformDotnetTestSupport>true` in csproj or Directory.Build.props | **MTP on SDK 8/9** — pass args after `--` |
| Neither signal present | **VSTest** |

### Step 2: Run tests

#### VSTest (any .NET SDK version)

```bash
dotnet test [<PROJECT> | <SOLUTION> | <DIRECTORY> | <DLL> | <EXE>]
```

Common flags:

| Flag | Description |
|------|-------------|
| `--framework <TFM>` | Target a specific framework in multi-TFM projects |
| `--no-build` | Skip build, use previously built output |
| `--filter <EXPRESSION>` | Run selected tests |
| `--logger trx` | Generate TRX results file |
| `--collect "Code Coverage"` | Collect code coverage |
| `--blame-hang-timeout <duration>` | Abort test if it hangs longer than duration |
| `-v <level>` | Verbosity: `quiet`, `minimal`, `normal`, `detailed`, `diagnostic` |

#### MTP with .NET SDK 8 or 9

MTP-specific arguments must be passed after `--`:

```bash
dotnet test [<PROJECT>] -- <MTP_ARGUMENTS>
```

#### MTP with .NET SDK 10+

```bash
dotnet test --project <PROJECT_OR_DIRECTORY> [<MTP_ARGUMENTS>]
```

### Step 3: Run filtered tests

Key filter syntax:

- **VSTest** (MSTest, xUnit v2, NUnit): `dotnet test --filter <EXPRESSION>` with `=`, `!=`, `~`, `!~` operators
- **MTP -- MSTest and NUnit**: Same `--filter` syntax as VSTest
- **MTP -- xUnit v3**: Uses `--filter-class`, `--filter-method`, `--filter-trait`
- **MTP -- TUnit**: Uses `--treenode-filter` with path-based syntax

## Common Pitfalls

| Pitfall | Solution |
|---------|----------|
| Missing `Microsoft.NET.Test.Sdk` in a VSTest project | Tests won't be discovered. Add `<PackageReference Include="Microsoft.NET.Test.Sdk" />` |
| Using VSTest `--filter` syntax with xUnit v3 on MTP | xUnit v3 on MTP uses `--filter-class`, `--filter-method`, etc. |
| Passing MTP args without `--` on .NET SDK 8/9 | Before .NET 10, MTP args must go after `--` |
| Using `--` for MTP args on .NET SDK 10+ | On .NET 10+, MTP args are passed directly — do NOT use `--` |
| Multi-TFM project runs tests for all frameworks | Use `--framework <TFM>` to target a specific framework |
