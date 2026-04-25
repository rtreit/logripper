using System;

namespace QsoRipper.Gui.Services;

/// <summary>
/// A single WPM measurement emitted by an upstream CW decoder.
///
/// The timestamp is a wall-clock UTC stamp captured by the GUI when the
/// sample arrives from the source — not the decoder's internal monotonic
/// clock. That way the aggregator can correctly window samples against a
/// QSO's start/end times even if the decoder subprocess restarts mid-QSO
/// (which would reset the decoder's internal clock to zero).
/// </summary>
/// <param name="ReceivedUtc">Wall-clock time the sample was received.</param>
/// <param name="Wpm">Estimated words-per-minute (uint-ish; fractional OK).</param>
/// <param name="Epoch">Monotonic counter that increments on every source restart.
/// Used by the aggregator to discard samples that straddle a restart so
/// stale per-decoder state cannot be averaged with fresh state.</param>
internal readonly record struct CwWpmSample(DateTimeOffset ReceivedUtc, double Wpm, long Epoch);
