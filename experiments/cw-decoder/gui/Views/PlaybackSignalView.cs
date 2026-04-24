using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using CwDecoderGui.Models;

namespace CwDecoderGui.Views;

internal sealed class PlaybackSignalView : Control
{
    public static readonly StyledProperty<IEnumerable<SignalProfilePoint>?> PointsProperty =
        AvaloniaProperty.Register<PlaybackSignalView, IEnumerable<SignalProfilePoint>?>(nameof(Points));

    public static readonly StyledProperty<double> DisplayStartSecondsProperty =
        AvaloniaProperty.Register<PlaybackSignalView, double>(nameof(DisplayStartSeconds));

    public static readonly StyledProperty<double> DisplayEndSecondsProperty =
        AvaloniaProperty.Register<PlaybackSignalView, double>(nameof(DisplayEndSeconds));

    public static readonly StyledProperty<double> ThresholdProperty =
        AvaloniaProperty.Register<PlaybackSignalView, double>(nameof(Threshold));

    public static readonly StyledProperty<double> PlayheadSecondsProperty =
        AvaloniaProperty.Register<PlaybackSignalView, double>(nameof(PlayheadSeconds));

    static PlaybackSignalView()
    {
        AffectsRender<PlaybackSignalView>(
            PointsProperty,
            DisplayStartSecondsProperty,
            DisplayEndSecondsProperty,
            ThresholdProperty,
            PlayheadSecondsProperty);
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

    public double Threshold
    {
        get => GetValue(ThresholdProperty);
        set => SetValue(ThresholdProperty, value);
    }

    public double PlayheadSeconds
    {
        get => GetValue(PlayheadSecondsProperty);
        set => SetValue(PlayheadSecondsProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        double width = double.IsFinite(availableSize.Width) ? availableSize.Width : 400;
        double height = double.IsFinite(availableSize.Height) ? availableSize.Height : 140;
        return new Size(width, height);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);

        var bounds = Bounds;
        if (bounds.Width <= 4 || bounds.Height <= 4)
        {
            return;
        }

        context.FillRectangle(new SolidColorBrush(Color.FromRgb(0x0A, 0x12, 0x1B)), bounds);
        context.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0x3C, 0x55)), 1), bounds);

        var plot = new Rect(bounds.X + 8, bounds.Y + 8, Math.Max(10, bounds.Width - 16), Math.Max(20, bounds.Height - 24));
        var points = SnapshotPoints();
        if (points.Count == 0 || DisplayEndSeconds <= DisplayStartSeconds)
        {
            DrawLabel(context, "playback profile unavailable", plot.TopLeft + new Vector(8, 8), Color.FromRgb(0x7A, 0x91, 0xAC), 12);
            return;
        }

        DrawTimeGrid(context, plot);
        DrawActiveRuns(context, plot, points);
        DrawWave(context, plot, points);
        DrawThreshold(context, plot, points);
        DrawPlayhead(context, plot);
        DrawLabel(context, $"{DisplayStartSeconds:F2}s - {DisplayEndSeconds:F2}s", new Point(plot.X, bounds.Bottom - 14), Color.FromRgb(0x7A, 0x91, 0xAC), 10);
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

    private void DrawTimeGrid(DrawingContext context, Rect plot)
    {
        var gridPen = new Pen(new SolidColorBrush(Color.FromArgb(0x28, 0x45, 0x66, 0x88)), 1);
        const int divisions = 8;
        for (int i = 0; i <= divisions; i++)
        {
            var x = plot.X + plot.Width * i / divisions;
            context.DrawLine(gridPen, new Point(x, plot.Y), new Point(x, plot.Bottom));
        }
    }

    private void DrawActiveRuns(DrawingContext context, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        int? start = null;
        for (int index = 0; index < points.Count; index++)
        {
            if (points[index].Active)
            {
                start ??= index;
            }
            else if (start is int activeStart)
            {
                DrawActiveRun(context, plot, points, activeStart, index - 1);
                start = null;
            }
        }

        if (start is int trailingStart)
        {
            DrawActiveRun(context, plot, points, trailingStart, points.Count - 1);
        }
    }

    private void DrawActiveRun(DrawingContext context, Rect plot, IReadOnlyList<SignalProfilePoint> points, int startIndex, int endIndex)
    {
        var left = XFromSeconds(points[startIndex].TimeSeconds, plot);
        var right = XFromSeconds(points[endIndex].TimeSeconds, plot);
        if (right <= left)
        {
            right = left + 1;
        }

        context.FillRectangle(
            new SolidColorBrush(Color.FromArgb(0x30, 0x84, 0xFF, 0x6E)),
            new Rect(left, plot.Y, right - left, plot.Height));
    }

    private void DrawWave(DrawingContext context, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        double maxPower = 1e-9;
        foreach (var point in points)
        {
            maxPower = Math.Max(maxPower, point.Power);
        }

        var geometry = new StreamGeometry();
        using (var gc = geometry.Open())
        {
            bool first = true;
            foreach (var point in points)
            {
                var x = XFromSeconds(point.TimeSeconds, plot);
                var y = YFromPower(point.Power, maxPower, plot);
                if (first)
                {
                    gc.BeginFigure(new Point(x, y), false);
                    first = false;
                }
                else
                {
                    gc.LineTo(new Point(x, y));
                }
            }
        }

        context.DrawGeometry(null, new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0xD3, 0xEE)), 1.4), geometry);
    }

    private void DrawThreshold(DrawingContext context, Rect plot, IReadOnlyList<SignalProfilePoint> points)
    {
        double maxPower = 1e-9;
        foreach (var point in points)
        {
            maxPower = Math.Max(maxPower, point.Power);
        }

        var y = YFromPower(Threshold, maxPower, plot);
        context.DrawLine(
            new Pen(new SolidColorBrush(Color.FromArgb(0xB0, 0xFF, 0xC8, 0x5C)), 1, dashStyle: new DashStyle([4, 4], 0)),
            new Point(plot.X, y),
            new Point(plot.Right, y));
    }

    private void DrawPlayhead(DrawingContext context, Rect plot)
    {
        var x = XFromSeconds(PlayheadSeconds, plot);
        context.DrawLine(
            new Pen(new SolidColorBrush(Color.FromRgb(0xFF, 0x4D, 0xB8)), 2),
            new Point(x, plot.Y),
            new Point(x, plot.Bottom));
    }

    private double XFromSeconds(double seconds, Rect plot)
    {
        if (DisplayEndSeconds <= DisplayStartSeconds)
        {
            return plot.X;
        }

        var normalized = (seconds - DisplayStartSeconds) / (DisplayEndSeconds - DisplayStartSeconds);
        return plot.X + Math.Clamp(normalized, 0.0, 1.0) * plot.Width;
    }

    private static double YFromPower(double power, double maxPower, Rect plot)
    {
        if (maxPower <= 1e-9)
        {
            return plot.Bottom;
        }

        var normalized = Math.Clamp(power / maxPower, 0.0, 1.0);
        return plot.Bottom - normalized * plot.Height;
    }

    private static void DrawLabel(DrawingContext context, string text, Point origin, Color color, double fontSize)
    {
        var formatted = new FormattedText(
            text,
            System.Globalization.CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight,
            new Typeface("Consolas"),
            fontSize,
            new SolidColorBrush(color));
        context.DrawText(formatted, origin);
    }
}
