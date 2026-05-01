using System;
using System.IO;
using System.Text;
using CwDecoderGui.Models;

namespace CwDecoderGui.ViewModels;

/// <summary>
/// Debug monitor: append one token for each stable classified event emitted by
/// the decoder's stream-live-v3 viz event stream. This is independent of
/// Avalonia redraws; it produces one long line of heard Morse/gap tokens.
///
///   . on_dit | - on_dah | off_intra omitted | / off_char | // off_word
/// </summary>
internal static class VizBarMonitor
{
    private static readonly StringBuilder RawBuffer = new();
    private static readonly StringBuilder TextBuffer = new();
    private static readonly StringBuilder PendingMorse = new();
    private const ulong EpsilonSamples = 16;
    private static string _label = "session";
    private static ulong _lastEmittedEndSample;
    private static int _eventCount;
    private static bool _flushed;

    public static string DecodedText => TextBuffer.ToString();

    public static void Reset(string label)
    {
        RawBuffer.Clear();
        TextBuffer.Clear();
        PendingMorse.Clear();
        _lastEmittedEndSample = 0;
        _eventCount = 0;
        _flushed = false;
        _label = SanitizeLabel(label);
    }

    public static bool Ingest(DecoderEvent ev)
    {
        if (ev.Events is null || ev.Events.Length == 0) return false;
        var sampleRate = ev.SampleRate.GetValueOrDefault();
        var windowStart = ev.WindowStartSample.GetValueOrDefault();
        var windowEnd = ev.WindowEndSample.GetValueOrDefault();
        if (sampleRate <= 0 || windowEnd <= windowStart) return false;

        var dotSeconds = ev.DotSeconds.GetValueOrDefault(0.04);
        var guardSeconds = Math.Max(0.10, dotSeconds * 8.0);
        var guardSamples = (ulong)Math.Round(guardSeconds * sampleRate);
        var stableEnd = windowEnd > guardSamples ? windowEnd - guardSamples : windowStart;
        var changed = false;

        foreach (var e in ev.Events)
        {
            var eventEnd = windowStart + (ulong)Math.Round(Math.Max(0.0, e.EndS) * sampleRate);
            if (eventEnd > stableEnd) continue;
            if (eventEnd <= _lastEmittedEndSample + EpsilonSamples) continue;

            var token = e.Kind switch
            {
                "on_dit" => ".",
                "on_dah" => "-",
                "off_char" => "/",
                "off_word" => "//",
                _ => "",
            };
            if (token.Length > 0)
            {
                RawBuffer.Append(token);
                _eventCount++;
            }

            changed |= DecodeEvent(e.Kind);
            _lastEmittedEndSample = eventEnd;
        }

        return changed;
    }

    public static string? Flush()
    {
        if (_flushed || _eventCount == 0) return null;
        try
        {
            var dir = ResolveOutputDir();
            Directory.CreateDirectory(dir);
            var stamp = DateTime.Now.ToString("yyyyMMdd-HHmmss");
            var path = Path.Combine(dir, $"cw-debug-bars-{_label}-{stamp}.txt");
            File.WriteAllText(path, RawBuffer.ToString());
            _flushed = true;
            return path;
        }
        catch
        {
            return null;
        }
    }

    private static string SanitizeLabel(string label)
    {
        if (string.IsNullOrWhiteSpace(label)) return "session";
        var bad = Path.GetInvalidFileNameChars();
        var sb = new StringBuilder(label.Length);
        foreach (var c in label) sb.Append(Array.IndexOf(bad, c) >= 0 ? '_' : c);
        var s = sb.ToString();
        return s.Length > 80 ? s.Substring(0, 80) : s;
    }

    private static string ResolveOutputDir()
    {
        var dir = AppContext.BaseDirectory;
        for (int i = 0; i < 10 && !string.IsNullOrEmpty(dir); i++)
        {
            if (File.Exists(Path.Combine(dir, "build.ps1")) &&
                File.Exists(Path.Combine(dir, "runall.ps1")))
            {
                return Path.Combine(dir, "artifacts", "run");
            }
            dir = Path.GetDirectoryName(dir) ?? "";
        }
        return Path.Combine(AppContext.BaseDirectory, "cw-debug-bars");
    }

    private static bool DecodeEvent(string kind)
    {
        switch (kind)
        {
            case "on_dit":
                PendingMorse.Append('.');
                return false;
            case "on_dah":
                PendingMorse.Append('-');
                return false;
            case "off_char":
                return FlushPendingCharacter();
            case "off_word":
                var changed = FlushPendingCharacter();
                if (TextBuffer.Length > 0 && TextBuffer[^1] != ' ')
                {
                    TextBuffer.Append(' ');
                    changed = true;
                }
                return changed;
            default:
                return false;
        }
    }

    private static bool FlushPendingCharacter()
    {
        if (PendingMorse.Length == 0) return false;
        TextBuffer.Append(MorseToChar(PendingMorse.ToString()) ?? '?');
        PendingMorse.Clear();
        return true;
    }

    private static char? MorseToChar(string morse) => morse switch
    {
        ".-" => 'A',
        "-..." => 'B',
        "-.-." => 'C',
        "-.." => 'D',
        "." => 'E',
        "..-." => 'F',
        "--." => 'G',
        "...." => 'H',
        ".." => 'I',
        ".---" => 'J',
        "-.-" => 'K',
        ".-.." => 'L',
        "--" => 'M',
        "-." => 'N',
        "---" => 'O',
        ".--." => 'P',
        "--.-" => 'Q',
        ".-." => 'R',
        "..." => 'S',
        "-" => 'T',
        "..-" => 'U',
        "...-" => 'V',
        ".--" => 'W',
        "-..-" => 'X',
        "-.--" => 'Y',
        "--.." => 'Z',
        ".----" => '1',
        "..---" => '2',
        "...--" => '3',
        "....-" => '4',
        "....." => '5',
        "-...." => '6',
        "--..." => '7',
        "---.." => '8',
        "----." => '9',
        "-----" => '0',
        ".-.-.-" => '.',
        "--..--" => ',',
        "..--.." => '?',
        ".----." => '\'',
        "-.-.--" => '!',
        "-..-." => '/',
        "-.--." => '(',
        "-.--.-" => ')',
        ".-..." => '&',
        "---..." => ':',
        "-.-.-." => ';',
        "-...-" => '=',
        ".-.-." => '+',
        "-....-" => '-',
        "..--.-" => '_',
        ".-..-." => '"',
        "...-..-" => '$',
        ".--.-." => '@',
        _ => null,
    };
}
