namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class CallsignParserTests
{
    [Theory]
    [InlineData("K7ABC", "K7ABC", null, ModifierPosition.None)]
    [InlineData("W1AW", "W1AW", null, ModifierPosition.None)]
    public void Parse_PlainCallsign_ReturnsBaseOnly(string input, string expectedBase, string? expectedModifier, ModifierPosition expectedPosition)
    {
        var result = CallsignParser.Parse(input);

        Assert.Equal(expectedBase, result.BaseCallsign);
        Assert.Equal(expectedModifier, result.Modifier);
        Assert.Equal(expectedPosition, result.Position);
    }

    [Theory]
    [InlineData("K7ABC/M", "K7ABC", "M", ModifierPosition.Suffix)]
    [InlineData("K7ABC/P", "K7ABC", "P", ModifierPosition.Suffix)]
    [InlineData("K7ABC/QRP", "K7ABC", "QRP", ModifierPosition.Suffix)]
    public void Parse_SuffixModifier_ExtractsBaseAndModifier(string input, string expectedBase, string expectedModifier, ModifierPosition expectedPosition)
    {
        var result = CallsignParser.Parse(input);

        Assert.Equal(expectedBase, result.BaseCallsign);
        Assert.Equal(expectedModifier, result.Modifier);
        Assert.Equal(expectedPosition, result.Position);
    }

    [Theory]
    [InlineData("VP2E/K7ABC", "K7ABC", "VP2E", ModifierPosition.Prefix)]
    [InlineData("EA8/K7ABC", "K7ABC", "EA8", ModifierPosition.Prefix)]
    public void Parse_PrefixModifier_ExtractsBaseAndModifier(string input, string expectedBase, string expectedModifier, ModifierPosition expectedPosition)
    {
        var result = CallsignParser.Parse(input);

        Assert.Equal(expectedBase, result.BaseCallsign);
        Assert.Equal(expectedModifier, result.Modifier);
        Assert.Equal(expectedPosition, result.Position);
    }

    [Fact]
    public void Parse_NormalizesToUppercase()
    {
        var result = CallsignParser.Parse("k7abc/m");

        Assert.Equal("K7ABC", result.BaseCallsign);
        Assert.Equal("M", result.Modifier);
        Assert.Equal("K7ABC/M", result.OriginalCallsign);
    }

    [Fact]
    public void Parse_TrimsWhitespace()
    {
        var result = CallsignParser.Parse("  K7ABC  ");

        Assert.Equal("K7ABC", result.BaseCallsign);
        Assert.Null(result.Modifier);
    }

    [Fact]
    public void Parse_ThrowsOnNullOrWhitespace()
    {
        Assert.ThrowsAny<ArgumentException>(() => CallsignParser.Parse(null!));
        Assert.ThrowsAny<ArgumentException>(() => CallsignParser.Parse("  "));
    }
}
