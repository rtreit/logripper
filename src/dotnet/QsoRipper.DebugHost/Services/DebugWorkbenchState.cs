using System.Linq;
using System.Net.Sockets;
using Grpc.Core;
using Grpc.Net.Client;
using Microsoft.Extensions.Options;
using QsoRipper.DebugHost.Models;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class DebugWorkbenchState
{
    private readonly DebugWorkbenchOptions _options;

    public DebugWorkbenchState(IOptions<DebugWorkbenchOptions> options)
    {
        ArgumentNullException.ThrowIfNull(options);

        _options = options.Value;
        var configuredProfile = string.IsNullOrWhiteSpace(_options.DefaultEngineProfile)
            ? _options.DefaultEngineImplementation
            : _options.DefaultEngineProfile;
        EngineProfile = EngineCatalog.ResolveProfile(configuredProfile);
        EngineEndpoint = EngineCatalog.ResolveEndpoint(EngineProfile, _options.DefaultEngineEndpoint);
        EngineStorageBackend = NormalizeStorageBackend(_options.DefaultEngineStorageBackend);
        EnginePersistenceLocation = NormalizePersistenceLocation(_options.DefaultEnginePersistenceLocation);
    }

    public EngineTargetProfile EngineProfile { get; private set; }

    public string EngineEndpoint { get; private set; }

    public string EngineStorageBackend { get; private set; }

    public string EnginePersistenceLocation { get; private set; }

    public EngineInfo? ReportedEngineInfo { get; private set; }

    public TransportProbeResult? LastProbe { get; private set; }

    public RuntimeConfigSnapshot? RuntimeConfigSnapshot { get; private set; }

    public string? RuntimeConfigErrorMessage { get; private set; }

    public SetupStatus? SetupStatus { get; private set; }

    public string? SetupErrorMessage { get; private set; }

    public ListStationProfilesResponse? StationProfileCatalog { get; private set; }

    public ActiveStationContext? ActiveStationContext { get; private set; }

    public string? StationProfileErrorMessage { get; private set; }

    public event Action? StateChanged;

    public void UpdateEngineEndpoint(string endpoint)
    {
        ArgumentNullException.ThrowIfNull(endpoint);

        EngineEndpoint = endpoint.Trim();
        LastProbe = null;
        ReportedEngineInfo = null;
        NotifyStateChanged();
    }

    public void UpdateEngineProfile(string profileId)
    {
        UpdateEngineProfile(EngineCatalog.GetProfile(profileId));
    }

    public void UpdateEngineProfile(EngineTargetProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        var previousProfile = EngineProfile;
        EngineProfile = profile;
        if (string.IsNullOrWhiteSpace(EngineEndpoint)
            || EngineCatalog.IsDefaultEndpoint(EngineEndpoint, previousProfile))
        {
            EngineEndpoint = profile.DefaultEndpoint;
        }

        LastProbe = null;
        ReportedEngineInfo = null;
        NotifyStateChanged();
    }

    public void UpdateStorageOptions(string backend, string persistenceLocation)
    {
        ArgumentNullException.ThrowIfNull(backend);
        ArgumentNullException.ThrowIfNull(persistenceLocation);

        EngineStorageBackend = NormalizeStorageBackend(backend);
        EnginePersistenceLocation = NormalizePersistenceLocation(persistenceLocation);
        NotifyStateChanged();
    }

    public void UpdateRuntimeConfig(RuntimeConfigSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        RuntimeConfigSnapshot = snapshot;
        RuntimeConfigErrorMessage = null;
        EngineStorageBackend = NormalizeStorageBackend(snapshot.ActiveStorageBackend);

        if (!string.IsNullOrWhiteSpace(snapshot.PersistenceLocation))
        {
            EnginePersistenceLocation = NormalizePersistenceLocation(snapshot.PersistenceLocation);
        }

        NotifyStateChanged();
    }

    public void UpdateRuntimeConfigError(string? message)
    {
        RuntimeConfigErrorMessage = string.IsNullOrWhiteSpace(message)
            ? null
            : message.Trim();
        NotifyStateChanged();
    }

    public void UpdateSetupStatus(SetupStatus status)
    {
        ArgumentNullException.ThrowIfNull(status);

        SetupStatus = status;
        SetupErrorMessage = null;

#pragma warning disable CS0612 // Type or member is obsolete
        var persistedLogFilePath = string.IsNullOrWhiteSpace(status.LogFilePath)
            ? status.SqlitePath
            : status.LogFilePath;
#pragma warning restore CS0612
        if (!string.IsNullOrWhiteSpace(persistedLogFilePath))
        {
            EnginePersistenceLocation = NormalizePersistenceLocation(persistedLogFilePath);
        }

        NotifyStateChanged();
    }

    public void UpdateSetupError(string? message)
    {
        SetupErrorMessage = string.IsNullOrWhiteSpace(message)
            ? null
            : message.Trim();
        NotifyStateChanged();
    }

    public void ClearSetupError()
    {
        SetupErrorMessage = null;
        NotifyStateChanged();
    }

    public void UpdateStationProfiles(
        ListStationProfilesResponse catalog,
        ActiveStationContext context)
    {
        ArgumentNullException.ThrowIfNull(catalog);
        ArgumentNullException.ThrowIfNull(context);

        StationProfileCatalog = catalog;
        ActiveStationContext = context;
        StationProfileErrorMessage = null;
        NotifyStateChanged();
    }

    public void UpdateStationProfileError(string? message)
    {
        StationProfileErrorMessage = string.IsNullOrWhiteSpace(message)
            ? null
            : message.Trim();
        NotifyStateChanged();
    }

    public void ClearStationProfileError()
    {
        StationProfileErrorMessage = null;
        NotifyStateChanged();
    }

    public string GetStorageBackendDisplayName()
    {
        if (!string.IsNullOrWhiteSpace(RuntimeConfigSnapshot?.PersistenceSummary))
        {
            return RuntimeConfigSnapshot.PersistenceSummary;
        }

        return FormatStorageBackendDisplayName(EngineStorageBackend);
    }

    public string? GetPersistenceLocation()
    {
        if (!string.IsNullOrWhiteSpace(RuntimeConfigSnapshot?.PersistenceLocation))
        {
            return RuntimeConfigSnapshot.PersistenceLocation;
        }

        return string.IsNullOrWhiteSpace(EnginePersistenceLocation)
            ? null
            : EnginePersistenceLocation;
    }

    public string GetSelectedEngineDisplayName()
    {
        return EngineProfile.DisplayName;
    }

    public string GetSelectedEngineId()
    {
        return EngineProfile.EngineId;
    }

    public string BuildEngineLaunchCommand()
    {
        var recipe = EngineProfile.LocalLaunchRecipe;
        if (recipe is null)
        {
            return "No local launch recipe is registered for the selected engine profile.";
        }

        return BuildCommandPreview(recipe.Command, BuildRecipeTokens(recipe));
    }

    public IReadOnlyDictionary<string, string> GetEngineEnvironmentOverrides()
    {
        if (RuntimeConfigSnapshot is not null)
        {
            return RuntimeConfigSnapshot.Values
                .Where(value => value.HasValue)
                .ToDictionary(
                    value => value.Key,
                    value => value.DisplayValue,
                    StringComparer.OrdinalIgnoreCase);
        }

        var recipe = EngineProfile.LocalLaunchRecipe;
        if (recipe is null)
        {
            return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        }

        var environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var tokens = BuildRecipeTokens(recipe);
        foreach (var template in recipe.EnvironmentTemplates)
        {
            var value = ExpandTemplate(template.Value, tokens);
            if (!string.IsNullOrWhiteSpace(value))
            {
                environment[template.Key] = value;
            }
        }

        return environment;
    }

    public async Task<TransportProbeResult> ProbeAsync(CancellationToken cancellationToken = default)
    {
        TransportProbeResult CompleteProbe(
            TransportProbeResult probe,
            EngineInfo? engineInfo = null,
            bool preserveReportedEngineInfo = false)
        {
            if (!preserveReportedEngineInfo)
            {
                ReportedEngineInfo = engineInfo?.Clone();
            }

            LastProbe = probe;
            NotifyStateChanged();
            return probe;
        }

        var attemptedAt = DateTimeOffset.UtcNow;
        if (!Uri.TryCreate(EngineEndpoint, UriKind.Absolute, out var endpointUri))
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.InvalidEndpoint,
                "Endpoint is not a valid absolute URI.",
                "Correct the endpoint format to an absolute http:// or https:// URI.",
                attemptedAt,
                EngineEndpoint));
        }

        var port = endpointUri.IsDefaultPort
            ? endpointUri.Scheme.Equals("https", StringComparison.OrdinalIgnoreCase) ? 443 : 80
            : endpointUri.Port;

        try
        {
            using var tcpTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            tcpTimeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.ProbeTimeoutSeconds));

            using var tcpClient = new TcpClient();
            await tcpClient.ConnectAsync(endpointUri.Host, port, tcpTimeoutSource.Token);
        }
        catch (OperationCanceledException ex)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection timed out: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint));
        }
        catch (SocketException ex)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection failed: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint));
        }
        catch (IOException ex)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection failed: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint));
        }

        // TCP succeeded; now probe gRPC capability via EngineService and GetSyncStatus.
        var grpcChannel = GrpcChannel.ForAddress(endpointUri);
        try
        {
            using var grpcTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            grpcTimeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.ProbeTimeoutSeconds));

            var callOptions = new CallOptions(cancellationToken: grpcTimeoutSource.Token);
            EngineInfo? engineInfo = null;
            var engineClient = new EngineService.EngineServiceClient(grpcChannel);
            try
            {
                engineInfo = (await engineClient.GetEngineInfoAsync(new GetEngineInfoRequest(), callOptions)).Engine;
            }
            catch (RpcException ex) when (ex.StatusCode == StatusCode.Unimplemented)
            {
            }

            var client = new LogbookService.LogbookServiceClient(grpcChannel);
            await client.GetSyncStatusAsync(new GetSyncStatusRequest(), callOptions);
            var engineLabel = engineInfo is null
                ? "Engine"
                : $"{engineInfo.DisplayName} ({engineInfo.EngineId})";

            return CompleteProbe(
                new TransportProbeResult(
                true,
                EngineProbeStage.MethodSucceeded,
                $"{engineLabel} is reachable and GetSyncStatus succeeded. Baseline service is live.",
                "The engine is operational. Proceed with logbook and lookup workflows.",
                attemptedAt,
                EngineEndpoint),
                engineInfo);
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.Unimplemented)
        {
            var engineLabel = ReportedEngineInfo is null
                ? "The selected engine"
                : $"{ReportedEngineInfo.DisplayName} ({ReportedEngineInfo.EngineId})";
            return CompleteProbe(
                new TransportProbeResult(
                false,
                EngineProbeStage.MethodUnimplemented,
                $"{engineLabel} is reachable, but GetSyncStatus is not implemented.",
                "Implement GetSyncStatus in the selected engine host to enable baseline service health checks.",
                attemptedAt,
                EngineEndpoint),
                preserveReportedEngineInfo: true);
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.Unavailable)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                $"TCP is reachable but gRPC is unavailable: {ex.Status.Detail}",
                "Start the selected gRPC engine host. The port is open but no gRPC service is responding.",
                attemptedAt,
                EngineEndpoint));
        }
        catch (RpcException ex)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                $"gRPC call failed ({ex.StatusCode}): {ex.Status.Detail}",
                "Investigate the gRPC service error. Check engine logs for details.",
                attemptedAt,
                EngineEndpoint));
        }
        catch (OperationCanceledException)
        {
            return CompleteProbe(new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                "gRPC call timed out. TCP is reachable but the gRPC service did not respond in time.",
                "Start the selected gRPC engine host. The port is open but no gRPC service is responding.",
                attemptedAt,
                EngineEndpoint));
        }
        finally
        {
            await grpcChannel.ShutdownAsync();
            grpcChannel.Dispose();
        }
    }

    private void NotifyStateChanged()
    {
        StateChanged?.Invoke();
    }

    private static string NormalizeStorageBackend(string? configuredBackend)
    {
        return string.IsNullOrWhiteSpace(configuredBackend)
            ? "memory"
            : configuredBackend.Trim();
    }

    private static string NormalizePersistenceLocation(string persistenceLocation)
    {
        return string.IsNullOrWhiteSpace(persistenceLocation)
            ? PersistenceSetup.DefaultRelativePersistencePath
            : persistenceLocation.Trim();
    }

    private static string FormatStorageBackendDisplayName(string storageBackend)
    {
        if (string.IsNullOrWhiteSpace(storageBackend))
        {
            return "Engine-managed";
        }

        return string.Join(
            " ",
            storageBackend
                .Split(['-', '_', ' '], StringSplitOptions.RemoveEmptyEntries)
                .Select(static segment => char.ToUpperInvariant(segment[0]) + segment[1..]));
    }

    private string GetListenAddress()
    {
        if (Uri.TryCreate(EngineEndpoint, UriKind.Absolute, out var endpointUri))
        {
            return $"{endpointUri.Host}:{endpointUri.Port}";
        }

        var fallbackUri = new Uri(EngineProfile.DefaultEndpoint, UriKind.Absolute);
        return $"{fallbackUri.Host}:{fallbackUri.Port}";
    }

    private Dictionary<string, string> BuildRecipeTokens(EngineLaunchRecipe recipe)
    {
        var configPath = string.IsNullOrWhiteSpace(recipe.DefaultConfigPath)
            ? string.Empty
            : recipe.DefaultConfigPath;
        var persistenceLocation = EnginePersistenceLocation;
        var enginePersistenceLocation = string.Equals(EngineStorageBackend, "memory", StringComparison.OrdinalIgnoreCase)
            ? string.Empty
            : EnginePersistenceLocation;

        return new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["configPath"] = configPath,
            ["enginePersistenceLocation"] = enginePersistenceLocation,
            ["listenAddress"] = GetListenAddress(),
            ["persistenceLocation"] = persistenceLocation,
            ["storageBackend"] = EngineStorageBackend,
        };
    }

    private static string BuildCommandPreview(
        EngineCommand command,
        IReadOnlyDictionary<string, string> tokens)
    {
        var parts = new List<string>
        {
            QuoteIfNeeded(ExpandTemplate(command.FilePath, tokens))
        };

        foreach (var argument in command.Arguments)
        {
            var expanded = ExpandTemplate(argument, tokens);
            if (!string.IsNullOrWhiteSpace(expanded))
            {
                parts.Add(QuoteIfNeeded(expanded));
            }
        }

        return string.Join(" ", parts);
    }

    private static string ExpandTemplate(
        string template,
        IReadOnlyDictionary<string, string> tokens)
    {
        var expanded = template;
        foreach (var token in tokens)
        {
            expanded = expanded.Replace($"{{{token.Key}}}", token.Value, StringComparison.Ordinal);
        }

        return expanded;
    }

    private static string QuoteIfNeeded(string value)
    {
        if (string.IsNullOrWhiteSpace(value) || !value.Contains(' ', StringComparison.Ordinal))
        {
            return value;
        }

        return $"\"{value}\"";
    }
}
