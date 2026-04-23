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
    public bool HasTone => ToneHz is double hz && hz > 0;
    public string ToneDisplay => HasTone ? $"{ToneHz:F1} Hz" : "";

    public static TranscriptCell Char(string ch, string morse, double? toneHz = null) =>
        new() { Kind = TranscriptKind.Char, Letter = ch, Morse = morse, ToneHz = toneHz };

    public static TranscriptCell Word() =>
        new() { Kind = TranscriptKind.Word, Letter = " ", Morse = "" };
}

public enum TranscriptKind
{
    Char,
    Word,
    Garbled,
}
