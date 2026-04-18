using System;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Threading;
using QsoRipper.Domain;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Views;

internal sealed partial class SettingsView : Window
{
    private readonly TabControl? _settingsSectionTabs;

    public SettingsView()
    {
        InitializeComponent();
        _settingsSectionTabs = this.FindControl<TabControl>("SettingsSectionTabs");
        if (_settingsSectionTabs is not null)
        {
            _settingsSectionTabs.SelectionChanged += OnSettingsSectionChanged;
        }

        Opened += OnOpened;
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
        if (_settingsSectionTabs is not null)
        {
            _settingsSectionTabs.SelectionChanged -= OnSettingsSectionChanged;
        }

        Opened -= OnOpened;

        if (DataContext is SettingsViewModel vm)
        {
            vm.CloseRequested -= OnCloseRequested;
        }

        base.OnClosed(e);
    }

    private void OnOpened(object? sender, EventArgs e)
    {
        FocusSelectedSectionStarter();
    }

    private void OnSettingsSectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        FocusSelectedSectionStarter();
    }

    private void FocusSelectedSectionStarter()
    {
        var target = _settingsSectionTabs?.SelectedIndex switch
        {
            0 => this.FindControl<Control>("SettingsCallsignBox"),
            1 => this.FindControl<Control>("SettingsSpaceWeatherVisibleCheckBox"),
            2 => this.FindControl<Control>("SettingsAutoSyncCheckBox"),
            3 => this.FindControl<Control>("SettingsQrzXmlUsernameBox"),
            4 => this.FindControl<Control>("SettingsRigControlEnabledCheckBox"),
            _ => null
        };

        if (target is null)
        {
            return;
        }

        Dispatcher.UIThread.Post(
            () => target.Focus(),
            DispatcherPriority.Input);
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
