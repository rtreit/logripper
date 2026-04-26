using System;
using Avalonia.Controls;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Views;

internal sealed partial class CallsignCardView : UserControl
{
    private CallsignCardViewModel? _attachedViewModel;

    public CallsignCardView()
    {
        InitializeComponent();
        DataContextChanged += OnDataContextChanged;
        DetachedFromVisualTree += (_, _) => Detach();
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        Detach();
        if (DataContext is CallsignCardViewModel vm)
        {
            _attachedViewModel = vm;
            vm.ExpandMapRequested += OnExpandMapRequested;
        }
    }

    private void Detach()
    {
        if (_attachedViewModel is not null)
        {
            _attachedViewModel.ExpandMapRequested -= OnExpandMapRequested;
            _attachedViewModel = null;
        }
    }

    private void OnExpandMapRequested(object? sender, EventArgs e)
    {
        if (_attachedViewModel is not { IsMapAvailable: true } vm)
        {
            return;
        }

        var popout = new MapPopoutWindow();
        var subtitle = string.IsNullOrWhiteSpace(vm.MapCountryLabel)
            ? vm.MapDistanceText
            : $"{vm.MapCountryLabel}  ·  {vm.MapDistanceText}";
        var title = string.IsNullOrWhiteSpace(vm.Callsign) ? "Azimuthal Map" : $"Azimuthal Map · {vm.Callsign}";
        popout.Configure(title, subtitle, vm.MapPath, vm.MapScaleKm);

        var owner = TopLevel.GetTopLevel(this) as Window;
        if (owner is not null)
        {
            popout.Show(owner);
        }
        else
        {
            popout.Show();
        }
    }
}
