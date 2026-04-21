using Avalonia.Input;

namespace QsoRipper.Gui.Views;

internal static class FullQsoCardNavigation
{
    internal const int TabCount = 6;

    internal static bool TryResolve(Key key, KeyModifiers modifiers, int currentIndex, out int targetIndex)
    {
        targetIndex = currentIndex;

        if ((modifiers & KeyModifiers.Control) == KeyModifiers.Control && key == Key.Tab)
        {
            var delta = (modifiers & KeyModifiers.Shift) == KeyModifiers.Shift ? -1 : 1;
            targetIndex = (currentIndex + delta + TabCount) % TabCount;
            return true;
        }

        if ((modifiers & KeyModifiers.Alt) != KeyModifiers.Alt)
        {
            return false;
        }

        targetIndex = key switch
        {
            Key.D1 or Key.NumPad1 => 0,
            Key.D2 or Key.NumPad2 => 1,
            Key.D3 or Key.NumPad3 => 2,
            Key.D4 or Key.NumPad4 => 3,
            Key.D5 or Key.NumPad5 => 4,
            Key.D6 or Key.NumPad6 => 5,
            _ => currentIndex,
        };

        return targetIndex != currentIndex
            || key is Key.D1 or Key.NumPad1
                or Key.D2 or Key.NumPad2
                or Key.D3 or Key.NumPad3
                or Key.D4 or Key.NumPad4
                or Key.D5 or Key.NumPad5
                or Key.D6 or Key.NumPad6;
    }
}
