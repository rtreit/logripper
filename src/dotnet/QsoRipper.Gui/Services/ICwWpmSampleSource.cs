using System;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Abstraction over a CW WPM sample stream. The GUI's CW WPM panel and
/// the per-QSO aggregator both consume this interface; the round 1
/// implementation is a subprocess host (<see cref="CwDecoderProcessSampleSource"/>)
/// but in round 2 this will be replaced by a streaming gRPC client of the
/// engine-side CwDecodeService without changing the consumer code.
/// </summary>
internal interface ICwWpmSampleSource : IDisposable
{
    /// <summary>True while the source is actively producing samples.</summary>
    bool IsRunning { get; }

    /// <summary>Most recently received sample, or <c>null</c> if none yet.</summary>
    CwWpmSample? LatestSample { get; }

    /// <summary>
    /// Most recent decoder lock/confidence state, derived from
    /// <c>confidence</c> NDJSON events. <see cref="CwLockState.Unknown"/>
    /// until the decoder emits its first confidence event.
    /// </summary>
    CwLockState CurrentLockState { get; }

    /// <summary>Raised on the source's I/O thread for every parsed WPM sample.</summary>
    event EventHandler<CwWpmSample>? SampleReceived;

    /// <summary>
    /// Raised on the source's I/O thread whenever the decoder reports a
    /// new <see cref="CurrentLockState"/>. Used by the GUI to distinguish
    /// fresh live readings from stale displayed values after the decoder
    /// stops emitting (lock dropped → no further wpm/char events).
    /// </summary>
    event EventHandler<CwLockState>? LockStateChanged;

    /// <summary>Raised when the source's running state changes.</summary>
    event EventHandler? StatusChanged;

    /// <summary>
    /// Raised on the source's I/O thread for every non-empty raw NDJSON line
    /// emitted by the underlying decoder. Subscribers (e.g. the diagnostics
    /// recorder) tee these to disk so the full event stream — confidence
    /// transitions, pitch updates, char/word/garbled events, power meter,
    /// etc. — can be replayed offline. Round 1 implementations will simply
    /// skip raising this if no subscribers are attached.
    /// </summary>
    event EventHandler<string>? RawLineReceived;

    /// <summary>Start the source, capturing from <paramref name="deviceOverride"/>
    /// (null/empty = system default capture device).</summary>
    void Start(string? deviceOverride);

    /// <summary>
    /// Tell the decoder the operator has heard a CW anchor manually, allowing
    /// rolling-stream decode to become active when tuning in mid-QSO.
    /// </summary>
    void MarkAnchorHeard();

    /// <summary>Stop the source. Safe to call when not running.</summary>
    void Stop();
}
