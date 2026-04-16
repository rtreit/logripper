using QsoRipper.Gui.Inspection;
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
}
