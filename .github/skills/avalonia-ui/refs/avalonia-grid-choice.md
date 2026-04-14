# Avalonia grid control choice

## When to consult this note
- Choosing the main control for dense QSO list rendering/editing.
- Considering migration from `DataGrid` to another control.
- Debugging a grid that looks unstyled or broken.
- Adding heavy cell templates and seeing scroll degradation.
- Evaluating `TreeDataGrid` for non-hierarchical data.

## Avalonia facts that matter
- `DataGrid` is documented as **Deprecated**.
- `DataGrid` is in a separate package: `Avalonia.Controls.DataGrid`.
- `DataGrid` also requires an explicit theme include in `App.axaml`.
- DataGrid package version must match the Avalonia version line in use.
- Sorting is on by default; template columns need `SortMemberPath` for sortable semantics.
- Virtualization support exists for `DataGrid` and `ItemsRepeater`.
- `TreeDataGrid` is available as part of **Avalonia Pro or higher** and requires licensing setup plus theme setup.
- `TreeDataGrid` v12 has notable API changes vs v11.

## QsoRipper rules
| Option | Use in QsoRipper when | Avoid in QsoRipper when |
|---|---|---|
| DataGrid | Dense editable tabular QSO list with existing keyboard workflows | Setup caveats are not handled (package/theme/version) |
| ItemsRepeater (custom virtualized list) | You need tighter control of template cost and scrolling behavior | You need built-in grid editing/sorting/grouping quickly |
| TreeDataGrid | You have a real hierarchical log/navigation requirement and accepted Pro licensing | You only need a flat QSO table |

- **Do this in QsoRipper:** default to `DataGrid` for flat logbook workflows, but treat setup correctness as mandatory.
- **Do this in QsoRipper:** keep rows single-line and dense.
- **Do this in QsoRipper:** cap long-text columns and ellipsize by default.
- **Do this in QsoRipper:** preserve keyboard-first editing and navigation behavior.
- **Do this in QsoRipper:** use `SortMemberPath` on `DataGridTemplateColumn` where sort behavior is required.
- **Avoid this in QsoRipper:** heavy visual templates in hot-path columns (nested panels, wrapped text, deep trees).
- **Avoid casual recommendation:** `TreeDataGrid` adds licensing/theme/setup complexity; require explicit justification.

## Common failure modes
- Package added, but DataGrid theme include missing -> unresolved resources / broken visuals.
- Theme include exists, but for wrong theme -> style conflicts.
- DataGrid package version does not align with Avalonia version -> runtime/style issues.
- Wide text columns are unconstrained -> row expansion, horizontal churn, poor scan speed.
- Template columns are used without `SortMemberPath`, leading to confusing sort behavior.
- TreeDataGrid is introduced without license key and theme registration, so control does not render correctly.

## Minimal example
```xml
<Application.Styles>
  <FluentTheme DensityStyle="Compact" />
  <StyleInclude Source="avares://Avalonia.Controls.DataGrid/Themes/Fluent.xaml" />
</Application.Styles>

<DataGrid ItemsSource="{Binding RecentQsos}" AutoGenerateColumns="False">
  <DataGrid.Columns>
    <DataGridTemplateColumn Header="Callsign" SortMemberPath="Callsign">
      <DataGridTemplateColumn.CellTemplate>
        <DataTemplate>
          <TextBlock Text="{Binding Callsign}" TextTrimming="CharacterEllipsis" />
        </DataTemplate>
      </DataGridTemplateColumn.CellTemplate>
    </DataGridTemplateColumn>
  </DataGrid.Columns>
</DataGrid>
```

## References
- https://docs.avaloniaui.net/controls/data-display/structured-data/datagrid
- https://docs.avaloniaui.net/docs/how-to/datagrid-how-to
- https://docs.avaloniaui.net/controls/data-display/structured-data/treedatagrid/
- https://docs.avaloniaui.net/docs/app-development/performance
