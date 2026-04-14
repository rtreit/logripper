# Avalonia performance for hot-path log views

## When to consult this note
- Scroll performance drops in the recent QSO list.
- Startup becomes slower after UI/template changes.
- The grid renders too many elements or stutters while updating.
- You are adding row details, wrapped text, or rich cell content.
- You need a first-pass triage flow for "UI feels slow."

## Avalonia facts that matter
- `ListBox`, `TreeView`, `DataGrid`, and `ItemsRepeater` support virtualization.
- Virtualization requires constrained layout bounds; infinite-height parents disable it.
- `StackPanel` around an items control is a common virtualization killer.
- `VirtualizingStackPanel.BufferFactor` can smooth scrolling at memory cost.
- Variable-height rows degrade virtualization quality and can cause extent recalculation/jumps.
- Deep visual trees and heavy templates increase measure/arrange/render cost.
- `IsVisible="False"` removes a subtree from layout and rendering.
- Compiled bindings reduce runtime reflection overhead and catch errors earlier.
- DevTools (F12 in debug) can inspect visual tree, styles, bounds, and layout/perf clues.
- Avalonia 12 removed `Avalonia.Diagnostics`; diagnostics setup is version-sensitive.

## QsoRipper rules
- **Hot-path rule:** the main QSO list is the product hot path; keep it virtualization-friendly by default.
- **Do this in QsoRipper:** compiled bindings by default.
- **Do this in QsoRipper:** virtualization as a non-negotiable default for large QSO collections.
- **Do this in QsoRipper:** shallow row/cell templates (prefer lightweight text display on non-editing cells).
- **Do this in QsoRipper:** fixed/single-line row presentation in hot-path tables.
- **Do this in QsoRipper:** no wrapped text in hot-path rows; use truncation + drill-in instead.
- **Do this in QsoRipper:** defer non-urgent work (counts, enrichment, expensive recomputation) off scroll/update path.
- **Avoid this in QsoRipper:** synchronous expensive status recomputation triggered by viewport changes.
- **DevTools/diagnostics:** use F12 visual inspection first; if diagnostics wiring changed across Avalonia versions, update to the current supported developer-tools package and API.

## Common failure modes
- Items control is inside `StackPanel` (or equivalent unconstrained container), disabling virtualization.
- Row details / wrapped text create variable-height rows and heavy remeasure churn.
- Complex controls are instantiated in every cell when read-only text is enough.
- Frequent layout-affecting property churn (`Margin`, `Width`, `Height`) during updates.
- Hidden overlays still hit-test or render unnecessarily.
- Performance investigation starts with style rewrites before verifying virtualization and bindings.

1. Verify virtualization is active (check parent layout constraints first).
2. Inspect bindings and trace logs for binding churn/failures.
3. Inspect layout/measure churn in DevTools and visual tree bounds.
4. Inspect template complexity in hot-path rows/cells.
5. Defer non-urgent work off the hot path (dispatcher/background path).

## Minimal example
```xml
<!-- BAD: StackPanel gives infinite height and can disable virtualization -->
<StackPanel>
  <DataGrid ItemsSource="{Binding RecentQsos}" />
</StackPanel>

<!-- GOOD: star-sized Grid row constrains viewport -->
<Grid RowDefinitions="*">
  <DataGrid Grid.Row="0" ItemsSource="{Binding RecentQsos}" />
</Grid>
```

## References
- https://docs.avaloniaui.net/docs/app-development/performance
- https://docs.avaloniaui.net/docs/how-to/debugging-how-to
- https://docs.avaloniaui.net/docs/avalonia12-breaking-changes
