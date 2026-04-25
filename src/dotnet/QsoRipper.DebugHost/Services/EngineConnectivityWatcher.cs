using System.Net.Sockets;
using Grpc.Core;
using Grpc.Net.Client;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using QsoRipper.DebugHost.Models;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class EngineConnectivityWatcher : BackgroundService
{
    internal delegate Task<EngineProbeOutcome> ProbeDelegate(string endpoint, CancellationToken cancellationToken);

    private static readonly TimeSpan DefaultInterval = TimeSpan.FromSeconds(5);

    private readonly DebugWorkbenchOptions _options;
    private readonly ILogger<EngineConnectivityWatcher>? _logger;
    private readonly ProbeDelegate _probe;
    private readonly TimeSpan _interval;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private bool _hasProbed;

    public EngineConnectivityWatcher(IOptions<DebugWorkbenchOptions> options, ILogger<EngineConnectivityWatcher> logger)
        : this(options, logger, probe: null, interval: null)
    {
    }

    internal EngineConnectivityWatcher(
        IOptions<DebugWorkbenchOptions> options,
        ILogger<EngineConnectivityWatcher>? logger,
        ProbeDelegate? probe,
        TimeSpan? interval)
    {
        ArgumentNullException.ThrowIfNull(options);

        _options = options.Value;
        _logger = logger;
        _probe = probe ?? DefaultProbeAsync;
        _interval = interval ?? DefaultInterval;
        LastEndpoint = ResolveEndpoint();
    }

    public bool IsEngineUnreachable { get; private set; }

    public string? LastError { get; private set; }

    public string? LastEndpoint { get; private set; }

    public DateTimeOffset LastProbeUtc { get; private set; }

    public event EventHandler? StateChanged;

    public async Task ProbeNowAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var endpoint = ResolveEndpoint();
            LastEndpoint = endpoint;

            EngineProbeOutcome outcome;
            try
            {
                outcome = await _probe(endpoint, cancellationToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
#pragma warning disable CA1031
            catch (Exception ex)
            {
                outcome = new EngineProbeOutcome(false, ex.Message);
            }
#pragma warning restore CA1031

            LastProbeUtc = DateTimeOffset.UtcNow;
            var previouslyUnreachable = IsEngineUnreachable;
            var nowUnreachable = !outcome.IsReachable;
            LastError = outcome.IsReachable ? null : outcome.Error;
            IsEngineUnreachable = nowUnreachable;

            if (!_hasProbed || previouslyUnreachable != nowUnreachable)
            {
                _hasProbed = true;
                RaiseStateChanged();
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await ProbeNowAsync(stoppingToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
#pragma warning disable CA1031
            catch (Exception ex)
            {
                LogProbeFailure(_logger, ex);
            }
#pragma warning restore CA1031

            try
            {
                await Task.Delay(_interval, stoppingToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }

    public override void Dispose()
    {
        _gate.Dispose();
        base.Dispose();
    }

    private static readonly Action<ILogger, Exception?> LogProbeFailureCore =
        LoggerMessage.Define(LogLevel.Debug, new EventId(1, nameof(EngineConnectivityWatcher)), "Engine connectivity probe threw unexpectedly.");

    private static void LogProbeFailure(ILogger? logger, Exception ex)
    {
        if (logger is not null)
        {
            LogProbeFailureCore(logger, ex);
        }
    }

    private void RaiseStateChanged()
    {
        StateChanged?.Invoke(this, EventArgs.Empty);
    }

    private string ResolveEndpoint()
    {
        var configuredProfile = string.IsNullOrWhiteSpace(_options.DefaultEngineProfile)
            ? _options.DefaultEngineImplementation
            : _options.DefaultEngineProfile;
        var profile = EngineCatalog.ResolveProfile(configuredProfile);
        return EngineCatalog.ResolveEndpoint(profile, _options.DefaultEngineEndpoint);
    }

    private async Task<EngineProbeOutcome> DefaultProbeAsync(string endpoint, CancellationToken cancellationToken)
    {
        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out var endpointUri))
        {
            return new EngineProbeOutcome(false, "Endpoint is not a valid absolute URI.");
        }

        var port = endpointUri.IsDefaultPort
            ? endpointUri.Scheme.Equals("https", StringComparison.OrdinalIgnoreCase) ? 443 : 80
            : endpointUri.Port;

        var timeout = TimeSpan.FromSeconds(Math.Max(1, _options.ProbeTimeoutSeconds));

        try
        {
            using var tcpTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            tcpTimeoutSource.CancelAfter(timeout);

            using var tcpClient = new TcpClient();
            await tcpClient.ConnectAsync(endpointUri.Host, port, tcpTimeoutSource.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (OperationCanceledException)
        {
            return new EngineProbeOutcome(false, "TCP connection timed out.");
        }
        catch (SocketException ex)
        {
            return new EngineProbeOutcome(false, $"TCP connection failed: {ex.Message}");
        }
        catch (IOException ex)
        {
            return new EngineProbeOutcome(false, $"TCP connection failed: {ex.Message}");
        }

        var channel = GrpcChannel.ForAddress(endpointUri);
        try
        {
            using var grpcTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            grpcTimeoutSource.CancelAfter(timeout);

            var callOptions = new CallOptions(cancellationToken: grpcTimeoutSource.Token);
            var client = new LogbookService.LogbookServiceClient(channel);
            await client.GetSyncStatusAsync(new GetSyncStatusRequest(), callOptions);
            return new EngineProbeOutcome(true, null);
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.Unimplemented)
        {
            return new EngineProbeOutcome(true, null);
        }
        catch (RpcException ex)
        {
            return new EngineProbeOutcome(false, $"gRPC call failed ({ex.StatusCode}): {ex.Status.Detail}");
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (OperationCanceledException)
        {
            return new EngineProbeOutcome(false, "gRPC call timed out.");
        }
        finally
        {
            await channel.ShutdownAsync().ConfigureAwait(false);
            channel.Dispose();
        }
    }
}

internal readonly record struct EngineProbeOutcome(bool IsReachable, string? Error);
