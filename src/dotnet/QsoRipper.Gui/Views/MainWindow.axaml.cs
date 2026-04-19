using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using QsoRipper.Gui.Utilities;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Views;

internal sealed partial class MainWindow : Window
{
    private readonly RecentQsoGridLayoutStore _gridLayoutStore = new();
    private readonly UiPreferencesStore _preferencesStore = new();
    private readonly MenuItem? _fileMenuItem;
    private readonly TextBox? _recentQsoSearchBox;
    private readonly DataGrid? _recentQsoGrid;
    private readonly double _preferredWidth;
    private readonly double _preferredHeight;
    private readonly double _preferredMinWidth;
    private readonly double _preferredMinHeight;
    private MainWindowViewModel? _viewModel;
    private bool _gridLayoutApplied;
    private bool _menuAccessKeysPrimed;
    private bool _loggedFirstRecentQsoRow;
    private Dictionary<RecentQsoGridColumn, DataGridColumn> _columnMap = [];
    internal bool IsInspectionMode { get; set; }

    public MainWindow()
    {
        InitializeComponent();
        _preferredWidth = Width;
        _preferredHeight = Height;
        _preferredMinWidth = MinWidth;
        _preferredMinHeight = MinHeight;
        _fileMenuItem = this.FindControl<MenuItem>("FileMenuItem");
        _recentQsoSearchBox = this.FindControl<TextBox>("RecentQsoSearchBox");
        _recentQsoGrid = this.FindControl<DataGrid>("RecentQsoGrid");
        if (_recentQsoGrid is not null)
        {
            _recentQsoGrid.LoadingRow += OnRecentQsoGridLoadingRow;
        }

        DataContextChanged += OnDataContextChanged;
        BuildColumnMap();
    }

    protected override async void OnOpened(EventArgs e)
    {
        base.OnOpened(e);
        GuiPerformanceTrace.Write(nameof(OnOpened) + ".start");
        try
        {
            ClampToCurrentScreen();
            if (!IsInspectionMode)
            {
                GuiPerformanceTrace.Write(nameof(OnOpened) + ".afterScheduleMenuAccessKeys");
                ApplyPersistedGridLayout();
                GuiPerformanceTrace.Write(nameof(OnOpened) + ".afterApplyPersistedGridLayout");
                if (DataContext is MainWindowViewModel vm)
                {
                    vm.ApplyPreferences(_preferencesStore.Load());
                    GuiPerformanceTrace.Write(nameof(OnOpened) + ".afterApplyPreferences");
                    await vm.CheckFirstRunAsync();
                    GuiPerformanceTrace.Write(nameof(OnOpened) + ".afterCheckFirstRun");
                }

                Dispatcher.UIThread.Post(PrimeMenuAccessKeys, DispatcherPriority.Background);
            }

            EnsureColumnHeadersFit();
        }
        catch (ObjectDisposedException ex)
        {
            GuiPerformanceTrace.Write(nameof(OnOpened) + ".error", ex.ToString());
            if (DataContext is MainWindowViewModel vm)
            {
                vm.StatusMessage = "Error: startup did not fully complete.";
            }
        }
        catch (InvalidOperationException ex)
        {
            GuiPerformanceTrace.Write(nameof(OnOpened) + ".error", ex.ToString());
            if (DataContext is MainWindowViewModel vm)
            {
                vm.StatusMessage = "Error: startup did not fully complete.";
            }
        }
    }

