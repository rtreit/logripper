---
name: directory-build-organization
description: >-
  Guide for organizing MSBuild infrastructure with Directory.Build.props,
  Directory.Build.targets, Directory.Packages.props, and Directory.Build.rsp.
  Use when structuring multi-project repos, centralizing build settings,
  implementing NuGet Central Package Management, or consolidating duplicated
  properties across .csproj files.
---

# Organizing Build Infrastructure with Directory.Build Files

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## Directory.Build.props vs Directory.Build.targets

Understanding which file to use is critical. They differ in **when** they are imported during evaluation:

**Evaluation order:**

```
Directory.Build.props → SDK .props → YourProject.csproj → SDK .targets → Directory.Build.targets
```

| Use `.props` for | Use `.targets` for |
|---|---|
| Setting property defaults | Custom build targets |
| Common item definitions | Late-bound property overrides |
| Properties projects can override | Post-build steps |
| Assembly/package metadata | Conditional logic on final values |
| Analyzer PackageReferences | Targets that depend on SDK-defined properties |

**Rule of thumb:** Properties and items go in `.props`. Custom targets and late-bound logic go in `.targets`.

### ⚠️ Critical: TargetFramework Availability in .props vs .targets

**Property conditions on `$(TargetFramework)` in `.props` files silently fail for single-targeting projects** — the property is empty during `.props` evaluation. Move TFM-conditional properties to `.targets` instead.

## Directory.Build.props

Good candidates: language settings, assembly/package metadata, build warnings, code analysis, common analyzers.

```xml
<Project>
  <PropertyGroup>
    <LangVersion>latest</LangVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <EnforceCodeStyleInBuild>true</EnforceCodeStyleInBuild>
  </PropertyGroup>
</Project>
```

**Do NOT put here:** project-specific TFMs, targets/build logic, or properties depending on SDK-defined values.

## Directory.Build.targets

Good candidates: custom build targets, late-bound property overrides, post-build validation.

```xml
<Project>
  <Target Name="ValidateProjectSettings" BeforeTargets="Build">
    <Error Text="All libraries must target netstandard2.0 or higher"
           Condition="'$(OutputType)' == 'Library' AND '$(TargetFramework)' == 'net472'" />
  </Target>
</Project>
```

## Directory.Packages.props (Central Package Management)

Central Package Management (CPM) provides a single source of truth for all NuGet package versions.

```xml
<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>

  <ItemGroup>
    <PackageVersion Include="Grpc.Tools" Version="2.62.0" />
    <PackageVersion Include="Google.Protobuf" Version="3.26.0" />
    <PackageVersion Include="Grpc.Net.Client" Version="2.62.0" />
  </ItemGroup>

  <ItemGroup>
    <GlobalPackageReference Include="Microsoft.CodeAnalysis.NetAnalyzers" Version="8.0.0" />
  </ItemGroup>
</Project>
```

## Directory.Build.rsp

Default MSBuild CLI arguments applied to all builds under the directory tree:

```
/maxcpucount
/nodeReuse:false
/consoleLoggerParameters:Summary;ForceNoAlign
```

## Multi-level Directory.Build Files

MSBuild only auto-imports the **first** `Directory.Build.props` it finds. To chain multiple levels:

```xml
<Project>
  <Import Project="$([MSBuild]::GetPathOfFileAbove('Directory.Build.props', '$(MSBuildThisFileDirectory)../'))"
         Condition="Exists('$([MSBuild]::GetPathOfFileAbove('Directory.Build.props', '$(MSBuildThisFileDirectory)../'))')" />

  <!-- Inner-level overrides go here -->
</Project>
```

**Example layout:**

```
repo/
  Directory.Build.props          ← repo-wide (lang version, analyzers)
  Directory.Build.targets        ← repo-wide targets
  Directory.Packages.props       ← central package versions
  src/
    Directory.Build.props        ← src-specific (IsPackable=true)
  test/
    Directory.Build.props        ← test-specific (IsPackable=false, test packages)
```

## Artifact Output Layout (.NET 8+)

Set `<ArtifactsPath>$(MSBuildThisFileDirectory)artifacts</ArtifactsPath>` in `Directory.Build.props` for project-name-separated output directories.

## Workflow: Organizing Build Infrastructure

1. **Audit all `.csproj` files** — Catalog repeated properties and items
2. **Create root `Directory.Build.props`** — Move shared property defaults
3. **Create root `Directory.Build.targets`** — Move custom build targets and SDK-dependent properties
4. **Create `Directory.Packages.props`** — Enable CPM, list all `PackageVersion` entries
5. **Set up multi-level hierarchy** — Inner `.props` files for `src/` and `test/`
6. **Simplify `.csproj` files** — Remove all centralized settings
7. **Validate** — `dotnet restore && dotnet build`

## Troubleshooting

| Problem | Cause | Fix |
|---|---|---|
| `.props` isn't picked up | File name casing wrong | Verify exact casing: `Directory.Build.props` |
| Properties from `.props` ignored | Project overrides after import | Move to `.targets` |
| Multi-level import doesn't work | Missing `GetPathOfFileAbove` | Add `<Import>` at top of inner file |
| SDK values empty in `.props` | Not defined during `.props` evaluation | Move to `.targets` |

**Diagnosis:** Use `dotnet msbuild -pp:output.xml MyProject.csproj` to see all imports and final property values.
