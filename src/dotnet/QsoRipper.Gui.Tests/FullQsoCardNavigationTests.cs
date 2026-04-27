using Avalonia.Input;
using QsoRipper.Gui.Views;

namespace QsoRipper.Gui.Tests;

#pragma warning disable CA1707 // xUnit test naming
public sealed class FullQsoCardNavigationTests
{
    [Theory]
    [InlineData(Key.D1, 0)]
    [InlineData(Key.NumPad1, 0)]
    [InlineData(Key.D2, 1)]
    [InlineData(Key.NumPad2, 1)]
    [InlineData(Key.D3, 2)]
    [InlineData(Key.NumPad3, 2)]
    [InlineData(Key.D4, 3)]
    [InlineData(Key.NumPad4, 3)]
    [InlineData(Key.D5, 4)]
    [InlineData(Key.NumPad5, 4)]
    [InlineData(Key.D6, 5)]
    [InlineData(Key.NumPad6, 5)]
    [InlineData(Key.D7, 6)]
    [InlineData(Key.NumPad7, 6)]
    public void Alt_digit_shortcuts_jump_to_expected_section(Key key, int expectedIndex)
    {
        var handled = FullQsoCardNavigation.TryResolve(
            key,
            KeyModifiers.Alt,
            currentIndex: 0,
            out var targetIndex);

        Assert.True(handled);
        Assert.Equal(expectedIndex, targetIndex);
    }

    [Fact]
    public void Ctrl_Tab_advances_to_next_section()
    {
        var handled = FullQsoCardNavigation.TryResolve(
            Key.Tab,
            KeyModifiers.Control,
            currentIndex: 2,
            out var targetIndex);

        Assert.True(handled);
        Assert.Equal(3, targetIndex);
    }

    [Fact]
    public void Ctrl_Shift_Tab_wraps_to_previous_section()
    {
        var handled = FullQsoCardNavigation.TryResolve(
            Key.Tab,
            KeyModifiers.Control | KeyModifiers.Shift,
            currentIndex: 0,
            out var targetIndex);

        Assert.True(handled);
        Assert.Equal(FullQsoCardNavigation.TabCount - 1, targetIndex);
    }

    [Fact]
    public void Unrelated_keys_are_not_handled()
    {
        var handled = FullQsoCardNavigation.TryResolve(
            Key.N,
            KeyModifiers.Alt,
            currentIndex: 1,
            out var targetIndex);

        Assert.False(handled);
        Assert.Equal(1, targetIndex);
    }
}
#pragma warning restore CA1707
