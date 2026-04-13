using System.Collections.Generic;
using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class LogFileStepViewModel : WizardStepViewModel
{
    public override string Title => "Log File";
    public override string Description => "Where should QsoRipper store your log?";

    [ObservableProperty]
    private string? _logFilePath;

    public override Dictionary<string, string> GetFields()
    {
        return new Dictionary<string, string>
        {
            ["log_file_path"] = LogFilePath ?? string.Empty,
        };
    }
}
