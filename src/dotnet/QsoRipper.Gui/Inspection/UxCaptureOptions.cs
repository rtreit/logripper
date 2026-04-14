using System;
using System.Collections.Generic;
using System.IO;
using Avalonia.Styling;

namespace QsoRipper.Gui.Inspection;

internal sealed record UxCaptureOptions(
    string Scenario,
    string OutputPath,
    string TargetName,
    string? FixturePath,
    ThemeVariant ThemeVariant)
{
    public static bool TryParse(IReadOnlyList<string> args, out UxCaptureOptions? options, out string? error)
    {
        options = null;
        error = null;

        if (args.Count == 0 || !ContainsCaptureFlag(args))
        {
            return true;
        }

        var scenario = "main-window";
        var targetName = "MainWindow";
        string? outputPath = null;
        string? fixturePath = null;
        var themeVariant = ThemeVariant.Dark;

        for (var i = 0; i < args.Count; i++)
        {
            switch (args[i])
            {
                case "--capture":
                    break;
                case "--capture-scenario":
                    if (!TryReadValue(args, ref i, out scenario, out error))
                    {
                        return false;
                    }

                    break;
                case "--capture-output":
                    if (!TryReadValue(args, ref i, out outputPath, out error))
                    {
                        return false;
                    }

                    break;
                case "--capture-target":
                    if (!TryReadValue(args, ref i, out targetName, out error))
                    {
                        return false;
                    }

                    break;
                case "--capture-fixture":
                    if (!TryReadValue(args, ref i, out fixturePath, out error))
                    {
                        return false;
                    }

                    break;
                case "--capture-theme":
                    if (!TryReadValue(args, ref i, out var themeValue, out error))
                    {
                        return false;
                    }

                    if (!TryParseThemeVariant(themeValue, out themeVariant))
                    {
                        error = $"Unsupported capture theme '{themeValue}'. Use Default, Dark, or Light.";
                        return false;
                    }

                    break;
            }
        }

        if (string.IsNullOrWhiteSpace(outputPath))
        {
            error = "Capture mode requires --capture-output <path>.";
            return false;
        }

        options = new UxCaptureOptions(
            Scenario: scenario.Trim(),
            OutputPath: Path.GetFullPath(outputPath),
            TargetName: string.IsNullOrWhiteSpace(targetName) ? "MainWindow" : targetName.Trim(),
            FixturePath: string.IsNullOrWhiteSpace(fixturePath) ? null : Path.GetFullPath(fixturePath),
            ThemeVariant: themeVariant);
        return true;
    }

    private static bool ContainsCaptureFlag(IReadOnlyList<string> args)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (string.Equals(args[i], "--capture", StringComparison.Ordinal))
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

    internal static bool TryParseThemeVariant(string value, out ThemeVariant themeVariant)
    {
        switch (value.Trim().ToLowerInvariant())
        {
            case "default":
                themeVariant = ThemeVariant.Default;
                return true;
            case "dark":
                themeVariant = ThemeVariant.Dark;
                return true;
            case "light":
                themeVariant = ThemeVariant.Light;
                return true;
            default:
                themeVariant = ThemeVariant.Default;
                return false;
        }
    }
}
