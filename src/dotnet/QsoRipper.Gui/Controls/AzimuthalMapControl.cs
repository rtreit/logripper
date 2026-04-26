using System;
using System.Collections.Generic;
using System.IO;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Platform;
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
    private const double AntipodeCullKm = 19500.0;
    private const double SubdivideThresholdKm = 600.0;
    private const int MaxSubdivisionSteps = 12;

    private static readonly Lazy<List<IReadOnlyList<GeoPoint>>> CoastlineLines =
        new(() => LoadPolylineResource("avares://QsoRipper.Gui/Assets/coastlines-110m.txt"), isThreadSafe: true);

    private static readonly Lazy<List<IReadOnlyList<GeoPoint>>> BorderLines =
        new(() => LoadPolylineResource("avares://QsoRipper.Gui/Assets/borders-110m.txt"), isThreadSafe: true);

    public static readonly StyledProperty<GreatCirclePath?> PathProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, GreatCirclePath?>(nameof(Path));

    public static readonly StyledProperty<string?> CountryLabelProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, string?>(nameof(CountryLabel));

    static AzimuthalMapControl()
    {
        AffectsRender<AzimuthalMapControl>(PathProperty);
        AffectsRender<AzimuthalMapControl>(CountryLabelProperty);
    }

    public GreatCirclePath? Path
    {
        get => GetValue(PathProperty);
        set => SetValue(PathProperty, value);
    }

    public string? CountryLabel
    {
        get => GetValue(CountryLabelProperty);
        set => SetValue(CountryLabelProperty, value);
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
        if (path?.Origin is { } mapOrigin)
        {
            DrawPolylineLayer(context, BorderLines, mapOrigin, center, radius,
                Color.FromArgb(0x55, 0x6a, 0x86, 0xb4), 0.7);
            DrawPolylineLayer(context, CoastlineLines, mapOrigin, center, radius,
                Color.FromArgb(0xc8, 0x88, 0xb4, 0xe0), 0.95);
        }

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

        DrawCountryLabel(context, contactPoint, center, bounds);
    }

    private void DrawCountryLabel(DrawingContext context, Point contactPoint, Point center, Rect bounds)
    {
        var label = CountryLabel;
        if (string.IsNullOrWhiteSpace(label))
        {
            return;
        }

        var formatted = new FormattedText(
            label.ToUpperInvariant(),
            System.Globalization.CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight,
            new Typeface(FontFamily.Default, FontStyle.Normal, FontWeight.SemiBold),
            10.5,
            new SolidColorBrush(Color.Parse("#ffd6e0")));

        var width = formatted.Width;
        var height = formatted.Height;

        var dx = contactPoint.X - center.X;
        var dy = contactPoint.Y - center.Y;
        var len = Math.Sqrt((dx * dx) + (dy * dy));
        var ux = len > 1e-3 ? dx / len : 0.0;
        var uy = len > 1e-3 ? dy / len : -1.0;

        var offset = 14.0;
        var x = contactPoint.X + (ux * offset) - (width / 2.0);
        var y = contactPoint.Y + (uy * offset) - (height / 2.0);

        x = Math.Max(4, Math.Min(bounds.Width - width - 4, x));
        y = Math.Max(2, Math.Min(bounds.Height - height - 2, y));

        var pad = 4.0;
        var bgRect = new Rect(x - pad, y - 1, width + (pad * 2), height + 2);
        context.DrawRectangle(
            new SolidColorBrush(Color.FromArgb(0xc0, 0x10, 0x1c, 0x30)),
            new Pen(new SolidColorBrush(Color.FromArgb(0x90, 0xff, 0x55, 0x77)), 0.8),
            bgRect,
            3.0,
            3.0);
        context.DrawText(formatted, new Point(x, y));
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

    private static void DrawPolylineLayer(
        DrawingContext context,
        Lazy<List<IReadOnlyList<GeoPoint>>> source,
        GeoPoint origin,
        Point center,
        double radius,
        Color color,
        double thickness)
    {
        List<IReadOnlyList<GeoPoint>> lines;
        try
        {
            lines = source.Value;
        }
        catch (IOException)
        {
            return;
        }
        catch (UriFormatException)
        {
            return;
        }
        if (lines.Count == 0)
        {
            return;
        }

        var pen = new Pen(new SolidColorBrush(color), thickness)
        {
            LineCap = PenLineCap.Round,
            LineJoin = PenLineJoin.Round,
        };

        var geometry = new StreamGeometry();
        using (var ctx = geometry.Open())
        {
            foreach (var polyline in lines)
            {
                ProjectPolylineInto(ctx, polyline, origin, center, radius);
            }
        }
        context.DrawGeometry(Brushes.Transparent, pen, geometry);
    }

    private static void DrawCoastlines(DrawingContext context, GeoPoint origin, Point center, double radius)
    {
        DrawPolylineLayer(context, CoastlineLines, origin, center, radius,
            Color.FromArgb(0xc8, 0x88, 0xb4, 0xe0), 0.95);
    }

    private static void ProjectPolylineInto(StreamGeometryContext ctx, IReadOnlyList<GeoPoint> polyline, GeoPoint origin, Point center, double radius)
    {
        if (polyline.Count < 2)
        {
            return;
        }

        var penDown = false;
        var prev = polyline[0];
        var prevDist = HaversineKm(origin, prev);
        for (var i = 1; i < polyline.Count; i++)
        {
            var curr = polyline[i];
            var currDist = HaversineKm(origin, curr);

            if (prevDist >= AntipodeCullKm && currDist >= AntipodeCullKm)
            {
                penDown = false;
                prev = curr;
                prevDist = currDist;
                continue;
            }

            var segDist = HaversineKm(prev, curr);
            var steps = segDist > SubdivideThresholdKm
                ? Math.Min(MaxSubdivisionSteps, (int)Math.Ceiling(segDist / SubdivideThresholdKm))
                : 1;

            if (!penDown)
            {
                ctx.BeginFigure(ProjectPoint(origin, prev, center, radius), false);
                penDown = true;
            }

            for (var s = 1; s <= steps; s++)
            {
                var t = (double)s / steps;
                var sub = steps == 1 ? curr : Slerp(prev, curr, t);
                ctx.LineTo(ProjectPoint(origin, sub, center, radius));
            }

            prev = curr;
            prevDist = currDist;
        }
        if (penDown)
        {
            ctx.EndFigure(false);
        }
    }

    private static GeoPoint Slerp(GeoPoint a, GeoPoint b, double t)
    {
        var lat1 = a.Latitude * Math.PI / 180.0;
        var lon1 = a.Longitude * Math.PI / 180.0;
        var lat2 = b.Latitude * Math.PI / 180.0;
        var lon2 = b.Longitude * Math.PI / 180.0;
        var dLat = (lat2 - lat1) / 2.0;
        var dLon = (lon2 - lon1) / 2.0;
        var hav = (Math.Sin(dLat) * Math.Sin(dLat))
            + (Math.Cos(lat1) * Math.Cos(lat2) * Math.Sin(dLon) * Math.Sin(dLon));
        var d = 2.0 * Math.Atan2(Math.Sqrt(hav), Math.Sqrt(1.0 - hav));
        if (d < 1e-9)
        {
            return new GeoPoint { Latitude = a.Latitude, Longitude = a.Longitude };
        }
        var aSin = Math.Sin((1 - t) * d) / Math.Sin(d);
        var bSin = Math.Sin(t * d) / Math.Sin(d);
        var x = (aSin * Math.Cos(lat1) * Math.Cos(lon1)) + (bSin * Math.Cos(lat2) * Math.Cos(lon2));
        var y = (aSin * Math.Cos(lat1) * Math.Sin(lon1)) + (bSin * Math.Cos(lat2) * Math.Sin(lon2));
        var z = (aSin * Math.Sin(lat1)) + (bSin * Math.Sin(lat2));
        var lat = Math.Atan2(z, Math.Sqrt((x * x) + (y * y)));
        var lon = Math.Atan2(y, x);
        return new GeoPoint
        {
            Latitude = lat * 180.0 / Math.PI,
            Longitude = lon * 180.0 / Math.PI,
        };
    }

    private static List<IReadOnlyList<GeoPoint>> LoadPolylineResource(string assetUri)
    {
        var uri = new Uri(assetUri);
        using var stream = AssetLoader.Open(uri);
        using var reader = new StreamReader(stream);
        var lines = new List<IReadOnlyList<GeoPoint>>(160);
        string? raw;
        while ((raw = reader.ReadLine()) is not null)
        {
            if (raw.Length == 0)
            {
                continue;
            }
            var parts = raw.Split(';');
            var pts = new List<GeoPoint>(parts.Length);
            foreach (var part in parts)
            {
                var commaIdx = part.IndexOf(',', StringComparison.Ordinal);
                if (commaIdx <= 0 || commaIdx == part.Length - 1)
                {
                    continue;
                }
                if (!double.TryParse(part.AsSpan(0, commaIdx), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var lat))
                {
                    continue;
                }
                if (!double.TryParse(part.AsSpan(commaIdx + 1), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var lon))
                {
                    continue;
                }
                pts.Add(new GeoPoint { Latitude = lat, Longitude = lon });
            }
            if (pts.Count >= 2)
            {
                lines.Add(pts);
            }
        }
        return lines;
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