    protected override void OnClosed(EventArgs e)
    {
        SaveGridLayout();
        SavePreferences();

        if (_viewModel is not null)
        {
            _viewModel.PropertyChanged -= OnViewModelPropertyChanged;
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
            _viewModel.GridFocusRequested -= OnGridFocusRequested;
            _viewModel.SettingsRequested -= OnSettingsRequested;
            _viewModel.LoggerFocusRequested -= OnLoggerFocusRequested;
            _viewModel.ColumnLayoutResetRequested -= OnColumnLayoutResetRequested;
            UnsubscribeColumnOptions(_viewModel.RecentQsos);
            _viewModel = null;
        }

        if (DataContext is IDisposable disposable)
        {
            disposable.Dispose();
        }

        base.OnClosed(e);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (_viewModel is null || _viewModel.IsWizardOpen)
        {
            base.OnKeyDown(e);
            return;
        }

        // Sort/column chooser keys take priority so digit and arrow keys
        // aren't swallowed by the grid or other handlers.
        if (TryHandleChooserKey(e))
        {
            return;
        }

        if (TryHandleLoggerKeyDown(e))
        {
            return;
        }

        if (TryHandleRecentQsoZoomKey(e))
        {
            return;
        }

        if (HandleRecentQsoGridKeyDown(e))
        {
            return;
        }

        // Global navigation keys — handled explicitly so they work even when
        // focus is inside a TextBox (e.g. the QSO logger fields) where the
        // XAML KeyBinding may not fire reliably.
        if (TryHandleGlobalNavigationKey(e))
        {
            return;
        }

        base.OnKeyDown(e);

        if (e.Handled || _viewModel is null || _viewModel.IsWizardOpen)
        {
            return;
        }

        if (string.Equals(e.KeySymbol, "/", StringComparison.Ordinal)
            && e.Source is not TextBox
            && !(_recentQsoSearchBox?.IsFocused ?? false))
        {
            FocusRecentQsoSearchBox();
            e.Handled = true;
            return;
        }

        if (e.Key == Key.Escape)
        {
            if (_viewModel.IsFullQsoCardOpen)
            {
                _viewModel.ToggleFullQsoCardCommand.Execute(null);
                e.Handled = true;
                return;
            }

            if (_viewModel.IsHelpOpen)
            {
                _viewModel.ToggleHelpCommand.Execute(null);
                e.Handled = true;
                return;
            }

            if (_viewModel.IsCallsignCardOpen)
            {
                _viewModel.CloseCallsignCardCommand.Execute(null);
                e.Handled = true;
                return;
            }

            if (_viewModel.IsInspectorOpen)
            {
                _viewModel.ToggleInspectorCommand.Execute(null);
                e.Handled = true;
            }
        }
    }

    private bool HandleRecentQsoGridKeyDown(KeyEventArgs e)
    {
        if (_recentQsoGrid is null || _viewModel is null || !_recentQsoGrid.IsKeyboardFocusWithin)
        {
            return false;
        }

        var isEditingTextBox = e.Source is TextBox;
        var modifiers = e.KeyModifiers;

        if (!isEditingTextBox && modifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.A)
        {
            _recentQsoGrid.SelectAll();
            e.Handled = true;
            return true;
        }

        if (modifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.S)
        {
            CommitAndSaveGridEdits();
            e.Handled = true;
            return true;
        }

        if (!isEditingTextBox && e.Key == Key.F2)
        {
            EnsureGridHasEditableCell();
            if (_recentQsoGrid.BeginEdit())
            {
                e.Handled = true;
                return true;
            }
        }

        if (!isEditingTextBox && e.Key == Key.Delete)
        {
            _viewModel.RecentQsos.RequestDeleteSelectedQso();
            e.Handled = true;
            return true;
        }

        if (!isEditingTextBox && modifiers.HasFlag(KeyModifiers.Control) && e.Key == Key.D)
        {
            _viewModel.RecentQsos.RequestDeleteSelectedQso();
            e.Handled = true;
            return true;
        }

        if (e.Key == Key.Enter && _recentQsoGrid.CommitEdit())
        {
            e.Handled = true;
            return true;
        }

        if (e.Key == Key.Escape && _recentQsoGrid.CancelEdit())
        {
            e.Handled = true;
            return true;
        }

        return false;
    }

