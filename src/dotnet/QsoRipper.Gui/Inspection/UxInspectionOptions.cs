using System;
using System.Collections.Generic;
using System.IO;
using Avalonia.Styling;

namespace QsoRipper.Gui.Inspection;

internal sealed record UxInspectionOptions(
    string? FixturePath,
    UxInspectionSurface Surface,
    ThemeVariant ThemeVariant)
{
    public static bool TryParse(IReadOnlyList<string> args, out UxInspectionOptions? options, out string? error)
    {
        options = null;
        error = null;

        if (args.Count == 0 || !ContainsInspectFlag(args))
        {
            return true;
        }

        string? fixturePath = null;
        var surface = UxInspectionSurface.MainWindow;
        var themeVariant = ThemeVariant.Dark;

        for (var i = 0; i < args.Count; i++)
        {
            switch (args[i])
            {
                case "--inspect":
                    break;
                case "--inspect-fixture":
                    if (!TryReadValue(args, ref i, out fixturePath, out error))
                    {
                        return false;
                    }

                    break;
                case "--inspect-surface":
                    if (!TryReadValue(args, ref i, out var surfaceValue, out error))
                    {
                        return false;
                    }

                    if (!TryParseSurface(surfaceValue, out surface))
                    {
                        error = $"Unsupported inspect surface '{surfaceValue}'. Use MainWindow, Settings, or Wizard.";
                        return false;
                    }

                    break;
                case "--inspect-theme":
                    if (!TryReadValue(args, ref i, out var themeValue, out error))
                    {
                        return false;
                    }

                    if (!UxCaptureOptions.TryParseThemeVariant(themeValue, out themeVariant))
                    {
                        error = $"Unsupported inspect theme '{themeValue}'. Use Default, Dark, or Light.";
                        return false;
                    }

                    break;
            }
        }

        options = new UxInspectionOptions(
            FixturePath: string.IsNullOrWhiteSpace(fixturePath) ? null : Path.GetFullPath(fixturePath),
            Surface: surface,
            ThemeVariant: themeVariant);
        return true;
    }

    private static bool ContainsInspectFlag(IReadOnlyList<string> args)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (string.Equals(args[i], "--inspect", StringComparison.Ordinal))
            {
                return true;
            }
        }

        return false;
    }

    private static bool TryReadValue(
        IReadOnlyList<string> args,
        ref int index,
        out string value,
        out string? error)
    {
        if (index >= args.Count - 1)
        {
            value = string.Empty;
            error = $"Missing value for {args[index]}.";
            return false;
        }

        value = args[++index];
        error = null;
        return true;
    }

    private static bool TryParseSurface(string value, out UxInspectionSurface surface)
    {
        switch (value.Trim().ToLowerInvariant())
        {
            case "mainwindow":
            case "main-window":
            case "main":
                surface = UxInspectionSurface.MainWindow;
                return true;
            case "settings":
                surface = UxInspectionSurface.Settings;
                return true;
            case "wizard":
            case "setup-wizard":
                surface = UxInspectionSurface.Wizard;
                return true;
            default:
                surface = UxInspectionSurface.MainWindow;
                return false;
        }
    }
}

internal enum UxInspectionSurface
{
    MainWindow = 0,
    Settings = 1,
    Wizard = 2
}
