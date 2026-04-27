using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Text.Json;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Captures decoded CW characters/words/garbled symbols emitted by the live
/// decoder via <see cref="ICwWpmSampleSource.RawLineReceived"/> and exposes
/// the concatenated transcript for an arbitrary
/// <c>[utcStart, utcEnd]</c> window — typically a freshly-completed QSO.
/// Mirrors the lifetime/ownership model of <see cref="CwQsoWpmAggregator"/>:
/// constructed by <c>MainWindowViewModel</c> alongside the WPM aggregator,
/// disposed when the source is torn down.
///
/// <para>
/// Robustness rules:
/// <list type="bullet">
///   <item>The handler runs on the source's stdout pump thread. All
///         exceptions are swallowed inside the handler so a single
///         malformed NDJSON line can never kill the pump (see #324
///         post-mortem).</item>
///   <item>The aggregator is internally locked; a second concurrent
///         subscriber on the same source is safe.</item>
///   <item>Retained fragments are capped at <see cref="MaxRetainedFragments"/>
///         to bound memory across long ragchews; older fragments are
///         dropped.</item>
///   <item>Text is normalized at read time (collapse runs of whitespace,
///         trim leading/trailing whitespace) so the transcript reads
///         naturally regardless of how `word` events fell relative to
///         `char` events.</item>
/// </list>
/// </para>
/// </summary>
internal sealed class CwQsoTranscriptAggregator : IDisposable
{
    /// <summary>Hard cap on retained fragment events. Sized for very long
    /// ragchews at typical CW rates without unbounded growth.</summary>
    public int MaxRetainedFragments { get; }

    private readonly object _lock = new();
    private readonly LinkedList<TranscriptFragment> _fragments = new();
    private ICwWpmSampleSource? _source;

    public CwQsoTranscriptAggregator(
        ICwWpmSampleSource source,
        int maxRetainedFragments = 65_536)
    {
        _source = source ?? throw new ArgumentNullException(nameof(source));
        MaxRetainedFragments = maxRetainedFragments;
        _source.RawLineReceived += OnRawLineReceived;
    }

    /// <summary>
    /// Returns the decoded transcript for the supplied window, or null
    /// if no usable fragments fall in the window. Window edges are
    /// inclusive on the start and exclusive on the end so adjacent QSOs
    /// don't double-count a boundary fragment.
    /// </summary>
    public string? GetTranscript(DateTimeOffset utcStart, DateTimeOffset utcEnd)
    {
        if (utcEnd <= utcStart)
        {
            return null;
        }

        TranscriptFragment[] snapshot;
        lock (_lock)
        {
            if (_fragments.Count == 0)
            {
                return null;
            }

            snapshot = _fragments.ToArray();
        }

        var sb = new StringBuilder(snapshot.Length);
        foreach (var fragment in snapshot)
        {
            if (fragment.ReceivedUtc < utcStart || fragment.ReceivedUtc >= utcEnd)
            {
                continue;
            }

            sb.Append(fragment.Text);
        }

        if (sb.Length == 0)
        {
            return null;
        }

        var normalized = Normalize(sb);
        return normalized.Length == 0 ? null : normalized;
    }

    /// <summary>Drops all retained fragments. Used on settings reset / tests.</summary>
    public void Clear()
    {
        lock (_lock)
        {
            _fragments.Clear();
        }
    }

    /// <summary>Test/diagnostic accessor for the retained fragment count.</summary>
    internal int FragmentCount
    {
        get { lock (_lock) { return _fragments.Count; } }
    }

    /// <summary>Inject a raw NDJSON line directly. For tests only.</summary>
    internal void IngestForTest(string ndjsonLine) => OnRawLineReceived(this, ndjsonLine);

    private void OnRawLineReceived(object? sender, string line)
    {
        // Runs on the source's stdout pump thread. Catch everything —
        // a JsonException, an OOM in StringBuilder, or any other failure
        // here must NOT kill the pump (which would silently freeze WPM
        // updates and any other downstream subscribers).
        try
        {
            if (string.IsNullOrWhiteSpace(line))
            {
                return;
            }

            var fragment = TryParseFragment(line);
            if (fragment is null)
            {
                return;
            }

            lock (_lock)
            {
                _fragments.AddLast(fragment.Value);
                while (_fragments.Count > MaxRetainedFragments)
                {
                    _fragments.RemoveFirst();
                }
            }
        }
#pragma warning disable CA1031, RCS1075 // background pump handler must not crash on a single bad line
        catch (Exception ex)
        {
            // Intentionally swallowed — see class doc on robustness rules.
            // A malformed NDJSON line, OOM, or any other failure here must
            // NOT kill the source's stdout pump (which would silently
            // freeze WPM updates and any other downstream subscribers).
            System.Diagnostics.Trace.WriteLine($"CwQsoTranscriptAggregator handler error: {ex.GetType().Name}: {ex.Message}");
        }
#pragma warning restore CA1031, RCS1075
    }

    private static TranscriptFragment? TryParseFragment(string line)
    {
        using var doc = JsonDocument.Parse(line);
        if (!doc.RootElement.TryGetProperty("type", out var typeProp))
        {
            return null;
        }

        var eventType = typeProp.GetString();
        switch (eventType)
        {
            case "char":
                {
                    if (!doc.RootElement.TryGetProperty("ch", out var chProp))
                    {
                        return null;
                    }
                    var ch = chProp.GetString();
                    if (string.IsNullOrEmpty(ch))
                    {
                        return null;
                    }
                    return new TranscriptFragment(DateTimeOffset.UtcNow, ch);
                }
            case "word":
                return new TranscriptFragment(DateTimeOffset.UtcNow, " ");
            case "garbled":
                return new TranscriptFragment(DateTimeOffset.UtcNow, "?");
            default:
                return null;
        }
    }

    /// <summary>
    /// Normalize the assembled transcript so it reads naturally:
    /// collapse runs of whitespace into a single space and trim leading
    /// or trailing whitespace introduced by `word` events at episode
    /// boundaries. Strips ASCII control characters (defensive — the
    /// decoder shouldn't emit them) but preserves all printable chars.
    /// </summary>
    internal static string Normalize(StringBuilder source)
    {
        var sb = new StringBuilder(source.Length);
        bool prevSpace = true;
        for (int i = 0; i < source.Length; i++)
        {
            var c = source[i];
            if (c == '\r' || c == '\n' || c == '\t' || c == ' ')
            {
                if (!prevSpace)
                {
                    sb.Append(' ');
                    prevSpace = true;
                }
                continue;
            }

            if (c < 0x20)
            {
                continue;
            }

            sb.Append(c);
            prevSpace = false;
        }

        // Trailing space normalization.
        while (sb.Length > 0 && sb[^1] == ' ')
        {
            sb.Length--;
        }

        return sb.ToString();
    }

    public void Dispose()
    {
        var source = _source;
        if (source is not null)
        {
            source.RawLineReceived -= OnRawLineReceived;
            _source = null;
        }
    }

    private readonly record struct TranscriptFragment(DateTimeOffset ReceivedUtc, string Text);
}
