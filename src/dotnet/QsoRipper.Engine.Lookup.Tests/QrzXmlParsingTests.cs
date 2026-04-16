using System.Xml.Linq;
using QsoRipper.Domain;
using QsoRipper.Engine.Lookup.Qrz;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class QrzXmlParsingTests
{
    private const string SampleResponse = """
        <?xml version="1.0" encoding="utf-8" ?>
        <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
            <Callsign>
                <call>W1AW</call>
                <xref>W1AW</xref>
                <aliases>AA1AW,W1MR</aliases>
                <dxcc>291</dxcc>
                <fname>ARRL</fname>
                <name>Headquarters Station</name>
                <nickname>Hiram</nickname>
                <name_fmt>ARRL HQ</name_fmt>
                <attn>QSL Bureau</attn>
                <addr1>225 Main Street</addr1>
                <addr2>Newington</addr2>
                <state>CT</state>
                <zip>06111</zip>
                <country>United States</country>
                <ccode>291</ccode>
                <lat>41.714775</lat>
                <lon>-72.727260</lon>
                <grid>FN31pr</grid>
                <county>Hartford</county>
                <fips>09003</fips>
                <geoloc>user</geoloc>
                <class>C</class>
                <efdate>2020-01-15</efdate>
                <expdate>2030-01-15</expdate>
                <codes>HAB</codes>
                <email>w1aw@arrl.org</email>
                <url>https://www.arrl.org/w1aw</url>
                <qslmgr>ARRL Bureau</qslmgr>
                <eqsl>1</eqsl>
                <lotw>Y</lotw>
                <mqsl>0</mqsl>
                <cqzone>5</cqzone>
                <ituzone>8</ituzone>
                <iota>NA-001</iota>
                <land>United States</land>
                <continent>NA</continent>
                <born>1914</born>
                <serial>12345</serial>
                <moddate>2024-03-15 14:30:00</moddate>
                <bio>1500</bio>
                <image>https://files.qrz.com/w/w1aw/w1aw.jpg</image>
                <MSA>3283</MSA>
                <AreaCode>860</AreaCode>
                <TimeZone>America/New_York</TimeZone>
                <GMTOffset>-5</GMTOffset>
                <DST>Y</DST>
                <u_views>123456</u_views>
            </Callsign>
            <Session>
                <Key>abc123</Key>
            </Session>
        </QRZDatabase>
        """;

    private static XElement ParseCallsignElement(string xml)
    {
        var doc = XDocument.Parse(xml);
        var ns = doc.Root!.Name.Namespace;
        return doc.Root.Element(ns + "Callsign")!;
    }

    [Fact]
    public void MapCallsignRecord_ParsesIdentityFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("W1AW", record.Callsign);
        Assert.Equal("W1AW", record.CrossRef);
        Assert.Contains("AA1AW", record.Aliases);
        Assert.Contains("W1MR", record.Aliases);
        Assert.Equal(2, record.Aliases.Count);
        Assert.Equal(291u, record.DxccEntityId);
    }

    [Fact]
    public void MapCallsignRecord_ParsesNameFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("ARRL", record.FirstName);
        Assert.Equal("Headquarters Station", record.LastName);
        Assert.Equal("Hiram", record.Nickname);
        Assert.Equal("ARRL HQ", record.FormattedName);
    }

    [Fact]
    public void MapCallsignRecord_ParsesAddressFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("QSL Bureau", record.Attention);
        Assert.Equal("225 Main Street", record.Addr1);
        Assert.Equal("Newington", record.Addr2);
        Assert.Equal("CT", record.State);
        Assert.Equal("06111", record.Zip);
        Assert.Equal("United States", record.Country);
        Assert.Equal(291u, record.CountryCode);
    }

    [Fact]
    public void MapCallsignRecord_ParsesLocationFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal(41.714775, record.Latitude, 5);
        Assert.Equal(-72.727260, record.Longitude, 5);
        Assert.Equal("FN31pr", record.GridSquare);
        Assert.Equal("Hartford", record.County);
        Assert.Equal("09003", record.Fips);
        Assert.Equal(GeoSource.User, record.GeoSource);
    }

    [Fact]
    public void MapCallsignRecord_ParsesLicenseFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("C", record.LicenseClass);
        Assert.NotNull(record.EffectiveDate);
        Assert.NotNull(record.ExpirationDate);
        Assert.Equal("HAB", record.LicenseCodes);
    }

    [Fact]
    public void MapCallsignRecord_ParsesContactFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("w1aw@arrl.org", record.Email);
        Assert.Equal("https://www.arrl.org/w1aw", record.WebUrl);
        Assert.Equal("ARRL Bureau", record.QslManager);
    }

    [Fact]
    public void MapCallsignRecord_ParsesQslPreferences()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal(QslPreference.Yes, record.Eqsl);
        Assert.Equal(QslPreference.Yes, record.Lotw);
        Assert.Equal(QslPreference.No, record.PaperQsl);
    }

    [Fact]
    public void MapCallsignRecord_ParsesZoneFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal(5u, record.CqZone);
        Assert.Equal(8u, record.ItuZone);
        Assert.Equal("NA-001", record.Iota);
    }

    [Fact]
    public void MapCallsignRecord_ParsesDxccInfo()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal("United States", record.DxccCountryName);
        Assert.Equal("NA", record.DxccContinent);
    }

    [Fact]
    public void MapCallsignRecord_ParsesMetadataFields()
    {
        var el = ParseCallsignElement(SampleResponse);
        var record = QrzXmlProvider.MapCallsignRecord("W1AW", el);

        Assert.Equal(1914u, record.BirthYear);
        Assert.Equal(12345ul, record.QrzSerial);
        Assert.NotNull(record.LastModified);
        Assert.Equal(1500u, record.BioLength);
        Assert.Equal("https://files.qrz.com/w/w1aw/w1aw.jpg", record.ImageUrl);
        Assert.Equal("3283", record.Msa);
        Assert.Equal("860", record.AreaCode);
        Assert.Equal("America/New_York", record.TimeZone);
        Assert.Equal(-5.0, record.GmtOffset);
        Assert.True(record.DstObserved);
        Assert.Equal(123456u, record.ProfileViews);
    }

    [Fact]
    public void MapQslPreference_MapsValues()
    {
        Assert.Equal(QslPreference.Yes, QrzXmlProvider.MapQslPreference("1"));
        Assert.Equal(QslPreference.Yes, QrzXmlProvider.MapQslPreference("Y"));
        Assert.Equal(QslPreference.No, QrzXmlProvider.MapQslPreference("0"));
        Assert.Equal(QslPreference.No, QrzXmlProvider.MapQslPreference("N"));
        Assert.Equal(QslPreference.Unknown, QrzXmlProvider.MapQslPreference(null));
        Assert.Equal(QslPreference.Unknown, QrzXmlProvider.MapQslPreference(""));
    }

    [Fact]
    public void MapGeoSource_MapsValues()
    {
        Assert.Equal(GeoSource.User, QrzXmlProvider.MapGeoSource("user"));
        Assert.Equal(GeoSource.Geocode, QrzXmlProvider.MapGeoSource("geocode"));
        Assert.Equal(GeoSource.Grid, QrzXmlProvider.MapGeoSource("grid"));
        Assert.Equal(GeoSource.Zip, QrzXmlProvider.MapGeoSource("zip"));
        Assert.Equal(GeoSource.State, QrzXmlProvider.MapGeoSource("state"));
        Assert.Equal(GeoSource.Dxcc, QrzXmlProvider.MapGeoSource("dxcc"));
        Assert.Equal(GeoSource.None, QrzXmlProvider.MapGeoSource("none"));
        Assert.Equal(GeoSource.Unspecified, QrzXmlProvider.MapGeoSource(null));
        Assert.Equal(GeoSource.Unspecified, QrzXmlProvider.MapGeoSource("bogus"));
    }

    [Fact]
    public void MapCallsignRecord_HandlesMinimalResponse()
    {
        const string minimal = """
            <?xml version="1.0" encoding="utf-8" ?>
            <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                <Callsign>
                    <call>N0CALL</call>
                </Callsign>
            </QRZDatabase>
            """;

        var doc = XDocument.Parse(minimal);
        var ns = doc.Root!.Name.Namespace;
        var el = doc.Root.Element(ns + "Callsign")!;
        var record = QrzXmlProvider.MapCallsignRecord("N0CALL", el);

        Assert.Equal("N0CALL", record.Callsign);
        Assert.Equal("N0CALL", record.CrossRef);
        Assert.Empty(record.Aliases);
        Assert.Equal(0u, record.DxccEntityId);
    }

    [Fact]
    public void MapCallsignRecord_UsesQueriedCallsign_WhenCallElementMissing()
    {
        const string noCall = """
            <?xml version="1.0" encoding="utf-8" ?>
            <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                <Callsign>
                    <fname>Test</fname>
                </Callsign>
            </QRZDatabase>
            """;

        var doc = XDocument.Parse(noCall);
        var ns = doc.Root!.Name.Namespace;
        var el = doc.Root.Element(ns + "Callsign")!;
        var record = QrzXmlProvider.MapCallsignRecord("QUERIED", el);

        Assert.Equal("QUERIED", record.Callsign);
        Assert.Equal("QUERIED", record.CrossRef);
    }

    [Fact]
    public void MapCallsignRecord_ParsesBioWithSlash()
    {
        const string bioSlash = """
            <?xml version="1.0" encoding="utf-8" ?>
            <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                <Callsign>
                    <call>TEST</call>
                    <bio>1500/2024-01-01</bio>
                </Callsign>
            </QRZDatabase>
            """;

        var doc = XDocument.Parse(bioSlash);
        var ns = doc.Root!.Name.Namespace;
        var el = doc.Root.Element(ns + "Callsign")!;
        var record = QrzXmlProvider.MapCallsignRecord("TEST", el);

        Assert.Equal(1500u, record.BioLength);
    }

    [Fact]
    public void MapCallsignRecord_ParsesGmtOffsetHhmm()
    {
        const string hhmm = """
            <?xml version="1.0" encoding="utf-8" ?>
            <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                <Callsign>
                    <call>TEST</call>
                    <GMTOffset>-0530</GMTOffset>
                </Callsign>
            </QRZDatabase>
            """;

        var doc = XDocument.Parse(hhmm);
        var ns = doc.Root!.Name.Namespace;
        var el = doc.Root.Element(ns + "Callsign")!;
        var record = QrzXmlProvider.MapCallsignRecord("TEST", el);

        Assert.Equal(-5.5, record.GmtOffset);
    }
}
