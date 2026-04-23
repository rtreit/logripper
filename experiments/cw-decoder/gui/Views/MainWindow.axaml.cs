using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using CwDecoderGui.ViewModels;
using System.Linq;

namespace CwDecoderGui.Views;

public partial class MainWindow : Window
{
    public MainWindow() => InitializeComponent();

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    private MainWindowViewModel? Vm => DataContext as MainWindowViewModel;

    private void OnStartStopClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ToggleStartStop();

    private void OnRefreshClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.RefreshDevices();

    private async void OnReplayClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.ReplayLastRecordingAsync();
    }

    private void OnResetSensitivityClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ResetSensitivity();

    private async void OnOpenFileClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (StorageProvider is null || Vm is null) return;
        var picked = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Pick a CW audio file",
            AllowMultiple = false,
            FileTypeFilter = new[]
            {
                new FilePickerFileType("Audio")
                {
                    Patterns = new[] { "*.wav", "*.mp3", "*.m4a", "*.aac" },
                },
            },
        });
        var first = picked.FirstOrDefault();
        if (first?.TryGetLocalPath() is string path)
            Vm.OpenFile(path);
    }

    private async void OnOpenHarvestFileClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (StorageProvider is null || Vm is null) return;
        var picked = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Pick a CW audio file for candidate harvest",
            AllowMultiple = false,
            FileTypeFilter = new[]
            {
                new FilePickerFileType("Audio")
                {
                    Patterns = new[] { "*.wav", "*.mp3", "*.m4a", "*.aac" },
                },
            },
        });
        var first = picked.FirstOrDefault();
        if (first?.TryGetLocalPath() is string path)
            Vm.SetHarvestFile(path);
    }

    private async void OnHarvestClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.HarvestCandidatesAsync();
    }

    private async void OnPlayPreviewClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.PlaySelectedCandidateAsync();
    }

    private void OnSaveLabelClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.SaveSelectedLabel();

    private void OnResetAdjustedSpanClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ResetAdjustedSpan();

    private void OnUseSuggestedSpanClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.UseSuggestedSpan();

    private async void OnRunLabelScoreClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.RunLabelScoreAsync();
    }

    private async void OnRunLabelSweepClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.RunLabelSweepAsync();
    }

    private void OnApplyTopSweepClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ApplyTopSweepResult();
}
