using System;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using QsoRipper.Domain;
using QsoRipper.Gui.Controls;

namespace QsoRipper.Gui.Views;

internal sealed partial class MapPopoutWindow : Window
{
    public MapPopoutWindow()
    {
        InitializeComponent();
        AddHandler(KeyDownEvent, OnWindowKeyDown, handledEventsToo: false);
    }

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    public void Configure(string title, string subtitle, GreatCirclePath? path, double scaleKm)
    {
        var titleText = this.FindControl<TextBlock>("TitleText");
        var subtitleText = this.FindControl<TextBlock>("SubtitleText");
        var scaleText = this.FindControl<TextBlock>("ScaleText");
        var map = this.FindControl<AzimuthalMapControl>("Map");

        if (titleText is not null)
        {
            titleText.Text = string.IsNullOrWhiteSpace(title) ? "Azimuthal Map" : title;
        }
        if (subtitleText is not null)
        {
            subtitleText.Text = subtitle ?? string.Empty;
        }
        if (map is not null)
        {
            map.Path = path;
            map.ScaleKm = scaleKm > 0 ? scaleKm : 20015.0;
            map.ResetView();
        }
        if (scaleText is not null)
        {
            scaleText.Text = string.Create(CultureInfo.InvariantCulture, $"scale ~{FormatKm(scaleKm)}");
        }
    }

    private static string FormatKm(double km) =>
        km >= 1000 ? string.Create(CultureInfo.InvariantCulture, $"{km / 1000:0.#}k km") : string.Create(CultureInfo.InvariantCulture, $"{km:F0} km");

    private AzimuthalMapControl? GetMap() => this.FindControl<AzimuthalMapControl>("Map");

    private void OnZoomIn(object? sender, RoutedEventArgs e)
    {
        if (GetMap() is { } m)
        {
            m.Zoom = Math.Clamp(m.Zoom * 1.4, 1.0, 32.0);
        }
    }

    private void OnZoomOut(object? sender, RoutedEventArgs e)
    {
        if (GetMap() is { } m)
        {
            m.Zoom = Math.Clamp(m.Zoom / 1.4, 1.0, 32.0);
        }
    }

    private void OnResetView(object? sender, RoutedEventArgs e) => GetMap()?.ResetView();

    private void OnRotateLeft(object? sender, RoutedEventArgs e) => GetMap()?.Rotate(-15.0);

    private void OnRotateRight(object? sender, RoutedEventArgs e) => GetMap()?.Rotate(15.0);

    private void OnClose(object? sender, RoutedEventArgs e) => Close();

    private void OnWindowKeyDown(object? sender, KeyEventArgs e)
    {
        switch (e.Key)
        {
            case Key.Escape:
                Close();
                e.Handled = true;
                break;
            case Key.OemPlus:
            case Key.Add:
                OnZoomIn(this, new RoutedEventArgs());
                e.Handled = true;
                break;
            case Key.OemMinus:
            case Key.Subtract:
                OnZoomOut(this, new RoutedEventArgs());
                e.Handled = true;
                break;
            case Key.Q:
                OnRotateLeft(this, new RoutedEventArgs());
                e.Handled = true;
                break;
            case Key.E:
                OnRotateRight(this, new RoutedEventArgs());
                e.Handled = true;
                break;
            case Key.R:
                OnResetView(this, new RoutedEventArgs());
                e.Handled = true;
                break;
        }
    }
}
