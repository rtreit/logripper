using System;
using System.Collections.Generic;
using System.IO;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
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

    public static readonly StyledProperty<double> ScaleKmProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, double>(nameof(ScaleKm), MaxRadiusKm);

    public static readonly StyledProperty<double> ZoomProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, double>(nameof(Zoom), 1.0);

    public static readonly StyledProperty<double> PanXProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, double>(nameof(PanX), 0.0);

    public static readonly StyledProperty<double> PanYProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, double>(nameof(PanY), 0.0);

    public static readonly StyledProperty<double> RotationDegProperty =
        AvaloniaProperty.Register<AzimuthalMapControl, double>(nameof(RotationDeg), 0.0);

    static AzimuthalMapControl()
    {
        AffectsRender<AzimuthalMapControl>(PathProperty);
        AffectsRender<AzimuthalMapControl>(CountryLabelProperty);
        AffectsRender<AzimuthalMapControl>(ScaleKmProperty);
        AffectsRender<AzimuthalMapControl>(ZoomProperty);
        AffectsRender<AzimuthalMapControl>(PanXProperty);
        AffectsRender<AzimuthalMapControl>(PanYProperty);
        AffectsRender<AzimuthalMapControl>(RotationDegProperty);
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

    /// <summary>
    /// Radial extent of the disk in kilometres. Defaults to a full hemisphere
    /// (~20015 km). Smaller values zoom in for nearby contacts.
    /// </summary>
    public double ScaleKm
    {
        get => GetValue(ScaleKmProperty);
        set => SetValue(ScaleKmProperty, value);
    }

    /// <summary>User-controlled zoom multiplier. Effective scale = ScaleKm / Zoom.</summary>
    public double Zoom
    {
        get => GetValue(ZoomProperty);
        set => SetValue(ZoomProperty, value);
    }

    /// <summary>Horizontal pan in pixels (user drag offset).</summary>
    public double PanX
    {
        get => GetValue(PanXProperty);
        set => SetValue(PanXProperty, value);
    }

    /// <summary>Vertical pan in pixels (user drag offset).</summary>
    public double PanY
    {
        get => GetValue(PanYProperty);
        set => SetValue(PanYProperty, value);
    }

    /// <summary>Rotation of the projection in degrees clockwise. 0 = North up.</summary>
    public double RotationDeg
    {
        get => GetValue(RotationDegProperty);
        set => SetValue(RotationDegProperty, value);
    }

    public void ResetView()
    {
        Zoom = 1.0;
        PanX = 0.0;
        PanY = 0.0;
        RotationDeg = 0.0;
    }

    public void Rotate(double deltaDeg)
    {
        var v = (RotationDeg + deltaDeg) % 360.0;
        if (v < -180)
        {
            v += 360;
        }
        if (v > 180)
        {
            v -= 360;
        }
        RotationDeg = v;
    }

    /// <summary>Enables mouse wheel zoom and click-drag pan. Off by default.</summary>
    public bool IsInteractive { get; set; }

    private Point? _dragStart;
    private double _dragStartPanX;
    private double _dragStartPanY;

    public AzimuthalMapControl()
    {
        ClipToBounds = true;
    }

    protected override void OnPointerWheelChanged(PointerWheelEventArgs e)
    {
        base.OnPointerWheelChanged(e);
        if (!IsInteractive)
        {
            return;
        }
        if ((e.KeyModifiers & KeyModifiers.Shift) != 0)
        {
            Rotate(e.Delta.Y > 0 ? -10.0 : 10.0);
            e.Handled = true;
            return;
        }
        var factor = e.Delta.Y > 0 ? 1.2 : 1.0 / 1.2;
        var newZoom = Math.Clamp(Zoom * factor, 1.0, 32.0);
        if (Math.Abs(newZoom - Zoom) > 1e-4)
        {
            Zoom = newZoom;
        }
        e.Handled = true;
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (!IsInteractive)
        {
            return;
        }
        var props = e.GetCurrentPoint(this).Properties;
        if (props.IsLeftButtonPressed)
        {
            _dragStart = e.GetPosition(this);
            _dragStartPanX = PanX;
            _dragStartPanY = PanY;
            e.Pointer.Capture(this);
            e.Handled = true;
        }
        else if (props.IsRightButtonPressed)
        {
            ResetView();
            e.Handled = true;
        }
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        base.OnPointerMoved(e);
        if (!IsInteractive || _dragStart is null)
        {
            return;
        }
        var p = e.GetPosition(this);
        PanX = _dragStartPanX + (p.X - _dragStart.Value.X);
        PanY = _dragStartPanY + (p.Y - _dragStart.Value.Y);
        e.Handled = true;
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        base.OnPointerReleased(e);
        if (_dragStart is not null)
        {
            _dragStart = null;
            e.Pointer.Capture(null);
            e.Handled = true;
        }
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

        var center = new Point((bounds.Width / 2.0) + PanX, (bounds.Height / 2.0) + PanY);
        var radius = (size / 2.0) - 6.0;
        var zoom = Math.Clamp(double.IsFinite(Zoom) && Zoom > 0 ? Zoom : 1.0, 0.25, 64.0);
        var baseScale = ScaleKm > 0 ? ScaleKm : MaxRadiusKm;
        var scaleKm = Math.Clamp(baseScale / zoom, 25.0, MaxRadiusKm);
        var rotationDeg = double.IsFinite(RotationDeg) ? RotationDeg : 0.0;
        var rotRad = rotationDeg * Math.PI / 180.0;

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
        var ringSteps = ChooseRingSteps(scaleKm);
        foreach (var ringKm in ringSteps)
        {
            var r = radius * (ringKm / scaleKm);
            if (r < 4 || r > radius - 0.5)
            {
                continue;
            }
            context.DrawEllipse(Brushes.Transparent, ringPen, center, r, r);
            DrawRingLabel(context, FormatKm(ringKm), new Point(center.X + 3, center.Y - r - 1));
        }
        var outerPen = new Pen(new SolidColorBrush(Color.Parse("#3a5078")), 1.5);
        context.DrawEllipse(Brushes.Transparent, outerPen, center, radius, radius);
        DrawRingLabel(context, FormatKm(scaleKm), new Point(center.X + 3, center.Y - radius - 1));

        var crosshairPen = new Pen(new SolidColorBrush(Color.Parse("#1f324a")), 1.0);
        // Crosshair rotates with the projection so it stays aligned with cardinal axes.
        var ch1 = RotateAround(new Point(center.X - radius, center.Y), center, rotRad);
        var ch2 = RotateAround(new Point(center.X + radius, center.Y), center, rotRad);
        var ch3 = RotateAround(new Point(center.X, center.Y - radius), center, rotRad);
        var ch4 = RotateAround(new Point(center.X, center.Y + radius), center, rotRad);
        context.DrawLine(crosshairPen, ch1, ch2);
        context.DrawLine(crosshairPen, ch3, ch4);

        var nPos = RotateAround(new Point(center.X, center.Y - radius - 12), center, rotRad);
        var sPos = RotateAround(new Point(center.X, center.Y + radius + 2), center, rotRad);
        var ePos = RotateAround(new Point(center.X + radius + 4, center.Y - 7), center, rotRad);
        var wPos = RotateAround(new Point(center.X - radius - 12, center.Y - 7), center, rotRad);
        DrawCardinalLabel(context, "N", nPos, TextAlignment.Center);
        DrawCardinalLabel(context, "S", sPos, TextAlignment.Center);
        DrawCardinalLabel(context, "E", ePos, TextAlignment.Left);
        DrawCardinalLabel(context, "W", wPos, TextAlignment.Left);

        var path = Path;
        if (path?.Origin is { } mapOrigin)
        {
            DrawPolylineLayer(context, BorderLines, mapOrigin, center, radius, scaleKm, rotRad,
                Color.FromArgb(0x55, 0x6a, 0x86, 0xb4), 0.7);
            DrawPolylineLayer(context, CoastlineLines, mapOrigin, center, radius, scaleKm, rotRad,
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
            projected.Add(ProjectPoint(origin, sample, center, radius, scaleKm, rotRad));
        }

        var lineThickness = Math.Max(0.9, radius / 130.0);
        var pathBrush = new SolidColorBrush(Color.FromArgb(0xe0, 0x6b, 0xe6, 0xff));
        var pathPen = new Pen(pathBrush, lineThickness)
        {
            LineCap = PenLineCap.Round,
            LineJoin = PenLineJoin.Round,
            DashStyle = new DashStyle(new double[] { 5, 3 }, 0),
        };
        var glowPen = new Pen(new SolidColorBrush(Color.FromArgb(0x35, 0x00, 0xd4, 0xff)), lineThickness * 2.4)
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
        context.DrawGeometry(Brushes.Transparent, pathPen, geometry);

        var dotCore = Math.Max(1.6, radius / 85.0);
        var dotHalo = dotCore * 2.4;

        var stationFill = new SolidColorBrush(Color.Parse("#ffcc44"));
        context.DrawEllipse(
            new SolidColorBrush(Color.FromArgb(0x40, 0xff, 0xcc, 0x44)),
            new Pen(Brushes.Transparent),
            center, dotHalo, dotHalo);
        context.DrawEllipse(stationFill, null, center, dotCore, dotCore);

        var contactPoint = projected[^1];
        var contactFill = new SolidColorBrush(Color.Parse("#ff5577"));
        context.DrawEllipse(
            new SolidColorBrush(Color.FromArgb(0x4a, 0xff, 0x55, 0x77)),
            new Pen(Brushes.Transparent),
            contactPoint, dotHalo, dotHalo);
        context.DrawEllipse(contactFill, null, contactPoint, dotCore, dotCore);
    }

    private static Point ProjectPoint(GeoPoint origin, GeoPoint sample, Point center, double radius, double scaleKm, double rotationRad)
    {
        var distance = HaversineKm(origin, sample);
        var bearing = InitialBearingDeg(origin, sample);
        var rNorm = Math.Min(1.0, distance / scaleKm);
        var angleRad = (bearing.HasValue ? bearing.Value * Math.PI / 180.0 : 0.0) + rotationRad;
        var x = center.X + (radius * rNorm * Math.Sin(angleRad));
        var y = center.Y - (radius * rNorm * Math.Cos(angleRad));
        return new Point(x, y);
    }

    private static Point RotateAround(Point p, Point center, double rotationRad)
    {
        if (Math.Abs(rotationRad) < 1e-6)
        {
            return p;
        }
        var dx = p.X - center.X;
        var dy = p.Y - center.Y;
        var c = Math.Cos(rotationRad);
        var s = Math.Sin(rotationRad);
        return new Point(center.X + (dx * c) - (dy * s), center.Y + (dx * s) + (dy * c));
    }

    private static List<double> ChooseRingSteps(double scaleKm)
    {
        // pick a "nice" step size such that we get 2-4 rings inside the disk
        var roughStep = scaleKm / 4.0;
        var pow = Math.Pow(10, Math.Floor(Math.Log10(roughStep)));
        var n = roughStep / pow;
        double step;
        if (n < 1.5)
        {
            step = pow;
        }
        else if (n < 3.5)
        {
            step = 2 * pow;
        }
        else if (n < 7.5)
        {
            step = 5 * pow;
        }
        else
        {
            step = 10 * pow;
        }
        var rings = new List<double>(5);
        for (var d = step; d < scaleKm - (step * 0.1); d += step)
        {
            rings.Add(d);
        }
        return rings;
    }

    private static string FormatKm(double km)
    {
        if (km >= 1000)
        {
            var thousands = km / 1000.0;
            return thousands >= 10
                ? $"{thousands:F0}k"
                : $"{thousands:0.#}k";
        }
        return $"{km:F0}";
    }

    private static void DrawRingLabel(DrawingContext context, string text, Point origin)
    {
        var formatted = new FormattedText(
            text,
            System.Globalization.CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight,
            Typeface.Default,
            8.5,
            new SolidColorBrush(Color.FromArgb(0xb0, 0x6e, 0x88, 0xa8)))
        {
            TextAlignment = TextAlignment.Left,
        };
        context.DrawText(formatted, origin);
    }

    private static void DrawPolylineLayer(
        DrawingContext context,
        Lazy<List<IReadOnlyList<GeoPoint>>> source,
        GeoPoint origin,
        Point center,
        double radius,
        double scaleKm,
        double rotationRad,
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
                ProjectPolylineInto(ctx, polyline, origin, center, radius, scaleKm, rotationRad);
            }
        }
        context.DrawGeometry(Brushes.Transparent, pen, geometry);
    }

    private static void ProjectPolylineInto(StreamGeometryContext ctx, IReadOnlyList<GeoPoint> polyline, GeoPoint origin, Point center, double radius, double scaleKm, double rotationRad)
    {
        if (polyline.Count < 2)
        {
            return;
        }

        var antipodeCull = Math.Min(AntipodeCullKm, scaleKm);
        var subdivideThreshold = Math.Max(60.0, Math.Min(SubdivideThresholdKm, scaleKm / 30.0));

        var penDown = false;
        var prev = polyline[0];
        var prevDist = HaversineKm(origin, prev);
        for (var i = 1; i < polyline.Count; i++)
        {
            var curr = polyline[i];
            var currDist = HaversineKm(origin, curr);

            if (prevDist >= antipodeCull && currDist >= antipodeCull)
            {
                penDown = false;
                prev = curr;
                prevDist = currDist;
                continue;
            }

            var segDist = HaversineKm(prev, curr);
            var steps = segDist > subdivideThreshold
                ? Math.Min(MaxSubdivisionSteps, (int)Math.Ceiling(segDist / subdivideThreshold))
                : 1;

            if (!penDown)
            {
                ctx.BeginFigure(ProjectPoint(origin, prev, center, radius, scaleKm, rotationRad), false);
                penDown = true;
            }

            for (var s = 1; s <= steps; s++)
            {
                var t = (double)s / steps;
                var sub = steps == 1 ? curr : Slerp(prev, curr, t);
                ctx.LineTo(ProjectPoint(origin, sub, center, radius, scaleKm, rotationRad));
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
