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
            _source.LockStateChanged += OnLockStateChanged;
            // Seed initial state from whatever the source already saw before
            // the pane was opened — otherwise the badge would lie about
            // "Waiting" while a real lock is in progress.
            ApplyLockState(_source.CurrentLockState);
        }
        else
        {
            StatusText = "Radio monitor is OFF (Settings → Radio Monitor)";
            ConfidenceText = "—";
            PitchText = "—";
            WpmText = "—";
            LockBadgeText = "○ MONITOR OFF";
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

    /// <summary>
    /// True only when the decoder is currently in the <see cref="CwLockState.Locked"/>
    /// state. Drives the stale/live styling in <c>CwStatsPaneView.axaml</c>:
    /// the WPM and decoded-text panes dim out when this is false to make it
    /// obvious to the operator that the displayed values are no longer being
    /// refreshed.
    /// </summary>
    [ObservableProperty]
    private bool _isLocked;

    /// <summary>
    /// Compact lock indicator shown in the pane header (e.g. "● LIVE",
    /// "◐ PROBATION", "○ HUNTING"). Distilled from <see cref="ICwWpmSampleSource.CurrentLockState"/>
    /// so the operator can see at a glance whether the displayed numbers
    /// are fresh or frozen.
    /// </summary>
    [ObservableProperty]
    private string _lockBadgeText = "○ WAITING";

    public bool IsLive => _source is not null;

    /// <summary>Raised when the operator presses Esc / F9 again to dismiss the pane.</summary>
    public event EventHandler? CloseRequested;

    [RelayCommand]
    private void Close() => CloseRequested?.Invoke(this, EventArgs.Empty);

    /// <summary>
    /// Reset the live displays to a quiescent state. Called by
    /// <c>MainWindowViewModel</c> when a QSO episode boundary fires
    /// (logged / cleared / abandoned) so the pane doesn't keep showing
    /// the previous QSO's last decoded text and WPM after the operator
    /// has moved on. The lock badge is recomputed from the source's
    /// current state so an in-progress lock is preserved across the
    /// reset (decoder doesn't know about QSO boundaries — only the GUI
    /// does).
    /// </summary>
    public void Reset()
    {
        Dispatcher.UIThread.Post(() =>
        {
            _decoded.Clear();
            DecodedText = string.Empty;
            LastGarbledText = string.Empty;
            WpmText = "—";
            PowerText = "—";
            // Re-derive the badge / IsLocked from whatever the source
            // currently reports so we don't lie about being locked when
            // we aren't (or vice versa).
            ApplyLockState(_source?.CurrentLockState ?? CwLockState.Unknown);
        });
    }

    private void OnLockStateChanged(object? sender, CwLockState newState)
        => Dispatcher.UIThread.Post(() => ApplyLockState(newState));

    private void ApplyLockState(CwLockState state)
    {
        IsLocked = state == CwLockState.Locked;
        switch (state)
        {
            case CwLockState.Locked:
                LockBadgeText = "● LIVE";
                ConfidenceText = "LOCKED";
                break;
            case CwLockState.Probation:
                LockBadgeText = "◐ PROBATION";
                ConfidenceText = "PROBATION";
                break;
            case CwLockState.Hunting:
                LockBadgeText = "○ HUNTING";
                ConfidenceText = "HUNTING";
                // Lock just dropped — clear the per-decode text so the
                // operator sees that we're no longer producing fresh
                // characters. Keep WPM as a "last known" reading
                // greyed-out via IsLocked styling.
                _decoded.Clear();
                DecodedText = string.Empty;
                break;
            case CwLockState.Unknown:
            default:
                LockBadgeText = "○ WAITING";
                ConfidenceText = "Waiting…";
                break;
        }
    }

    private void OnSampleReceived(object? sender, CwWpmSample sample)
    {
        // SampleReceived fires on the decoder's stdout pump background thread;
        // marshal to UI thread before mutating ObservableProperty backing fields
        // so that PropertyChanged subscribers (Avalonia bindings) run on the
        // dispatcher.
        Dispatcher.UIThread.Post(() =>
        {
            // The decoder filters wpm events while Hunting (see
            // streaming.rs::filter_for_confidence), but we may briefly
            // observe a wpm sample queued from before the lock dropped.
            // Always show the value but leave IsLocked driving the
            // staleness styling — never display a fresh value alongside
            // a "no lock" badge.
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
                    // Confidence transitions are now driven by the source's
                    // LockStateChanged event (which fires before this raw
                    // line is teed) — that path also drives the lock badge,
                    // IsLocked styling, and decoded-text reset. Skip the
                    // duplicate handling here to avoid double-formatting.
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
            _source.LockStateChanged -= OnLockStateChanged;
        }
    }
}
