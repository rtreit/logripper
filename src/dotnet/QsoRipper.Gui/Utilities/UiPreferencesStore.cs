using System.Text.Json;

namespace QsoRipper.Gui.Utilities;

internal sealed class UiPreferencesStore
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true
    };

    private readonly string _filePath;

    public UiPreferencesStore(string? filePath = null)
    {
        _filePath = string.IsNullOrWhiteSpace(filePath)
            ? GetDefaultFilePath()
            : filePath;
    }

    public UiPreferences? Load()
    {
        if (!File.Exists(_filePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(_filePath);
            return JsonSerializer.Deserialize<UiPreferences>(stream, JsonOptions);
        }
        catch (IOException)
        {
            return null;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    public void Save(UiPreferences prefs)
    {
        ArgumentNullException.ThrowIfNull(prefs);

        var directory = Path.GetDirectoryName(_filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var stream = File.Create(_filePath);
        JsonSerializer.Serialize(stream, prefs, JsonOptions);
    }

    internal static string GetDefaultFilePath()
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return Path.Combine(localAppData, "QsoRipper", "ui-preferences.json");
    }
}

internal sealed class UiPreferences
{
    public bool IsRigEnabled { get; set; }

    public bool IsSpaceWeatherVisible { get; set; }

    public bool IsInspectorOpen { get; set; }

    public string? EngineProfileId { get; set; }

    public string? EngineEndpoint { get; set; }

    public bool IsCwDecoderEnabled { get; set; }

    public string? CwDecoderDeviceOverride { get; set; }

    public bool IsCwDecoderLoopback { get; set; }

    public bool IsCwWpmStatusBarVisible { get; set; }

    /// <summary>
    /// When true, the GUI mirrors all cw-decoder audio + NDJSON events to disk
    /// under <c>%LOCALAPPDATA%\QsoRipper\diagnostics\session-&lt;utc&gt;\</c> so a
    /// developer can compare what was displayed in the UX vs. what the decoder
    /// emitted vs. what gets logged on the QSO. See
    /// <c>experiments/cw-decoder/README.md</c>.
    /// </summary>
    public bool IsCwDiagnosticsEnabled { get; set; }
}