    private bool TryHandleLoggerKeyDown(KeyEventArgs e)
    {
        if (_viewModel is null)
        {
            return false;
        }

        var source = e.Source as Control;
        var bandButton = this.FindControl<Button>("LoggerBandButton");
        var modeButton = this.FindControl<Button>("LoggerModeButton");

        // Left/Right on band button cycles bands
        if (source == bandButton)
        {
            if (e.Key == Key.Left)
            {
                _viewModel.Logger.CycleBandBackwardCommand.Execute(null);
                e.Handled = true;
                return true;
            }

            if (e.Key == Key.Right)
            {
                _viewModel.Logger.CycleBandForwardCommand.Execute(null);
                e.Handled = true;
                return true;
            }
        }

        // Left/Right on mode button cycles modes
        if (source == modeButton)
        {
            if (e.Key == Key.Left)
            {
                _viewModel.Logger.CycleModeBackwardCommand.Execute(null);
                e.Handled = true;
                return true;
            }

            if (e.Key == Key.Right)
            {
                _viewModel.Logger.CycleModeForwardCommand.Execute(null);
                e.Handled = true;
                return true;
            }
        }

        return false;
    }

    private bool TryHandleGlobalNavigationKey(KeyEventArgs e)
    {
        if (_viewModel is null || e.KeyModifiers != KeyModifiers.None)
        {
            return false;
        }

        switch (e.Key)
        {
            case Key.F3:
                _viewModel.FocusGridCommand.Execute(null);
                e.Handled = true;
                return true;
            case Key.F4:
                _viewModel.FocusSearchCommand.Execute(null);
                e.Handled = true;
                return true;
            default:
                return false;
        }
    }

    private bool TryHandleRecentQsoZoomKey(KeyEventArgs e)
    {
        if (_viewModel is null || !e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            return false;
        }

        switch (e.Key)
        {
            case Key.Add:
            case Key.OemPlus:
                _viewModel.RecentQsos.ZoomInGrid();
                e.Handled = true;
                return true;
            case Key.Subtract:
            case Key.OemMinus:
                _viewModel.RecentQsos.ZoomOutGrid();
                e.Handled = true;
                return true;
            case Key.D0:
            case Key.NumPad0:
                _viewModel.RecentQsos.ResetGridZoom();
                e.Handled = true;
                return true;
        }

        return e.KeySymbol switch
        {
            "+" or "=" => HandleZoomAction(_viewModel.RecentQsos.ZoomInGrid, e),
            "-" or "_" => HandleZoomAction(_viewModel.RecentQsos.ZoomOutGrid, e),
            "0" => HandleZoomAction(_viewModel.RecentQsos.ResetGridZoom, e),
            _ => false
        };
    }

    private void PrimeMenuAccessKeys()
    {
        if (_menuAccessKeysPrimed || _fileMenuItem is null)
        {
            return;
        }

        _menuAccessKeysPrimed = true;

        // Avalonia access-key mode does not fully initialize until a menu has been shown once.
        Dispatcher.UIThread.Post(
            () =>
            {
                _fileMenuItem.IsSubMenuOpen = true;
                Dispatcher.UIThread.Post(
                    () => _fileMenuItem.IsSubMenuOpen = false,
                    DispatcherPriority.Background);
            },
            DispatcherPriority.Background);
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (_viewModel is not null)
        {
            _viewModel.PropertyChanged -= OnViewModelPropertyChanged;
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
            _viewModel.GridFocusRequested -= OnGridFocusRequested;
            _viewModel.SettingsRequested -= OnSettingsRequested;
            _viewModel.LoggerFocusRequested -= OnLoggerFocusRequested;
            _viewModel.ColumnLayoutResetRequested -= OnColumnLayoutResetRequested;
            UnsubscribeColumnOptions(_viewModel.RecentQsos);
        }

        _viewModel = DataContext as MainWindowViewModel;
        if (_viewModel is not null)
        {
            _viewModel.PropertyChanged += OnViewModelPropertyChanged;
            _viewModel.SearchFocusRequested += OnSearchFocusRequested;
            _viewModel.GridFocusRequested += OnGridFocusRequested;
            _viewModel.SettingsRequested += OnSettingsRequested;
            _viewModel.LoggerFocusRequested += OnLoggerFocusRequested;
            _viewModel.ColumnLayoutResetRequested += OnColumnLayoutResetRequested;
            SubscribeColumnOptions(_viewModel.RecentQsos);
            ApplyDefaultColumnVisibility();
            WireLoggerFocusTracking();
            if (!IsInspectionMode)
            {
                ApplyPersistedGridLayout();
            }
        }
    }

