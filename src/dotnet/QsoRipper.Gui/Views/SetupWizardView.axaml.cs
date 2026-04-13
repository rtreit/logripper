using System;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Controls.Presenters;
using Avalonia.Data.Converters;
using Avalonia.VisualTree;

namespace QsoRipper.Gui.Views;

internal sealed partial class SetupWizardView : UserControl
{
    public SetupWizardView()
    {
        InitializeComponent();
    }

    /// <summary>
    /// Converts IsComplete bool to a step indicator character.
    /// </summary>
    public static readonly FuncValueConverter<bool, string> StepIndicator =
        new(isComplete => isComplete == true ? "✓" : "○");

    /// <summary>
    /// Converts IsLastStep bool to button label text.
    /// </summary>
    public static readonly FuncValueConverter<bool, string> NextOrSave =
        new(isLast => isLast == true ? "Save & Start Logging" : "Next");

    /// <summary>
    /// Gets the index of an item within its parent ItemsControl.
    /// Used for NavigateToStep command parameter.
    /// </summary>
    public static readonly FuncValueConverter<Control?, int> ItemIndex =
        new(control =>
        {
            if (control is null)
            {
                return -1;
            }

            var itemsControl = control.FindAncestorOfType<ItemsControl>();
            if (itemsControl is null)
            {
                return -1;
            }

            var container = control.FindAncestorOfType<ContentPresenter>();
            if (container is not null)
            {
                return itemsControl.IndexFromContainer(container);
            }

            return -1;
        });
}
