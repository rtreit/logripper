using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.Json;
using Google.Protobuf;
using QsoRipper.Domain;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Records everything needed to debug the GUI's CW WPM auto-fill flow offline.
///
/// One <see cref="CwDiagnosticsRecorder"/> = one radio-monitor session. The
/// session directory holds:
/// <list type="bullet">
///   <item><c>session.json</c> — startup metadata: cw-decoder binary path,
///         arguments, capture device, host info, session start UTC.</item>
///   <item><c>session.wav</c> — continuous mirror of the audio fed into the
///         decoder. Created by cw-decoder itself via <c>--record</c>; this
///         class only records the path it asked for.</item>
///   <item><c>session-events.ndjson</c> — every raw NDJSON line emitted by
///         the decoder, tee'd as it arrives.</item>
///   <item><c>episodes\episode-NNN\events.ndjson</c> — slice of NDJSON
///         events from the moment the decoder reported
///         <c>confidence/state="locked"</c> until a QSO save or clear
///         finalized the episode.</item>
///   <item><c>episodes\episode-NNN\ux-snapshot.json</c> — the comparison
///         payload: live WPM samples in the QSO window, the value the UX
///         was displaying at finalize time, the aggregator's mean (i.e.
///         what landed on <c>QsoRecord.CwDecodeRxWpm</c>), the QSO record
///         itself if logged, and a copy/paste command to re-run the
///         decoder offline against <c>session.wav</c> over the same
///         time window.</item>
/// </list>
///
/// All file I/O happens under a single lock to guarantee ordering between
/// the stdout pump thread (raw lines) and the UI thread (episode finalize).
/// </summary>
internal sealed class CwDiagnosticsRecorder : IDisposable
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private readonly object _lock = new();
    private readonly string _sessionDir;
    private readonly string _sessionWavPath;
    private readonly string _sessionEventsPath;
    private readonly string _episodesDir;
    private readonly DateTimeOffset _sessionStartUtc;
    private StreamWriter? _sessionEventsWriter;
    private EpisodeRecorder? _activeEpisode;
    private int _episodeCounter;
    private bool _disposed;

    public string SessionDirectory => _sessionDir;
    public string SessionWavPath => _sessionWavPath;
    public string SessionEventsPath => _sessionEventsPath;

    public CwDiagnosticsRecorder(
        string sessionDirectory,
        DateTimeOffset sessionStartUtc,
        string? decoderBinaryPath,
        string? deviceName,
        bool loopback)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(sessionDirectory);
        _sessionDir = sessionDirectory;
        _sessionStartUtc = sessionStartUtc;
        Directory.CreateDirectory(_sessionDir);
        _episodesDir = Path.Combine(_sessionDir, "episodes");
        Directory.CreateDirectory(_episodesDir);
        _sessionWavPath = Path.Combine(_sessionDir, "session.wav");
        _sessionEventsPath = Path.Combine(_sessionDir, "session-events.ndjson");

        // Open the session events writer immediately so raw lines that
        // arrive before the first lock event are still captured.
        _sessionEventsWriter = new StreamWriter(
            new FileStream(_sessionEventsPath, FileMode.Create, FileAccess.Write, FileShare.Read))
        {
            AutoFlush = true,
        };

        WriteSessionMetadata(decoderBinaryPath, deviceName, loopback);
    }

    /// <summary>
    /// Tee a single raw NDJSON line from the decoder. Always writes to the
    /// session-wide stream. If an episode is currently open (started by an
    /// explicit <see cref="BeginEpisode"/> call from the logger), also
    /// writes the line into that episode's events stream so the per-QSO
    /// bundle contains the exact sequence of decoder events the user saw
    /// between callsign-first-typed and save/clear.
    /// </summary>
    public void IngestRawLine(string ndjsonLine)
    {
        if (string.IsNullOrWhiteSpace(ndjsonLine))
        {
            return;
        }

        lock (_lock)
        {
            if (_disposed || _sessionEventsWriter is null)
            {
                return;
            }

            _sessionEventsWriter.WriteLine(ndjsonLine);
            _activeEpisode?.WriteLine(ndjsonLine);
        }
    }

    /// <summary>
    /// Open a new diagnostic episode aligned to the start of operator
    /// activity on a QSO (the moment a callsign is first typed). Idempotent
    /// when an episode is already open — multiple keystrokes in the same
    /// QSO won't double-start. The matching call is
    /// <see cref="FinalizeEpisode"/>.
    /// </summary>
    public void BeginEpisode(DateTimeOffset utcStart)
    {
        lock (_lock)
        {
            if (_disposed || _activeEpisode is not null)
            {
                return;
            }
            StartEpisodeLocked(utcStart);
        }
    }

    /// <summary>
    /// Close the current episode (if any) and write its <c>ux-snapshot.json</c>
    /// containing the comparison payload between live UX state and the
    /// decoder's underlying samples. Safe to call when no episode is active —
    /// in that case nothing is written.
    /// </summary>
    /// <param name="reason">"logged" or "cleared".</param>
    /// <param name="loggedQso">QSO record that was just persisted, or
    ///     <c>null</c> when the operator cleared the form.</param>
    /// <param name="displayedUiWpm">What the status bar was showing
    ///     (e.g. <c>_cwSampleSource.LatestSample.Wpm</c>) at finalize time.
    ///     Null if the UI had no current value.</param>
    /// <param name="displayedStatusText">Raw status-text string the user
    ///     was looking at (e.g. "CW WPM: 13.4"). Null if unavailable.</param>
    /// <param name="aggregateMeanWpm">Aggregator's time-weighted mean over
    ///     <c>[utcStart, utcEnd]</c> — the value that was about to be (or
    ///     just was) written onto <c>QsoRecord.CwDecodeRxWpm</c>.</param>
    /// <param name="samplesInWindow">Slice of CwWpmSamples the aggregator
    ///     considered when computing the mean.</param>
    /// <param name="utcStart">QSO window start.</param>
    /// <param name="utcEnd">QSO window end.</param>
    public void FinalizeEpisode(
        string reason,
        QsoRecord? loggedQso,
        double? displayedUiWpm,
        string? displayedStatusText,
        double? aggregateMeanWpm,
        IReadOnlyList<CwWpmSample> samplesInWindow,
        DateTimeOffset utcStart,
        DateTimeOffset utcEnd)
    {
        ArgumentNullException.ThrowIfNull(reason);
        ArgumentNullException.ThrowIfNull(samplesInWindow);

        EpisodeRecorder? episode;
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }
            episode = _activeEpisode;
            _activeEpisode = null;
        }

        if (episode is null)
        {
            return;
        }

        var endUtc = utcEnd;
        episode.WriteSnapshot(
            reason,
            loggedQso,
            displayedUiWpm,
            displayedStatusText,
            aggregateMeanWpm,
            samplesInWindow,
            utcStart,
            endUtc,
            _sessionStartUtc,
            _sessionWavPath);
        episode.Dispose();
    }

    public void Dispose()
    {
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;

            _activeEpisode?.Dispose();
            _activeEpisode = null;

            try
            {
                _sessionEventsWriter?.Flush();
                _sessionEventsWriter?.Dispose();
            }
            catch (IOException)
            {
                // Best-effort flush on shutdown.
            }
            _sessionEventsWriter = null;
        }
    }

    /// <summary>
    /// Test/diagnostic accessor — true when an episode has been opened
    /// (i.e. the decoder has reported <c>confidence/locked</c>) and not yet
    /// finalized.
    /// </summary>
    internal bool HasActiveEpisode
    {
        get { lock (_lock) { return _activeEpisode is not null; } }
    }

    /// <summary>
    /// Test hook that opens an episode without going through the
    /// callsign-keystroke path. Used by recorder unit tests.
    /// </summary>
    internal void StartEpisodeForTest(DateTimeOffset? utcStart = null)
    {
        lock (_lock)
        {
            if (_disposed || _activeEpisode is not null)
            {
                return;
            }
            StartEpisodeLocked(utcStart ?? DateTimeOffset.UtcNow);
        }
    }

    private void StartEpisodeLocked(DateTimeOffset utcStart)
    {
        _episodeCounter++;
        var episodeDir = Path.Combine(
            _episodesDir,
            string.Create(CultureInfo.InvariantCulture, $"episode-{_episodeCounter:000}"));
        Directory.CreateDirectory(episodeDir);
        _activeEpisode = new EpisodeRecorder(
            _episodeCounter,
            episodeDir,
            utcStart);
    }

    private void WriteSessionMetadata(string? decoderBinaryPath, string? deviceName, bool loopback)
    {
        var meta = new SessionMetadata
        {
            SessionStartUtc = _sessionStartUtc,
            DecoderBinaryPath = decoderBinaryPath,
            DeviceName = deviceName,
            Loopback = loopback,
            SessionWavPath = _sessionWavPath,
            HostMachineName = Environment.MachineName,
            HostOs = Environment.OSVersion.VersionString,
            ProcessId = Environment.ProcessId,
            ClrVersion = Environment.Version.ToString(),
        };

        var path = Path.Combine(_sessionDir, "session.json");
        File.WriteAllText(path, JsonSerializer.Serialize(meta, JsonOptions));
    }

    private sealed class EpisodeRecorder : IDisposable
    {
        private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

        private readonly int _index;
        private readonly string _dir;
        private readonly DateTimeOffset _startUtc;
        private StreamWriter? _events;

        public EpisodeRecorder(int index, string dir, DateTimeOffset startUtc)
        {
            _index = index;
            _dir = dir;
            _startUtc = startUtc;
            _events = new StreamWriter(
                new FileStream(Path.Combine(dir, "events.ndjson"), FileMode.Create, FileAccess.Write, FileShare.Read))
            {
                AutoFlush = true,
            };
        }

        public void WriteLine(string ndjsonLine)
        {
            try
            {
                _events?.WriteLine(ndjsonLine);
            }
            catch (IOException)
            {
                // Best effort during shutdown.
            }
        }

        public void WriteSnapshot(
            string reason,
            QsoRecord? loggedQso,
            double? displayedUiWpm,
            string? displayedStatusText,
            double? aggregateMeanWpm,
            IReadOnlyList<CwWpmSample> samplesInWindow,
            DateTimeOffset utcStart,
            DateTimeOffset utcEnd,
            DateTimeOffset sessionStartUtc,
            string sessionWavPath)
        {
            // Wall-clock window → seconds-into-WAV mapping. The WAV starts
            // recording at sessionStartUtc, so any time T inside the window
            // corresponds to (T - sessionStartUtc).TotalSeconds in the WAV.
            var startOffsetSec = Math.Max(0.0, (utcStart - sessionStartUtc).TotalSeconds);
            var endOffsetSec = Math.Max(startOffsetSec, (utcEnd - sessionStartUtc).TotalSeconds);

            var snapshot = new EpisodeSnapshot
            {
                EpisodeIndex = _index,
                Reason = reason,
                EpisodeStartUtc = _startUtc,
                QsoWindowStartUtc = utcStart,
                QsoWindowEndUtc = utcEnd,
                SessionStartUtc = sessionStartUtc,
                WavStartOffsetSec = startOffsetSec,
                WavEndOffsetSec = endOffsetSec,
                DisplayedUiWpm = displayedUiWpm,
                DisplayedStatusText = displayedStatusText,
                AggregateMeanWpm = aggregateMeanWpm,
                LoggedQsoJson = loggedQso is null ? null : SafeFormatQso(loggedQso),
                LiveSamples = samplesInWindow
                    .Select(s => new SampleEntry
                    {
                        UtcReceived = s.ReceivedUtc,
                        SecondsIntoWav = Math.Max(0.0, (s.ReceivedUtc - sessionStartUtc).TotalSeconds),
                        Wpm = s.Wpm,
                        Epoch = s.Epoch,
                    })
                    .ToArray(),
                ReproCommand = BuildReproCommand(sessionWavPath, startOffsetSec, endOffsetSec),
            };

            File.WriteAllText(
                Path.Combine(_dir, "ux-snapshot.json"),
                JsonSerializer.Serialize(snapshot, JsonOptions));
        }

        public void Dispose()
        {
            try
            {
                _events?.Flush();
                _events?.Dispose();
            }
            catch (IOException)
            {
                // Best effort during shutdown.
            }
            _events = null;
        }

        private static string? SafeFormatQso(QsoRecord qso)
        {
            try
            {
                return JsonFormatter.Default.Format(qso);
            }
#pragma warning disable CA1031 // Diagnostics formatting must never throw on weird proto state.
            catch (Exception ex)
            {
                return $"<qso-format-error: {ex.GetType().Name}: {ex.Message}>";
            }
#pragma warning restore CA1031
        }

        private static string BuildReproCommand(string wavPath, double startSec, double endSec)
        {
            // decode-and-play supports --start/--end --json over an arbitrary
            // file region. The user can paste this and get a single offline
            // decode pass over the same audio the live UX saw.
            return string.Create(
                CultureInfo.InvariantCulture,
                $"cw-decoder decode-and-play --json --start {startSec:F2} --end {endSec:F2} \"{wavPath}\"");
        }
    }

    private sealed class SessionMetadata
    {
        public DateTimeOffset SessionStartUtc { get; set; }
        public string? DecoderBinaryPath { get; set; }
        public string? DeviceName { get; set; }
        public bool Loopback { get; set; }
        public string? SessionWavPath { get; set; }
        public string? HostMachineName { get; set; }
        public string? HostOs { get; set; }
        public int ProcessId { get; set; }
        public string? ClrVersion { get; set; }
    }

    private sealed class EpisodeSnapshot
    {
        public int EpisodeIndex { get; set; }
        public string Reason { get; set; } = string.Empty;
        public DateTimeOffset EpisodeStartUtc { get; set; }
        public DateTimeOffset QsoWindowStartUtc { get; set; }
        public DateTimeOffset QsoWindowEndUtc { get; set; }
        public DateTimeOffset SessionStartUtc { get; set; }
        public double WavStartOffsetSec { get; set; }
        public double WavEndOffsetSec { get; set; }
        public double? DisplayedUiWpm { get; set; }
        public string? DisplayedStatusText { get; set; }
        public double? AggregateMeanWpm { get; set; }
        public string? LoggedQsoJson { get; set; }
        public SampleEntry[] LiveSamples { get; set; } = Array.Empty<SampleEntry>();
        public string ReproCommand { get; set; } = string.Empty;
    }

    private sealed class SampleEntry
    {
        public DateTimeOffset UtcReceived { get; set; }
        public double SecondsIntoWav { get; set; }
        public double Wpm { get; set; }
        public long Epoch { get; set; }
    }
}
