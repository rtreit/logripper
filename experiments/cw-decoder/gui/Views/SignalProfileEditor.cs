using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using CwDecoderGui.Models;

namespace CwDecoderGui.Views;

internal sealed class SignalProfileEditor : Control
{
    public static readonly StyledProperty<IEnumerable<SignalProfilePoint>?> PointsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, IEnumerable<SignalProfilePoint>?>(nameof(Points));

    public static readonly StyledProperty<double> DisplayStartSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(DisplayStartSeconds));

    public static readonly StyledProperty<double> DisplayEndSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(DisplayEndSeconds));

    public static readonly StyledProperty<double> OriginalStartSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(OriginalStartSeconds));

    public static readonly StyledProperty<double> OriginalEndSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(OriginalEndSeconds));

    public static readonly StyledProperty<double> SuggestedStartSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(SuggestedStartSeconds));

    public static readonly StyledProperty<double> SuggestedEndSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(SuggestedEndSeconds));

    public static readonly StyledProperty<double> SelectionStartSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(
            nameof(SelectionStartSeconds),
            defaultBindingMode: Avalonia.Data.BindingMode.TwoWay);

    public static readonly StyledProperty<double> SelectionEndSecondsProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(
            nameof(SelectionEndSeconds),
            defaultBindingMode: Avalonia.Data.BindingMode.TwoWay);

    public static readonly StyledProperty<double> ThresholdProperty =
        AvaloniaProperty.Register<SignalProfileEditor, double>(nameof(Threshold));

    private const double HandleHitPixels = 10;
    private const double MinSelectionSeconds = 0.08;

    private DragMode _dragMode;
    private double _dragOffsetSeconds;
    private double _dragSelectionWidth;

    static SignalProfileEditor()
    {
        AffectsRender<SignalProfileEditor>(
            PointsProperty,
            DisplayStartSecondsProperty,
            DisplayEndSecondsProperty,
            OriginalStartSecondsProperty,
            OriginalEndSecondsProperty,
            SuggestedStartSecondsProperty,
            SuggestedEndSecondsProperty,
            SelectionStartSecondsProperty,
            SelectionEndSecondsProperty,
            ThresholdProperty);
    }

    public IEnumerable<SignalProfilePoint>? Points
    {
        get => GetValue(PointsProperty);
        set => SetValue(PointsProperty, value);
    }

    public double DisplayStartSeconds
    {
        get => GetValue(DisplayStartSecondsProperty);
        set => SetValue(DisplayStartSecondsProperty, value);
    }

    public double DisplayEndSeconds
    {
        get => GetValue(DisplayEndSecondsProperty);
        set => SetValue(DisplayEndSecondsProperty, value);
    }

    public double OriginalStartSeconds
    {
        get => GetValue(OriginalStartSecondsProperty);
        set => SetValue(OriginalStartSecondsProperty, value);
    }

    public double OriginalEndSeconds
    {
        get => GetValue(OriginalEndSecondsProperty);
        set => SetValue(OriginalEndSecondsProperty, value);
    }

    public double SuggestedStartSeconds
    {
        get => GetValue(SuggestedStartSecondsProperty);
        set => SetValue(SuggestedStartSecondsProperty, value);
    }

    public double SuggestedEndSeconds
    {
        get => GetValue(SuggestedEndSecondsProperty);
        set => SetValue(SuggestedEndSecondsProperty, value);
    }

    public double SelectionStartSeconds
    {
        get => GetValue(SelectionStartSecondsProperty);
        set => SetValue(SelectionStartSecondsProperty, value);
    }

    public double SelectionEndSeconds
    {
        get => GetValue(SelectionEndSecondsProperty);
        set => SetValue(SelectionEndSecondsProperty, value);
    }

    public double Threshold
    {
        get => GetValue(ThresholdProperty);
        set => SetValue(ThresholdProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        double w = double.IsFinite(availableSize.Width) ? availableSize.Width : 400;
        double h = double.IsFinite(availableSize.Height) ? availableSize.Height : 180;
        return new Size(w, h);
    }

    public override void Render(DrawingContext ctx)
    {
        base.Render(ctx);

        var bounds = Bounds;
        if (bounds.Width <= 4 || bounds.Height <= 4)
        {
            return;
        }

        ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(0x0A, 0x12, 0x1B)), bounds);
        ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0x3C, 0x55)), 1), bounds);

        var plot = new Rect(bounds.X + 10, bounds.Y + 10, Math.Max(10, bounds.Width - 20), Math.Max(20, bounds.Height - 28));
        var points = SnapshotPoints();
        if (points.Count == 0 || DisplayEndSeconds <= DisplayStartSeconds)
        {
            DrawLabel(ctx, "profile unavailable", plot.TopLeft + new Vector(8, 8), Color.FromRgb(0x7A, 0x91, 0xAC), 12);
            return;
        }

        DrawTimeGrid(ctx, plot);
        DrawActiveRuns(ctx, plot, points);
        DrawWave(ctx, plot, points);
        DrawThreshold(ctx, plot, points);
        DrawSpanGuide(ctx, plot, SuggestedStartSeconds, SuggestedEndSeconds, Color.FromArgb(0x90, 0x84, 0xFF, 0x6E), dashed: true);
        DrawSpanGuide(ctx, plot, OriginalStartSeconds, OriginalEndSeconds, Color.FromArgb(0x90, 0x22, 0xD3, 0xEE), dashed: true);
        DrawSelection(ctx, plot);

        DrawLabel(ctx, $"view {DisplayStartSeconds:F2}s - {DisplayEndSeconds:F2}s", new Point(plot.X, bounds.Bottom - 16), Color.FromRgb(0x7A, 0x91, 0xAC), 10);
        DrawLabel(ctx, $"selected {SelectionStartSeconds:F2}s - {SelectionEndSeconds:F2}s", new Point(plot.Right - 210, bounds.Bottom - 16), Color.FromRgb(0xE6, 0xF2, 0xFF), 10);
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (DisplayEndSeconds <= DisplayStartSeconds)
        {
            return;
        }

        var point = e.GetPosition(this);
        var sec = SecondsFromX(point.X);
        var startX = XFromSeconds(SelectionStartSeconds);
        var endX = XFromSeconds(SelectionEndSeconds);
        if (Math.Abs(point.X - startX) <= HandleHitPixels)
        {
            _dragMode = DragMode.StartHandle;
        }
        else if (Math.Abs(point.X - endX) <= HandleHitPixels)
        {
            _dragMode = DragMode.EndHandle;
        }
        else if (sec >= SelectionStartSeconds && sec <= SelectionEndSeconds)
        {
            _dragMode = DragMode.MoveSelection;
            _dragOffsetSeconds = sec - SelectionStartSeconds;
            _dragSelectionWidth = Math.Max(MinSelectionSeconds, SelectionEndSeconds - SelectionStartSeconds);
        }
        else
        {
            return;
        }

        e.Pointer.Capture(this);
        e.Handled = true;
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        base.OnPointerMoved(e);
        if (_dragMode == DragMode.None)
        {
            return;
        }

        var sec = SecondsFromX(e.GetPosition(this).X);
        switch (_dragMode)
        {
            case DragMode.StartHandle:
                SetCurrentValue(
                    SelectionStartSecondsProperty,
                    Math.Clamp(sec, DisplayStartSeconds, SelectionEndSeconds - MinSelectionSeconds));
                break;
            case DragMode.EndHandle:
                SetCurrentValue(
                    SelectionEndSecondsProperty,
                    Math.Clamp(sec, SelectionStartSeconds + MinSelectionSeconds, DisplayEndSeconds));
                break;
            case DragMode.MoveSelection:
                var newStart = Math.Clamp(
                    sec - _dragOffsetSeconds,
                    DisplayStartSeconds,
                    DisplayEndSeconds - _dragSelectionWidth);
                SetCurrentValue(SelectionStartSecondsProperty, newStart);
                SetCurrentValue(SelectionEndSecondsProperty, newStart + _dragSelectionWidth);
                break;
        }

        e.Handled = true;
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        base.OnPointerReleased(e);
        ReleaseDrag(e.Pointer);
    }

    protected override void OnPointerCaptureLost(PointerCaptureLostEventArgs e)
    {
        base.OnPointerCaptureLost(e);
        _dragMode = DragMode.None;
    }

    private void ReleaseDrag(IPointer pointer)
    {
        pointer.Capture(null);
        _dragMode = DragMode.None;
    }

    private List<SignalProfilePoint> SnapshotPoints()
    {
        var points = new List<SignalProfilePoint>();
        if (Points is null)
        {
            return points;
        }

        foreach (var point in Points)
        {
            points.Add(point);
        }

        return points;
    }

    private void DrawTimeGrid(DrawingContext ctx, Rect plot)
    {
        var pen = new Pen(new SolidColorBrush(Color.FromArgb(0x35, 0x33, 0x55, 0x77)), 1);
        for (int i = 1; i < 4; i++)
        {
            double x = plot.X + plot.Width * i / 4.0;
            ctx.DrawLine(pen, new Point(x, plot.Y), new Point(x, plot.Bottom));
        }
    }

    private void DrawActiveRuns(DrawingContext ctx, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        var brush = new SolidColorBrush(Color.FromArgb(0x30, 0x22, 0xD3, 0xEE));
        int start = -1;
        for (int i = 0; i < points.Count; i++)
        {
            if (points[i].Active)
            {
                start = start < 0 ? i : start;
                continue;
            }

            if (start >= 0)
            {
                DrawActiveRun(ctx, plot, points, start, i - 1, brush);
                start = -1;
            }
        }

        if (start >= 0)
        {
            DrawActiveRun(ctx, plot, points, start, points.Count - 1, brush);
        }
    }

    private void DrawActiveRun(DrawingContext ctx, Rect plot, IReadOnlyList<SignalProfilePoint> points, int startIndex, int endIndex, IBrush brush)
    {
        double x0 = XFromSeconds(points[startIndex].TimeSeconds, plot);
        double x1 = XFromSeconds(points[endIndex].TimeSeconds, plot);
        ctx.FillRectangle(brush, new Rect(Math.Min(x0, x1), plot.Y, Math.Max(2, Math.Abs(x1 - x0)), plot.Height));
    }

    private void DrawWave(DrawingContext ctx, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        double maxPower = Math.Max(Threshold * 1.2, 1e-9);
        foreach (var point in points)
        {
            maxPower = Math.Max(maxPower, point.Power);
        }

        var line = new StreamGeometry();
        var fill = new StreamGeometry();
        using var lc = line.Open();
        using var fc = fill.Open();
        var first = new Point(XFromSeconds(points[0].TimeSeconds, plot), YFromPower(points[0].Power, maxPower, plot));
        lc.BeginFigure(first, false);
        fc.BeginFigure(new Point(first.X, plot.Bottom), true);
        fc.LineTo(first);
        for (int i = 1; i < points.Count; i++)
        {
            var point = new Point(XFromSeconds(points[i].TimeSeconds, plot), YFromPower(points[i].Power, maxPower, plot));
            lc.LineTo(point);
            fc.LineTo(point);
        }
        fc.LineTo(new Point(XFromSeconds(points[^1].TimeSeconds, plot), plot.Bottom));
        fc.EndFigure(true);
        lc.EndFigure(false);

        var fillBrush = new LinearGradientBrush
        {
            StartPoint = new RelativePoint(0.5, 0, RelativeUnit.Relative),
            EndPoint = new RelativePoint(0.5, 1, RelativeUnit.Relative),
            GradientStops =
            {
                new GradientStop(Color.FromArgb(0x70, 0x22, 0xD3, 0xEE), 0.0),
                new GradientStop(Color.FromArgb(0x08, 0x22, 0xD3, 0xEE), 1.0),
            },
        };
        ctx.DrawGeometry(fillBrush, null, fill);
        ctx.DrawGeometry(null, new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0xD3, 0xEE)), 1.8), line);
    }

    private void DrawThreshold(DrawingContext ctx, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        double maxPower = Math.Max(Threshold * 1.2, 1e-9);
        foreach (var point in points)
        {
            maxPower = Math.Max(maxPower, point.Power);
        }

        double y = YFromPower(Threshold, maxPower, plot);
        ctx.DrawLine(new Pen(new SolidColorBrush(Color.FromRgb(0xFF, 0xC0, 0x5C)), 1), new Point(plot.X, y), new Point(plot.Right, y));
    }

    private void DrawSpanGuide(DrawingContext ctx, Rect plot, double startSeconds, double endSeconds, Color color, bool dashed)
    {
        if (endSeconds <= startSeconds)
        {
            return;
        }

        var pen = new Pen(new SolidColorBrush(color), 1)
        {
            DashStyle = dashed ? new DashStyle(new[] { 4d, 4d }, 0) : null,
        };
        double x0 = XFromSeconds(startSeconds, plot);
        double x1 = XFromSeconds(endSeconds, plot);
        ctx.DrawRectangle(null, pen, new Rect(Math.Min(x0, x1), plot.Y + 6, Math.Max(2, Math.Abs(x1 - x0)), plot.Height - 12));
    }

    private void DrawSelection(DrawingContext ctx, Rect plot)
    {
        if (SelectionEndSeconds <= SelectionStartSeconds)
        {
            return;
        }

        double x0 = XFromSeconds(SelectionStartSeconds, plot);
        double x1 = XFromSeconds(SelectionEndSeconds, plot);
        var left = Math.Min(x0, x1);
        var width = Math.Max(2, Math.Abs(x1 - x0));

        ctx.FillRectangle(new SolidColorBrush(Color.FromArgb(0x20, 0xFF, 0x5B, 0xD0)), new Rect(left, plot.Y, width, plot.Height));
        var pen = new Pen(new SolidColorBrush(Color.FromRgb(0xFF, 0x5B, 0xD0)), 2);
        ctx.DrawRectangle(null, pen, new Rect(left, plot.Y + 2, width, plot.Height - 4));

        DrawHandle(ctx, x0, plot);
        DrawHandle(ctx, x1, plot);
    }

    private static void DrawHandle(DrawingContext ctx, double x, Rect plot)
    {
        var rect = new Rect(x - 4, plot.Y + 4, 8, plot.Height - 8);
        ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(0xFF, 0x5B, 0xD0)), rect);
    }

    private double XFromSeconds(double seconds) => XFromSeconds(seconds, PlotRect());

    private double XFromSeconds(double seconds, Rect plot)
    {
        double span = Math.Max(0.001, DisplayEndSeconds - DisplayStartSeconds);
        double norm = (seconds - DisplayStartSeconds) / span;
        return plot.X + Math.Clamp(norm, 0.0, 1.0) * plot.Width;
    }

    private double SecondsFromX(double x)
    {
        var plot = PlotRect();
        double span = Math.Max(0.001, DisplayEndSeconds - DisplayStartSeconds);
        double norm = Math.Clamp((x - plot.X) / Math.Max(1, plot.Width), 0.0, 1.0);
        return DisplayStartSeconds + norm * span;
    }

    private Rect PlotRect() =>
        new(Bounds.X + 10, Bounds.Y + 10, Math.Max(10, Bounds.Width - 20), Math.Max(20, Bounds.Height - 28));

    private static double YFromPower(double power, double maxPower, Rect plot)
    {
        double norm = maxPower <= 0 ? 0 : Math.Clamp(power / maxPower, 0.0, 1.0);
        return plot.Bottom - norm * plot.Height;
    }

    private static void DrawLabel(DrawingContext ctx, string text, Point origin, Color color, double size)
    {
        var label = new FormattedText(
            text,
            System.Globalization.CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight,
            new Typeface("Consolas"),
            size,
            new SolidColorBrush(color));
        ctx.DrawText(label, origin);
    }

    private enum DragMode
    {
        None,
        StartHandle,
        EndHandle,
        MoveSelection,
    }
}