    private void OnSearchFocusRequested(object? sender, EventArgs e)
    {
        FocusRecentQsoSearchBox();
    }

    private void OnGridFocusRequested(object? sender, EventArgs e)
    {
        if (_recentQsoGrid is null)
        {
            return;
        }

        // Defer focus at Background priority so the current key event and any
        // visual-tree changes (e.g. inspector panel collapse) finish first.
        // Input priority was too early — Avalonia's focus cleanup after removing
        // the inspector content could re-steal focus after our call.
        Dispatcher.UIThread.Post(
            () =>
            {
                _recentQsoGrid.Focus();
                if (_recentQsoGrid.SelectedIndex < 0 && _viewModel?.RecentQsos.VisibleItems.Count > 0)
                {
                    _recentQsoGrid.SelectedIndex = 0;
                }
            },
            DispatcherPriority.Background);
    }

    private async void OnSettingsRequested(object? sender, EventArgs e)
    {
        try
        {
            await ShowSettingsDialogAsync();
        }
        catch (ObjectDisposedException ex)
        {
            GuiPerformanceTrace.Write(nameof(OnSettingsRequested) + ".error", ex.ToString());
            if (_viewModel is not null)
            {
                _viewModel.StatusMessage = "Error: unable to open settings.";
            }
        }
        catch (InvalidOperationException ex)
        {
            GuiPerformanceTrace.Write(nameof(OnSettingsRequested) + ".error", ex.ToString());
            if (_viewModel is not null)
            {
                _viewModel.StatusMessage = "Error: unable to open settings.";
            }
        }
    }

    private void OnLoggerFocusRequested(object? sender, EventArgs e)
    {
        var loggerBox = this.FindControl<TextBox>("LoggerCallsignBox");
        if (loggerBox is not null)
        {
            Dispatcher.UIThread.Post(() =>
            {
                loggerBox.Focus();
                loggerBox.SelectAll();
            }, DispatcherPriority.Input);
        }
    }

    private void WireLoggerFocusTracking()
    {
        var loggerBox = this.FindControl<TextBox>("LoggerCallsignBox");
        if (loggerBox is not null)
        {
            loggerBox.GotFocus += OnLoggerGotFocus;
            loggerBox.LostFocus += OnLoggerLostFocus;
        }

        var bandButton = this.FindControl<Button>("LoggerBandButton");
        var modeButton = this.FindControl<Button>("LoggerModeButton");
        if (bandButton is not null)
        {
            bandButton.GotFocus += OnLoggerGotFocus;
            bandButton.LostFocus += OnLoggerLostFocus;
        }

        if (modeButton is not null)
        {
            modeButton.GotFocus += OnLoggerGotFocus;
            modeButton.LostFocus += OnLoggerLostFocus;
        }
    }

    private void OnLoggerGotFocus(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (_viewModel is not null)
        {
            _viewModel.IsLoggerFocused = true;
        }
    }

    private void OnLoggerLostFocus(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        // Delay slightly — focus may be transitioning between logger fields
        Dispatcher.UIThread.Post(() =>
        {
            if (_viewModel is null)
            {
                return;
            }

            var focused = FocusManager?.GetFocusedElement() as Control;
            var loggerPanel = this.FindControl<Border>("LoggerPanel");
            // Check if focus moved to another control within the logger panel
            bool isStillInLogger = false;
            var parent = focused;
            while (parent is not null)
            {
                if (parent == loggerPanel)
                {
                    isStillInLogger = true;
                    break;
                }

                parent = parent.Parent as Control;
            }

            _viewModel.IsLoggerFocused = isStillInLogger;
        }, DispatcherPriority.Background);
    }

    private void FocusRecentQsoSearchBox()
    {
        if (_recentQsoSearchBox is null)
        {
            return;
        }

        Dispatcher.UIThread.Post(
            () =>
            {
                _recentQsoSearchBox.Focus();
                _recentQsoSearchBox.SelectAll();
            },
            DispatcherPriority.Input);
    }

