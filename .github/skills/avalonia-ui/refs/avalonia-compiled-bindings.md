# Avalonia compiled bindings

## When to consult this note
- A screen renders blank or stale values after a refactor.
- A DataTemplate binding works in one place but fails in another.
- You are adding a new view, template, or reusable control.
- You need to decide between compiled vs reflection bindings.
- The UI feels "glitchy" and layout restyling is being considered.

## Avalonia facts that matter
- Avalonia compiles XAML to IL at build time (XamlX), unlike runtime-interpreted WPF defaults.
- Compiled bindings provide build-time validation and avoid reflection at runtime.
- Compiled bindings need type context via `x:DataType`.
- Per-control opt-in uses `x:CompileBindings="True"` plus `x:DataType`.
- Project-wide default can be enabled with `AvaloniaUseCompiledBindingsByDefault`.
- In Avalonia 12, compiled bindings are enabled by default for `Binding` in XAML.
- `ReflectionBinding` is the intentional escape hatch for runtime/dynamic scenarios.
- Binding failures are logged to trace output (`LogToTrace`) and should be treated as first-line diagnostics.

## QsoRipper rules
- **Do this in QsoRipper:** compiled bindings are the default where practical.
- **Why:** build-time binding checks, better runtime performance, fewer silent runtime failures.
- **Do this in QsoRipper:** declare `x:DataType` on each view root (`Window`, `UserControl`) and each `DataTemplate` with a different context.
- **Do this in QsoRipper:** keep project-wide compiled bindings enabled unless a migration needs temporary carve-outs.
- **Do this in QsoRipper:** use `ReflectionBinding` only intentionally for truly dynamic paths, and keep usage local and obvious.
- **Check this first in QsoRipper:** if UI output looks wrong, audit bindings before reworking layout/styles.

## Common failure modes
- Typo in binding path compiles only because the binding fell back to reflection unexpectedly.
- Correct path, wrong `DataContext` scope (especially inside templates or nested controls).
- Missing or incorrect `x:DataType`, so compiled validation cannot protect you.
- Template uses parent type assumptions while actual item type differs.
- Binding warnings in trace are ignored, leading to "random" visual glitches later.

## Minimal example
```xml
<UserControl xmlns="https://github.com/avaloniaui"
             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
             xmlns:vm="using:QsoRipper.Gui.ViewModels"
             x:DataType="vm:RecentQsoListViewModel"
             x:CompileBindings="True">
  <ListBox ItemsSource="{Binding Rows}">
    <ListBox.ItemTemplate>
      <DataTemplate x:DataType="vm:RecentQsoRowViewModel">
        <TextBlock Text="{Binding Callsign}" />
      </DataTemplate>
    </ListBox.ItemTemplate>
  </ListBox>
</UserControl>
```

```csharp
public static AppBuilder BuildAvaloniaApp() =>
    AppBuilder.Configure<App>()
        .UsePlatformDetect()
        .LogToTrace(LogEventLevel.Warning);
```

1. Check the binding path.
2. Check the active `DataContext` scope.
3. Check `x:DataType` on the view/template.
4. Check template scope assumptions vs actual item type.
5. Check trace logs for binding warnings/errors.

## References
- https://docs.avaloniaui.net/docs/xaml/compilation
- https://docs.avaloniaui.net/docs/how-to/debugging-how-to
- https://docs.avaloniaui.net/docs/avalonia12-breaking-changes
