using System;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading.Tasks;
using Avalonia.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class CallsignCardViewModel : ObservableObject
{
    private static readonly HttpClient SharedHttpClient = new()
    {
        Timeout = TimeSpan.FromSeconds(10)
    };

    private readonly IEngineClient _engine;

    [ObservableProperty]
    private string _callsign = string.Empty;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isLoaded;

    [ObservableProperty]
    private bool _isError;

    [ObservableProperty]
    private string _errorMessage = string.Empty;

    // Identity
    [ObservableProperty]
    private string _fullName = string.Empty;

    [ObservableProperty]
    private string _nickname = string.Empty;

    [ObservableProperty]
    private string _licenseClass = string.Empty;

    // Location
    [ObservableProperty]
    private string _address = string.Empty;

    [ObservableProperty]
    private string _country = string.Empty;

    [ObservableProperty]
    private string _grid = string.Empty;

    [ObservableProperty]
    private string _county = string.Empty;

    [ObservableProperty]
    private string _state = string.Empty;

    [ObservableProperty]
    private string _timeZone = string.Empty;

    // QRZ / Zones
    [ObservableProperty]
    private string _dxccCountry = string.Empty;

    [ObservableProperty]
    private string _dxccContinent = string.Empty;

    [ObservableProperty]
    private string _cqZone = string.Empty;

    [ObservableProperty]
    private string _ituZone = string.Empty;

    [ObservableProperty]
    private string _iota = string.Empty;

    // QSL
    [ObservableProperty]
    private string _eqslStatus = string.Empty;

    [ObservableProperty]
    private string _lotwStatus = string.Empty;

    [ObservableProperty]
    private string _paperQslStatus = string.Empty;

    [ObservableProperty]
    private string _qslManager = string.Empty;

    // Profile
    [ObservableProperty]
    private string? _imageUrl;

    [ObservableProperty]
    private Bitmap? _profileImage;

    [ObservableProperty]
    private string _webUrl = string.Empty;

    [ObservableProperty]
    private string _email = string.Empty;

    [ObservableProperty]
    private string _aliases = string.Empty;

    [ObservableProperty]
    private string _previousCall = string.Empty;

    [ObservableProperty]
    private bool _hasImage;

    [ObservableProperty]
    private string _latencyText = string.Empty;

    public CallsignCardViewModel(IEngineClient engine)
    {
        _engine = engine;
    }

    // Computed
    public bool HasNickname => !string.IsNullOrEmpty(Nickname);

    public bool HasAliases => !string.IsNullOrEmpty(Aliases);

    public bool HasPreviousCall => !string.IsNullOrEmpty(PreviousCall);

    public bool HasQslManager => !string.IsNullOrEmpty(QslManager);

    public bool HasIota => !string.IsNullOrEmpty(Iota);

    public bool HasWebUrl => !string.IsNullOrEmpty(WebUrl);

    public bool HasEmail => !string.IsNullOrEmpty(Email);

    public bool HasCounty => !string.IsNullOrEmpty(County);

    public bool HasTimeZone => !string.IsNullOrEmpty(TimeZone);

    public async Task LoadAsync(string callsign)
    {
        Callsign = callsign.ToUpperInvariant();
        IsLoading = true;
        IsLoaded = false;
        IsError = false;
        ErrorMessage = string.Empty;
        ProfileImage = null;
        HasImage = false;

        try
        {
            var response = await _engine.LookupCallsignAsync(callsign);
            var result = response.Result;

            if (result.State == LookupState.Found && result.Record is { } record)
            {
                MapRecord(record);
                LatencyText = result.LookupLatencyMs > 0
                    ? $"{result.LookupLatencyMs}ms{(result.CacheHit ? " (cached)" : string.Empty)}"
                    : string.Empty;

                if (!string.IsNullOrEmpty(record.ImageUrl))
                {
                    await LoadImageAsync(record.ImageUrl);
                }

                IsLoaded = true;
                RecordLoaded?.Invoke(this, record);
            }
            else if (result.State == LookupState.NotFound)
            {
                IsError = true;
                ErrorMessage = $"Callsign '{callsign}' not found.";
            }
            else
            {
                IsError = true;
                ErrorMessage = result.ErrorMessage ?? $"Lookup failed ({result.State}).";
            }
        }
        catch (Grpc.Core.RpcException ex)
        {
            IsError = true;
            ErrorMessage = $"Lookup error: {ex.Status.Detail}";
        }
        catch (InvalidOperationException ex)
        {
            IsError = true;
            ErrorMessage = $"Lookup error: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    private void Close()
    {
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }

    internal event EventHandler? CloseRequested;

    /// <summary>
    /// Raised when a callsign record is successfully loaded, carrying the
    /// resolved <see cref="CallsignRecord"/> so the QSO logger can use it
    /// for enrichment.
    /// </summary>
    internal event EventHandler<CallsignRecord>? RecordLoaded;

    private void MapRecord(CallsignRecord record)
    {
        Callsign = record.Callsign;
        FullName = BuildFullName(record);
        Nickname = record.Nickname ?? string.Empty;
        LicenseClass = record.LicenseClass ?? string.Empty;

        Address = BuildAddress(record);
        Country = record.Country ?? string.Empty;
        Grid = record.GridSquare ?? string.Empty;
        County = record.County ?? string.Empty;
        State = record.State ?? string.Empty;
        TimeZone = record.TimeZone ?? string.Empty;

        DxccCountry = record.DxccCountryName ?? string.Empty;
        DxccContinent = record.DxccContinent ?? string.Empty;
        CqZone = record.HasCqZone ? record.CqZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        ItuZone = record.HasItuZone ? record.ItuZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        Iota = record.Iota ?? string.Empty;

        EqslStatus = FormatQslStatus(record.Eqsl);
        LotwStatus = FormatQslStatus(record.Lotw);
        PaperQslStatus = FormatQslStatus(record.PaperQsl);
        QslManager = record.QslManager ?? string.Empty;

        ImageUrl = record.ImageUrl;
        WebUrl = record.WebUrl ?? string.Empty;
        Email = record.Email ?? string.Empty;
        Aliases = record.Aliases.Count > 0 ? string.Join(", ", record.Aliases) : string.Empty;
        PreviousCall = record.PreviousCall ?? string.Empty;

        OnPropertyChanged(nameof(HasNickname));
        OnPropertyChanged(nameof(HasAliases));
        OnPropertyChanged(nameof(HasPreviousCall));
        OnPropertyChanged(nameof(HasQslManager));
        OnPropertyChanged(nameof(HasIota));
        OnPropertyChanged(nameof(HasWebUrl));
        OnPropertyChanged(nameof(HasEmail));
        OnPropertyChanged(nameof(HasCounty));
        OnPropertyChanged(nameof(HasTimeZone));
    }

    private async Task LoadImageAsync(string url)
    {
        try
        {
            var bytes = await SharedHttpClient.GetByteArrayAsync(new Uri(url));
            using var stream = new MemoryStream(bytes);
            ProfileImage = new Bitmap(stream);
            HasImage = true;
        }
        catch (HttpRequestException)
        {
            // Image download failed — leave HasImage false.
        }
        catch (TaskCanceledException)
        {
            // Image download timed out — leave HasImage false.
        }
    }

    private static string BuildFullName(CallsignRecord record)
    {
        if (!string.IsNullOrWhiteSpace(record.FormattedName))
        {
            return record.FormattedName;
        }

        var first = record.FirstName ?? string.Empty;
        var last = record.LastName ?? string.Empty;
        return string.IsNullOrWhiteSpace(first) && string.IsNullOrWhiteSpace(last)
            ? string.Empty
            : $"{first} {last}".Trim();
    }

    private static string BuildAddress(CallsignRecord record)
    {
        var city = record.Addr2 ?? string.Empty;
        var state = record.State ?? string.Empty;
        var zip = record.Zip ?? string.Empty;

        if (string.IsNullOrWhiteSpace(city) && string.IsNullOrWhiteSpace(state))
        {
            return string.Empty;
        }

        var parts = new[] { city, state, zip }
            .Where(part => !string.IsNullOrWhiteSpace(part));
        return string.Join(", ", parts);
    }

    private static string FormatQslStatus(QslPreference preference) => preference switch
    {
        QslPreference.Yes => "Yes",
        QslPreference.No => "No",
        _ => "?"
    };
}
