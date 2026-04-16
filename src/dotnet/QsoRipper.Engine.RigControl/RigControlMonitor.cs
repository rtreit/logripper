using System.Diagnostics;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Caches the latest rig snapshot and refreshes it when the configured staleness
/// threshold elapses. Thread-safe via <see cref="Lock"/>.
/// </summary>
/// <remarks>
/// Follows the same pattern as the Rust <c>RigControlMonitor</c>:
/// <list type="bullet">
///   <item>On success: cache new snapshot with <see cref="RigConnectionStatus.Connected"/>.</item>
///   <item>On failure with cache: return stale snapshot with <see cref="RigConnectionStatus.Error"/> + error message.</item>
///   <item>On failure without cache: return snapshot with <see cref="RigConnectionStatus.Error"/>
///         (or <see cref="RigConnectionStatus.Disabled"/> when the provider is disabled).</item>
/// </list>
/// </remarks>
public sealed class RigControlMonitor
{
    /// <summary>Default stale threshold in milliseconds.</summary>
    public const int DefaultStaleThresholdMs = 500;

    private readonly IRigControlProvider _provider;
    private readonly TimeSpan _staleThreshold;
    private readonly Lock _gate = new();
    private RigSnapshot? _cached;
    private long _lastFetchTimestamp;

    public RigControlMonitor(IRigControlProvider provider, TimeSpan staleThreshold)
    {
        ArgumentNullException.ThrowIfNull(provider);
        _provider = provider;
        _staleThreshold = staleThreshold;
    }

    /// <summary>
    /// Return the current snapshot, refreshing if stale or missing.
    /// </summary>
    public RigSnapshot CurrentSnapshot()
    {
        lock (_gate)
        {
            if (_cached is not null && Stopwatch.GetElapsedTime(_lastFetchTimestamp) < _staleThreshold)
            {
                return _cached.Clone();
            }

            return RefreshSnapshotCore();
        }
    }

    /// <summary>
    /// Force a refresh and return the latest available snapshot.
    /// </summary>
    public RigSnapshot RefreshSnapshot()
    {
        lock (_gate)
        {
            return RefreshSnapshotCore();
        }
    }

    private RigSnapshot RefreshSnapshotCore()
    {
        try
        {
            var snapshot = _provider.GetSnapshot();
            snapshot.SampledAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow);
            snapshot.Status = RigConnectionStatus.Connected;
            snapshot.ClearErrorMessage();
            _cached = snapshot;
            _lastFetchTimestamp = Stopwatch.GetTimestamp();
            return snapshot.Clone();
        }
        catch (RigControlException ex)
        {
            if (_cached is not null)
            {
                var stale = _cached.Clone();
                stale.Status = RigConnectionStatus.Error;
                stale.ErrorMessage = ex.Message;
                return stale;
            }

            var status = ex.Kind == RigControlErrorKind.Disabled
                ? RigConnectionStatus.Disabled
                : RigConnectionStatus.Error;

            return new RigSnapshot
            {
                SampledAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
                Status = status,
                ErrorMessage = ex.Message,
            };
        }
    }
}
