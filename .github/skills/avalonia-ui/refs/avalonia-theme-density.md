# Avalonia theme density

## When to consult this note
- Adding or changing `App.axaml` theme setup.
- Reviewing row density in the main QSO log view.
- Seeing too few rows on screen after a style change.
- Debating Fluent vs Simple vs third-party themes.
- Tweaking dark theme colors and contrast.
- Deciding whether a new screen should follow compact defaults or declare an exception.
- Investigating visual noise from separators, borders, or oversized spacing.

## Avalonia facts that matter
- Official built-in themes are `FluentTheme` and `SimpleTheme`.
- `FluentTheme` supports `DensityStyle`, including `Compact`.
- `FluentTheme` supports only Light and Dark variants for palettes.
- Palette overrides are per variant via `ColorPaletteResources`.
- In Fluent palettes, `Accent` can change at runtime; most other palette values are read once at startup.

## QsoRipper rules
- **Do this in QsoRipper:** default to `FluentTheme` with `DensityStyle="Compact"` unless a deliberate exception is documented.
- **Why:** compact density increases rows-per-screen and reduces operator eye travel in a high-volume logbook.
- **Do this in QsoRipper:** keep the main log to dense, single-line rows.
- **Do this in QsoRipper:** keep top chrome compact (minimal header height, minimal non-grid vertical space).
- **Do this in QsoRipper:** use restrained separators/borders; avoid decorative heavy outlines.
- **Do this in QsoRipper:** ellipsize long text in hot-path columns by default.
- **Check this first in QsoRipper:** verify `FluentTheme` is the active base theme before debugging style oddities.
- **Check this first in QsoRipper:** verify dense spacing comes from theme density, not ad hoc per-control margin hacks.
- **Avoid this in QsoRipper:** dashboard-card layouts in the primary log path.
- **Avoid this in QsoRipper:** touch-first padding/margins in desktop hot-path screens.
- **Avoid this in QsoRipper:** large stacked explanatory text above the grid.
- If palette customization is needed, keep it practical: dark base, readable contrast, minimal overrides.
- Practical dark-palette defaults for QsoRipper:
  - Keep background/region tones close enough to reduce visual glare but distinct enough for focus cues.
  - Keep accent usage functional (selection, focus, active state), not decorative.
  - Avoid high-saturation accents in every row; reserve strong colors for status and validation.
  - Recheck row readability when alternating backgrounds and separators are both enabled.

## Common failure modes
- `FluentTheme` is enabled but `DensityStyle` is left at default, reducing data density.
- UI regresses to card-like spacing after introducing templates copied from generic samples.
- Overriding many palette values without contrast checks reduces readability in dense rows.
- Vertical chrome creep (toolbars/help text/filter ribbons) shrinks usable grid viewport.
- Long callsign or notes text wraps and causes row height churn.
- Dense rows become visually noisy due to strong borders on every cell and container.
- Compact theme is enabled globally, but local styles reintroduce large `Padding`/`MinHeight` on key controls.

## Minimal example
```xml
<Application xmlns="https://github.com/avaloniaui"
             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
             x:Class="QsoRipper.App"
             RequestedThemeVariant="Dark">
  <Application.Styles>
    <FluentTheme DensityStyle="Compact" />
  </Application.Styles>
</Application>
```

## References
- https://docs.avaloniaui.net/docs/styling/themes
