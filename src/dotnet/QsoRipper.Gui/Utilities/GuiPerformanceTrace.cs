using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Threading;

namespace QsoRipper.Gui.Utilities;

internal static class GuiPerformanceTrace
{
    private static readonly object SyncRoot = new();
    private static readonly Stopwatch Stopwatch = Stopwatch.StartNew();
    private static readonly string? FilePath = ResolveFilePath();
    private static long _sequence;

    public static bool IsEnabled => FilePath is not null;

    public static void Write(string eventName, string? details = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(eventName);

        var filePath = FilePath;
        if (filePath is null)
        {
            return;
        }

        var line = string.Create(
            CultureInfo.InvariantCulture,
            $"{Stopwatch.Elapsed.TotalMilliseconds:F1} ms | #{Interlocked.Increment(ref _sequence)} | {eventName}");

        if (!string.IsNullOrWhiteSpace(details))
        {
            line = $"{line} | {details}";
        }

        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        lock (SyncRoot)
        {
            File.AppendAllText(filePath, line + Environment.NewLine);
        }
    }

    private static string? ResolveFilePath()
    {
        var rawValue = Environment.GetEnvironmentVariable("QSORIPPER_GUI_PERF_TRACE");
        if (string.IsNullOrWhiteSpace(rawValue))
        {
            return null;
        }

        var trimmedValue = rawValue.Trim();
        if (string.Equals(trimmedValue, "1", StringComparison.Ordinal))
        {
            return Path.Combine(
                Path.GetTempPath(),
                $"qsoripper-gui-perf-{Environment.ProcessId}.log");
        }

        return Path.GetFullPath(Environment.ExpandEnvironmentVariables(trimmedValue));
    }
}
