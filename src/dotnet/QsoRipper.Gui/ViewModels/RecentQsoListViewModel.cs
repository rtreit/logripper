using System.Collections;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using Avalonia.Collections;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Grpc.Core;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class RecentQsoListViewModel : ObservableObject
{
    private const int DefaultLimit = 500;
    private const double DefaultGridFontSize = 12;
    private const double MinGridFontSize = 10;
    private const double MaxGridFontSize = 18;
    private const double GridFontSizeStep = 1;

    private readonly IEngineClient _engine;
    private readonly List<RecentQsoItemViewModel> _allItems = [];
    private readonly ObservableCollection<RecentQsoItemViewModel> _viewItems = [];
    private ParsedSearchQuery _parsedSearchQuery = ParsedSearchQuery.Empty;
    private bool _suppressSortStateSync;

    public RecentQsoListViewModel(IEngineClient engine)
    {
        _engine = engine;
        View = new DataGridCollectionView(_viewItems);
        View.Filter = FilterVisibleItem;
        View.SortDescriptions.CollectionChanged += OnSortDescriptionsChanged;
        ColumnOptions = new ObservableCollection<RecentQsoColumnOptionViewModel>(CreateColumnOptions());
        ApplyPersistedSort(RecentQsoSortColumn.Utc, ascending: false);
    }

    public DataGridCollectionView View { get; }

    public ObservableCollection<RecentQsoColumnOptionViewModel> ColumnOptions { get; }

    public ObservableCollection<string> ActiveFilterTokens { get; } = [];

    public IReadOnlyList<RecentQsoItemViewModel> VisibleItems => View.Cast<RecentQsoItemViewModel>().ToArray();

    public bool HasVisibleItems => VisibleItemCount > 0;

    public bool HasActiveFilterTokens => ActiveFilterTokens.Count > 0;

    public bool HasPendingEdits => PendingEditCount > 0;

    public string EmptyStateMessage
    {
        get
        {
            if (!HasLoaded)
            {
                return "Recent QSOs will appear here after setup completes.";
            }

            if (!string.IsNullOrWhiteSpace(ErrorMessage) && _allItems.Count == 0)
            {
                return ErrorMessage;
            }

            if (_parsedSearchQuery.HasTokens)
            {
                return "No recent QSOs match the current search.";
            }

            return "No QSOs have been logged yet.";
        }
    }

    public string RefreshIndicatorText => IsSaving
        ? "Saving"
        : IsLoading
            ? "Refreshing"
        : LastLoadedAtUtc is null
            ? "Not loaded"
            : $"Loaded {LastLoadedAtUtc.Value:HH:mm:ss}";

    public string CountStatusText => $"{_allItems.Count:N0} QSOs";

    public string FilterStatusText => _parsedSearchQuery.HasTokens
        ? $"{VisibleItemCount:N0} filtered"
        : "No filter";

    public string SortStatusText => $"Sort: {GetSortLabel(CurrentSortColumn)} {(SortAscending ? "asc" : "desc")}";

    public string EditStatusText => IsSaving
        ? "Saving edits"
        : HasPendingEdits
            ? $"{PendingEditCount:N0} unsaved"
            : "No pending edits";

    public string GridZoomStatusText => $"Zoom {Math.Round((GridFontSize / DefaultGridFontSize) * 100):0}%";

    public double GridRowHeight => Math.Round(19 * (GridFontSize / DefaultGridFontSize));

    public double GridHeaderHeight => Math.Round(21 * (GridFontSize / DefaultGridFontSize));

    public string SyncSummaryText => BuildSyncSummaryText();

    public string TopSyncIndicatorText => BuildTopSyncIndicatorText();

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    [NotifyPropertyChangedFor(nameof(RefreshIndicatorText))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(SaveEditsCommand))]
    [NotifyPropertyChangedFor(nameof(EditStatusText))]
    [NotifyPropertyChangedFor(nameof(RefreshIndicatorText))]
    private bool _isSaving;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    private string? _errorMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    private bool _hasLoaded;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(RefreshIndicatorText))]
    private DateTimeOffset? _lastLoadedAtUtc;

    [ObservableProperty]
    private string _searchText = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasVisibleItems))]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    [NotifyPropertyChangedFor(nameof(FilterStatusText))]
    private int _visibleItemCount;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(SaveEditsCommand))]
    [NotifyPropertyChangedFor(nameof(HasPendingEdits))]
    [NotifyPropertyChangedFor(nameof(EditStatusText))]
    private int _pendingEditCount;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(GridZoomStatusText))]
    [NotifyPropertyChangedFor(nameof(GridRowHeight))]
    [NotifyPropertyChangedFor(nameof(GridHeaderHeight))]
    private double _gridFontSize = DefaultGridFontSize;

    [ObservableProperty]
    private RecentQsoItemViewModel? _selectedQso;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SortStatusText))]
    private RecentQsoSortColumn _currentSortColumn = RecentQsoSortColumn.Utc;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SortStatusText))]
    private bool _sortAscending;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(TimestampFormatLabel))]
    private string _timestampFormat = TimestampFormatOption.Default.FormatString;

    public string TimestampFormatLabel =>
        TimestampFormatOption.FindOrDefault(TimestampFormat).Label;

    partial void OnSearchTextChanged(string value)
    {
        _parsedSearchQuery = ParseSearchQuery(value);
        UpdateFilterTokens();
        RefreshView();
    }

    partial void OnTimestampFormatChanged(string value)
    {
        RefreshTimestampDisplay();
    }

    [RelayCommand(CanExecute = nameof(CanRefresh))]
    internal async Task RefreshAsync()
    {
        IsLoading = true;
        ErrorMessage = null;

        try
        {
            var selectedLocalId = SelectedQso?.LocalId;
            var qsos = await _engine.ListRecentQsosAsync(DefaultLimit);

            var format = TimestampFormat;
            ReplaceItems(qsos.Select(q => RecentQsoItemViewModel.FromQso(q, format)));

            HasLoaded = true;
            LastLoadedAtUtc = DateTimeOffset.UtcNow;
            RefreshView(selectedLocalId);
        }
        catch (RpcException ex)
        {
            HasLoaded = true;
            ErrorMessage = string.IsNullOrWhiteSpace(ex.Status.Detail)
                ? $"Recent QSOs could not be loaded ({ex.StatusCode})."
                : ex.Status.Detail;
            RefreshView();
        }
        finally
        {
            IsLoading = false;
            NotifyStatusPropertiesChanged();
        }
    }

    internal static bool MatchesSearch(RecentQsoItemViewModel item, string? searchText)
    {
        ArgumentNullException.ThrowIfNull(item);

        return MatchesSearch(item, ParseSearchQuery(searchText));
    }

    [RelayCommand]
    private void ReverseSortDirection()
    {
        ReverseCurrentSortDirection();
    }

    [RelayCommand]
    private void SortByColumn(RecentQsoSortColumn column)
    {
        ApplySort(column);
    }

    [RelayCommand(CanExecute = nameof(CanSaveEdits))]
    internal async Task SaveEditsAsync()
    {
        if (!HasPendingEdits)
        {
            return;
        }

        IsSaving = true;
        ErrorMessage = null;

        try
        {
            var dirtyItems = _allItems.Where(item => item.IsDirty).ToArray();
            foreach (var item in dirtyItems)
            {
                if (!item.TryBuildUpdatedQso(out var qso, out var validationError))
                {
                    SelectedQso = item;
                    ErrorMessage = validationError;
                    return;
                }

                var response = await _engine.UpdateQsoAsync(qso!);
                if (!response.Success)
                {
                    SelectedQso = item;
                    ErrorMessage = string.IsNullOrWhiteSpace(response.Error)
                        ? $"Update failed for {item.WorkedCallsign}."
                        : response.Error;
                    return;
                }

                item.AcceptSavedChanges(qso!);
            }

            await RefreshAsync();
        }
        catch (RpcException ex)
        {
            ErrorMessage = string.IsNullOrWhiteSpace(ex.Status.Detail)
                ? $"QSO edits could not be saved ({ex.StatusCode})."
                : ex.Status.Detail;
        }
        finally
        {
            IsSaving = false;
            UpdatePendingEditCount();
            NotifyStatusPropertiesChanged();
        }
    }

    [ObservableProperty]
    private bool _isDeletePending;

    [ObservableProperty]
    private string _deleteConfirmCallsign = string.Empty;

    private string? _pendingDeleteLocalId;

    internal void RequestDeleteSelectedQso()
    {
        var selected = SelectedQso;
        if (selected is null || string.IsNullOrWhiteSpace(selected.LocalId))
        {
            return;
        }

        _pendingDeleteLocalId = selected.LocalId;
        DeleteConfirmCallsign = selected.WorkedCallsign;
        IsDeletePending = true;
    }

    [RelayCommand]
    internal async Task ConfirmDeleteAsync()
    {
        if (_pendingDeleteLocalId is null)
        {
            return;
        }

        var localId = _pendingDeleteLocalId;
        IsDeletePending = false;
        _pendingDeleteLocalId = null;
        DeleteConfirmCallsign = string.Empty;

        try
        {
            var response = await _engine.DeleteQsoAsync(localId);
            if (response.Success)
            {
                await RefreshAsync();
            }
            else
            {
                ErrorMessage = response.Error ?? "Delete failed.";
                NotifyStatusPropertiesChanged();
            }
        }
        catch (RpcException ex)
        {
            ErrorMessage = string.IsNullOrWhiteSpace(ex.Status.Detail)
                ? $"Delete failed ({ex.StatusCode})."
                : ex.Status.Detail;
            NotifyStatusPropertiesChanged();
        }
    }

    [RelayCommand]
    private void CancelDelete()
    {
        IsDeletePending = false;
        _pendingDeleteLocalId = null;
        DeleteConfirmCallsign = string.Empty;
    }

    [RelayCommand]
    private void ZoomIn()
    {
        ZoomInGrid();
    }

    [RelayCommand]
    private void ZoomOut()
    {
        ZoomOutGrid();
    }

    [RelayCommand]
    private void ResetZoom()
    {
        ResetGridZoom();
    }

    [RelayCommand]
    private void CycleTimestampFormat()
    {
        TimestampFormat = TimestampFormatOption.CycleNext(TimestampFormat).FormatString;
    }

    internal void ApplyPersistedSort(RecentQsoSortColumn column, bool ascending)
    {
        CurrentSortColumn = column;
        SortAscending = ascending;
        ApplySortDescriptions(column, ascending);
    }

    internal void ApplyPersistedGridFontSize(double fontSize)
    {
        if (fontSize <= 0)
        {
            return;
        }

        GridFontSize = Math.Clamp(fontSize, MinGridFontSize, MaxGridFontSize);
    }

    internal void ApplyPersistedTimestampFormat(string? format)
    {
        TimestampFormat = TimestampFormatOption.FindOrDefault(format).FormatString;
    }

    internal void ApplySort(RecentQsoSortColumn column)
    {
        if (CurrentSortColumn == column)
        {
            SortAscending = !SortAscending;
        }
        else
        {
            CurrentSortColumn = column;
            SortAscending = GetDefaultSortAscending(column);
        }

        ApplySortDescriptions(CurrentSortColumn, SortAscending);
        RefreshView();
    }

    internal void ReverseCurrentSortDirection()
    {
        SortAscending = !SortAscending;
        ApplySortDescriptions(CurrentSortColumn, SortAscending);
        RefreshView();
    }

    internal void ZoomInGrid()
    {
        AdjustZoom(1);
    }

    internal void ZoomOutGrid()
    {
        AdjustZoom(-1);
    }

    internal void ResetGridZoom()
    {
        GridFontSize = DefaultGridFontSize;
    }

    internal bool AdjustZoom(int direction)
    {
        var nextFontSize = Math.Clamp(
            GridFontSize + (direction * GridFontSizeStep),
            MinGridFontSize,
            MaxGridFontSize);

        if (Math.Abs(nextFontSize - GridFontSize) < 0.01)
        {
            return false;
        }

        GridFontSize = nextFontSize;
        return true;
    }

    internal void SetColumnVisibility(RecentQsoGridColumn column, bool isVisible)
    {
        var option = ColumnOptions.FirstOrDefault(item => item.Column == column);
        if (option is not null)
        {
            option.IsVisible = isVisible;
        }
    }

    private static bool MatchesSearch(RecentQsoItemViewModel item, ParsedSearchQuery query)
    {
        if (!query.HasTokens)
        {
            return true;
        }

        return query.FreeTextTokens.All(token => item.SearchDocument.Contains(token, StringComparison.Ordinal))
            && query.FieldTokens.All(token => item.MatchesFieldToken(token.Key, token.Value));
    }

    private bool CanRefresh() => !IsLoading && !IsSaving && !HasPendingEdits;

    private bool CanSaveEdits() => !IsLoading && !IsSaving && HasPendingEdits;

    private bool FilterVisibleItem(object item) =>
        item is RecentQsoItemViewModel recentQso && MatchesSearch(recentQso, _parsedSearchQuery);

    private void RefreshView(string? preferredLocalId = null)
    {
        View.Refresh();
        UpdateVisibleSelectionAndCounts(preferredLocalId);
        NotifyStatusPropertiesChanged();
    }

    private static bool GetDefaultSortAscending(RecentQsoSortColumn column) => column switch
    {
        RecentQsoSortColumn.Utc => false,
        RecentQsoSortColumn.UtcEnd => false,
        _ => true
    };

    private static string GetSortLabel(RecentQsoSortColumn column) => column switch
    {
        RecentQsoSortColumn.Utc => "UTC time",
        RecentQsoSortColumn.Callsign => "callsign",
        RecentQsoSortColumn.Band => "band",
        RecentQsoSortColumn.Mode => "mode",
        RecentQsoSortColumn.Frequency => "frequency",
        RecentQsoSortColumn.Rst => "RST",
        RecentQsoSortColumn.RstSent => "RST sent",
        RecentQsoSortColumn.RstReceived => "RST rcvd",
        RecentQsoSortColumn.Dxcc => "DXCC",
        RecentQsoSortColumn.Country => "country",
        RecentQsoSortColumn.Name => "name",
        RecentQsoSortColumn.Grid => "grid",
        RecentQsoSortColumn.Exchange => "exchange",
        RecentQsoSortColumn.Contest => "contest",
        RecentQsoSortColumn.Station => "station",
        RecentQsoSortColumn.Note => "note",
        RecentQsoSortColumn.UtcEnd => "end time",
        RecentQsoSortColumn.CqZone => "CQ zone",
        RecentQsoSortColumn.ItuZone => "ITU zone",
        RecentQsoSortColumn.Qth => "QTH",
        RecentQsoSortColumn.State => "state",
        RecentQsoSortColumn.County => "county",
        RecentQsoSortColumn.Sync => "sync",
        _ => "recent QSOs"
    };

    private void ApplySortDescriptions(RecentQsoSortColumn column, bool ascending)
    {
        var primaryDirection = ascending ? ListSortDirection.Ascending : ListSortDirection.Descending;
        _suppressSortStateSync = true;
        try
        {
            View.SortDescriptions.Clear();
            View.SortDescriptions.Add(
                new DataGridComparerSortDescription(
                    new PropertyPathComparer(GetSortMemberPath(column)),
                    primaryDirection));

            if (column != RecentQsoSortColumn.Utc)
            {
                View.SortDescriptions.Add(
                    new DataGridComparerSortDescription(
                        new PropertyPathComparer(nameof(RecentQsoItemViewModel.UtcSortKey)),
                        ListSortDirection.Descending));
            }
        }
        finally
        {
            _suppressSortStateSync = false;
        }
    }

    private void UpdateVisibleSelectionAndCounts(string? preferredLocalId = null)
    {
        var selectedLocalId = preferredLocalId ?? SelectedQso?.LocalId;
        var visibleItems = VisibleItems;
        VisibleItemCount = visibleItems.Count;
        SelectedQso = visibleItems.FirstOrDefault(item => string.Equals(item.LocalId, selectedLocalId, StringComparison.Ordinal))
            ?? (visibleItems.Count > 0 ? visibleItems[0] : null);
    }

    private void NotifyStatusPropertiesChanged()
    {
        OnPropertyChanged(nameof(CountStatusText));
        OnPropertyChanged(nameof(FilterStatusText));
        OnPropertyChanged(nameof(SortStatusText));
        OnPropertyChanged(nameof(EditStatusText));
        OnPropertyChanged(nameof(SyncSummaryText));
        OnPropertyChanged(nameof(TopSyncIndicatorText));
        OnPropertyChanged(nameof(RefreshIndicatorText));
        OnPropertyChanged(nameof(HasActiveFilterTokens));
    }

    private void UpdateFilterTokens()
    {
        ActiveFilterTokens.Clear();
        foreach (var token in _parsedSearchQuery.DisplayTokens)
        {
            ActiveFilterTokens.Add(token);
        }

        OnPropertyChanged(nameof(HasActiveFilterTokens));
    }

    private void OnSortDescriptionsChanged(object? sender, EventArgs e)
    {
        if (_suppressSortStateSync)
        {
            return;
        }

        SyncSortStateFromView();
        NotifyStatusPropertiesChanged();
    }

    private void ReplaceItems(IEnumerable<RecentQsoItemViewModel> items)
    {
        foreach (var item in _allItems)
        {
            item.PropertyChanged -= OnItemPropertyChanged;
        }

        _allItems.Clear();
        _allItems.AddRange(items);

        _viewItems.Clear();
        foreach (var item in _allItems)
        {
            item.PropertyChanged += OnItemPropertyChanged;
            _viewItems.Add(item);
        }

        UpdatePendingEditCount();
    }

    private void RefreshTimestampDisplay()
    {
        var format = TimestampFormat;
        foreach (var item in _allItems)
        {
            item.UpdateTimestampFormat(format);
        }
    }

    private void OnItemPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(RecentQsoItemViewModel.IsDirty))
        {
            return;
        }

        UpdatePendingEditCount();
        NotifyStatusPropertiesChanged();
    }

    private void UpdatePendingEditCount()
    {
        PendingEditCount = _allItems.Count(item => item.IsDirty);
    }

    private void SyncSortStateFromView()
    {
        if (View.SortDescriptions.Count == 0)
        {
            CurrentSortColumn = RecentQsoSortColumn.Utc;
            SortAscending = false;
            return;
        }

        var primarySort = View.SortDescriptions[0];
        CurrentSortColumn = GetSortColumn(primarySort.PropertyPath);
        SortAscending = primarySort.Direction == ListSortDirection.Ascending;
    }

    private string BuildSyncSummaryText()
    {
        if (_allItems.Count == 0)
        {
            return "Sync: idle";
        }

        var localOnly = _allItems.Count(item => string.Equals(item.SyncStatus, "Local", StringComparison.Ordinal));
        var modified = _allItems.Count(item => string.Equals(item.SyncStatus, "Modified", StringComparison.Ordinal));
        var conflict = _allItems.Count(item => string.Equals(item.SyncStatus, "Conflict", StringComparison.Ordinal));
        var synced = _allItems.Count(item => string.Equals(item.SyncStatus, "Synced", StringComparison.Ordinal));

        return $"Sync: {localOnly} local | {modified} modified | {conflict} conflict | {synced} synced";
    }

    private string BuildTopSyncIndicatorText()
    {
        if (_allItems.Count == 0)
        {
            return "Sync idle";
        }

        if (_allItems.Any(item => string.Equals(item.SyncStatus, "Conflict", StringComparison.Ordinal)))
        {
            return "Sync conflict";
        }

        if (_allItems.Any(item => string.Equals(item.SyncStatus, "Modified", StringComparison.Ordinal)))
        {
            return "Sync modified";
        }

        if (_allItems.Any(item => string.Equals(item.SyncStatus, "Local", StringComparison.Ordinal)))
        {
            return "Sync local";
        }

        return "Sync ready";
    }

    private static string GetSortMemberPath(RecentQsoSortColumn column) => column switch
    {
        RecentQsoSortColumn.Utc => nameof(RecentQsoItemViewModel.UtcSortKey),
        RecentQsoSortColumn.Callsign => nameof(RecentQsoItemViewModel.WorkedCallsign),
        RecentQsoSortColumn.Band => nameof(RecentQsoItemViewModel.Band),
        RecentQsoSortColumn.Mode => nameof(RecentQsoItemViewModel.Mode),
        RecentQsoSortColumn.Frequency => nameof(RecentQsoItemViewModel.FrequencySortKey),
        RecentQsoSortColumn.Rst => nameof(RecentQsoItemViewModel.Rst),
        RecentQsoSortColumn.RstSent => nameof(RecentQsoItemViewModel.RstSent),
        RecentQsoSortColumn.RstReceived => nameof(RecentQsoItemViewModel.RstReceived),
        RecentQsoSortColumn.Dxcc => nameof(RecentQsoItemViewModel.DxccSortKey),
        RecentQsoSortColumn.Country => nameof(RecentQsoItemViewModel.Country),
        RecentQsoSortColumn.Name => nameof(RecentQsoItemViewModel.OperatorName),
        RecentQsoSortColumn.Grid => nameof(RecentQsoItemViewModel.Grid),
        RecentQsoSortColumn.Exchange => nameof(RecentQsoItemViewModel.Exchange),
        RecentQsoSortColumn.Contest => nameof(RecentQsoItemViewModel.Contest),
        RecentQsoSortColumn.Station => nameof(RecentQsoItemViewModel.Station),
        RecentQsoSortColumn.Note => nameof(RecentQsoItemViewModel.Note),
        RecentQsoSortColumn.UtcEnd => nameof(RecentQsoItemViewModel.UtcEndSortKey),
        RecentQsoSortColumn.CqZone => nameof(RecentQsoItemViewModel.CqZone),
        RecentQsoSortColumn.ItuZone => nameof(RecentQsoItemViewModel.ItuZone),
        RecentQsoSortColumn.Qth => nameof(RecentQsoItemViewModel.Qth),
        RecentQsoSortColumn.State => nameof(RecentQsoItemViewModel.State),
        RecentQsoSortColumn.County => nameof(RecentQsoItemViewModel.County),
        RecentQsoSortColumn.Comment => nameof(RecentQsoItemViewModel.Comment),
        RecentQsoSortColumn.Sync => nameof(RecentQsoItemViewModel.SyncStatus),
        _ => nameof(RecentQsoItemViewModel.UtcSortKey)
    };

    private static RecentQsoSortColumn GetSortColumn(string memberPath) => memberPath switch
    {
        nameof(RecentQsoItemViewModel.UtcSortKey) => RecentQsoSortColumn.Utc,
        nameof(RecentQsoItemViewModel.WorkedCallsign) => RecentQsoSortColumn.Callsign,
        nameof(RecentQsoItemViewModel.Band) => RecentQsoSortColumn.Band,
        nameof(RecentQsoItemViewModel.Mode) => RecentQsoSortColumn.Mode,
        nameof(RecentQsoItemViewModel.FrequencySortKey) => RecentQsoSortColumn.Frequency,
        nameof(RecentQsoItemViewModel.Rst) => RecentQsoSortColumn.Rst,
        nameof(RecentQsoItemViewModel.RstSent) => RecentQsoSortColumn.RstSent,
        nameof(RecentQsoItemViewModel.RstReceived) => RecentQsoSortColumn.RstReceived,
        nameof(RecentQsoItemViewModel.DxccSortKey) => RecentQsoSortColumn.Dxcc,
        nameof(RecentQsoItemViewModel.Country) => RecentQsoSortColumn.Country,
        nameof(RecentQsoItemViewModel.OperatorName) => RecentQsoSortColumn.Name,
        nameof(RecentQsoItemViewModel.Grid) => RecentQsoSortColumn.Grid,
        nameof(RecentQsoItemViewModel.Exchange) => RecentQsoSortColumn.Exchange,
        nameof(RecentQsoItemViewModel.Contest) => RecentQsoSortColumn.Contest,
        nameof(RecentQsoItemViewModel.Station) => RecentQsoSortColumn.Station,
        nameof(RecentQsoItemViewModel.Note) => RecentQsoSortColumn.Note,
        nameof(RecentQsoItemViewModel.UtcEndSortKey) => RecentQsoSortColumn.UtcEnd,
        nameof(RecentQsoItemViewModel.CqZone) => RecentQsoSortColumn.CqZone,
        nameof(RecentQsoItemViewModel.ItuZone) => RecentQsoSortColumn.ItuZone,
        nameof(RecentQsoItemViewModel.Qth) => RecentQsoSortColumn.Qth,
        nameof(RecentQsoItemViewModel.State) => RecentQsoSortColumn.State,
        nameof(RecentQsoItemViewModel.County) => RecentQsoSortColumn.County,
        nameof(RecentQsoItemViewModel.Comment) => RecentQsoSortColumn.Comment,
        nameof(RecentQsoItemViewModel.SyncStatus) => RecentQsoSortColumn.Sync,
        _ => RecentQsoSortColumn.Utc
    };

    private static ParsedSearchQuery ParseSearchQuery(string? searchText)
    {
        if (string.IsNullOrWhiteSpace(searchText))
        {
            return ParsedSearchQuery.Empty;
        }

        var freeTextTokens = new List<string>();
        var fieldTokens = new List<SearchToken>();
        var displayTokens = new List<string>();

        foreach (var rawToken in searchText.Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var separatorIndex = rawToken.IndexOf(':', StringComparison.Ordinal);
            if (separatorIndex > 0 && separatorIndex < rawToken.Length - 1)
            {
                var key = rawToken[..separatorIndex].Trim().ToUpperInvariant();
                var value = rawToken[(separatorIndex + 1)..].Trim().ToUpperInvariant();
                fieldTokens.Add(new SearchToken(key, value));
                displayTokens.Add(rawToken.Trim());
            }
            else
            {
                freeTextTokens.Add(rawToken.ToUpperInvariant());
            }
        }

        return new ParsedSearchQuery(
            freeTextTokens.ToArray(),
            fieldTokens.ToArray(),
            displayTokens.ToArray());
    }

    private static IEnumerable<RecentQsoColumnOptionViewModel> CreateColumnOptions()
    {
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Utc, "UTC", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Callsign, "Call", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Band, "Band", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Mode, "Mode", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Frequency, "Freq", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.RstSent, "S", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.RstReceived, "R", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Dxcc, "DXCC", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Country, "Country", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Name, "Name", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Grid, "Grid", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Exchange, "Exch", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Contest, "Contest", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Station, "Station", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Note, "Note", true);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Comment, "Comment", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.UtcEnd, "End", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.CqZone, "CQ", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.ItuZone, "ITU", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.State, "State", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.County, "County", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Sync, "Sync", false);
        yield return new RecentQsoColumnOptionViewModel(RecentQsoGridColumn.Continent, "Cont", false);
    }

    private readonly record struct SearchToken(string Key, string Value);

    private sealed record ParsedSearchQuery(
        string[] FreeTextTokens,
        SearchToken[] FieldTokens,
        string[] DisplayTokens)
    {
        public static readonly ParsedSearchQuery Empty = new([], [], []);

        public bool HasTokens => FreeTextTokens.Length > 0 || FieldTokens.Length > 0;
    }

    private sealed class PropertyPathComparer(string propertyPath) : IComparer
    {
        public int Compare(object? x, object? y)
        {
            var left = GetComparableValue(x);
            var right = GetComparableValue(y);

            if (left is null && right is null)
            {
                return 0;
            }

            if (left is null)
            {
                return -1;
            }

            if (right is null)
            {
                return 1;
            }

            return left switch
            {
                string leftString when right is string rightString => StringComparer.OrdinalIgnoreCase.Compare(leftString, rightString),
                IComparable comparable => comparable.CompareTo(right),
                _ => StringComparer.OrdinalIgnoreCase.Compare(left.ToString(), right.ToString())
            };
        }

        private object? GetComparableValue(object? candidate)
        {
            var item = candidate as RecentQsoItemViewModel;
            return item switch
            {
                null => null,
                _ when propertyPath == nameof(RecentQsoItemViewModel.UtcSortKey) => item.UtcSortKey,
                _ when propertyPath == nameof(RecentQsoItemViewModel.WorkedCallsign) => item.WorkedCallsign,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Band) => item.Band,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Mode) => item.Mode,
                _ when propertyPath == nameof(RecentQsoItemViewModel.FrequencySortKey) => item.FrequencySortKey,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Rst) => item.Rst,
                _ when propertyPath == nameof(RecentQsoItemViewModel.RstSent) => item.RstSent,
                _ when propertyPath == nameof(RecentQsoItemViewModel.RstReceived) => item.RstReceived,
                _ when propertyPath == nameof(RecentQsoItemViewModel.DxccSortKey) => item.DxccSortKey,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Country) => item.Country,
                _ when propertyPath == nameof(RecentQsoItemViewModel.OperatorName) => item.OperatorName,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Grid) => item.Grid,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Exchange) => item.Exchange,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Contest) => item.Contest,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Station) => item.Station,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Note) => item.Note,
                _ when propertyPath == nameof(RecentQsoItemViewModel.UtcEndSortKey) => item.UtcEndSortKey,
                _ when propertyPath == nameof(RecentQsoItemViewModel.CqZone) => item.CqZone,
                _ when propertyPath == nameof(RecentQsoItemViewModel.ItuZone) => item.ItuZone,
                _ when propertyPath == nameof(RecentQsoItemViewModel.Qth) => item.Qth,
                _ when propertyPath == nameof(RecentQsoItemViewModel.State) => item.State,
                _ when propertyPath == nameof(RecentQsoItemViewModel.County) => item.County,
                _ when propertyPath == nameof(RecentQsoItemViewModel.SyncStatus) => item.SyncStatus,
                _ => null
            };
        }
    }
}
