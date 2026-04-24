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

    /// <summary>Raised on the source's I/O thread for every parsed WPM sample.</summary>
    event EventHandler<CwWpmSample>? SampleReceived;

    /// <summary>Raised when the source's running state changes.</summary>
    event EventHandler? StatusChanged;

    /// <summary>Start the source, capturing from <paramref name="deviceOverride"/>
    /// (null/empty = system default capture device).</summary>
    void Start(string? deviceOverride);

    /// <summary>Stop the source. Safe to call when not running.</summary>
    void Stop();
}
