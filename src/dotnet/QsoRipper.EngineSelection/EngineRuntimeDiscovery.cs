using System.Diagnostics;
using System.Globalization;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace QsoRipper.EngineSelection;

public static partial class EngineRuntimeDiscovery
{
    private const string LegacyStateFileName = "qsoripper-engine.json";
    private const string ProfileStateGlob = "qsoripper-engine-*.json";
    private const string ModernStateGlob = "qsoripper-*.state.json";
    private static readonly JsonSerializerOptions JsonOptions = new() { PropertyNameCaseInsensitive = true };

    public static IReadOnlyList<LocalEngineRuntimeEntry> DiscoverLocalEngines(
        EngineRuntimeDiscoveryOptions? options = null)
    {
        options ??= new EngineRuntimeDiscoveryOptions();
        if (string.IsNullOrWhiteSpace(options.RuntimeDirectory))
        {
            return [];
        }

        if (!Directory.Exists(options.RuntimeDirectory))
        {
            return [];
        }

        var entries = new List<LocalEngineRuntimeEntry>();
        foreach (var statePath in EnumerateStatePaths(options.RuntimeDirectory))
        {
            if (!TryReadState(statePath, out var state))
            {
                continue;
            }

            if (!TryResolveProfile(state, out var profile))
            {
                continue;
            }

            var listenAddress = state.ListenAddress;
            var endpoint = TryBuildEndpoint(listenAddress, out var parsedEndpoint)
                ? parsedEndpoint
                : profile.DefaultEndpoint;
            if (string.IsNullOrWhiteSpace(listenAddress))
            {
                listenAddress = BuildListenAddress(profile.DefaultEndpoint);
            }

            var processId = state.Pid > 0 ? state.Pid : 0;
            var isProcessAlive = processId > 0 && IsProcessAlive(processId);
            var isTransportReachable = options.ValidateTcpReachability
                ? TestTcpEndpoint(endpoint, options.TcpProbeTimeout)
                : isProcessAlive;

            entries.Add(new LocalEngineRuntimeEntry(
                profile,
                endpoint,
                listenAddress ?? string.Empty,
                processId,
                isProcessAlive,
                isTransportReachable,
                TryParseTimestamp(state.StartedAtUtc),
                statePath));
        }

        return entries
            .GroupBy(entry => entry.Profile.ProfileId, StringComparer.OrdinalIgnoreCase)
            .Select(group => group
                .OrderByDescending(static entry => entry.StartedAtUtc ?? DateTimeOffset.MinValue)
                .ThenByDescending(static entry => entry.StatePath, StringComparer.OrdinalIgnoreCase)
                .First())
            .OrderBy(static entry => entry.Profile.DisplayName, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static IEnumerable<string> EnumerateStatePaths(string runtimeDirectory)
    {
        var candidatePaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            Path.Combine(runtimeDirectory, LegacyStateFileName)
        };

        foreach (var filePath in Directory.EnumerateFiles(runtimeDirectory, ProfileStateGlob))
        {
            candidatePaths.Add(filePath);
        }

        foreach (var filePath in Directory.EnumerateFiles(runtimeDirectory, ModernStateGlob))
        {
            candidatePaths.Add(filePath);
        }

        return candidatePaths
            .Where(File.Exists)
            .OrderBy(static path => path, StringComparer.OrdinalIgnoreCase);
    }

    private static bool TryReadState(string path, out EngineRuntimeStateDocument state)
    {
        state = new EngineRuntimeStateDocument();
        try
        {
            var json = File.ReadAllText(path);
            var parsed = JsonSerializer.Deserialize<EngineRuntimeStateDocument>(json, JsonOptions);
            if (parsed is null)
            {
                return false;
            }

            state = parsed;
            return true;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
        catch (JsonException)
        {
            return false;
        }
    }

    private static bool TryResolveProfile(
        EngineRuntimeStateDocument state,
        out EngineTargetProfile profile)
    {
        if (EngineCatalog.TryResolveProfile(state.Engine, out var resolvedByProfile))
        {
            profile = resolvedByProfile;
            return true;
        }

        if (EngineCatalog.TryResolveProfile(state.EngineId, out var resolvedById))
        {
            profile = resolvedById;
            return true;
        }

        profile = null!;
        return false;
    }

    private static bool TryBuildEndpoint(string? listenAddress, out string endpoint)
    {
        endpoint = string.Empty;
        if (string.IsNullOrWhiteSpace(listenAddress))
        {
            return false;
        }

        if (!TryParseListenAddress(listenAddress, out var host, out var port))
        {
            return false;
        }

        endpoint = FormatHttpEndpoint(NormalizeListenHost(host), port);
        return true;
    }

    private static string BuildListenAddress(string endpoint)
    {
        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out var uri))
        {
            return string.Empty;
        }

        return $"{uri.Host}:{uri.Port}";
    }

