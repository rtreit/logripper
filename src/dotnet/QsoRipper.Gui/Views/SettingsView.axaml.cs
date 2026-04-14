using System;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Media;
using QsoRipper.Domain;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Views;

internal sealed partial class SettingsView : Window
{
    public SettingsView()
    {
        InitializeComponent();
        DataContextChanged += OnDataContextChanged;
    }

    /// <summary>
    /// Converts a bool (success/failure) to a green or red brush for test result text.
    /// </summary>
    public static readonly FuncValueConverter<bool, IBrush> TestResultColor =
        new(success => success ? Brushes.ForestGreen : Brushes.IndianRed);

    /// <summary>
    /// Two-way converter between <see cref="ConflictPolicy"/> enum and ComboBox selected index.
    /// </summary>
    public static readonly IValueConverter ConflictPolicyIndex = new ConflictPolicyIndexConverter();

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (DataContext is SettingsViewModel vm)
        {
            vm.CloseRequested += OnCloseRequested;
        }
    }

    private void OnCloseRequested(object? sender, bool didSave)
    {
        if (sender is SettingsViewModel vm)
        {
            vm.CloseRequested -= OnCloseRequested;
        }

        Close(didSave);
    }

    protected override void OnClosed(EventArgs e)
    {
        if (DataContext is SettingsViewModel vm)
        {
            vm.CloseRequested -= OnCloseRequested;
        }

        base.OnClosed(e);
    }

    private sealed class ConflictPolicyIndexConverter : IValueConverter
    {
        public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        {
            return value is ConflictPolicy policy ? (int)policy : 0;
        }

        public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        {
            return value is int index && Enum.IsDefined(typeof(ConflictPolicy), index)
                ? (ConflictPolicy)index
                : ConflictPolicy.LastWriteWins;
        }
    }
}
