using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Inspection;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public class UxFixtureEngineClientTests
{
    [Fact]
    public async Task SaveSetupAsyncUpdatesFixtureBackedState()
    {
        var fixture = new UxCaptureFixture
        {
            SetupComplete = false,
            IsFirstRun = true,
            ActiveLogFilePath = @"C:\logs\initial.db",
            HasQrzLogbookApiKey = false
        };
        var client = new UxFixtureEngineClient(fixture);

        var response = await client.SaveSetupAsync(
            new SaveSetupRequest
            {
                LogFilePath = @"C:\logs\automation.db",
                StationProfile = new StationProfile
                {
                    ProfileName = "Automation",
                    StationCallsign = "K7AUT",
                    OperatorCallsign = "K7AUT",
                    Grid = "CN87"
                },
                QrzXmlUsername = "demo-user",
                QrzXmlPassword = "demo-password",
                QrzLogbookApiKey = "demo-api-key",
                SyncConfig = new SyncConfig
                {
                    AutoSyncEnabled = true,
                    SyncIntervalSeconds = 600,
                    ConflictPolicy = ConflictPolicy.FlagForReview
                },
                RigControl = new RigControlSettings
                {
                    Enabled = true,
                    Host = "127.0.0.1",
                    Port = 4532,
                    ReadTimeoutMs = 2500,
                    StaleThresholdMs = 6000
                }
            });

        Assert.True(response.Status.SetupComplete);
        Assert.Equal(@"C:\logs\automation.db", response.Status.LogFilePath);
        Assert.Equal("K7AUT", response.Status.StationProfile.StationCallsign);
        Assert.True(response.Status.HasQrzXmlPassword);
        Assert.True(response.Status.HasQrzLogbookApiKey);
        Assert.Equal(600u, response.Status.SyncConfig.SyncIntervalSeconds);
        Assert.Equal(ConflictPolicy.FlagForReview, response.Status.SyncConfig.ConflictPolicy);
        Assert.NotNull(response.Status.RigControl);
        Assert.True(response.Status.RigControl.Enabled);
        Assert.Equal("127.0.0.1", response.Status.RigControl.Host);
        Assert.Equal(4532u, response.Status.RigControl.Port);
        Assert.Equal(2500ul, response.Status.RigControl.ReadTimeoutMs);
        Assert.Equal(6000ul, response.Status.RigControl.StaleThresholdMs);

        var state = await client.GetWizardStateAsync();
        Assert.False(state.Status.IsFirstRun);
        Assert.Single(state.StationProfiles);
        Assert.All(state.Steps, step => Assert.True(step.Complete || step.Step == SetupWizardStep.QrzIntegration));
    }

    [Fact]
    public async Task ValidateStepAsyncReturnsFieldErrorsForMissingRequiredFields()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());

        var response = await client.ValidateStepAsync(
            new ValidateSetupStepRequest
            {
                Step = SetupWizardStep.StationProfiles,
                StationProfile = new StationProfile()
            });

        Assert.False(response.Valid);
        Assert.Contains(response.Fields, field => field.Field == "profile_name" && !field.Valid);
        Assert.Contains(response.Fields, field => field.Field == "callsign" && !field.Valid);
        Assert.Contains(response.Fields, field => field.Field == "operator_callsign" && !field.Valid);
        Assert.Contains(response.Fields, field => field.Field == "grid_square" && !field.Valid);
    }

    [Fact]
    public async Task SyncWithQrzAsyncMarksPendingRecordsSyncedWhenApiKeyConfigured()
    {
        var fixture = new UxCaptureFixture
        {
            HasQrzLogbookApiKey = true,
            RecentQsos =
            [
                new UxCaptureQsoFixtureItem
                {
                    LocalId = "qso-1",
                    WorkedCallsign = "W1AW",
                    SyncStatus = nameof(QsoRipper.Domain.SyncStatus.LocalOnly),
                    UtcTimestamp = new DateTimeOffset(2026, 4, 13, 22, 16, 0, TimeSpan.Zero)
                }
            ]
        };
        var client = new UxFixtureEngineClient(fixture);

        var response = await client.SyncWithQrzAsync();
        var syncStatus = await client.GetSyncStatusAsync();
        var records = await client.ListRecentQsosAsync();

        Assert.Equal(1u, response.UploadedRecords);
        Assert.Equal(0u, syncStatus.PendingUpload);
        Assert.Equal(QsoRipper.Domain.SyncStatus.Synced, Assert.Single(records).SyncStatus);
        Assert.NotNull(syncStatus.LastSync);
    }
}
