using System;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Input;
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

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (TryHandleSectionShortcut(e))
        {
            return;
        }

        base.OnKeyDown(e);
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

    private bool TryHandleSectionShortcut(KeyEventArgs e)
    {
        if (DataContext is not SettingsViewModel vm || !e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            return false;
        }

        switch (e.Key)
        {
            case Key.D1:
            case Key.NumPad1:
                vm.SelectStationSectionCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.D2:
            case Key.NumPad2:
                vm.SelectDisplaySectionCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.D3:
            case Key.NumPad3:
                vm.SelectStorageSyncSectionCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.D4:
            case Key.NumPad4:
                vm.SelectQrzSectionCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.D5:
            case Key.NumPad5:
                vm.SelectRigSectionCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.Tab:
                if (e.KeyModifiers.HasFlag(KeyModifiers.Shift))
                {
                    vm.SelectPreviousSectionCommand.Execute(null);
                }
                else
                {
                    vm.SelectNextSectionCommand.Execute(null);
                }

                e.Handled = true;
                return true;
            default:
                return false;
        }
    }

    private sealed class ConflictPolicyIndexConverter : IValueConverter
    {
        public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        {
            return value switch
            {
                ConflictPolicy.LastWriteWins => 0,
                ConflictPolicy.FlagForReview => 1,
                _ => 1,
            };
        }

        public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        {
            return value is int index
                ? index switch
                {
                    0 => ConflictPolicy.LastWriteWins,
                    1 => ConflictPolicy.FlagForReview,
                    _ => ConflictPolicy.FlagForReview,
                }
                : ConflictPolicy.FlagForReview;
        }
    }
}
