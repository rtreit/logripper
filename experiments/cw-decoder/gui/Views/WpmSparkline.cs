using System;
using System.Collections.Generic;
using System.Collections.Specialized;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace CwDecoderGui.Views;

/// <summary>
/// Live WPM history sparkline. Bind <see cref="Values"/> to an
/// ObservableCollection&lt;double&gt; and the chart re-renders on every change.
/// Auto-scales the y-axis between 0 and 1.25× current peak (with a 5 WPM floor),
/// draws a fading gradient under the polyline, and overlays the latest value.
/// </summary>
internal sealed class WpmSparkline : Control
{
    public static readonly StyledProperty<System.Collections.IEnumerable?> ValuesProperty =
        AvaloniaProperty.Register<WpmSparkline, System.Collections.IEnumerable?>(nameof(Values));

    public static readonly StyledProperty<double> CurrentProperty =
        AvaloniaProperty.Register<WpmSparkline, double>(nameof(Current));

    static WpmSparkline()
    {
        ValuesProperty.Changed.AddClassHandler<WpmSparkline>((c, e) => c.OnValuesChanged(e));
        AffectsRender<WpmSparkline>(CurrentProperty);
    }

    private INotifyCollectionChanged? _hooked;

    public System.Collections.IEnumerable? Values
    {
        get => GetValue(ValuesProperty);
        set => SetValue(ValuesProperty, value);
    }
    public double Current
    {
        get => GetValue(CurrentProperty);
        set => SetValue(CurrentProperty, value);
    }

    private void OnValuesChanged(AvaloniaPropertyChangedEventArgs e)
    {
        if (_hooked is not null) _hooked.CollectionChanged -= OnCollectionChanged;
        _hooked = e.NewValue as INotifyCollectionChanged;
        if (_hooked is not null) _hooked.CollectionChanged += OnCollectionChanged;
        InvalidateVisual();
    }

    private void OnCollectionChanged(object? sender, NotifyCollectionChangedEventArgs e) => InvalidateVisual();

    public override void Render(DrawingContext ctx)
    {
        base.Render(ctx);
        var b = Bounds;
        if (b.Width <= 4 || b.Height <= 4) return;

        var bg = new SolidColorBrush(Color.FromRgb(0x0A, 0x12, 0x1B));
        ctx.FillRectangle(bg, b);

        // Frame
        var frame = new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0x3C, 0x55)), 1);
        ctx.DrawRectangle(null, frame, b);

        // Snapshot points
        var pts = new List<double>();
        if (Values is not null)
            foreach (var v in Values) if (v is double d) pts.Add(d);

        // y axis bounds
        double yMax = 5.0;
        foreach (var v in pts) if (v > yMax) yMax = v;
        yMax = Math.Ceiling(yMax * 1.25 / 5.0) * 5.0;
        if (yMax < 10) yMax = 10;

        // Horizontal grid lines every 5 WPM
        var gridPen = new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1);
        var labelBrush = new SolidColorBrush(Color.FromRgb(0x7A, 0x91, 0xAC));
        var typeface = new Typeface("Consolas");
        for (double w = 5; w <= yMax; w += 5)
        {
            double y = b.Y + b.Height - (w / yMax) * b.Height;
            ctx.DrawLine(gridPen, new Point(b.X, y), new Point(b.X + b.Width, y));
            var ft = new FormattedText($"{w:0}", System.Globalization.CultureInfo.InvariantCulture,
                FlowDirection.LeftToRight, typeface, 9, labelBrush);
            ctx.DrawText(ft, new Point(b.X + 4, y - ft.Height));
        }

        if (pts.Count < 2) return;

        // Polyline + fill
        double xStep = b.Width / Math.Max(pts.Count - 1, 1);
        var line = new StreamGeometry();
        var fill = new StreamGeometry();
        using (var lc = line.Open())
        using (var fc = fill.Open())
        {
            var first = new Point(b.X, b.Y + b.Height - (pts[0] / yMax) * b.Height);
            lc.BeginFigure(first, false);
            fc.BeginFigure(new Point(b.X, b.Y + b.Height), true);
            fc.LineTo(first);
            for (int i = 1; i < pts.Count; i++)
            {
                var p = new Point(b.X + i * xStep, b.Y + b.Height - (pts[i] / yMax) * b.Height);
                lc.LineTo(p);
                fc.LineTo(p);
            }
            fc.LineTo(new Point(b.X + b.Width, b.Y + b.Height));
            fc.EndFigure(true);
            lc.EndFigure(false);
        }

        var fillBrush = new LinearGradientBrush
        {
            StartPoint = new RelativePoint(0.5, 0, RelativeUnit.Relative),
            EndPoint = new RelativePoint(0.5, 1, RelativeUnit.Relative),
            GradientStops =
            {
                new GradientStop(Color.FromArgb(0x80, 0x22, 0xD3, 0xEE), 0.0),
                new GradientStop(Color.FromArgb(0x10, 0x22, 0xD3, 0xEE), 1.0),
            },
        };
        ctx.DrawGeometry(fillBrush, null, fill);

        var linePen = new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0xD3, 0xEE)), 2);
        ctx.DrawGeometry(null, linePen, line);

        // Last point dot
        var last = pts[^1];
        var lastPt = new Point(b.X + (pts.Count - 1) * xStep, b.Y + b.Height - (last / yMax) * b.Height);
        ctx.DrawEllipse(new SolidColorBrush(Color.FromRgb(0x84, 0xFF, 0x6E)), null, lastPt, 4, 4);

        // Live readout
        var readout = new FormattedText($"{Current:F1} WPM",
            System.Globalization.CultureInfo.InvariantCulture, FlowDirection.LeftToRight,
            new Typeface("Consolas"), 14,
            new SolidColorBrush(Color.FromRgb(0xE6, 0xF2, 0xFF)));
        ctx.DrawText(readout, new Point(b.X + b.Width - readout.Width - 8, b.Y + 6));
    }
}
