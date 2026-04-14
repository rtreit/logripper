using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Grpc.Core;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class RecentQsoListViewModel : ObservableObject
{
    private const int DefaultLimit = 500;

    private readonly IEngineClient _engine;
    private readonly List<RecentQsoItemViewModel> _allItems = [];

    public RecentQsoListViewModel(IEngineClient engine)
    {
        _engine = engine;
    }

    public ObservableCollection<RecentQsoItemViewModel> VisibleItems { get; } = [];

    public bool HasVisibleItems => VisibleItems.Count > 0;

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

            if (!string.IsNullOrWhiteSpace(SearchText))
            {
                return "No recent QSOs match the current search.";
            }

            return "No QSOs have been logged yet.";
        }
    }

    public string LastLoadedText => LastLoadedAtUtc is null
        ? "Not loaded yet"
        : $"Updated {LastLoadedAtUtc.Value:HH:mm:ss} UTC";

    public string SortStatusText => $"Sorted by {GetSortLabel(CurrentSortColumn)} ({(SortAscending ? "ascending" : "descending")})";

    public string UtcHeaderText => BuildHeaderText("UTC", RecentQsoSortColumn.Utc);

    public string CallsignHeaderText => BuildHeaderText("Call", RecentQsoSortColumn.Callsign);

    public string BandHeaderText => BuildHeaderText("Band", RecentQsoSortColumn.Band);

    public string ModeHeaderText => BuildHeaderText("Mode", RecentQsoSortColumn.Mode);

    public string CountryHeaderText => BuildHeaderText("Country", RecentQsoSortColumn.Country);

    public string NameHeaderText => BuildHeaderText("Name", RecentQsoSortColumn.Name);

    public string FrequencyHeaderText => BuildHeaderText("Freq", RecentQsoSortColumn.Frequency);

    public string RstSentHeaderText => BuildHeaderText("S", RecentQsoSortColumn.RstSent);

    public string RstReceivedHeaderText => BuildHeaderText("R", RecentQsoSortColumn.RstReceived);

    public string GridHeaderText => BuildHeaderText("Grid", RecentQsoSortColumn.Grid);

    public string CommentHeaderText => BuildHeaderText("Comment", RecentQsoSortColumn.Comment);

    public string UtcEndHeaderText => BuildHeaderText("End", RecentQsoSortColumn.UtcEnd);

    public string SyncHeaderText => BuildHeaderText("Sync", RecentQsoSortColumn.Sync);

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    private string? _errorMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EmptyStateMessage))]
    private bool _hasLoaded;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(LastLoadedText))]
    private DateTimeOffset? _lastLoadedAtUtc;

    [ObservableProperty]
    private string _searchText = string.Empty;

    [ObservableProperty]
    private string _summaryText = "Recent QSOs will appear here after setup completes.";

    [ObservableProperty]
    private RecentQsoItemViewModel? _selectedQso;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SortStatusText))]
    [NotifyPropertyChangedFor(nameof(UtcHeaderText))]
    [NotifyPropertyChangedFor(nameof(CallsignHeaderText))]
    [NotifyPropertyChangedFor(nameof(BandHeaderText))]
    [NotifyPropertyChangedFor(nameof(ModeHeaderText))]
    [NotifyPropertyChangedFor(nameof(CountryHeaderText))]
    [NotifyPropertyChangedFor(nameof(NameHeaderText))]
    [NotifyPropertyChangedFor(nameof(FrequencyHeaderText))]
    [NotifyPropertyChangedFor(nameof(RstSentHeaderText))]
    [NotifyPropertyChangedFor(nameof(RstReceivedHeaderText))]
    [NotifyPropertyChangedFor(nameof(GridHeaderText))]
    [NotifyPropertyChangedFor(nameof(CommentHeaderText))]
    [NotifyPropertyChangedFor(nameof(UtcEndHeaderText))]
    [NotifyPropertyChangedFor(nameof(SyncHeaderText))]
    private RecentQsoSortColumn _currentSortColumn = RecentQsoSortColumn.Utc;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SortStatusText))]
    [NotifyPropertyChangedFor(nameof(UtcHeaderText))]
    [NotifyPropertyChangedFor(nameof(CallsignHeaderText))]
    [NotifyPropertyChangedFor(nameof(BandHeaderText))]
    [NotifyPropertyChangedFor(nameof(ModeHeaderText))]
    [NotifyPropertyChangedFor(nameof(CountryHeaderText))]
    [NotifyPropertyChangedFor(nameof(NameHeaderText))]
    [NotifyPropertyChangedFor(nameof(FrequencyHeaderText))]
    [NotifyPropertyChangedFor(nameof(RstSentHeaderText))]
    [NotifyPropertyChangedFor(nameof(RstReceivedHeaderText))]
    [NotifyPropertyChangedFor(nameof(GridHeaderText))]
    [NotifyPropertyChangedFor(nameof(CommentHeaderText))]
    [NotifyPropertyChangedFor(nameof(UtcEndHeaderText))]
    [NotifyPropertyChangedFor(nameof(SyncHeaderText))]
    private bool _sortAscending;

    partial void OnSearchTextChanged(string value)
    {
        ApplyFilter();
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

            _allItems.Clear();
            _allItems.AddRange(qsos.Select(RecentQsoItemViewModel.FromQso));

            HasLoaded = true;
            LastLoadedAtUtc = DateTimeOffset.UtcNow;
            ApplyFilter(selectedLocalId);
        }
        catch (RpcException ex)
        {
            HasLoaded = true;
            ErrorMessage = string.IsNullOrWhiteSpace(ex.Status.Detail)
                ? $"Recent QSOs could not be loaded ({ex.StatusCode})."
                : ex.Status.Detail;
            ApplyFilter();
        }
        finally
        {
            IsLoading = false;
        }
    }

    internal static bool MatchesSearch(RecentQsoItemViewModel item, string? searchText)
    {
        ArgumentNullException.ThrowIfNull(item);

        if (string.IsNullOrWhiteSpace(searchText))
        {
            return true;
        }

        var searchTokens = searchText
            .Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .Select(static token => token.ToUpperInvariant())
            .ToArray();

        return searchTokens.All(token => item.SearchDocument.Contains(token, StringComparison.Ordinal));
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

        ApplyFilter();
    }

    internal void ReverseCurrentSortDirection()
    {
        SortAscending = !SortAscending;
        ApplyFilter();
    }

    private bool CanRefresh() => !IsLoading;

    private void ApplyFilter(string? preferredLocalId = null)
    {
        var selectedLocalId = preferredLocalId ?? SelectedQso?.LocalId;
        var filteredItems = SortItems(_allItems.Where(item => MatchesSearch(item, SearchText))).ToArray();

        VisibleItems.Clear();
        foreach (var item in filteredItems)
        {
            VisibleItems.Add(item);
        }

        SelectedQso = filteredItems.FirstOrDefault(item => string.Equals(item.LocalId, selectedLocalId, StringComparison.Ordinal))
            ?? filteredItems.FirstOrDefault();

        SummaryText = BuildSummaryText(filteredItems.Length);
        OnPropertyChanged(nameof(HasVisibleItems));
        OnPropertyChanged(nameof(EmptyStateMessage));
    }

    private string BuildHeaderText(string label, RecentQsoSortColumn column)
    {
        return CurrentSortColumn == column
            ? $"{label} {(SortAscending ? '\u2191' : '\u2193')}"
            : label;
    }

    private string BuildSummaryText(int filteredCount)
    {
        if (!HasLoaded)
        {
            return "Recent QSOs will appear here after setup completes.";
        }

        if (!string.IsNullOrWhiteSpace(ErrorMessage) && _allItems.Count == 0)
        {
            return "Recent QSOs unavailable";
        }

        if (_allItems.Count == 0)
        {
            return "No QSOs logged yet";
        }

        return filteredCount == _allItems.Count
            ? $"Showing {_allItems.Count} recent QSOs"
            : $"Showing {filteredCount} of {_allItems.Count} recent QSOs";
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
        RecentQsoSortColumn.Country => "country",
        RecentQsoSortColumn.Name => "name",
        RecentQsoSortColumn.Frequency => "frequency",
        RecentQsoSortColumn.RstSent => "RST sent",
        RecentQsoSortColumn.RstReceived => "RST received",
        RecentQsoSortColumn.Grid => "grid",
        RecentQsoSortColumn.Comment => "comment",
        RecentQsoSortColumn.UtcEnd => "end time",
        RecentQsoSortColumn.Sync => "sync",
        _ => "recent QSOs"
    };

    private IEnumerable<RecentQsoItemViewModel> SortItems(IEnumerable<RecentQsoItemViewModel> items)
    {
        var ordered = CurrentSortColumn switch
        {
            RecentQsoSortColumn.Utc => SortAscending
                ? items.OrderBy(item => item.UtcSortKey)
                : items.OrderByDescending(item => item.UtcSortKey),
            RecentQsoSortColumn.Callsign => SortAscending
                ? items.OrderBy(item => item.CallsignSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.CallsignSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Band => SortAscending
                ? items.OrderBy(item => item.BandSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.BandSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Mode => SortAscending
                ? items.OrderBy(item => item.ModeSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.ModeSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Country => SortAscending
                ? items.OrderBy(item => item.CountrySortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.CountrySortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Name => SortAscending
                ? items.OrderBy(item => item.NameSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.NameSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Frequency => SortAscending
                ? items.OrderBy(item => item.FrequencySortKey)
                : items.OrderByDescending(item => item.FrequencySortKey),
            RecentQsoSortColumn.RstSent => SortAscending
                ? items.OrderBy(item => item.RstSentSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.RstSentSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.RstReceived => SortAscending
                ? items.OrderBy(item => item.RstReceivedSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.RstReceivedSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Grid => SortAscending
                ? items.OrderBy(item => item.GridSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.GridSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.Comment => SortAscending
                ? items.OrderBy(item => item.CommentSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.CommentSortKey, StringComparer.Ordinal),
            RecentQsoSortColumn.UtcEnd => SortAscending
                ? items.OrderBy(item => item.UtcEndSortKey)
                : items.OrderByDescending(item => item.UtcEndSortKey),
            RecentQsoSortColumn.Sync => SortAscending
                ? items.OrderBy(item => item.SyncStatusSortKey, StringComparer.Ordinal)
                : items.OrderByDescending(item => item.SyncStatusSortKey, StringComparer.Ordinal),
            _ => items.OrderByDescending(item => item.UtcSortKey)
        };

        return ordered
            .ThenByDescending(item => item.UtcSortKey)
            .ThenBy(item => item.CallsignSortKey, StringComparer.Ordinal);
    }
}
