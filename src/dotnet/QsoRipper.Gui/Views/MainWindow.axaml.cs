using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;
using QsoRipper.Gui.Utilities;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Views;

internal sealed partial class MainWindow : Window
{
    private readonly RecentQsoGridLayoutStore _gridLayoutStore = new();
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
        DataContextChanged += OnDataContextChanged;
        BuildColumnMap();
    }

    protected override async void OnOpened(EventArgs e)
    {
        base.OnOpened(e);
        ClampToCurrentScreen();
        if (!IsInspectionMode)
        {
            PrimeMenuAccessKeys();
            ApplyPersistedGridLayout();
            if (DataContext is MainWindowViewModel vm)
            {
                await vm.CheckFirstRunAsync();
            }
        }
    }

    protected override void OnClosed(EventArgs e)
    {
        SaveGridLayout();

        if (_viewModel is not null)
        {
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
            _viewModel.SettingsRequested -= OnSettingsRequested;
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

        if (TryHandleRecentQsoZoomKey(e))
        {
            return;
        }

        if (HandleRecentQsoGridKeyDown(e))
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
            if (_viewModel.IsCallsignCardOpen)
            {
                _viewModel.CloseCallsignCardCommand.Execute(null);
                e.Handled = true;
                return;
            }

            if (_viewModel.IsColumnChooserOpen || _viewModel.IsSortChooserOpen)
            {
                _viewModel.CloseTransientPanelsCommand.Execute(null);
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
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
            _viewModel.SettingsRequested -= OnSettingsRequested;
            UnsubscribeColumnOptions(_viewModel.RecentQsos);
        }

        _viewModel = DataContext as MainWindowViewModel;
        if (_viewModel is not null)
        {
            _viewModel.SearchFocusRequested += OnSearchFocusRequested;
            _viewModel.SettingsRequested += OnSettingsRequested;
            SubscribeColumnOptions(_viewModel.RecentQsos);
            ApplyDefaultColumnVisibility();
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

    private async void OnSettingsRequested(object? sender, EventArgs e)
    {
        await ShowSettingsDialogAsync();
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
        if (_recentQsoGrid is null || _recentQsoGrid.Columns.Count < 19)
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
            [RecentQsoGridColumn.Rst] = _recentQsoGrid.Columns[5],
            [RecentQsoGridColumn.Dxcc] = _recentQsoGrid.Columns[6],
            [RecentQsoGridColumn.Country] = _recentQsoGrid.Columns[7],
            [RecentQsoGridColumn.Name] = _recentQsoGrid.Columns[8],
            [RecentQsoGridColumn.Grid] = _recentQsoGrid.Columns[9],
            [RecentQsoGridColumn.Exchange] = _recentQsoGrid.Columns[10],
            [RecentQsoGridColumn.Contest] = _recentQsoGrid.Columns[11],
            [RecentQsoGridColumn.Station] = _recentQsoGrid.Columns[12],
            [RecentQsoGridColumn.Note] = _recentQsoGrid.Columns[13],
            [RecentQsoGridColumn.UtcEnd] = _recentQsoGrid.Columns[14],
            [RecentQsoGridColumn.CqZone] = _recentQsoGrid.Columns[15],
            [RecentQsoGridColumn.ItuZone] = _recentQsoGrid.Columns[16],
            [RecentQsoGridColumn.Qth] = _recentQsoGrid.Columns[17],
            [RecentQsoGridColumn.Sync] = _recentQsoGrid.Columns[18]
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

    private static bool HandleZoomAction(Action handler, KeyEventArgs e)
    {
        handler();
        e.Handled = true;
        return true;
    }
}
