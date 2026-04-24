using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.Json;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

/// <summary>
/// Live CW stats overlay, opened with F9 (analogous to the F8 Callsign Card).
/// Subscribes to <see cref="ICwWpmSampleSource.RawLineReceived"/> and parses
/// the cw-decoder NDJSON event stream (the same stream the CW Scope tooling
/// uses) into an at-a-glance display: signal pitch (Hz), confidence/lock
/// state, current WPM, and the most recently decoded characters/words.
/// </summary>
/// <remarks>
/// <para>This pane is independent of the advanced diagnostics recorder —
/// they both consume <c>RawLineReceived</c>, but the pane is the operator
/// surface, while the recorder is the offline-debug bundle.</para>
/// <para>If the radio monitor is OFF (no <see cref="ICwWpmSampleSource"/>
/// available), the pane shows a clear "monitor disabled" state instead of
/// pretending to be live.</para>
/// </remarks>
internal sealed partial class CwStatsPaneViewModel : ObservableObject, IDisposable
{
    private const int MaxDecodedChars = 96;

    private readonly ICwWpmSampleSource? _source;
    private readonly StringBuilder _decoded = new(MaxDecodedChars * 2);
    private bool _disposed;

    public CwStatsPaneViewModel(ICwWpmSampleSource? source)
    {
        _source = source;
        if (_source is not null)
        {
            _source.RawLineReceived += OnRawLineReceived;
            _source.SampleReceived += OnSampleReceived;
        }
        else
        {
            StatusText = "Radio monitor is OFF (Settings → Radio Monitor)";
            ConfidenceText = "—";
            PitchText = "—";
            WpmText = "—";
        }
    }

    [ObservableProperty]
    private string _confidenceText = "Waiting…";

    [ObservableProperty]
    private string _pitchText = "—";

    [ObservableProperty]
    private string _wpmText = "—";

    [ObservableProperty]
    private string _powerText = "—";

    [ObservableProperty]
    private string _statusText = "Listening for events…";

    [ObservableProperty]
    private string _decodedText = string.Empty;

    [ObservableProperty]
    private string _lastGarbledText = string.Empty;

    public bool IsLive => _source is not null;

    /// <summary>Raised when the operator presses Esc / F9 again to dismiss the pane.</summary>
    public event EventHandler? CloseRequested;

    [RelayCommand]
    private void Close() => CloseRequested?.Invoke(this, EventArgs.Empty);

    private void OnSampleReceived(object? sender, CwWpmSample sample)
    {
        // SampleReceived fires on the decoder's stdout pump background thread;
        // marshal to UI thread before mutating ObservableProperty backing fields
        // so that PropertyChanged subscribers (Avalonia bindings) run on the
        // dispatcher.
        Dispatcher.UIThread.Post(() =>
        {
            WpmText = $"{sample.Wpm:0.0} WPM";
        });
    }

    private void OnRawLineReceived(object? sender, string line)
    {
        if (string.IsNullOrWhiteSpace(line))
        {
            return;
        }

        // Defer the entire parse + property write to the UI thread to keep all
        // observable mutations on a single thread.
        Dispatcher.UIThread.Post(() => ProcessRawLine(line));
    }