    private async Task ShowSettingsDialogAsync()
    {
        if (_viewModel is null || _viewModel.IsWizardOpen || _viewModel.IsSettingsOpen)
        {
            return;
        }

        _viewModel.IsSettingsOpen = true;
        var settingsVm = _viewModel.CreateSettingsViewModel();
        await settingsVm.LoadAsync();

        var dialog = new SettingsView { DataContext = settingsVm };
        await dialog.ShowDialog(this);
        if (settingsVm.DidSave)
        {
            _viewModel.ApplySettingsUiPreferences(settingsVm.IsSpaceWeatherVisible);
            SavePreferences();
        }

        await _viewModel.OnSettingsClosedAsync(settingsVm.DidSave);
    }
    private void CommitAndSaveGridEdits()
    {
        if (_recentQsoGrid is null || _viewModel is null)
        {
            return;
        }

        _recentQsoGrid.CommitEdit();
        if (_viewModel.RecentQsos.SaveEditsCommand.CanExecute(null))
        {
            _ = _viewModel.RecentQsos.SaveEditsCommand.ExecuteAsync(null);
        }
    }

    private void EnsureGridHasEditableCell()
    {
        if (_recentQsoGrid is null || _viewModel is null)
        {
            return;
        }

        if (_recentQsoGrid.SelectedItem is null && _viewModel.RecentQsos.VisibleItems.Count > 0)
        {
            _recentQsoGrid.SelectedItem = _viewModel.RecentQsos.VisibleItems[0];
        }

        if (_recentQsoGrid.CurrentColumn is { IsVisible: true, IsReadOnly: false })
        {
            return;
        }

        _recentQsoGrid.CurrentColumn = _recentQsoGrid.Columns
            .Where(column => column.IsVisible && !column.IsReadOnly)
            .OrderBy(column => column.DisplayIndex)
            .FirstOrDefault();
    }

    private void OnRecentQsoGridPointerWheelChanged(object? sender, PointerWheelEventArgs e)
    {
        if (_viewModel?.IsWizardOpen != false || !e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            return;
        }

        if (_viewModel.RecentQsos.AdjustZoom(Math.Sign(e.Delta.Y)))
        {
            e.Handled = true;
        }
    }

    private void OnRecentQsoGridLoadingRow(object? sender, DataGridRowEventArgs e)
    {
        if (e.Row.DataContext is RecentQsoItemViewModel item)
        {
            e.Row.Background = item.ContinentBrush;
            if (!_loggedFirstRecentQsoRow)
            {
                _loggedFirstRecentQsoRow = true;
                GuiPerformanceTrace.Write(
                    nameof(OnRecentQsoGridLoadingRow) + ".firstRow",
                    $"callsign={item.WorkedCallsign}; utc={item.UtcDisplay}");
            }
        }
    }


    private void ClampToCurrentScreen()
    {
        var screen = Screens.ScreenFromWindow(this) ?? Screens.Primary;
        if (screen is null)
        {
            return;
        }

        var layout = ResponsiveWindowLayout.ClampToWorkingArea(
            _preferredWidth,
            _preferredHeight,
            _preferredMinWidth,
            _preferredMinHeight,
            screen.WorkingArea,
            screen.Scaling);

        MinWidth = layout.MinWidth;
        MinHeight = layout.MinHeight;
        Width = layout.Width;
        Height = layout.Height;
        Position = layout.Position;
    }

