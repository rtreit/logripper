---
name: dotnet-aot-compat
description: >-
  Make .NET projects compatible with Native AOT and trimming by systematically
  resolving IL trim/AOT analyzer warnings. Use when making projects AOT-compatible,
  fixing trimming warnings, resolving IL warnings (IL2026, IL2070, IL2067, IL3050),
  adding DynamicallyAccessedMembers annotations, or enabling IsAotCompatible.
---

# .NET AOT Compatibility

Make .NET projects compatible with Native AOT and trimming by systematically resolving all IL trim/AOT analyzer warnings.

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## When to Use

- Make a project AOT-compatible
- Fix trimming warnings or IL warnings
- Resolve IL2070 / IL2067 / IL2072 / IL2026 / IL3050 warnings
- Add `DynamicallyAccessedMembers` annotations
- Enable `IsAotCompatible` in a .csproj

## When Not to Use

- Project exclusively targets .NET Framework (net4x)

## Critical Rules

### ❌ Never suppress warnings incorrectly

- **NEVER** use `#pragma warning disable` for IL warnings — it hides warnings from the Roslyn analyzer but the IL linker still sees the issue
- **NEVER** use `[UnconditionalSuppressMessage]` — it tells both analyzer and linker to ignore the warning

### 💡 Preferred approaches

- **Prefer** `[DynamicallyAccessedMembers]` annotations to flow type information through the call chain
- **Prefer** refactoring to eliminate patterns that break annotation flow
- **Use** `[RequiresUnreferencedCode]` / `[RequiresDynamicCode]` to mark fundamentally incompatible methods

## Step-by-Step Procedure

### Step 1: Enable AOT analysis in the .csproj

```xml
<PropertyGroup>
  <IsAotCompatible Condition="$([MSBuild]::IsTargetFrameworkCompatible('$(TargetFramework)', 'net8.0'))">true</IsAotCompatible>
</PropertyGroup>
```

### Step 2: Build and collect warnings

```bash
dotnet build <project.csproj> -f <net8.0-or-later-tfm> --no-incremental 2>&1 | grep 'IL[0-9]\{4\}'
```

Common warning codes:
- **IL2070**: Reflection call on `Type` parameter missing `[DynamicallyAccessedMembers]`
- **IL2067**: Passing an unannotated `Type` to a method expecting annotation
- **IL2072**: Return value or extracted value missing annotation
- **IL2026**: Calling a method marked `[RequiresUnreferencedCode]`
- **IL3050**: Calling a method marked `[RequiresDynamicCode]`

### Step 3: Triage warnings by code

Group warnings by code and count. Start with the most common pattern:

| Pattern | Typical fix |
|---------|-------------|
| Many IL2026 + IL3050 from `JsonSerializer` | Create a `JsonSerializerContext` with source generation |
| IL2070/IL2087 on `Type` parameters | Add `[DynamicallyAccessedMembers]` annotations |
| IL2067 passing unannotated `Type` | Annotate the parameter at the source |

### Step 4: Fix warnings iteratively

Work from the **innermost** reflection call outward. Fix 5-10 warnings, then rebuild.

#### Strategy A: Add `[DynamicallyAccessedMembers]` (preferred)

```csharp
// Before (warns IL2070):
void Process(Type t) {
    var method = t.GetMethod("Foo");
}

// After (clean):
void Process([DynamicallyAccessedMembers(DynamicallyAccessedMemberTypes.PublicMethods)] Type t) {
    var method = t.GetMethod("Foo");
}
```

#### Strategy B: Refactor to preserve annotation flow

When boxing breaks annotation flow (storing `Type` in `object[]`), refactor to pass `Type` directly as an annotated parameter.

#### Strategy C: Source-generated JSON serialization

For IL2026/IL3050 from `JsonSerializer`, create a `JsonSerializerContext`:

```csharp
[JsonSerializerContext]
[JsonSerializable(typeof(MyType))]
internal partial class MyProjectJsonContext : JsonSerializerContext { }
```

Then update call sites to use the context.

#### Strategy D: `[RequiresUnreferencedCode]` (last resort)

For methods that fundamentally require arbitrary reflection:

```csharp
[RequiresUnreferencedCode("Loads plugins by name using Assembly.Load")]
public void LoadPlugin(string assemblyName) { ... }
```

### Step 5: Rebuild and repeat

After each batch of fixes, rebuild with `--no-incremental`. Repeat until 0 warnings.

### Step 6: Validate all TFMs

```bash
dotnet build <project.csproj>  # builds all TFMs
```

Ensure 0 IL warnings on net8.0+ and clean builds on older TFMs.

## Checklist

- [ ] Added `<IsAotCompatible>` with TFM condition to .csproj
- [ ] Built with AOT analyzers enabled (net8.0+ TFM)
- [ ] Fixed all IL warnings via annotations or refactoring
- [ ] No `#pragma warning disable` or `[UnconditionalSuppressMessage]` for IL warnings
- [ ] All target frameworks build with 0 warnings
