using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using QsoRipper.Domain;

namespace QsoRipper.Gui.Controls;

/// <summary>
/// Azimuthal-equidistant projection rendered with native Avalonia drawing
/// primitives. Centered on the local station; the contact's bearing
/// becomes its angular position on the disk and its distance from the
/// station maps linearly to the radial distance from the center.
/// </summary>
internal sealed class AzimuthalMapControl : Control
{
    private const double MaxRadiusKm = 20015.0;

    public static readonly StyledProperty<GreatCirclePath?> PathProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, GreatCirclePath?>(nameof(Path));

    static AzimuthalMapControl()
    {
        AffectsRender<AzimuthalMapControl>(PathProperty);
    }

    public GreatCirclePath? Path
    {
        get => GetValue(PathProperty);
        set => SetValue(PathProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        var w = double.IsFinite(availableSize.Width) ? availableSize.Width : 240;
        var h = double.IsFinite(availableSize.Height) ? availableSize.Height : 240;
        var side = Math.Max(160, Math.Min(w, h));
        return new Size(side, side);
    }

    public override void Render(DrawingContext context)
    {
        var bounds = new Rect(Bounds.Size);
        var size = Math.Min(bounds.Width, bounds.Height);
        if (size <= 4)
        {
            return;
        }

        var center = new Point(bounds.Width / 2.0, bounds.Height / 2.0);
        var radius = (size / 2.0) - 6.0;

        var diskBrush = new RadialGradientBrush
        {
            Center = new RelativePoint(0.5, 0.5, RelativeUnit.Relative),
            GradientOrigin = new RelativePoint(0.5, 0.5, RelativeUnit.Relative),
            RadiusX = new RelativeScalar(0.5, RelativeUnit.Relative),
            RadiusY = new RelativeScalar(0.5, RelativeUnit.Relative),
        };
        diskBrush.GradientStops.Add(new GradientStop(Color.Parse("#142a3e"), 0.0));
        diskBrush.GradientStops.Add(new GradientStop(Color.Parse("#0a1424"), 1.0));
        context.DrawEllipse(diskBrush, new Pen(Brushes.Transparent), center, radius, radius);

        var ringPen = new Pen(new SolidColorBrush(Color.Parse("#2a3e5a")), 1.0)
        {
            DashStyle = new DashStyle(new double[] { 2, 4 }, 0),
        };
        foreach (var km in new[] { 5000.0, 10000.0, 15000.0 })
        {
            var r = radius * (km / MaxRadiusKm);
            context.DrawEllipse(Brushes.Transparent, ringPen, center, r, r);
        }
        var outerPen = new Pen(new SolidColorBrush(Color.Parse("#3a5078")), 1.5);
        context.DrawEllipse(Brushes.Transparent, outerPen, center, radius, radius);

        var crosshairPen = new Pen(new SolidColorBrush(Color.Parse("#1f324a")), 1.0);
        context.DrawLine(crosshairPen, new Point(center.X - radius, center.Y), new Point(center.X + radius, center.Y));
        context.DrawLine(crosshairPen, new Point(center.X, center.Y - radius), new Point(center.X, center.Y + radius));
        DrawCardinalLabel(context, "N", new Point(center.X, center.Y - radius - 12), TextAlignment.Center);
        DrawCardinalLabel(context, "S", new Point(center.X, center.Y + radius + 2), TextAlignment.Center);
        DrawCardinalLabel(context, "E", new Point(center.X + radius + 4, center.Y - 7), TextAlignment.Left);
        DrawCardinalLabel(context, "W", new Point(center.X - radius - 12, center.Y - 7), TextAlignment.Left);

        var path = Path;
        if (path is null || path.Origin is null || path.Target is null || path.Samples.Count < 2)
        {
            return;
        }

        var origin = path.Origin;
        var projected = new List<Point>(path.Samples.Count);
        foreach (var sample in path.Samples)
        {
            projected.Add(ProjectPoint(origin, sample, center, radius));
        }

        var glowPen = new Pen(new SolidColorBrush(Color.FromArgb(0x55, 0x00, 0xd4, 0xff)), 5.0)
        {
            LineCap = PenLineCap.Round,
            LineJoin = PenLineJoin.Round,
        };
        var corePen = new Pen(new SolidColorBrush(Color.Parse("#00d4ff")), 1.8)
        {
            LineCap = PenLineCap.Round,
            LineJoin = PenLineJoin.Round,
        };
        var geometry = new StreamGeometry();
        using (var ctx = geometry.Open())
        {
            ctx.BeginFigure(projected[0], false);
            for (var i = 1; i < projected.Count; i++)
            {
                ctx.LineTo(projected[i]);
            }
            ctx.EndFigure(false);
        }
        context.DrawGeometry(Brushes.Transparent, glowPen, geometry);
        context.DrawGeometry(Brushes.Transparent, corePen, geometry);

        var stationFill = new SolidColorBrush(Color.Parse("#ffcc44"));
        context.DrawEllipse(
            new SolidColorBrush(Color.FromArgb(0x55, 0xff, 0xcc, 0x44)),
            new Pen(Brushes.Transparent),
            center, 9.0, 9.0);
        context.DrawEllipse(stationFill, new Pen(stationFill), center, 4.5, 4.5);

        var contactPoint = projected[^1];
        var contactFill = new SolidColorBrush(Color.Parse("#ff5577"));
        context.DrawEllipse(
            new SolidColorBrush(Color.FromArgb(0x66, 0xff, 0x55, 0x77)),
            new Pen(Brushes.Transparent),
            contactPoint, 9.0, 9.0);
        context.DrawEllipse(contactFill, new Pen(contactFill), contactPoint, 4.5, 4.5);
    }

    private static Point ProjectPoint(GeoPoint origin, GeoPoint sample, Point center, double radius)
    {
        var distance = HaversineKm(origin, sample);
        var bearing = InitialBearingDeg(origin, sample);
        var rNorm = Math.Min(1.0, distance / MaxRadiusKm);
        var angleRad = bearing.HasValue ? bearing.Value * Math.PI / 180.0 : 0.0;
        var x = center.X + (radius * rNorm * Math.Sin(angleRad));
        var y = center.Y - (radius * rNorm * Math.Cos(angleRad));
        return new Point(x, y);
    }

    private static double HaversineKm(GeoPoint a, GeoPoint b)
    {
        const double R = 6371.0088;
        var lat1 = a.Latitude * Math.PI / 180.0;
        var lat2 = b.Latitude * Math.PI / 180.0;
        var dLat = (b.Latitude - a.Latitude) * Math.PI / 180.0;
        var dLon = (b.Longitude - a.Longitude) * Math.PI / 180.0;
        var h = (Math.Sin(dLat / 2) * Math.Sin(dLat / 2))
            + (Math.Cos(lat1) * Math.Cos(lat2) * Math.Sin(dLon / 2) * Math.Sin(dLon / 2));
        return 2.0 * R * Math.Atan2(Math.Sqrt(h), Math.Sqrt(1.0 - h));
    }

    private static double? InitialBearingDeg(GeoPoint a, GeoPoint b)
    {
        var same = Math.Abs(a.Latitude - b.Latitude) < 1e-9
            && Math.Abs(a.Longitude - b.Longitude) < 1e-9;
        if (same)
        {
            return null;
        }
        var lat1 = a.Latitude * Math.PI / 180.0;
        var lat2 = b.Latitude * Math.PI / 180.0;
        var dLon = (b.Longitude - a.Longitude) * Math.PI / 180.0;
        var y = Math.Sin(dLon) * Math.Cos(lat2);
        var x = (Math.Cos(lat1) * Math.Sin(lat2)) - (Math.Sin(lat1) * Math.Cos(lat2) * Math.Cos(dLon));
        var deg = Math.Atan2(y, x) * 180.0 / Math.PI;
        var normalized = deg - (Math.Floor(deg / 360.0) * 360.0);
        return double.IsNaN(normalized) ? 0.0 : normalized;
    }

    private static void DrawCardinalLabel(DrawingContext context, string text, Point origin, TextAlignment alignment)
    {
        var formatted = new FormattedText(
            text,
            System.Globalization.CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight,
            Typeface.Default,
            10.0,
            new SolidColorBrush(Color.Parse("#7a8aa6")))
        {
            TextAlignment = alignment,
        };
        context.DrawText(formatted, origin);
    }
}
