using System.Text.Json;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Utilities;

internal sealed class RecentQsoGridLayoutStore
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true
    };

    private readonly string _filePath;

    public RecentQsoGridLayoutStore(string? filePath = null)
    {
        _filePath = string.IsNullOrWhiteSpace(filePath)
            ? GetDefaultFilePath()
            : filePath;
    }

    public RecentQsoGridLayoutState? Load()
    {
        if (!File.Exists(_filePath))
        {
            return null;
        }

        try
        {
            using var stream = File.OpenRead(_filePath);
            return JsonSerializer.Deserialize<RecentQsoGridLayoutState>(stream, JsonOptions);
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

    public void Save(RecentQsoGridLayoutState state)
    {
        ArgumentNullException.ThrowIfNull(state);

        var directory = Path.GetDirectoryName(_filePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        using var stream = File.Create(_filePath);
        JsonSerializer.Serialize(stream, state, JsonOptions);
    }

    private static string GetDefaultFilePath()
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return Path.Combine(localAppData, "QsoRipper", "recent-qso-grid-layout.json");
    }
}

internal sealed class RecentQsoGridLayoutState
{
    public RecentQsoSortColumn SortColumn { get; set; } = RecentQsoSortColumn.Utc;

    public bool SortAscending { get; set; }

    public double GridFontSize { get; set; } = 12;

    public List<RecentQsoGridColumnState> Columns { get; set; } = [];
}

internal sealed class RecentQsoGridColumnState
{
    public RecentQsoGridColumn Column { get; set; }

    public bool IsVisible { get; set; } = true;

    public int DisplayIndex { get; set; }

    public double Width { get; set; }
}
