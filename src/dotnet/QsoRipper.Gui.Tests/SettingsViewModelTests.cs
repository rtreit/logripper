using QsoRipper.EngineSelection;
using QsoRipper.Gui.Inspection;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

public class SettingsViewModelTests
{
    [Fact]
    public async Task SaveCommandRejectsInvalidRigControlValuesWithoutPersistingChanges()
    {
        var client = new UxFixtureEngineClient(
            new UxCaptureFixture
            {
                RigControlEnabled = true,
                RigControlHost = "127.0.0.1",
                RigControlPort = 4532,
                RigControlReadTimeoutMs = 2000,
                RigControlStaleThresholdMs = 5000
            });
        var viewModel = new SettingsViewModel(client);

        await viewModel.LoadAsync();
        viewModel.RigControlPort = "not-a-port";

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.False(viewModel.DidSave);
        Assert.Equal(
            "Rig control port must be a whole number between 1 and 65535.",
            viewModel.ErrorMessage);

        var status = await client.GetSetupStatusAsync();
        Assert.NotNull(status.Status.RigControl);
        Assert.True(status.Status.RigControl.HasPort);
        Assert.Equal(4532u, status.Status.RigControl.Port);
    }

    [Fact]
    public async Task LoadAsyncUsesEngineNeutralPersistenceMetadata()
    {
        var client = new UxFixtureEngineClient(
            new UxCaptureFixture
            {
                ActiveLogFilePath = string.Empty,
                PersistenceStepEnabled = false,
                PersistenceLabel = "Storage",
                PersistenceDescription = "In-memory logbook"
            });
        var viewModel = new SettingsViewModel(client);

        await viewModel.LoadAsync();

        Assert.False(viewModel.RequiresLogFilePath);
        Assert.True(viewModel.ShowsPersistenceInfoOnly);
        Assert.Equal("Storage", viewModel.PersistenceSectionTitle);
        Assert.Equal("In-memory logbook", viewModel.PersistenceDescription);
        Assert.Equal(string.Empty, viewModel.LogFilePath);
    }

    [Fact]
    public async Task SaveCommandIncludesPersistencePathValueWhenRequired()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());
        var viewModel = new SettingsViewModel(client);

        await viewModel.LoadAsync();
        viewModel.LogFilePath = @"C:\logs\portable.db";

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.True(viewModel.DidSave);
        Assert.NotNull(client.LastSaveSetupRequest);
        Assert.False(client.LastSaveSetupRequest.HasLogFilePath);
        Assert.Equal(string.Empty, client.LastSaveSetupRequest.LogFilePath);
        var persistenceValue = Assert.Single(client.LastSaveSetupRequest.PersistenceValues);
        Assert.Equal(PersistenceSetup.PathKey, persistenceValue.Key);
        Assert.Equal(@"C:\logs\portable.db", persistenceValue.Value);
    }

    [Fact]
    public void RadioMonitorPropertiesRoundTripDefaultsAndUpdates()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());
        var viewModel = new SettingsViewModel(client);

        // Defaults: monitor off, status bar hidden, no device pre-selected.
        Assert.False(viewModel.IsRadioMonitorEnabled);
        Assert.False(viewModel.IsCwWpmStatusBarVisible);
        Assert.Null(viewModel.SelectedRadioMonitorDevice);
        Assert.Equal(string.Empty, viewModel.ResolvedCaptureDevice);
        Assert.False(viewModel.ResolvedIsLoopback);

        viewModel.IsRadioMonitorEnabled = true;
        viewModel.IsCwWpmStatusBarVisible = true;
        viewModel.SelectedRadioMonitorDevice = new RadioMonitorDevice("USB Audio CODEC", IsLoopback: false);

        Assert.True(viewModel.IsRadioMonitorEnabled);
        Assert.True(viewModel.IsCwWpmStatusBarVisible);
        Assert.Equal("USB Audio CODEC", viewModel.ResolvedCaptureDevice);
        Assert.False(viewModel.ResolvedIsLoopback);

        // Loopback flows through to ResolvedIsLoopback.
        viewModel.SelectedRadioMonitorDevice = new RadioMonitorDevice("Speakers (Realtek)", IsLoopback: true);
        Assert.True(viewModel.ResolvedIsLoopback);
        Assert.Equal("Speakers (Realtek)", viewModel.ResolvedCaptureDevice);

        // System default sentinel resolves to empty string + non-loopback.
        viewModel.SelectedRadioMonitorDevice = RadioMonitorDeviceCatalog.SystemDefault;
        Assert.Equal(string.Empty, viewModel.ResolvedCaptureDevice);
        Assert.False(viewModel.ResolvedIsLoopback);
    }
}
