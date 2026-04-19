using QsoRipper.Engine.QrzLogbook;

#pragma warning disable CA1307 // Use StringComparison for string comparison

namespace QsoRipper.Engine.QrzLogbook.Tests;

#pragma warning disable CA1707 // Remove underscores from member names

public sealed class QrzResponseParserTests
{
    // -- ParseKeyValueResponse ----------------------------------------------

    [Fact]
    public void Parse_kv_response_extracts_fields()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=OK&LOGID=12345&COUNT=1");

        Assert.Equal("OK", map["RESULT"]);
        Assert.Equal("12345", map["LOGID"]);
        Assert.Equal("1", map["COUNT"]);
    }

    [Fact]
    public void Parse_kv_response_uppercases_keys()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("result=OK&logid=99");

        Assert.True(map.ContainsKey("RESULT"));
        Assert.True(map.ContainsKey("LOGID"));
    }

    [Fact]
    public void Parse_kv_response_handles_empty_values()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=OK&LOGID=");

        Assert.Equal(string.Empty, map["LOGID"]);
    }

    // -- CheckResult --------------------------------------------------------

    [Fact]
    public void CheckResult_ok_returns_map()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=OK&LOGID=12345");

        var result = QrzResponseParser.CheckResult(map);

        Assert.Equal("12345", result["LOGID"]);
    }

    [Fact]
    public void CheckResult_fail_throws_exception()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=FAIL&REASON=bad record format");

        var ex = Assert.Throws<QrzLogbookException>(() => QrzResponseParser.CheckResult(map));
        Assert.Contains("bad record format", ex.Message);
    }

    [Fact]
    public void CheckResult_fail_auth_throws_auth_exception()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=FAIL&REASON=invalid api key");

        Assert.Throws<QrzLogbookAuthException>(() => QrzResponseParser.CheckResult(map));
    }

    [Fact]
    public void CheckResult_missing_result_throws()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("LOGID=12345");

        Assert.Throws<QrzLogbookException>(() => QrzResponseParser.CheckResult(map));
    }

    [Fact]
    public void CheckResult_unexpected_result_value_throws()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=MAYBE");

        Assert.Throws<QrzLogbookException>(() => QrzResponseParser.CheckResult(map));
    }

    // -- ExtractAdifPayload -------------------------------------------------

    [Fact]
    public void ExtractAdifPayload_returns_content_after_marker()
    {
        const string body = "RESULT=OK&COUNT=2&ADIF=<CALL:4>W1AW<eor>";

        var payload = QrzResponseParser.ExtractAdifPayload(body);

        Assert.Equal("<CALL:4>W1AW<eor>", payload);
    }

    [Fact]
    public void ExtractAdifPayload_returns_null_when_no_marker()
    {
        const string body = "RESULT=OK&COUNT=0";

        Assert.Null(QrzResponseParser.ExtractAdifPayload(body));
    }

    [Fact]
    public void ExtractAdifPayload_case_insensitive()
    {
        const string body = "RESULT=OK&adif=<CALL:4>W1AW<eor>";

        var payload = QrzResponseParser.ExtractAdifPayload(body);

        Assert.NotNull(payload);
        Assert.Contains("W1AW", payload);
    }

    // -- DecodeHtmlEntities -------------------------------------------------

    [Fact]
    public void DecodeHtmlEntities_decodes_angle_brackets()
    {
        const string encoded = "&lt;CALL:4&gt;W1AW";

        Assert.Equal("<CALL:4>W1AW", QrzResponseParser.DecodeHtmlEntities(encoded));
    }

    [Fact]
    public void DecodeHtmlEntities_decodes_quotes_and_apostrophes()
    {
        const string encoded = "&quot;hello&#39;world&quot;";

        Assert.Equal("\"hello'world\"", QrzResponseParser.DecodeHtmlEntities(encoded));
    }

    [Fact]
    public void DecodeHtmlEntities_decodes_ampersand_last()
    {
        // "&amp;lt;" should decode to "&lt;", not "<"
        const string encoded = "&amp;lt;";

        Assert.Equal("&lt;", QrzResponseParser.DecodeHtmlEntities(encoded));
    }

    [Fact]
    public void DecodeHtmlEntities_noop_when_no_entities()
    {
        const string plain = "<CALL:4>W1AW";

        Assert.Equal(plain, QrzResponseParser.DecodeHtmlEntities(plain));
    }

    // -- EnsureAdifHasEoh ---------------------------------------------------

    [Fact]
    public void EnsureAdifHasEoh_prepends_when_missing()
    {
        const string adif = "<CALL:4>W1AW<eor>";

        var result = QrzResponseParser.EnsureAdifHasEoh(adif);

        Assert.StartsWith("<EOH>", result);
    }

    [Fact]
    public void EnsureAdifHasEoh_noop_when_present()
    {
        const string adif = "<EOH><CALL:4>W1AW<eor>";

        Assert.Equal(adif, QrzResponseParser.EnsureAdifHasEoh(adif));
    }

    // -- ParseFetchPrefix ---------------------------------------------------

    [Fact]
    public void ParseFetchPrefix_extracts_result_and_count()
    {
        const string body = "RESULT=OK&COUNT=773&ADIF=<CALL:4>W1AW<eor>";

        var prefix = QrzResponseParser.ParseFetchPrefix(body);

        Assert.Equal("OK", prefix["RESULT"]);
        Assert.Equal("773", prefix["COUNT"]);
        Assert.False(prefix.ContainsKey("ADIF"));
    }

    // -- IsEmptyFetchFail ---------------------------------------------------

    [Fact]
    public void IsEmptyFetchFail_count0_no_reason_returns_true()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("COUNT=0&RESULT=FAIL");
        Assert.True(QrzResponseParser.IsEmptyFetchFail(map));
    }

    [Fact]
    public void IsEmptyFetchFail_result_first_returns_true()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("RESULT=FAIL&COUNT=0");
        Assert.True(QrzResponseParser.IsEmptyFetchFail(map));
    }

    [Fact]
    public void IsEmptyFetchFail_with_reason_returns_false()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("COUNT=0&RESULT=FAIL&REASON=bad key");
        Assert.False(QrzResponseParser.IsEmptyFetchFail(map));
    }

    [Fact]
    public void IsEmptyFetchFail_count_nonzero_returns_false()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("COUNT=5&RESULT=FAIL");
        Assert.False(QrzResponseParser.IsEmptyFetchFail(map));
    }

    [Fact]
    public void IsEmptyFetchFail_result_ok_returns_false()
    {
        var map = QrzResponseParser.ParseKeyValueResponse("COUNT=0&RESULT=OK");
        Assert.False(QrzResponseParser.IsEmptyFetchFail(map));
    }
}
