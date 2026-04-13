using System.Collections.Generic;
using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class LogFileStepViewModel : WizardStepViewModel
{
    public override string Title => "Log File";
    public override string Description => "Where should QsoRipper store your log?";

    [ObservableProperty]
    private string? _logFolder;

    [ObservableProperty]
    private string _logFileName = "qsoripper";

    [ObservableProperty]
    private bool _offerCreateDirectory;

    [ObservableProperty]
    private string? _directoryMessage;

    /// <summary>
    /// Computed full path: folder + name + .db extension.
    /// </summary>
    public string? LogFilePath
    {
        get
        {
            if (string.IsNullOrWhiteSpace(LogFolder))
            {
                return null;
            }

            var name = string.IsNullOrWhiteSpace(LogFileName) ? "qsoripper" : LogFileName.Trim();
            if (!name.EndsWith(".db", System.StringComparison.OrdinalIgnoreCase))
            {
                name += ".db";
            }

            return Path.Combine(LogFolder.Trim(), name);
        }
        set
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                LogFolder = null;
                LogFileName = "qsoripper";
                return;
            }

            var dir = Path.GetDirectoryName(value);
            var file = Path.GetFileNameWithoutExtension(value);
            LogFolder = dir ?? string.Empty;
            LogFileName = string.IsNullOrWhiteSpace(file) ? "qsoripper" : file;
        }
    }

    partial void OnLogFolderChanged(string? value)
    {
        CheckDirectory();
    }

    [RelayCommand]
    private void CreateDirectory()
    {
        if (!string.IsNullOrWhiteSpace(LogFolder) && !Directory.Exists(LogFolder))
        {
            try
            {
                Directory.CreateDirectory(LogFolder);
                DirectoryMessage = "✓ Directory created.";
                OfferCreateDirectory = false;
            }
            catch (UnauthorizedAccessException ex)
            {
                DirectoryMessage = $"Failed to create directory: {ex.Message}";
            }
            catch (IOException ex)
            {
                DirectoryMessage = $"Failed to create directory: {ex.Message}";
            }
        }
    }

    private void CheckDirectory()
    {
        if (string.IsNullOrWhiteSpace(LogFolder))
        {
            OfferCreateDirectory = false;
            DirectoryMessage = null;
            return;
        }

        if (Directory.Exists(LogFolder))
        {
            OfferCreateDirectory = false;
            DirectoryMessage = "✓ Directory exists.";
        }
        else
        {
            OfferCreateDirectory = true;
            DirectoryMessage = $"Directory '{LogFolder}' does not exist.";
        }
    }

    public override Dictionary<string, string> GetFields()
    {
        return new Dictionary<string, string>
        {
            ["log_file_path"] = LogFilePath ?? string.Empty,
        };
    }
}
