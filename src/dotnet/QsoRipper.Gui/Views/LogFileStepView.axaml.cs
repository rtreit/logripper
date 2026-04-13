using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Platform.Storage;

namespace QsoRipper.Gui.Views;

internal sealed partial class LogFileStepView : UserControl
{
    public LogFileStepView()
    {
        InitializeComponent();
        var browseBtn = this.FindControl<Button>("BrowseButton");
        if (browseBtn is not null)
        {
            browseBtn.Click += OnBrowseClick;
        }
    }

    public static readonly FuncValueConverter<bool, IBrush> DirectoryColor =
        new(offerCreate => offerCreate ? Brushes.Orange : Brushes.ForestGreen);

    private async void OnBrowseClick(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel is null)
        {
            return;
        }

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(
            new FolderPickerOpenOptions
            {
                Title = "Choose log folder",
                AllowMultiple = false,
            });

        if (folders.Count > 0 && DataContext is ViewModels.LogFileStepViewModel vm)
        {
            vm.LogFolder = folders[0].Path.LocalPath;
        }
    }
}
