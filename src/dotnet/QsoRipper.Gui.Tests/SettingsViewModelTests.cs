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
    }

    [Fact]
    public void PreselectRadioMonitorDeviceWithMissingDeviceInsertsPlaceholder()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());
        var viewModel = new SettingsViewModel(client);

        // Persisted device that is no longer enumerable should round-trip via
        // a synthesized "(not currently available)" entry so the user keeps
        // visibility into what was previously chosen.
        viewModel.PreselectRadioMonitorDevice("Missing Mic", isLoopback: false);

        Assert.NotNull(viewModel.SelectedRadioMonitorDevice);
        Assert.Equal("Missing Mic", viewModel.ResolvedCaptureDevice);
        Assert.Equal("Missing Mic", viewModel.SelectedRadioMonitorDevice!.Name);
        Assert.True(viewModel.SelectedRadioMonitorDevice.IsUnavailable);
        Assert.False(viewModel.ResolvedIsLoopback);
        Assert.Contains("(not currently available)", viewModel.SelectedRadioMonitorDevice.DisplayName, StringComparison.Ordinal);
    }

    [Fact]
    public void PreselectRadioMonitorDeviceWithEmptyOverrideSelectsSystemDefault()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());
        var viewModel = new SettingsViewModel(client);

        viewModel.PreselectRadioMonitorDevice(string.Empty, isLoopback: false);

        Assert.Same(RadioMonitorDeviceCatalog.SystemDefault, viewModel.SelectedRadioMonitorDevice);
        Assert.Equal(string.Empty, viewModel.ResolvedCaptureDevice);
    }

    [Fact]
    public void PreselectRadioMonitorDeviceMatchesByNameAndLoopbackFlag()
    {
        var client = new UxFixtureEngineClient(new UxCaptureFixture());
        var viewModel = new SettingsViewModel(client);

        // Simulate the catalog populating two entries with the same name but
        // different loopback flags (a corner case but worth guarding).
        var inputDevice = new RadioMonitorDevice("Speakers (Realtek)", IsLoopback: false);
        var loopbackDevice = new RadioMonitorDevice("Speakers (Realtek)", IsLoopback: true);
        viewModel.RadioMonitorDevices.Add(RadioMonitorDeviceCatalog.SystemDefault);
        viewModel.RadioMonitorDevices.Add(inputDevice);
        viewModel.RadioMonitorDevices.Add(loopbackDevice);

        viewModel.PreselectRadioMonitorDevice("Speakers (Realtek)", isLoopback: true);

        Assert.Same(loopbackDevice, viewModel.SelectedRadioMonitorDevice);
        Assert.True(viewModel.ResolvedIsLoopback);
    }
}
