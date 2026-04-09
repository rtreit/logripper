---
name: csharp-scripts
description: >-
  Run single-file C# programs as scripts (file-based apps) for quick
  experimentation, prototyping, and concept testing. Use when the user wants
  to write and execute a small C# program without creating a full project.
  Requires .NET 10+ SDK for file-based apps, with fallback for older SDKs.
---

# C# Scripts

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## When to Use

- Testing a C# concept, API, or language feature with a quick one-file program
- Prototyping logic before integrating it into a larger project

## When Not to Use

- The user needs a full project with multiple files or project references
- The user is working inside an existing .NET solution
- The program is too large or complex for a single file

## Workflow

### Step 1: Check the .NET SDK version

Run `dotnet --version`. File-based apps require .NET 10+. If below 10, use the [fallback](#fallback-for-net-9-and-earlier).

### Step 2: Write the script file

Create a single `.cs` file using top-level statements. Place it outside any existing project directory.

```csharp
// hello.cs
Console.WriteLine("Hello from a C# script!");

var numbers = new[] { 1, 2, 3, 4, 5 };
Console.WriteLine($"Sum: {numbers.Sum()}");
```

Guidelines:
- Use top-level statements (no `Main` method boilerplate)
- Place `using` directives at the top
- Place type declarations after all top-level statements

### Step 3: Run the script

```bash
dotnet hello.cs
```

Pass arguments after `--`:

```bash
dotnet hello.cs -- arg1 arg2
```

### Step 4: Add directives (if needed)

Place `#:` directives at the top of the file, before any `using` directives.

#### `#:package` — NuGet package references

```csharp
#:package Humanizer@2.14.1
using Humanizer;
Console.WriteLine("hello world".Titleize());
```

#### `#:property` — MSBuild properties

```csharp
#:property AllowUnsafeBlocks=true
#:property PublishAot=false
```

#### `#:project` — Project references

```csharp
#:project ../MyLibrary/MyLibrary.csproj
```

### Step 5: Clean up

Remove the script file when done. Clear cached artifacts with `dotnet clean hello.cs`.

## Source-generated JSON

File-based apps enable native AOT by default. Use source-generated serialization:

```csharp
using System.Text.Json;
using System.Text.Json.Serialization;

var person = new Person("Alice", 30);
var json = JsonSerializer.Serialize(person, AppJsonContext.Default.Person);
Console.WriteLine(json);

record Person(string Name, int Age);

[JsonSerializable(typeof(Person))]
partial class AppJsonContext : JsonSerializerContext;
```

## Converting to a project

When a script outgrows a single file:

```bash
dotnet project convert hello.cs
```

## Fallback for .NET 9 and earlier

```bash
mkdir -p /tmp/csharp-script && cd /tmp/csharp-script
dotnet new console -o . --force
```

Replace `Program.cs` with the script content and run with `dotnet run`.

## Common Pitfalls

| Pitfall | Solution |
|---------|----------|
| `.cs` file inside a directory with a `.csproj` | Move script outside the project directory |
| `#:package` without a version | Specify a version: `#:package PackageName@1.2.3` |
| Directives placed after C# code | All `#:` directives must appear before `using` directives |
| Reflection-based JSON serialization fails | Use source-generated JSON with `JsonSerializerContext` |
