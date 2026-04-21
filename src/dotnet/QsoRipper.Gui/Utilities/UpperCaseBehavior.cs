using Avalonia;
using Avalonia.Controls;

namespace QsoRipper.Gui.Utilities;

/// <summary>
/// Attached behavior that forces a TextBox value to uppercase.
/// Usage: &lt;TextBox util:UpperCaseBehavior.IsEnabled="True" /&gt;
/// </summary>
internal static class UpperCaseBehavior
{
    public static readonly AttachedProperty<bool> IsEnabledProperty =
        AvaloniaProperty.RegisterAttached<TextBox, bool>("IsEnabled", typeof(UpperCaseBehavior));

    static UpperCaseBehavior()
    {
        IsEnabledProperty.Changed.AddClassHandler<TextBox>(OnIsEnabledChanged);
    }

    public static bool GetIsEnabled(TextBox textBox)
    {
        ArgumentNullException.ThrowIfNull(textBox);
        return textBox.GetValue(IsEnabledProperty);
    }

    public static void SetIsEnabled(TextBox textBox, bool value)
    {
        ArgumentNullException.ThrowIfNull(textBox);
        textBox.SetValue(IsEnabledProperty, value);
    }

    private static void OnIsEnabledChanged(TextBox textBox, AvaloniaPropertyChangedEventArgs e)
    {
        if (e.NewValue is true)
        {
            textBox.TextChanged += OnTextChanged;
        }
        else
        {
            textBox.TextChanged -= OnTextChanged;
        }
    }

    private static void OnTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (sender is not TextBox textBox || textBox.Text is null)
        {
            return;
        }

        var upper = textBox.Text.ToUpperInvariant();
        if (upper != textBox.Text)
        {
            var caretIndex = textBox.CaretIndex;
            textBox.Text = upper;
            textBox.CaretIndex = caretIndex;
        }
    }
}
