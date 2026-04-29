using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using CwDecoderGui.ViewModels;
using System.ComponentModel;
using System.Linq;

namespace CwDecoderGui.Views;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        DataContextChanged += (_, _) => HookVmForTranscriptScroll();
    }

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    private MainWindowViewModel? _hookedVm;

    private void HookVmForTranscriptScroll()
    {
        if (_hookedVm is not null)
        {
            _hookedVm.PropertyChanged -= OnVmPropertyChanged;
            _hookedVm = null;
        }
        if (DataContext is MainWindowViewModel vm)
        {
            _hookedVm = vm;
            vm.PropertyChanged += OnVmPropertyChanged;
        }
    }

    private void OnVmPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(MainWindowViewModel.VizTranscript)) return;
        Dispatcher.UIThread.Post(() =>
        {
            if (this.FindControl<ScrollViewer>("VizTranscriptScroll") is { } sv)
            {
                sv.ScrollToEnd();
            }
        }, DispatcherPriority.Background);
    }

    private MainWindowViewModel? Vm => DataContext as MainWindowViewModel;

    private void OnStartStopClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ToggleStartStop();

    private void OnVizStartStopClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ToggleViz();

    private async void OnVizPlayFileClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (StorageProvider is null || Vm is null) return;
        var picked = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Pick a CW audio file to visualize",
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
            Vm.StartVizFile(path);
    }

    private void OnRefreshClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.RefreshDevices();

    private async void OnReplayClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.ReplayLastRecordingAsync();
    }

    private void OnResetSensitivityClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ResetSensitivity();

    private void OnMicModePresetClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ApplyMicModePreset();

    private void OnRadioModePresetClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ApplyRadioModePreset();

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
            await Vm.OpenFileAsync(path);
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

    private async void OnToggleLabelingRecordClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.ToggleLabelingRecordAsync();
    }

    private async void OnPlayPreviewClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.PlaySelectedCandidateAsync();
    }

    private void OnStartPlaybackClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.StartPlayback();

    private void OnStopPlaybackClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.StopPlayback();

    private void OnClosePlaybackClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ClosePlaybackPreview();

    private void OnPauseResumeClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.TogglePauseResume();

    private async void OnApplyRegionClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.ApplyRegionAsync();
    }

    private void OnCorrectCopyTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (sender is not TextBox textBox || string.IsNullOrEmpty(textBox.Text))
        {
            return;
        }

        var upper = textBox.Text.ToUpperInvariant();
        if (string.Equals(upper, textBox.Text, System.StringComparison.Ordinal))
        {
            return;
        }

        var caret = textBox.CaretIndex;
        textBox.Text = upper;
        textBox.CaretIndex = System.Math.Min(caret, upper.Length);
    }

    private void OnSaveLabelClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.SaveSelectedLabel();

    private void OnExportSelectionToTrainingSetClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ExportSelectionToTrainingSet();

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

    private void OnRefreshLabelCorpusClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        Vm?.RefreshLabelCorpus();
    }

    private async void OnRunStrategySweepClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.RunStrategySweepAsync();
    }

    private void OnResetStrategyDefaultsClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ResetStrategyDefaults();

    private async void OnCopyStrategySweepMarkdownClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        var md = Vm.BuildStrategySweepMarkdown();
        if (string.IsNullOrEmpty(md)) return;
        var top = Avalonia.Application.Current?.ApplicationLifetime as Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime;
        var window = top?.MainWindow;
        if (window?.Clipboard is { } clip)
        {
            await clip.SetTextAsync(md);
        }
    }

    private void OnApplyTopSweepClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
        => Vm?.ApplyTopSweepResult();

    private async void OnRunBenchClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (Vm is null) return;
        await Vm.ToggleRunBenchAsync();
    }

    private async void OnPickBenchFileClick(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (StorageProvider is null || Vm is null) return;
        var picked = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Pick a CW audio file for the bench",
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
            Vm.SetBenchFile(path);
    }
}
