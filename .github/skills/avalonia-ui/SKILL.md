---
name: avalonia-ui
description: >-
  Build and debug Avalonia UI in QsoRipper. Use when editing AXAML, Avalonia
  view code, themes, styles, bindings, grid controls, window chrome, or
  diagnosing visual/layout/binding issues in the desktop UI.
---

# Skill: Avalonia UI

## When to Use

- Editing `.axaml` views, styles, templates, or Avalonia UI-related C# code
- Diagnosing broken layouts, clipped content, missing theme resources, or glitchy rendering
- Reviewing data binding behavior, especially when the UI looks correct structurally but values are missing or stale
- Improving density, compactness, keyboard-first behavior, or grid performance
- Working on custom title bar, window chrome, drag regions, or Windows-specific UI behavior
- Choosing between `DataGrid`, custom virtualized lists, or other Avalonia controls for large datasets

## Design Rules

1. Prefer a dense desktop layout over dashboard-style cards and stacked explanatory text.
2. Treat the QSO list as the product. Optimize for scanability, keyboard flow, and row density.
3. Use Avalonia Fluent compact density as the baseline for QsoRipper UI work.
4. Prefer compiled bindings for views and templates to catch binding issues at build time and reduce runtime reflection.
5. Keep long-text cells truncated by default; use tooltips or a details pane for full content.
6. Avoid styling fixes that mask underlying binding, layout, or control-choice problems.
7. Prefer virtualization-friendly controls and item templates for large QSO collections.
8. Be careful with custom title bar and custom window chrome work; validate drag regions, caption behavior, and Windows-specific theme behavior.
9. Do not assume WPF patterns map directly to Avalonia without verification.
10. Keep XAML and styling lightweight; avoid unnecessary visual tree depth on hot screens.

## Review Checklist

- Is the UI using compact density appropriately?
- Are bindings compiled where practical via `x:DataType` and `x:CompileBindings`?
- Are there likely missing bindings, wrong data context scopes, or stale property names?
- Is the visual tree too heavy for a high-density log view?
- Is the grid/list choice appropriate for the number of rows and editing model?
- Are long text fields capped and ellipsized instead of consuming the layout?
- Is window chrome/title bar customization using Avalonia-supported mechanisms?
- Are theme resources for any separate control package correctly included?
- Is this code accidentally using WPF habits that do not fit Avalonia well?
- If performance is poor, are virtualization and compiled bindings being used?

## QsoRipper-Specific Rules

1. Keep the non-grid chrome minimal. The main log view should be dominated by the QSO list.
2. Default to compact dark Fluent styling.
3. Preserve keyboard-first navigation and edit workflows.
4. Prefer single-line rows in the QSO list.
5. Treat note/comment columns as secondary in the default layout.
6. Prefer explicit widths and constrained defaults for verbose columns.
7. Do not introduce oversized panels, padded cards, or touch-first spacing.
8. Keep status metadata in compact command/status bars, not in large descriptive regions.

## Troubleshooting Order

When the Avalonia UI looks wrong, debug in this order:

1. **Binding correctness**
   - Verify data context scope
   - Prefer compiled bindings
   - Check for property name drift and template scope mistakes

2. **Theme/resource correctness**
   - Verify the correct Fluent theme setup
   - Verify any extra control theme includes are present

3. **Layout correctness**
   - Inspect column widths, alignment, clipping, ellipsis, and container constraints
   - Reduce nested panels if the layout is unstable

4. **Control choice**
   - Re-check whether the chosen control is appropriate for dense, virtualized, editable tabular data

5. **Window/chrome correctness**
   - For shell/title bar issues, verify drag regions and custom decoration behavior

## Current References

Prioritize these official Avalonia references when reasoning about QsoRipper UI work:

- Themes / Fluent / compact density
- XAML compilation and compiled bindings
- App performance troubleshooting
- DataGrid reference and DataGrid how-to
- Window management / Windows-specific behavior / custom title bar
- Window-drawn decorations

## Notes for This Repo

- Use this skill together with:
  - `instructions/ui-ux.instructions.md`
  - `skills/keyboard-first-ui/SKILL.md`
  - `instructions/performance.instructions.md`
- If editing AXAML or Avalonia view code, prefer consulting this skill before inventing custom layout behavior.
- When changing the main QSO list, explicitly consider density, virtualization, sort/edit workflow, and long-text truncation.

## Local References

Consult these local references when relevant:

- `refs/avalonia-theme-density.md`
  - Use when editing theme, spacing, density, row height, compact styling, or Fluent theme configuration.

- `refs/avalonia-compiled-bindings.md`
  - Use when bindings are missing, stale, or behaving unexpectedly.

- `refs/avalonia-grid-choice.md`
  - Use when choosing between DataGrid, custom lists, or other tabular controls.

- `refs/avalonia-performance.md`
  - Use when the QSO list is slow, glitchy, or rendering too much visual tree.

- `refs/avalonia-window-titlebar.md`
  - Use when editing custom title bar, shell chrome, drag regions, or Windows-specific window behavior.
