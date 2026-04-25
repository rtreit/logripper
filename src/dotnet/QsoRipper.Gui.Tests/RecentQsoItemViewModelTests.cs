using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

public sealed class RecentQsoItemViewModelTests
{
    [Fact]
    public void GridRstColumnsUseStructuredRstWhenRawIsMissing()
    {
        var item = RecentQsoItemViewModel.FromQso(new QsoRecord
        {
            LocalId = "qso-rst",
            WorkedCallsign = "PW5K",
            StationCallsign = "K7RND",
            UtcTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 20, 3, 30, 0, TimeSpan.Zero)),
            Band = Band._20M,
            Mode = Mode.Cw,
            RstSent = new RstReport { Readability = 5, Strength = 3, Tone = 9 },
            RstReceived = new RstReport { Readability = 5, Strength = 9, Tone = 9 },
        });

        Assert.Equal("539", item.RstSent);
        Assert.Equal("599", item.RstReceived);
        Assert.Equal("539/599", item.Rst);
    }

    [Fact]
    public void SortKeysTrackEditableFieldChanges()
    {
        var item = RecentQsoItemViewModel.FromQso(new QsoRecord
        {
            LocalId = "qso-1",
            WorkedCallsign = "W1AW",
            StationCallsign = "K7RND",
            UtcTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero)),
            UtcEndTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 13, 22, 25, 0, TimeSpan.Zero)),
            Band = Band._20M,
            Mode = Mode.Cw,
            FrequencyHz = 14_025_000,
            WorkedDxcc = 291,
            RstSent = new RstReport { Raw = "59" },
            RstReceived = new RstReport { Raw = "57" },
        });

        item.UtcDisplay = "2026-04-14T01:02:03Z";
        item.Frequency = "14.250";
        item.Dxcc = "110";
        item.UtcEndDisplay = "2026-04-14T01:12:03Z";

        Assert.Equal(new DateTimeOffset(2026, 4, 14, 1, 2, 3, TimeSpan.Zero), item.UtcSortKey);
        Assert.Equal((ulong)14_250_000, item.FrequencySortKey);
        Assert.Equal((uint)110, item.DxccSortKey);
        Assert.Equal(new DateTimeOffset(2026, 4, 14, 1, 12, 3, TimeSpan.Zero), item.UtcEndSortKey);
    }

    [Fact]
    public void RxWpmDisplayShowsValueWhenPresent()
    {
        var item = RecentQsoItemViewModel.FromQso(new QsoRecord
        {
            LocalId = "qso-wpm",
            WorkedCallsign = "K1ABC",
            StationCallsign = "K7RND",
            Mode = Mode.Cw,
            CwDecodeRxWpm = 22,
        });

        Assert.Equal("22", item.RxWpmDisplay);
        Assert.Equal((uint)22, item.RxWpmSortKey);
    }

    [Fact]
    public void RxWpmDisplayUsesEmDashWhenAbsent()
    {
        var item = RecentQsoItemViewModel.FromQso(new QsoRecord
        {
            LocalId = "qso-no-wpm",
            WorkedCallsign = "K2XYZ",
            StationCallsign = "K7RND",
            Mode = Mode.Cw,
        });

        Assert.Equal("\u2014", item.RxWpmDisplay);
        Assert.Equal((uint)0, item.RxWpmSortKey);
    }

    [Fact]
    public void AcceptSavedChangesRefreshesRxWpmDisplay()
    {
        var qso = new QsoRecord
        {
            LocalId = "qso-edit",
            WorkedCallsign = "W1AW",
            StationCallsign = "K7RND",
            Mode = Mode.Cw,
        };
        var item = RecentQsoItemViewModel.FromQso(qso);

        Assert.Equal("\u2014", item.RxWpmDisplay);

        var updated = qso.Clone();
        updated.CwDecodeRxWpm = 30;
        var raised = new List<string?>();
        item.PropertyChanged += (_, e) => raised.Add(e.PropertyName);

        item.AcceptSavedChanges(updated);

        Assert.Equal("30", item.RxWpmDisplay);
        Assert.Equal((uint)30, item.RxWpmSortKey);
        Assert.Contains(nameof(RecentQsoItemViewModel.RxWpmDisplay), raised);
        Assert.Contains(nameof(RecentQsoItemViewModel.RxWpmSortKey), raised);
    }
}
