using System.ComponentModel;
using System.Diagnostics;
using System.Text.Json;

namespace QsoRipper.Gui.Services;

/// <summary>
/// A capture device offered to the user for the radio monitor. Internally,
/// "loopback" devices are system OUTPUT devices that we capture from via
/// WASAPI loopback so users can audition CW playing through their speakers
/// without setting up a virtual audio cable.
/// </summary>
/// <param name="Name">
/// Underlying device name passed to cw-decoder via <c>--device</c>. Must
/// stay verbatim — never suffix it for display purposes.
/// </param>
/// <param name="IsLoopback">
/// True when <see cref="Name"/> refers to a system OUTPUT device that the
/// decoder should capture in WASAPI loopback mode.
/// </param>
/// <param name="IsUnavailable">
/// True when this entry was synthesized to represent a persisted device
/// that is no longer enumerable (e.g. radio unplugged). The dropdown
/// shows a "(not currently available)" suffix but <see cref="Name"/>
/// stays clean so the user can still see what was previously chosen.
/// </param>
internal sealed record RadioMonitorDevice(string Name, bool IsLoopback, bool IsUnavailable = false)
{
    public string DisplayName
    {
        get
        {
            if (IsUnavailable)
            {
                return $"{Name}  (not currently available)";
            }

            return IsLoopback ? $"{Name}  (system output / loopback)" : Name;
        }
    }
}

/// <summary>
/// Helper that enumerates capture devices by invoking
/// <c>cw-decoder devices --json</c>. Returns a flat list combining real
/// inputs (mics, USB sound cards) and loopback-eligible outputs (speakers).
/// </summary>
internal static class RadioMonitorDeviceCatalog
{
    /// <summary>
    /// First entry in the dropdown. Selecting this clears the device override
    /// so the decoder uses the host default input.
    /// </summary>
    internal static readonly RadioMonitorDevice SystemDefault =
        new("(System default input)", IsLoopback: false);

    internal static async Task<IReadOnlyList<RadioMonitorDevice>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var binary = CwDecoderProcessSampleSource.LocateBinary();
        if (binary is null)
        {
            return new List<RadioMonitorDevice> { SystemDefault };
        }

        var psi = new ProcessStartInfo(binary, "devices --json")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        try
        {
            using var process = Process.Start(psi);
            if (process is null)
            {
                return new List<RadioMonitorDevice> { SystemDefault };
            }

            var stdoutTask = process.StandardOutput.ReadToEndAsync(cancellationToken);
            await process.WaitForExitAsync(cancellationToken).ConfigureAwait(false);
            var stdout = await stdoutTask.ConfigureAwait(false);

            return Parse(stdout);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Win32Exception)
        {
            return new List<RadioMonitorDevice> { SystemDefault };
        }
        catch (InvalidOperationException)
        {
            return new List<RadioMonitorDevice> { SystemDefault };
        }
        catch (IOException)
        {
            return new List<RadioMonitorDevice> { SystemDefault };
        }
    }

    /// <summary>
    /// Parses the JSON payload produced by <c>cw-decoder devices --json</c>.
    /// Public for unit tests.
    /// </summary>
    internal static IReadOnlyList<RadioMonitorDevice> Parse(string json)
    {
        var result = new List<RadioMonitorDevice> { SystemDefault };
        if (string.IsNullOrWhiteSpace(json))
        {
            return result;
        }

        try
        {
            using var doc = JsonDocument.Parse(json);
            if (doc.RootElement.TryGetProperty("inputs", out var inputs)
                && inputs.ValueKind == JsonValueKind.Array)
            {
                foreach (var item in inputs.EnumerateArray())
                {
                    var name = item.GetString();
                    if (!string.IsNullOrWhiteSpace(name))
                    {
                        result.Add(new RadioMonitorDevice(name, IsLoopback: false));
                    }
                }
            }

            if (doc.RootElement.TryGetProperty("loopback", out var loopback)
                && loopback.ValueKind == JsonValueKind.Array)
            {
                foreach (var item in loopback.EnumerateArray())
                {
                    var name = item.GetString();
                    if (!string.IsNullOrWhiteSpace(name))
                    {
                        result.Add(new RadioMonitorDevice(name, IsLoopback: true));
                    }
                }
            }
        }
        catch (JsonException)
        {
            // Older cw-decoder builds without --json support print human-readable
            // text; fall back to "system default only" rather than crashing.
        }

        return result;
    }
}
