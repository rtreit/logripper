using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace QsoRipper.Gui.Views;

internal sealed partial class QrzStepView : UserControl
{
    public QrzStepView()
    {
        InitializeComponent();
    }

    public static readonly FuncValueConverter<bool, IBrush> TestColor =
        new(success => success ? Brushes.ForestGreen : Brushes.IndianRed);
}
