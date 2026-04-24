namespace CwDecoderGui.Models;

/// <summary>
/// One decoded element of the transcript. Either a real character
/// (<see cref="Morse"/> + <see cref="Letter"/>), a word break (a space
/// rendered as a wider gap), or a garbled placeholder.
/// </summary>
public sealed class TranscriptCell
{
    public TranscriptKind Kind { get; init; }
    public string Letter { get; init; } = "";
    public string Morse { get; init; } = "";
    public double? ToneHz { get; init; }
    public double? TonePurity { get; init; }
    public bool HasTone => ToneHz is double hz && hz > 0;
    public bool HasPurity => TonePurity is double p && p > 0;
    public string ToneDisplay => HasTone ? $"{ToneHz:F1} Hz" : "";
    public string PurityDisplay => HasPurity ? $"purity {TonePurity:F1}" : "";
    public string DebugDisplay
    {
        get
        {
            if (HasTone && HasPurity) return $"{Letter} @ {ToneHz:F1} Hz ({PurityDisplay})";
            if (HasTone) return $"{Letter} @ {ToneHz:F1} Hz";
            return Letter;
        }
    }

    public static TranscriptCell Char(string ch, string morse, double? toneHz = null, double? tonePurity = null) =>
        new() { Kind = TranscriptKind.Char, Letter = ch, Morse = morse, ToneHz = toneHz, TonePurity = tonePurity };

    public static TranscriptCell Word() =>
        new() { Kind = TranscriptKind.Word, Letter = " ", Morse = "" };
}

public enum TranscriptKind
{
    Char,
    Word,
    Garbled,
}
