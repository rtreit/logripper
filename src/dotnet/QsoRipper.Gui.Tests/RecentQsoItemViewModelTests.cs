using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

public sealed class RecentQsoItemViewModelTests
{
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
            FrequencyKhz = 14025,
            WorkedDxcc = 291,
            RstSent = new RstReport { Raw = "59" },
            RstReceived = new RstReport { Raw = "57" },
        });

        item.UtcDisplay = "2026-04-14T01:02:03Z";
        item.Frequency = "14250";
        item.Dxcc = "110";
        item.UtcEndDisplay = "2026-04-14T01:12:03Z";

        Assert.Equal(new DateTimeOffset(2026, 4, 14, 1, 2, 3, TimeSpan.Zero), item.UtcSortKey);
        Assert.Equal((ulong)14250, item.FrequencySortKey);
        Assert.Equal((uint)110, item.DxccSortKey);
        Assert.Equal(new DateTimeOffset(2026, 4, 14, 1, 12, 3, TimeSpan.Zero), item.UtcEndSortKey);
    }
}