    private static bool TryParseListenAddress(
        string listenAddress,
        out string host,
        out int port)
    {
        var candidate = listenAddress.Trim();
        var ipv6Match = Ipv6ListenAddressRegex().Match(candidate);
        if (ipv6Match.Success)
        {
            host = ipv6Match.Groups["host"].Value;
            return int.TryParse(
                ipv6Match.Groups["port"].Value,
                NumberStyles.None,
                CultureInfo.InvariantCulture,
                out port);
        }

        var separator = candidate.LastIndexOf(':');
        if (separator <= 0 || separator >= candidate.Length - 1)
        {
            host = string.Empty;
            port = 0;
            return false;
        }

        host = candidate[..separator];
        return int.TryParse(
            candidate[(separator + 1)..],
            NumberStyles.None,
            CultureInfo.InvariantCulture,
            out port);
    }

    private static string NormalizeListenHost(string host)
    {
        return host switch
        {
            "0.0.0.0" => "127.0.0.1",
            "::" => "127.0.0.1",
            "[::]" => "127.0.0.1",
            "*" => "127.0.0.1",
            "+" => "127.0.0.1",
            _ => host
        };
    }

    private static string FormatHttpEndpoint(string host, int port)
    {
        if (host.Contains(':', StringComparison.Ordinal) && !host.StartsWith('['))
        {
            return $"http://[{host}]:{port}";
        }

        return $"http://{host}:{port}";
    }

    private static bool IsProcessAlive(int processId)
    {
        try
        {
            using var process = Process.GetProcessById(processId);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
        catch (InvalidOperationException)
        {
            return false;
        }
    }

    private static bool TestTcpEndpoint(string endpoint, TimeSpan timeout)
    {
        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out var uri))
        {
            return false;
        }

        var timeoutValue = timeout <= TimeSpan.Zero
            ? TimeSpan.FromMilliseconds(500)
            : timeout;
        var port = uri.IsDefaultPort
            ? uri.Scheme.Equals("https", StringComparison.OrdinalIgnoreCase) ? 443 : 80
            : uri.Port;

        using var client = new TcpClient();
        try
        {
            var connectTask = client.ConnectAsync(uri.Host, port);
            if (!connectTask.Wait(timeoutValue))
            {
                return false;
            }

            return client.Connected;
        }
        catch (SocketException)
        {
            return false;
        }
        catch (IOException)
        {
            return false;
        }
    }

    private static DateTimeOffset? TryParseTimestamp(string? startedAtUtc)
    {
        if (string.IsNullOrWhiteSpace(startedAtUtc))
        {
            return null;
        }

        return DateTimeOffset.TryParse(
            startedAtUtc,
            CultureInfo.InvariantCulture,
            DateTimeStyles.RoundtripKind,
            out var parsed)
            ? parsed
            : null;
    }

    [GeneratedRegex("^\\[(?<host>.+)\\]:(?<port>\\d+)$")]
    private static partial Regex Ipv6ListenAddressRegex();

    private sealed class EngineRuntimeStateDocument
    {
        public string? DisplayName { get; set; }

        public string? Engine { get; set; }

        public string? EngineId { get; set; }

        public string? ListenAddress { get; set; }

        public int Pid { get; set; }

        public string? StartedAtUtc { get; set; }
    }
}
