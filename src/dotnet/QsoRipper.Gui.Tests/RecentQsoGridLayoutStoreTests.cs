using QsoRipper.Gui.Utilities;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

public sealed class RecentQsoGridLayoutStoreTests
{
    [Fact]
    public void LoadReturnsNullWhenLayoutFileIsMissing()
    {
        var layoutPath = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName(), "grid-layout.json");
        var store = new RecentQsoGridLayoutStore(layoutPath);

        Assert.Null(store.Load());
    }

    [Fact]
    public void SaveAndLoadRoundTripsGridLayout()
    {
        var layoutDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        var layoutPath = Path.Combine(layoutDirectory, "grid-layout.json");
        var store = new RecentQsoGridLayoutStore(layoutPath);
        var expected = new RecentQsoGridLayoutState
        {
            SortColumn = RecentQsoSortColumn.Callsign,
            SortAscending = true,
            GridFontSize = 14,
            Columns =
            [
                new RecentQsoGridColumnState
                {
                    Column = RecentQsoGridColumn.Utc,
                    DisplayIndex = 0,
                    IsVisible = true,
                    Width = 104,
                },
                new RecentQsoGridColumnState
                {
                    Column = RecentQsoGridColumn.Note,
                    DisplayIndex = 13,
                    IsVisible = true,
                    Width = 220,
                }
            ]
        };

        try
        {
            store.Save(expected);

            var actual = store.Load();

            Assert.NotNull(actual);
            Assert.Equal(RecentQsoSortColumn.Callsign, actual.SortColumn);
            Assert.True(actual.SortAscending);
            Assert.Equal(14, actual.GridFontSize);
            Assert.Equal(2, actual.Columns.Count);
            Assert.Equal(RecentQsoGridColumn.Note, actual.Columns[1].Column);
            Assert.Equal(220, actual.Columns[1].Width);
        }
        finally
        {
            if (Directory.Exists(layoutDirectory))
            {
                Directory.Delete(layoutDirectory, recursive: true);
            }
        }
    }

    [Fact]
    public void DeleteRemovesPersistedFile()
    {
        var layoutDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        var layoutPath = Path.Combine(layoutDirectory, "grid-layout.json");
        var store = new RecentQsoGridLayoutStore(layoutPath);

        try
        {
            store.Save(new RecentQsoGridLayoutState());
            Assert.True(File.Exists(layoutPath));

            store.Delete();
            Assert.False(File.Exists(layoutPath));

            // Delete on already-missing file is a no-op.
            store.Delete();
        }
        finally
        {
            if (Directory.Exists(layoutDirectory))
            {
                Directory.Delete(layoutDirectory, recursive: true);
            }
        }
    }
}
