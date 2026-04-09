---
name: including-generated-files
description: >-
  Fix MSBuild targets that generate files during the build but those files are
  missing from compilation or output. Use when generated source files are not
  compiling, custom build tasks create files that are invisible to subsequent
  targets, or globs are not capturing build-generated files. Covers correct
  BeforeTargets timing, adding to Compile/FileWrites item groups, and using
  IntermediateOutputPath. Relevant for Grpc.Tools and other code generators.
---

# Including Generated Files Into Your Build

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## Overview

Files generated during the build are generally ignored by the build process. This leads to:
- Generated files not being included in the output directory
- Generated source files not being compiled
- Globs not capturing files created during the build

This happens because of MSBuild's two-phase evaluation model.

## Quick Takeaway

For code files generated during the build — add those to `Compile` and `FileWrites` item groups within the generating target:

```xml
<ItemGroup>
  <Compile Include="$(GeneratedFilePath)" />
  <FileWrites Include="$(GeneratedFilePath)" />
</ItemGroup>
```

The target should be hooked before CoreCompile: `BeforeTargets="CoreCompile;BeforeCompile"`

## Why Generated Files Are Ignored

### Evaluation Phase

MSBuild reads your project, imports everything, creates Properties, expands globs for Items **outside of Targets**, and sets up the build process.

### Execution Phase

MSBuild runs Targets & Tasks with the provided Properties & Items to perform the build.

**Key Takeaway:** Files generated during execution don't exist during evaluation, therefore they aren't found. This particularly affects files that are globbed by default, such as `.cs` source files.

## Solution: Manually Add Generated Files

### Use `$(IntermediateOutputPath)` for Generated File Location

Always use `$(IntermediateOutputPath)` as the base directory for generated files. **Do not** hardcode `obj\` or construct the path manually.

### Always Add Generated Files to `FileWrites`

Every generated file should be added to the `FileWrites` item group. This ensures MSBuild's `Clean` target properly removes generated files.

### Basic Pattern (Non-Code Files)

```xml
<Target Name="IncludeGeneratedFiles" BeforeTargets="BeforeBuild">
  <!-- Your logic that generates files goes here -->
  <ItemGroup>
    <None Include="$(IntermediateOutputPath)my-generated-file.xyz" CopyToOutputDirectory="PreserveNewest"/>
    <FileWrites Include="$(IntermediateOutputPath)my-generated-file.xyz" />
  </ItemGroup>
</Target>
```

### For Generated Source Files (Code That Needs Compilation)

Use **`BeforeTargets="CoreCompile;BeforeCompile"`** — this is the correct timing for adding `Compile` items.

```xml
<Target Name="IncludeGeneratedSourceFiles" BeforeTargets="CoreCompile;BeforeCompile">
  <PropertyGroup>
    <GeneratedCodeDir>$(IntermediateOutputPath)Generated\</GeneratedCodeDir>
    <GeneratedFilePath>$(GeneratedCodeDir)MyGeneratedFile.cs</GeneratedFilePath>
  </PropertyGroup>

  <MakeDir Directories="$(GeneratedCodeDir)" />

  <!-- Your logic that generates the .cs file goes here -->

  <ItemGroup>
    <Compile Include="$(GeneratedFilePath)" />
    <FileWrites Include="$(GeneratedFilePath)" />
  </ItemGroup>
</Target>
```

## Target Timing

| `BeforeTargets` value | Use for |
|---|---|
| `BeforeBuild` | Non-code files added to `None` or `Content` |
| `CoreCompile;BeforeCompile` | Generated source files added to `Compile` |
| `AssignTargetPaths` | Fallback if `BeforeBuild` is too early |

## Globbing Behavior

| Glob Location | Files Captured |
|---|---|
| Outside of a target | Only files visible during Evaluation phase (before build starts) |
| Inside of a target | Files visible when the target runs (can capture generated files if timed correctly) |

This is why the solution places the `<ItemGroup>` inside a `<Target>` — the glob runs during execution when the generated files exist.
