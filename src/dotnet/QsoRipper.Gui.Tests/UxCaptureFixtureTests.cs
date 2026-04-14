using QsoRipper.Gui.Inspection;

namespace QsoRipper.Gui.Tests;

public class UxCaptureFixtureTests
{
    [Fact]
    public void LoadWithoutPathReturnsDeterministicDefaultFixture()
    {
        var fixture = UxCaptureFixture.Load(null);

        Assert.Equal("Portable", fixture.ProfileName);
        Assert.Equal("K7RND", fixture.StationCallsign);
        Assert.Equal("CN87", fixture.GridSquare);
        Assert.True(fixture.SetupComplete);
        Assert.NotEmpty(fixture.RecentQsos);
        Assert.NotEmpty(fixture.BuildRecentQsoRecords());
        Assert.Equal("K7RND", fixture.BuildStationProfile().OperatorCallsign);
    }

    [Fact]
    public void LoadFromPathParsesRecentQsosAndMetadata()
    {
        var tempPath = Path.GetTempFileName();

        try
        {
            File.WriteAllText(
                tempPath,
                """
                {
                  "profileName": "Field Day",
                  "stationCallsign": "W1AW",
                  "operatorCallsign": "W1AW",
                  "gridSquare": "FN31",
                  "setupComplete": false,
                  "searchText": "call:w1aw",
                  "recentQsos": [
                    {
                      "localId": "fixture-qso-1",
                      "workedCallsign": "DL1XYZ",
                      "band": "20M",
                      "mode": "FT8",
                      "frequencyKhz": 14074,
                      "workedCountry": "Federal Republic of Germany",
                      "syncStatus": "Synced"
                    }
                  ]
                }
                """);

            var fixture = UxCaptureFixture.Load(tempPath);
            var record = Assert.Single(fixture.BuildRecentQsoRecords());

            Assert.Equal("Field Day", fixture.ProfileName);
            Assert.Equal("W1AW", fixture.OperatorCallsign);
            Assert.Equal("FN31", fixture.GridSquare);
            Assert.False(fixture.SetupComplete);
            Assert.Equal("call:w1aw", fixture.SearchText);
            Assert.Equal("fixture-qso-1", record.LocalId);
            Assert.Equal("DL1XYZ", record.WorkedCallsign);
            Assert.Equal(QsoRipper.Domain.SyncStatus.Synced, record.SyncStatus);
        }
        finally
        {
            File.Delete(tempPath);
        }
    }
}
