using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

/// <summary>
/// Verifies the QSO-entry episode lifecycle exposed by
/// <see cref="QsoLoggerViewModel"/>. <c>MainWindowViewModel</c> uses these
/// signals to gate the cw-decoder subprocess, so the contract must be:
/// <list type="bullet">
///   <item><c>IsLoggerEpisodeActive</c> is false on construction (no QSO).</item>
///   <item>Typing a callsign flips it true and raises <c>CwEpisodeStarted</c> exactly once.</item>
///   <item>Clearing the callsign back to empty raises <c>CwEpisodeBoundary</c> with <c>"abandoned"</c>
///         and flips it false.</item>
///   <item><c>Clear()</c> raises <c>CwEpisodeBoundary</c> with <c>"cleared"</c> and flips it false.</item>
/// </list>
/// </summary>
public sealed class QsoLoggerEpisodeLifecycleTests
{
    [Fact]
    public void NewLoggerHasNoActiveEpisode()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());

        Assert.False(logger.IsLoggerEpisodeActive);
    }

    [Fact]
    public void TypingCallsignActivatesEpisodeAndRaisesEpisodeStartedOnce()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());
        var startedCount = 0;
        logger.CwEpisodeStarted += (_, _) => startedCount++;

        logger.Callsign = "K";

        Assert.True(logger.IsLoggerEpisodeActive);
        Assert.Equal(1, startedCount);

        // Continuing to type does NOT re-raise the start (idempotent).
        logger.Callsign = "K7";
        Assert.Equal(1, startedCount);
        Assert.True(logger.IsLoggerEpisodeActive);
    }

    [Fact]
    public void EmptyingCallsignRaisesAbandonedBoundaryAndDeactivatesEpisode()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());
        logger.Callsign = "K7ABC";
        Assert.True(logger.IsLoggerEpisodeActive);

        CwEpisodeBoundaryEventArgs? boundary = null;
        logger.CwEpisodeBoundary += (_, e) => boundary = e;

        logger.Callsign = string.Empty;

        Assert.False(logger.IsLoggerEpisodeActive);
        Assert.NotNull(boundary);
        Assert.Equal("abandoned", boundary!.Reason);
    }

    [Fact]
    public void ClearCommandRaisesClearedBoundaryAndDeactivatesEpisode()
    {
        var logger = new QsoLoggerViewModel(new MinimalEngineClient());
        logger.Callsign = "W7XYZ";
        Assert.True(logger.IsLoggerEpisodeActive);

        CwEpisodeBoundaryEventArgs? boundary = null;
        logger.CwEpisodeBoundary += (_, e) => boundary = e;

        logger.ClearCommand.Execute(null);

        Assert.False(logger.IsLoggerEpisodeActive);
        Assert.NotNull(boundary);
        Assert.Equal("cleared", boundary!.Reason);
        Assert.Equal(string.Empty, logger.Callsign);
    }

    private sealed class MinimalEngineClient : IEngineClient
    {
        public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(string username, string password, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<QsoRecord>>([]);
        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) => Task.FromResult(new LogQsoResponse { LocalId = "x" });
        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<PurgeDeletedQsosResponse> PurgeDeletedQsosAsync(IReadOnlyList<string>? localIds = null, Timestamp? olderThan = null, bool includePendingRemoteDeletes = false, CancellationToken ct = default) => throw new NotImplementedException();
    }
}
