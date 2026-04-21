using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using CwDecoderGui.Models;

namespace CwDecoderGui.Views;

/// <summary>True when the cell is a Word break, false otherwise.</summary>
internal sealed class IsWordCellConverter : IValueConverter
{
    public static readonly IsWordCellConverter Instance = new();
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is TranscriptCell c && c.Kind == TranscriptKind.Word;
    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>True for character cells (so the morse/letter is visible).</summary>
internal sealed class IsCharCellConverter : IValueConverter
{
    public static readonly IsCharCellConverter Instance = new();
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is TranscriptCell c && c.Kind == TranscriptKind.Char;
    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>Maps a bool keying state to a glowing brush.</summary>
internal sealed class KeyingBrushConverter : IValueConverter
{
    public static readonly KeyingBrushConverter Instance = new();
    private static readonly IBrush On = new SolidColorBrush(Color.FromRgb(0xFF, 0x5B, 0xD0));
    private static readonly IBrush Off = new SolidColorBrush(Color.FromRgb(0x3D, 0x55, 0x6F));
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is bool b && b ? On : Off;
    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>Maps bool to "ON" / "OFF" string.</summary>
internal sealed class KeyingTextConverter : IValueConverter
{
    public static readonly KeyingTextConverter Instance = new();
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is bool b && b ? "ON" : "OFF";
    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>Returns a BlurEffect when the bound bool is true, null otherwise.</summary>
internal sealed class BoolToBlurEffectConverter : IValueConverter
{
    public static readonly BoolToBlurEffectConverter Instance = new();
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is bool b && b ? new BlurEffect { Radius = 22 } : null;
    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

