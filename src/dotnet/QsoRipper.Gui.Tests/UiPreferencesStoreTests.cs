using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.Tests;

public sealed class UiPreferencesStoreTests
{
    [Fact]
    public void LoadReturnsNullWhenFileIsMissing()
    {
        var path = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName(), "prefs.json");
        var store = new UiPreferencesStore(path);

        Assert.Null(store.Load());
    }

    [Fact]
    public void SaveAndLoadRoundTripsPreferences()
    {
        var directory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        var path = Path.Combine(directory, "prefs.json");
        var store = new UiPreferencesStore(path);
        var expected = new UiPreferences
        {
            IsRigEnabled = true,
            IsSpaceWeatherVisible = true,
            IsInspectorOpen = false,
            EngineProfileId = "local-dotnet",
            EngineEndpoint = "http://127.0.0.1:50052",
        };

        try
        {
            store.Save(expected);

            var actual = store.Load();

            Assert.NotNull(actual);
            Assert.True(actual.IsRigEnabled);
            Assert.True(actual.IsSpaceWeatherVisible);
            Assert.False(actual.IsInspectorOpen);
            Assert.Equal("local-dotnet", actual.EngineProfileId);
            Assert.Equal("http://127.0.0.1:50052", actual.EngineEndpoint);
        }
        finally
        {
            if (Directory.Exists(directory))
            {
                Directory.Delete(directory, recursive: true);
            }
        }
    }

    [Fact]
    public void LoadReturnsNullForCorruptJson()
    {
        var directory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        var path = Path.Combine(directory, "prefs.json");

        try
        {
            Directory.CreateDirectory(directory);
            File.WriteAllText(path, "NOT VALID JSON {{{");

            var store = new UiPreferencesStore(path);

            Assert.Null(store.Load());
        }
        finally
        {
            if (Directory.Exists(directory))
            {
                Directory.Delete(directory, recursive: true);
            }
        }
    }

    [Fact]
    public void DefaultFilePathIsUnderLocalAppData()
    {
        var path = UiPreferencesStore.GetDefaultFilePath();
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);

        Assert.StartsWith(Path.Combine(localAppData, "QsoRipper"), path, StringComparison.Ordinal);
        Assert.EndsWith("ui-preferences.json", path, StringComparison.Ordinal);
    }
}