    private void BuildColumnMap()
    {
        if (_recentQsoGrid is null || _recentQsoGrid.Columns.Count < 23)
        {
            return;
        }

        _columnMap = new Dictionary<RecentQsoGridColumn, DataGridColumn>
        {
            [RecentQsoGridColumn.Utc] = _recentQsoGrid.Columns[0],
            [RecentQsoGridColumn.Callsign] = _recentQsoGrid.Columns[1],
            [RecentQsoGridColumn.Band] = _recentQsoGrid.Columns[2],
            [RecentQsoGridColumn.Mode] = _recentQsoGrid.Columns[3],
            [RecentQsoGridColumn.Frequency] = _recentQsoGrid.Columns[4],
            [RecentQsoGridColumn.RstSent] = _recentQsoGrid.Columns[5],
            [RecentQsoGridColumn.RstReceived] = _recentQsoGrid.Columns[6],
            [RecentQsoGridColumn.Dxcc] = _recentQsoGrid.Columns[7],
            [RecentQsoGridColumn.Country] = _recentQsoGrid.Columns[8],
            [RecentQsoGridColumn.Name] = _recentQsoGrid.Columns[9],
            [RecentQsoGridColumn.Grid] = _recentQsoGrid.Columns[10],
            [RecentQsoGridColumn.Exchange] = _recentQsoGrid.Columns[11],
            [RecentQsoGridColumn.Contest] = _recentQsoGrid.Columns[12],
            [RecentQsoGridColumn.Station] = _recentQsoGrid.Columns[13],
            [RecentQsoGridColumn.Note] = _recentQsoGrid.Columns[14],
            [RecentQsoGridColumn.Comment] = _recentQsoGrid.Columns[15],
            [RecentQsoGridColumn.UtcEnd] = _recentQsoGrid.Columns[16],
            [RecentQsoGridColumn.CqZone] = _recentQsoGrid.Columns[17],
            [RecentQsoGridColumn.ItuZone] = _recentQsoGrid.Columns[18],
            [RecentQsoGridColumn.State] = _recentQsoGrid.Columns[19],
            [RecentQsoGridColumn.County] = _recentQsoGrid.Columns[20],
            [RecentQsoGridColumn.Sync] = _recentQsoGrid.Columns[21],
            [RecentQsoGridColumn.Continent] = _recentQsoGrid.Columns[22]
        };
    }

    private void SubscribeColumnOptions(RecentQsoListViewModel viewModel)
    {
        foreach (var option in viewModel.ColumnOptions)
        {
            option.PropertyChanged += OnColumnOptionPropertyChanged;
        }
    }

    private void UnsubscribeColumnOptions(RecentQsoListViewModel viewModel)
    {
        foreach (var option in viewModel.ColumnOptions)
        {
            option.PropertyChanged -= OnColumnOptionPropertyChanged;
        }
    }

