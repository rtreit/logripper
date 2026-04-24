using System;
using System.Collections.Generic;
using System.Linq;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Buffers CW WPM samples from an <see cref="ICwWpmSampleSource"/> and
/// computes a single representative WPM value for an arbitrary
/// <c>[utcStart, utcEnd]</c> window — typically a freshly-completed QSO.
///
/// Aggregation is a <b>time-weighted</b> mean: each sample contributes a
/// weight equal to the duration over which it was the "current" reading
/// (sample i is held until sample i+1 arrives, or until <paramref
/// name="MaxSampleHoldDuration"/> expires). This avoids the bias the
/// arithmetic mean would have toward periods where the decoder happened
/// to emit more frequent updates.
///
/// Restart awareness: every sample carries a monotonic epoch from the
/// source. A source restart bumps the epoch, which the UI can surface;
/// the aggregator itself relies on the wall-clock gap and on the next
/// sample (regardless of epoch) capping the previous sample's hold so
/// stale per-decoder state never silently extends across a restart.
/// </summary>
internal sealed class CwQsoWpmAggregator : IDisposable
{
    /// <summary>
    /// Maximum amount of time a single sample is allowed to "carry" with
    /// no follow-up. Past this, the sample is treated as no longer
    /// representative of the signal — useful when the decoder pauses or
    /// the operator's CW transmission has gaps.
    /// </summary>
    public TimeSpan MaxSampleHoldDuration { get; }

    /// <summary>
    /// Hard cap on retained samples. The aggregator keeps the most recent
    /// samples up to this limit; older ones are dropped. Default sized
    /// for the longest realistic QSO ragchew at typical update rates.
    /// </summary>
    public int MaxRetainedSamples { get; }

    private readonly object _lock = new();
    private readonly LinkedList<CwWpmSample> _samples = new();
    private ICwWpmSampleSource? _source;

    public CwQsoWpmAggregator(
        ICwWpmSampleSource source,
        TimeSpan? maxSampleHoldDuration = null,
        int maxRetainedSamples = 8192)
    {
        _source = source ?? throw new ArgumentNullException(nameof(source));
        MaxSampleHoldDuration = maxSampleHoldDuration ?? TimeSpan.FromSeconds(5);
        MaxRetainedSamples = maxRetainedSamples;
        _source.SampleReceived += OnSampleReceived;
    }

    /// <summary>
    /// Returns the time-weighted mean WPM across the supplied window, or
    /// <c>null</c> if no usable samples fall in the window. The window
    /// boundaries may sit before the first sample or after the last; in
    /// either case only the overlapping portion contributes weight.
    /// </summary>
    public double? GetMeanWpm(DateTimeOffset utcStart, DateTimeOffset utcEnd)
    {
        if (utcEnd <= utcStart)
        {
            return null;
        }

        // Snapshot sample list under the lock so we don't trip over the
        // background pump thread; aggregation math runs lock-free.
        CwWpmSample[] snapshot;
        lock (_lock)
        {
            if (_samples.Count == 0)
            {
                return null;
            }

            snapshot = _samples.ToArray();
        }

        double weightedSum = 0.0;
        double totalWeight = 0.0;

        for (int i = 0; i < snapshot.Length; i++)
        {
            var current = snapshot[i];

            // Each sample carries its WPM forward until the next sample
            // arrives or MaxSampleHoldDuration elapses, whichever comes
            // first. The next sample terminates the current segment even
            // when its epoch differs (i.e. across a source restart) — a
            // restart should never extend the previous sample's "alive"
            // window, only shorten it.
            var segmentEnd = current.ReceivedUtc + MaxSampleHoldDuration;
            if (i + 1 < snapshot.Length)
            {
                var next = snapshot[i + 1];
                if (next.ReceivedUtc < segmentEnd)
                {
                    segmentEnd = next.ReceivedUtc;
                }
            }

            // Clip [current.ReceivedUtc, segmentEnd] against the QSO window.
            var clipStart = current.ReceivedUtc < utcStart ? utcStart : current.ReceivedUtc;
            var clipEnd = segmentEnd > utcEnd ? utcEnd : segmentEnd;
            if (clipEnd <= clipStart)
            {
                continue;
            }

            var weight = (clipEnd - clipStart).TotalSeconds;
            if (weight <= 0 || !double.IsFinite(current.Wpm) || current.Wpm <= 0)
            {
                continue;
            }

            weightedSum += current.Wpm * weight;
            totalWeight += weight;
        }

        if (totalWeight <= 0)
        {
            return null;
        }

        return weightedSum / totalWeight;
    }

    /// <summary>
    /// Returns a snapshot of all retained samples whose <see cref="CwWpmSample.ReceivedUtc"/>
    /// timestamp falls inside the supplied window (inclusive of both ends).
    /// Used by the diagnostics recorder to attach the slice of live samples
    /// the aggregator considered when computing the QSO's WPM. Returns an
    /// empty list if no samples fall in the window.
    /// </summary>
    public IReadOnlyList<CwWpmSample> GetSamplesInWindow(DateTimeOffset utcStart, DateTimeOffset utcEnd)
    {
        if (utcEnd < utcStart)
        {
            return Array.Empty<CwWpmSample>();
        }

        lock (_lock)
        {
            return _samples
                .Where(s => s.ReceivedUtc >= utcStart && s.ReceivedUtc <= utcEnd)
                .ToArray();
        }
    }

    /// <summary>Drops all retained samples. Used by tests and on settings reset.</summary>
    public void Clear()
    {
        lock (_lock)
        {
            _samples.Clear();
        }
    }

    /// <summary>Test/diagnostic accessor for the retained sample count.</summary>
    internal int SampleCount
    {
        get { lock (_lock) { return _samples.Count; } }
    }

    /// <summary>Inject a sample directly. For tests only.</summary>
    internal void IngestForTest(CwWpmSample sample) => OnSampleReceived(this, sample);

    private void OnSampleReceived(object? sender, CwWpmSample sample)
    {
        lock (_lock)
        {
            _samples.AddLast(sample);
            while (_samples.Count > MaxRetainedSamples)
            {
                _samples.RemoveFirst();
            }
        }
    }

    public void Dispose()
    {
        var source = _source;
        if (source is not null)
        {
            source.SampleReceived -= OnSampleReceived;
            _source = null;
        }
    }
}
