using System;
using System.Collections.ObjectModel;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Threading;
using CwDecoderGui.Models;
using CwDecoderGui.Services;

namespace CwDecoderGui.ViewModels;

/// <summary>
/// Bench tab state and behaviour. Lives in a partial so we don't perturb
/// the main 3000-line view model. Owns the form fields, run lifecycle,
/// scenario row collection, log buffer and aggregate summary.
/// </summary>
public sealed partial class MainWindowViewModel
{
    private CancellationTokenSource? _benchCts;

    public ObservableCollection<BenchScenarioResult> BenchResults { get; } = new();

    private string _benchLabel = "default";
    public string BenchLabel
    {
        get => _benchLabel;
        set
        {
            if (_benchLabel == value) return;
            _benchLabel = value;
            OnPropertyChanged();
        }
    }

    public enum BenchSourceMode { Synthetic, FromFile }

    private BenchSourceMode _benchSource = BenchSourceMode.Synthetic;
    public BenchSourceMode BenchSource
    {
        get => _benchSource;
        set
        {
            if (_benchSource == value) return;
            _benchSource = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(IsBenchSourceSynthetic));
            OnPropertyChanged(nameof(IsBenchSourceFile));
            OnPropertyChanged(nameof(BenchSourceSummary));
            OnPropertyChanged(nameof(CanRunBench));
        }
    }

    public bool IsBenchSourceSynthetic
    {
        get => _benchSource == BenchSourceMode.Synthetic;
        set
        {
            if (value) BenchSource = BenchSourceMode.Synthetic;
        }
    }

    public bool IsBenchSourceFile
    {
        get => _benchSource == BenchSourceMode.FromFile;
        set
        {
            if (value) BenchSource = BenchSourceMode.FromFile;
        }
    }

    private string? _benchFilePath;
    public string? BenchFilePath
    {
        get => _benchFilePath;
        set
        {
            if (_benchFilePath == value) return;
            _benchFilePath = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(BenchSourceSummary));
            OnPropertyChanged(nameof(CanRunBench));
        }
    }

    private string _benchTruth = string.Empty;
    public string BenchTruth
    {
        get => _benchTruth;
        set
        {
            if (_benchTruth == value) return;
            _benchTruth = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(CanRunBench));
        }
    }

    private double _benchOnsetMs;
    public double BenchOnsetMs
    {
        get => _benchOnsetMs;
        set
        {
            if (Math.Abs(_benchOnsetMs - value) < 0.001) return;
            _benchOnsetMs = value;
            OnPropertyChanged();
        }
    }

    private double _benchStableN = 5;
    public double BenchStableN
    {
        get => _benchStableN;
        set
        {
            if (Math.Abs(_benchStableN - value) < 0.001) return;
            _benchStableN = value;
            OnPropertyChanged();
        }
    }

    private double _benchPurity = 3.0;
    public double BenchPurity
    {
        get => _benchPurity;
        set
        {
            if (Math.Abs(_benchPurity - value) < 0.001) return;
            _benchPurity = value;
            OnPropertyChanged();
        }
    }

    private double _benchWideBins;
    public double BenchWideBins
    {
        get => _benchWideBins;
        set
        {
            if (Math.Abs(_benchWideBins - value) < 0.001) return;
            _benchWideBins = value;
            OnPropertyChanged();
        }
    }

    private bool _benchAutoThreshold = true;
    public bool BenchAutoThreshold
    {
        get => _benchAutoThreshold;
        set
        {
            if (_benchAutoThreshold == value) return;
            _benchAutoThreshold = value;
            OnPropertyChanged();
        }
    }

    private double _benchForcePitchHz;
    public double BenchForcePitchHz
    {
        get => _benchForcePitchHz;
        set
        {
            if (Math.Abs(_benchForcePitchHz - value) < 0.001) return;
            _benchForcePitchHz = value;
            OnPropertyChanged();
        }
    }

    private bool _isBenchRunning;
    public bool IsBenchRunning
    {
        get => _isBenchRunning;
        private set
        {
            if (_isBenchRunning == value) return;
            _isBenchRunning = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(CanRunBench));
            OnPropertyChanged(nameof(CanCancelBench));
            OnPropertyChanged(nameof(BenchRunButtonLabel));
        }
    }

    public string BenchRunButtonLabel => IsBenchRunning ? "CANCEL" : "RUN BENCH";
    public bool CanCancelBench => IsBenchRunning;

    public bool CanRunBench
    {
        get
        {
            if (IsBenchRunning) return true; // button doubles as cancel
            if (_benchSource == BenchSourceMode.FromFile)
            {
                return !string.IsNullOrWhiteSpace(_benchFilePath)
                    && File.Exists(_benchFilePath)
                    && !string.IsNullOrWhiteSpace(_benchTruth);
            }
            return true;
        }
    }

    public string BenchSourceSummary => _benchSource switch
    {
        BenchSourceMode.Synthetic =>
            "Synthetic suite: silence/noise/voice lead-ins + 30 s clean-CW lock-stability stress.",
        BenchSourceMode.FromFile =>
            string.IsNullOrWhiteSpace(_benchFilePath)
                ? "Pick an audio file and enter the expected uppercase truth."
                : $"File: {Path.GetFileName(_benchFilePath)}",
        _ => string.Empty,
    };

    private string _benchStatus = "Idle.";
    public string BenchStatus
    {
        get => _benchStatus;
        private set
        {
            if (_benchStatus == value) return;
            _benchStatus = value;
            OnPropertyChanged();
        }
    }

    private string _benchSummary = string.Empty;
    public string BenchSummary
    {
        get => _benchSummary;
        private set
        {
            if (_benchSummary == value) return;
            _benchSummary = value;
            OnPropertyChanged();
        }
    }

    private readonly StringBuilder _benchLog = new();
    private string _benchLogText = string.Empty;
    public string BenchLogText
    {
        get => _benchLogText;
        private set
        {
            if (_benchLogText == value) return;
            _benchLogText = value;
            OnPropertyChanged();
        }
    }

    public void SetBenchFile(string path)
    {
        BenchFilePath = path;
        BenchSource = BenchSourceMode.FromFile;
    }

    public Task ToggleRunBenchAsync()
    {
        if (IsBenchRunning)
        {
            _benchCts?.Cancel();
            return Task.CompletedTask;
        }
        return RunBenchAsync();
    }

    private async Task RunBenchAsync()
    {
        if (IsBenchRunning) return;
        if (!CanRunBench)
        {
            BenchStatus = "Pick a file and enter truth before running file-mode bench.";
            return;
        }

        BenchResults.Clear();
        _benchLog.Clear();
        BenchLogText = string.Empty;
        BenchSummary = string.Empty;
        BenchStatus = "Running…";
        IsBenchRunning = true;

        var opts = new CwBenchRunner.Options
        {
            Label = string.IsNullOrWhiteSpace(_benchLabel) ? "default" : _benchLabel,
            StableN = (int)Math.Max(1, _benchStableN),
            ChunkMs = 100,
            SynthSampleRate = 16000,
            FromFile = _benchSource == BenchSourceMode.FromFile ? _benchFilePath : null,
            CwOnsetMs = (uint)Math.Max(0, _benchOnsetMs),
            Truth = _benchSource == BenchSourceMode.FromFile ? _benchTruth : null,
            Purity = (float)_benchPurity,
            WideBins = (int)Math.Max(0, _benchWideBins),
            DisableAutoThreshold = !_benchAutoThreshold,
            ForcePitchHz = _benchForcePitchHz > 0 ? (float)_benchForcePitchHz : (float?)null,
        };

        _benchCts?.Dispose();
        _benchCts = new CancellationTokenSource();
        var ct = _benchCts.Token;

        try
        {
            var result = await CwBenchRunner.RunAsync(
                opts,
                row => Dispatcher.UIThread.Post(() =>
                {
                    BenchResults.Add(row);
                    BenchStatus = $"{BenchResults.Count} scenario(s) complete…";
                }),
                line => Dispatcher.UIThread.Post(() =>
                {
                    _benchLog.AppendLine(line);
                    BenchLogText = _benchLog.ToString();
                }),
                ct).ConfigureAwait(true);

            BenchSummary = ComputeBenchSummary(result.Scenarios);
            BenchStatus = result.ExitCode == 0
                ? $"Done. {result.Scenarios.Count} scenario(s)."
                : $"cw-decoder exited with code {result.ExitCode}.";
        }
        catch (OperationCanceledException)
        {
            BenchStatus = "Cancelled.";
        }
        catch (Exception ex)
        {
            BenchStatus = $"Error: {ex.Message}";
        }
        finally
        {
            IsBenchRunning = false;
        }
    }

    private static string ComputeBenchSummary(System.Collections.Generic.IReadOnlyList<BenchScenarioResult> rows)
    {
        if (rows.Count == 0) return "No scenarios completed.";

        int hits = 0;
        long latencySum = 0;
        long worstLatency = 0;
        int totalDrops = 0;
        int totalRelocks = 0;
        double uptimeSum = 0;
        double worstUptime = 1.0;
        bool anyUptime = false;
        int totalGhosts = 0;

        foreach (var r in rows)
        {
            totalDrops += r.NPitchLostAfterLock;
            totalRelocks += r.NRelockCycles;
            totalGhosts += r.FalseCharsBeforeStable;
            if (r.AcquisitionLatencyMs is long lat)
            {
                hits++;
                latencySum += lat;
                if (lat > worstLatency) worstLatency = lat;
            }
            if (r.LockUptimeRatio is float u)
            {
                uptimeSum += u;
                if (u < worstUptime) worstUptime = u;
                anyUptime = true;
            }
        }

        var meanLat = hits > 0
            ? (latencySum / (double)hits / 1000.0).ToString("0.00", CultureInfo.InvariantCulture)
            : "—";
        var worstLatStr = hits > 0
            ? (worstLatency / 1000.0).ToString("0.00", CultureInfo.InvariantCulture)
            : "—";
        var meanUptime = anyUptime
            ? ((uptimeSum / rows.Count) * 100.0).ToString("0.0", CultureInfo.InvariantCulture)
            : "—";
        var worstUptimeStr = anyUptime
            ? (worstUptime * 100.0).ToString("0.0", CultureInfo.InvariantCulture)
            : "—";

        return $"hits {hits}/{rows.Count}  ·  mean lat {meanLat}s  ·  worst lat {worstLatStr}s  ·  " +
               $"mean uptime {meanUptime}%  ·  worst uptime {worstUptimeStr}%  ·  " +
               $"drops {totalDrops}  ·  relocks {totalRelocks}  ·  ghosts {totalGhosts}";
    }
}