    private void OnColumnOptionPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(RecentQsoColumnOptionViewModel.IsVisible)
            || sender is not RecentQsoColumnOptionViewModel option)
        {
            return;
        }

        ApplyColumnVisibility(option.Column, option.IsVisible);
    }

    private void ApplyDefaultColumnVisibility()
    {
        if (_viewModel is null)
        {
            return;
        }

        foreach (var option in _viewModel.RecentQsos.ColumnOptions)
        {
            ApplyColumnVisibility(option.Column, option.IsVisible);
        }
    }

    private void ApplyPersistedGridLayout()
    {
        if (IsInspectionMode || _gridLayoutApplied || _viewModel is null || _recentQsoGrid is null || _columnMap.Count == 0)
        {
            return;
        }

        ApplyDefaultColumnVisibility();

        var layout = _gridLayoutStore.Load();
        if (layout is null)
        {
            _gridLayoutApplied = true;
            return;
        }

        _viewModel.RecentQsos.ApplyPersistedGridFontSize(layout.GridFontSize);

        foreach (var columnState in layout.Columns.OrderBy(state => state.DisplayIndex))
        {
            if (!_columnMap.TryGetValue(columnState.Column, out var column))
            {
                continue;
            }

            _viewModel.RecentQsos.SetColumnVisibility(columnState.Column, columnState.IsVisible);
            column.DisplayIndex = columnState.DisplayIndex;
            if (columnState.Width > 0)
            {
                column.Width = new DataGridLength(columnState.Width);
            }
        }

        _viewModel.RecentQsos.ApplyPersistedSort(layout.SortColumn, layout.SortAscending);
        _gridLayoutApplied = true;
    }

    private void ApplyColumnVisibility(RecentQsoGridColumn column, bool isVisible)
    {
        if (_columnMap.TryGetValue(column, out var dataGridColumn))
        {
            dataGridColumn.IsVisible = isVisible;
        }
    }

    private void SaveGridLayout()
    {
        if (IsInspectionMode || _viewModel is null || _columnMap.Count == 0)
        {
            return;
        }

        var state = new RecentQsoGridLayoutState
        {
            SortColumn = _viewModel.RecentQsos.CurrentSortColumn,
            SortAscending = _viewModel.RecentQsos.SortAscending,
            GridFontSize = _viewModel.RecentQsos.GridFontSize
        };

        foreach (var pair in _columnMap.OrderBy(item => item.Value.DisplayIndex))
        {
            state.Columns.Add(
                new RecentQsoGridColumnState
                {
                    Column = pair.Key,
                    IsVisible = pair.Value.IsVisible,
                    DisplayIndex = pair.Value.DisplayIndex,
                    Width = pair.Value.ActualWidth
                });
        }

        _gridLayoutStore.Save(state);
    }

    private void SavePreferences()
    {
        if (IsInspectionMode || _viewModel is null)
        {
            return;
        }

        _preferencesStore.Save(_viewModel.CapturePreferences());
    }

    private void OnColumnLayoutResetRequested(object? sender, EventArgs e)
    {
        ResetGridLayout();
    }

    private void ResetGridLayout()
    {
        if (_viewModel is null || _recentQsoGrid is null || _columnMap.Count == 0)
        {
            return;
        }

        // Reset display indices to XAML declaration order.
        var xamlOrder = 0;
        foreach (var column in _recentQsoGrid.Columns)
        {
            column.DisplayIndex = xamlOrder++;
        }

        // Reset visibility and widths to defaults.
        _viewModel.RecentQsos.ResetColumnOptions();
        ApplyDefaultColumnVisibility();

        // Delete the persisted file so a fresh save captures clean state.
        _gridLayoutStore.Delete();
        _gridLayoutApplied = false;

        _recentQsoGrid.Focus();
    }

    /// <summary>
    /// Measures each column's header text at runtime and ensures both MinWidth
    /// and current Width are large enough to display the full header label at
    /// any DPI, font, or theme configuration. Accounts for sort-indicator glyph
    /// and cell padding overhead inside the DataGridColumnHeader.
    /// </summary>
    private void EnsureColumnHeadersFit()
    {
        if (_recentQsoGrid is null)
        {
            return;
        }

        // Sort glyph MinWidth (~32px from Fluent theme) + cell padding (2+2=4px)
        const double headerOverhead = 36;

        foreach (var column in _recentQsoGrid.Columns)
        {
            if (column.Header is not string headerText || string.IsNullOrEmpty(headerText))
            {
                continue;
            }

            var measure = new TextBlock
            {
                Text = headerText,
                FontWeight = FontWeight.SemiBold,
            };
            measure.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            double requiredWidth = Math.Ceiling(measure.DesiredSize.Width + headerOverhead);

            if (column.MinWidth < requiredWidth)
            {
                column.MinWidth = requiredWidth;
            }

            if (!column.Width.IsStar && column.Width.Value < requiredWidth)
            {
                column.Width = new DataGridLength(requiredWidth);
            }
        }
    }

    private static bool HandleZoomAction(Action handler, KeyEventArgs e)
    {
        handler();
        e.Handled = true;
        return true;
    }

    /// <summary>
    /// Handles keyboard input when the sort chooser or column chooser is open.
    /// </summary>
    private bool TryHandleChooserKey(KeyEventArgs e)
    {
        if (_viewModel!.IsSortChooserOpen)
        {
            return HandleSortChooserKey(e);
        }

        if (_viewModel!.IsColumnChooserOpen)
        {
            return HandleColumnChooserKey(e);
        }

        return false;
    }

    private bool HandleSortChooserKey(KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            CloseSortChooser();
            e.Handled = true;
            return true;
        }

        // Digit shortcuts: 1-9 select a sort column, 0 reverses direction.
        // Letter shortcuts A-G select additional columns.
        RecentQsoSortColumn? column = e.Key switch
        {
            Key.D1 or Key.NumPad1 => RecentQsoSortColumn.Utc,
            Key.D2 or Key.NumPad2 => RecentQsoSortColumn.Callsign,
            Key.D3 or Key.NumPad3 => RecentQsoSortColumn.Band,
            Key.D4 or Key.NumPad4 => RecentQsoSortColumn.Mode,
            Key.D5 or Key.NumPad5 => RecentQsoSortColumn.Frequency,
            Key.D6 or Key.NumPad6 => RecentQsoSortColumn.Dxcc,
            Key.D7 or Key.NumPad7 => RecentQsoSortColumn.Country,
            Key.D8 or Key.NumPad8 => RecentQsoSortColumn.Contest,
            Key.D9 or Key.NumPad9 => RecentQsoSortColumn.Note,
            Key.A => RecentQsoSortColumn.Comment,
            Key.B => RecentQsoSortColumn.Grid,
            Key.C => RecentQsoSortColumn.RstSent,
            Key.D => RecentQsoSortColumn.RstReceived,
            Key.E => RecentQsoSortColumn.Name,
            Key.F => RecentQsoSortColumn.Station,
            Key.G => RecentQsoSortColumn.Exchange,
            _ => null
        };

        if (column is not null)
        {
            _viewModel!.RecentQsos.ApplySort(column.Value);
            CloseSortChooser();
            e.Handled = true;
            return true;
        }

        if (e.Key is Key.D0 or Key.NumPad0)
        {
            _viewModel!.RecentQsos.ReverseCurrentSortDirection();
            CloseSortChooser();
            e.Handled = true;
            return true;
        }

        // Arrow keys navigate between sort buttons.
        if (e.Key is Key.Up or Key.Down)
        {
            NavigateFocusInPanel<Button>("SortChooserPanel", e.Key == Key.Down);
            e.Handled = true;
            return true;
        }

        return false;
    }

    private bool HandleColumnChooserKey(KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            CloseColumnChooser();
            e.Handled = true;
            return true;
        }

        if (e.Key is Key.Up or Key.Down)
        {
            NavigateFocusInPanel<CheckBox>("ColumnChooserPanel", e.Key == Key.Down);
            e.Handled = true;
            return true;
        }

        return false;
    }

    private void CloseSortChooser()
    {
        _viewModel!.IsSortChooserOpen = false;
        _recentQsoGrid?.Focus();
    }

    private void CloseColumnChooser()
    {
        _viewModel!.IsColumnChooserOpen = false;
        _recentQsoGrid?.Focus();
    }

    private void NavigateFocusInPanel<T>(string panelName, bool forward) where T : Control
    {
        var panel = this.FindControl<Border>(panelName);
        if (panel is null)
        {
            return;
        }

        var items = panel.GetVisualDescendants().OfType<T>().ToList();
        if (items.Count == 0)
        {
            return;
        }

        var focusedIndex = items.FindIndex(item => item.IsFocused);
        int nextIndex;

        if (forward)
        {
            nextIndex = focusedIndex < items.Count - 1 ? focusedIndex + 1 : 0;
        }
        else
        {
            nextIndex = focusedIndex > 0 ? focusedIndex - 1 : items.Count - 1;
        }

        items[nextIndex].Focus();
        items[nextIndex].BringIntoView();
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(MainWindowViewModel.IsSortChooserOpen)
            && _viewModel?.IsSortChooserOpen == true)
        {
            FocusFirstInPanel<Button>("SortChooserPanel");
        }
        else if (e.PropertyName == nameof(MainWindowViewModel.IsColumnChooserOpen)
            && _viewModel?.IsColumnChooserOpen == true)
        {
            FocusFirstInPanel<CheckBox>("ColumnChooserPanel");
        }
    }

    private void FocusFirstInPanel<T>(string panelName) where T : Control
    {
        Dispatcher.UIThread.Post(
            () =>
            {
                var panel = this.FindControl<Border>(panelName);
                var first = panel?.GetVisualDescendants().OfType<T>().FirstOrDefault();
                first?.Focus();
            },
            DispatcherPriority.Loaded);
    }
}