    private void ProcessRawLine(string line)
    {
        try
        {
            using var doc = JsonDocument.Parse(line);
            // cw-decoder NDJSON uses "type" as the event discriminator, not "event".
            // See experiments/cw-decoder/src/main.rs Event* serializers and the
            // captured session-events.ndjson schema.
            if (!doc.RootElement.TryGetProperty("type", out var typeProp))
            {
                return;
            }

            var eventType = typeProp.GetString();
            switch (eventType)
            {
                case "confidence":
                    HandleConfidence(doc.RootElement);
                    break;
                case "pitch":
                    HandlePitch(doc.RootElement);
                    break;
                case "pitch_lost":
                    PitchText = "lost";
                    StatusText = TryGetString(doc.RootElement, "reason") ?? "Pitch lost";
                    break;
                case "wpm":
                    HandleWpm(doc.RootElement);
                    break;
                case "char":
                    HandleChar(doc.RootElement);
                    break;
                case "word":
                    // The decoder emits a word event as a boundary marker only
                    // (no text payload). Insert a space so the running decoded
                    // text reads naturally.
                    AppendDecoded(" ");
                    break;
                case "garbled":
                    HandleGarbled(doc.RootElement);
                    break;
                case "power":
                    HandlePower(doc.RootElement);
                    break;
                case "ready":
                    StatusText = "Decoder ready";
                    break;
                default:
                    break;
            }
        }
        catch (JsonException)
        {
            // Non-JSON or unexpected line — ignore silently. The decoder
            // also writes occasional human-readable status, which is fine.
        }
    }

    private void HandleConfidence(JsonElement root)
    {
        var state = TryGetString(root, "state") ?? "?";
        // The decoder's confidence event payload is just {state: "hunting"|"locked"|"probation"}.
        // No score field — round 1 confidence reporting is binary, not graded.
        ConfidenceText = state.ToUpperInvariant();
    }

    private void HandlePitch(JsonElement root)
    {
        var hz = TryGetDouble(root, "hz");
        if (hz.HasValue)
        {
            PitchText = string.Create(CultureInfo.InvariantCulture, $"{hz.Value:0} Hz");
        }
    }

    private void HandleWpm(JsonElement root)
    {
        var wpm = TryGetDouble(root, "wpm");
        if (wpm.HasValue)
        {
            WpmText = string.Create(CultureInfo.InvariantCulture, $"{wpm.Value:0.0} WPM");
        }
    }

    private void HandleChar(JsonElement root)
    {
        // Decoder emits {ch: "N", morse: "-.", ...} — the field is "ch", not "char".
        var ch = TryGetString(root, "ch");
        if (!string.IsNullOrEmpty(ch))
        {
            AppendDecoded(ch);
        }
    }

    private void HandleGarbled(JsonElement root)
    {
        var symbol = TryGetString(root, "symbol")
            ?? TryGetString(root, "morse")
            ?? TryGetString(root, "raw")
            ?? "?";
        LastGarbledText = symbol;
        AppendDecoded("·");
    }

    private void HandlePower(JsonElement root)
    {
        // SNR is far more operator-useful than raw power amplitude here.
        var snr = TryGetDouble(root, "snr");
        if (snr.HasValue)
        {
            // SNR is dimensionless ratio; show as dB for at-a-glance comparison.
            var snrDb = snr.Value > 0 ? 10.0 * Math.Log10(snr.Value) : double.NegativeInfinity;
            PowerText = double.IsFinite(snrDb)
                ? string.Create(CultureInfo.InvariantCulture, $"SNR {snrDb:0.0} dB")
                : "SNR —";
        }
    }

    private void AppendDecoded(string fragment)
    {
        _decoded.Append(fragment);
        if (_decoded.Length > MaxDecodedChars)
        {
            _decoded.Remove(0, _decoded.Length - MaxDecodedChars);
        }
        DecodedText = _decoded.ToString();
    }

    private static string? TryGetString(JsonElement root, string name)
        => root.TryGetProperty(name, out var p) && p.ValueKind == JsonValueKind.String ? p.GetString() : null;

    private static double? TryGetDouble(JsonElement root, string name)
    {
        if (!root.TryGetProperty(name, out var p))
        {
            return null;
        }
        return p.ValueKind switch
        {
            JsonValueKind.Number => p.TryGetDouble(out var d) ? d : null,
            JsonValueKind.String => double.TryParse(p.GetString(), NumberStyles.Float, CultureInfo.InvariantCulture, out var d) ? d : null,
            _ => null,
        };
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        if (_source is not null)
        {
            _source.RawLineReceived -= OnRawLineReceived;
            _source.SampleReceived -= OnSampleReceived;
        }
    }
}
