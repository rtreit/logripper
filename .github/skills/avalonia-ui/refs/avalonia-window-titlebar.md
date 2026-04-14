# Avalonia window and titlebar customization

## When to consult this note
- Considering custom titlebar/chrome changes.
- Fixing drag behavior, caption buttons, or resize affordances.
- Adding shell/header controls near the title area.
- Implementing Windows-specific Mica/transparency behavior.
- Migrating window chrome code between Avalonia major versions.

## Avalonia facts that matter
- Standard window behavior is configured with `Window` properties (`Title`, resize flags, startup location, decorations, etc.).
- Custom titlebar flow uses `ExtendClientAreaToDecorationsHint` and `WindowDecorations.ElementRole="TitleBar"` for drag region semantics.
- Elements marked as title bar support native drag and double-click maximize behavior.
- Windows supports all transparency hints (`Transparent`, `AcrylicBlur`, `Mica`) with fallback behavior.
- Transparency effects require transparent background setup to be visible.
- In Avalonia 12, `WindowDrawnDecorations` replaces old `TitleBar`, `CaptionButtons`, and `ChromeOverlayLayer` classes.
- Avalonia 12 removed `ExtendClientAreaChromeHints`; use current decoration APIs.
- Platform behavior differs: validate custom chrome on Windows specifically (and check Linux/macOS differences separately).

## QsoRipper rules
- **Default posture:** keep native window chrome unless a custom title area has clear UX value.
- **Do this in QsoRipper:** keep shell/header area compact and native-feeling; prioritize content height for the log grid.
- **Do this in QsoRipper:** if custom chrome is used, mark only intentional drag zones as `TitleBar`.
- **Do this in QsoRipper:** keep critical interactive controls outside drag-only zones, or explicitly verify they remain reliably clickable.
- **Do this in QsoRipper:** preserve expected desktop affordances (move, resize, minimize/maximize/close, snap behavior).
- **Do this in QsoRipper:** validate behavior on Windows after any titlebar/chrome change.
- **Use effects conservatively:** Mica/transparency are optional and must not reduce readability or responsiveness.
- **Version-sensitive rule:** custom chrome APIs changed in Avalonia 12; avoid copying pre-v12 snippets without updating them.
- **Version-sensitive rule:** use the decoration property names from your exact target API/docs (`SystemDecorations` vs `WindowDecorations`) and keep examples aligned to that version.

## Common failure modes
- Entire top row is accidentally marked as `TitleBar`, causing confusing drag interactions.
- Custom chrome removes native affordances but does not reintroduce usable caption controls.
- Old APIs (`TitleBar`, `CaptionButtons`, `ExtendClientAreaChromeHints`) are used in v12 code.
- Transparency hint is set but background remains opaque, so effect appears "broken."
- Window behavior is tested only on one platform and regresses on Windows edge cases.

## Minimal example
```xml
<Window xmlns="https://github.com/avaloniaui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        x:Class="QsoRipper.Gui.MainWindow"
        ExtendClientAreaToDecorationsHint="True"
        SystemDecorations="None">
  <Grid RowDefinitions="28,*">
    <DockPanel Grid.Row="0">
      <Button DockPanel.Dock="Right" Content="X" Width="40" />
      <Border Padding="8,0" WindowDecorations.ElementRole="TitleBar">
        <TextBlock Text="QsoRipper" VerticalAlignment="Center" />
      </Border>
    </DockPanel>
    <ContentPresenter Grid.Row="1" />
  </Grid>
</Window>
```

## References
- https://docs.avaloniaui.net/docs/app-development/window-management
- https://docs.avaloniaui.net/docs/platform-specific-guides/windows
- https://docs.avaloniaui.net/controls/primitives/windowdrawndecorations
- https://docs.avaloniaui.net/docs/avalonia12-breaking-changes
