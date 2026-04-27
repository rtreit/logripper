using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.Tests;

public sealed class CwDiagnosticsRecorderTests : IDisposable
{
    private readonly string _tempRoot;

    public CwDiagnosticsRecorderTests()
    {
        _tempRoot = Path.Combine(Path.GetTempPath(), "qsoripper-cwdiag-tests", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_tempRoot);
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_tempRoot))
            {
                Directory.Delete(_tempRoot, recursive: true);
            }
        }
        catch (IOException)
        {
            // best-effort cleanup
        }
    }

    [Fact]
    public void WhenEpisodeRunsThenSidecarFilesAreWritten()
    {
        var sessionDir = Path.Combine(_tempRoot, "session-1");
        var sessionStart = new DateTimeOffset(2025, 1, 1, 12, 0, 0, TimeSpan.Zero);
        var qsoStart = sessionStart.AddSeconds(10);
        var qsoEnd = sessionStart.AddSeconds(40);

        using (var recorder = new CwDiagnosticsRecorder(
                   sessionDir,
                   sessionStart,
                   decoderBinaryPath: "fake/cw-decoder.exe",
                   deviceName: "Test Device",
                   loopback: false))
        {
            // Lines arriving before BeginEpisode should still hit the
            // session-wide stream but NOT the (nonexistent) episode stream.
            recorder.IngestRawLine("{\"event\":\"power\",\"dbfs\":-22.5}");

            recorder.BeginEpisode(qsoStart);
            recorder.IngestRawLine("{\"event\":\"confidence\",\"state\":\"locked\",\"score\":0.92}");
            recorder.IngestRawLine("{\"event\":\"wpm\",\"wpm\":18.4}");

            var samples = new List<CwWpmSample>
            {
                new(qsoStart.AddSeconds(2), 18.4, Epoch: 1),
                new(qsoStart.AddSeconds(8), 19.1, Epoch: 1),
            };

            var qso = new QsoRecord { WorkedCallsign = "K7TEST", CwDecodeRxWpm = 19 };

            recorder.FinalizeEpisode(
                reason: "logged",
                loggedQso: qso,
                displayedUiWpm: 19.1,
                displayedStatusText: "CW WPM: 19.1",
                aggregateMeanWpm: 18.7,
                samplesInWindow: samples,
                utcStart: qsoStart,
                utcEnd: qsoEnd);
        }

        Assert.True(File.Exists(Path.Combine(sessionDir, "session.json")), "session.json missing");
        Assert.True(File.Exists(Path.Combine(sessionDir, "session-events.ndjson")), "session-events.ndjson missing");
        var episodeDir = Path.Combine(sessionDir, "episodes", "episode-001");
        Assert.True(Directory.Exists(episodeDir), "episode-001 dir missing");
        Assert.True(File.Exists(Path.Combine(episodeDir, "events.ndjson")), "episode events.ndjson missing");
        var snapshotPath = Path.Combine(episodeDir, "ux-snapshot.json");
        Assert.True(File.Exists(snapshotPath), "ux-snapshot.json missing");

        using var doc = JsonDocument.Parse(File.ReadAllText(snapshotPath));
        var root = doc.RootElement;
        Assert.Equal("logged", root.GetProperty("Reason").GetString());
        Assert.Equal(18.7, root.GetProperty("AggregateMeanWpm").GetDouble(), 4);
        Assert.Equal(19.1, root.GetProperty("DisplayedUiWpm").GetDouble(), 4);
        Assert.Equal(2, root.GetProperty("LiveSamples").GetArrayLength());

        var sessionEvents = File.ReadAllLines(Path.Combine(sessionDir, "session-events.ndjson"));
        Assert.Equal(3, sessionEvents.Length);

        var episodeEvents = File.ReadAllLines(Path.Combine(episodeDir, "events.ndjson"));
        Assert.Equal(2, episodeEvents.Length);
    }

    [Fact]
    public void WhenFinalizeCalledWithoutBeginThenNoEpisodeFilesWritten()
    {
        var sessionDir = Path.Combine(_tempRoot, "session-2");
        var sessionStart = new DateTimeOffset(2025, 1, 1, 12, 0, 0, TimeSpan.Zero);

        using (var recorder = new CwDiagnosticsRecorder(
                   sessionDir,
                   sessionStart,
                   decoderBinaryPath: null,
                   deviceName: null,
                   loopback: true))
        {
            recorder.FinalizeEpisode(
                reason: "cleared",
                loggedQso: null,
                displayedUiWpm: null,
                displayedStatusText: null,
                aggregateMeanWpm: null,
                samplesInWindow: Array.Empty<CwWpmSample>(),
                utcStart: sessionStart,
                utcEnd: sessionStart.AddSeconds(1));
        }

        var episodesDir = Path.Combine(sessionDir, "episodes");
        Assert.True(Directory.Exists(episodesDir));
        Assert.Empty(Directory.GetDirectories(episodesDir));
    }
}
