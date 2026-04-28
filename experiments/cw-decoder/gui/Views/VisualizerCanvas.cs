using System;
using System.Globalization;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using CwDecoderGui.Models;
using CwDecoderGui.ViewModels;

namespace CwDecoderGui.Views;

/// <summary>
/// Renders one "viz frame" from the cw-decoder stream-live-v3 path:
/// audio envelope + noise/signal floors + hysteresis thresholds across the
/// top, classified on/off events as colored bars below, and an on-duration
/// histogram with k-means dot/dah centroids on the right side.
///
/// Bind <see cref="Frame"/> to a VizFrameVm in the view model. Each new
/// frame triggers an InvalidateVisual.
/// </summary>
public sealed class VisualizerCanvas : Control
{
    public static readonly StyledProperty<VizFrameVm?> FrameProperty =
        AvaloniaProperty.Register<VisualizerCanvas, VizFrameVm?>(nameof(Frame));

    /// <summary>Time window shown in seconds (default 10s).</summary>
    public static readonly StyledProperty<double> WindowSecondsProperty =
        AvaloniaProperty.Register<VisualizerCanvas, double>(nameof(WindowSeconds), 10.0);

    static VisualizerCanvas()
    {
        AffectsRender<VisualizerCanvas>(FrameProperty, WindowSecondsProperty);
    }

    public VizFrameVm? Frame
    {
        get => GetValue(FrameProperty);
        set => SetValue(FrameProperty, value);
    }

    public double WindowSeconds
    {
        get => GetValue(WindowSecondsProperty);
        set => SetValue(WindowSecondsProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        double w = double.IsFinite(availableSize.Width) ? availableSize.Width : 800;
        double h = double.IsFinite(availableSize.Height) ? availableSize.Height : 320;
        return new Size(w, h);
    }

    public override void Render(DrawingContext ctx)
    {
        base.Render(ctx);
        var b = Bounds;
        if (b.Width < 80 || b.Height < 60) return;

        var bg = new SolidColorBrush(Color.FromRgb(0x07, 0x10, 0x18));
        ctx.FillRectangle(bg, b);
        var frame = new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0x3C, 0x55)), 1);
        ctx.DrawRectangle(null, frame, b);

        var typeface = new Typeface("Consolas");
        var labelBrush = new SolidColorBrush(Color.FromRgb(0x7A, 0x91, 0xAC));

        var f = Frame;
        if (f is null || f.Envelope is null || f.Envelope.Length == 0)
        {
            var msg = new FormattedText("waiting for audio…", CultureInfo.InvariantCulture,
                FlowDirection.LeftToRight, typeface, 14, labelBrush);
            ctx.DrawText(msg, new Point(b.X + 12, b.Y + 12));
            return;
        }

        // Layout: left ~78% = envelope+events. right ~22% = histogram.
        double histWidth = Math.Max(120, b.Width * 0.22);
        var envRect = new Rect(b.X + 4, b.Y + 4, b.Width - histWidth - 12, b.Height - 8);
        var histRect = new Rect(envRect.Right + 4, b.Y + 4, histWidth, b.Height - 8);

        DrawEnvelope(ctx, envRect, f, typeface, labelBrush);
        DrawHistogram(ctx, histRect, f, typeface, labelBrush);
    }

    private void DrawEnvelope(DrawingContext ctx, Rect r, VizFrameVm f, Typeface typeface, IBrush labelBrush)
    {
        // Top 60% = envelope curve. Bottom 40% = event bars.
        var envHeight = r.Height * 0.6;
        var barHeight = r.Height * 0.35;
        var envR = new Rect(r.X, r.Y, r.Width, envHeight);
        var barR = new Rect(r.X, r.Y + envHeight + 4, r.Width, barHeight);

        // Time mapping. Show the latest WindowSeconds of audio (right-justified).
        double winS = Math.Max(1.0, WindowSeconds);
        double bufS = Math.Max(winS, f.BufferSeconds);
        double tStart = bufS - winS;
        double tEnd = bufS;
        double xPerSec = envR.Width / winS;

        // ---- Envelope curve ----
        double envMaxY = Math.Max(f.EnvelopeMax, f.SignalFloor) * 1.1;
        if (envMaxY <= 1e-9) envMaxY = 1.0;

        // Background frame for envelope panel
        ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1), envR);

        // Low-SNR overlay: when the engine suppressed text emission, tint
        // the envelope panel red and badge it so the operator immediately
        // sees that the visualizer is still rendering noise but the
        // transcript was intentionally muted.
        if (f.SnrSuppressed)
        {
            ctx.FillRectangle(new SolidColorBrush(Color.FromArgb(0x22, 0xFF, 0x33, 0x33)), envR);
            var badge = new FormattedText(
                $"LOW SNR ({f.SnrDb:F1} dB) — text suppressed",
                CultureInfo.InvariantCulture,
                FlowDirection.LeftToRight,
                typeface,
                12,
                new SolidColorBrush(Color.FromRgb(0xFF, 0x88, 0x88)));
            ctx.DrawText(badge, new Point(envR.X + 8, envR.Y + 4));
        }

        // Noise/signal floor band (shaded).
        var floorTop = ScaleY(envR, f.SignalFloor, envMaxY);
        var floorBot = ScaleY(envR, f.NoiseFloor, envMaxY);
        var floorRect = new Rect(envR.X, floorTop, envR.Width, Math.Max(1, floorBot - floorTop));
        ctx.FillRectangle(new SolidColorBrush(Color.FromArgb(0x18, 0x66, 0x99, 0xCC)), floorRect);

        // Hysteresis lines.
        DrawHLine(ctx, envR, ScaleY(envR, f.HystHigh, envMaxY),
            new SolidColorBrush(Color.FromArgb(0xA0, 0x84, 0xFF, 0x6E)), 1, "high");
        DrawHLine(ctx, envR, ScaleY(envR, f.HystLow, envMaxY),
            new SolidColorBrush(Color.FromArgb(0xA0, 0xFF, 0xCC, 0x44)), 1, "low");

        // Envelope polyline. Map array index linearly to time over the
        // full buffer duration, then drop samples outside [tStart, tEnd].
        var env = f.Envelope!;
        double dt = f.BufferSeconds / Math.Max(1, env.Length);
        var line = new StreamGeometry();
        bool started = false;
        var linePen = new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0xD3, 0xEE)), 1.2);
        using (var lc = line.Open())
        {
            for (int i = 0; i < env.Length; i++)
            {
                double t = i * dt;
                if (t < tStart) continue;
                double x = envR.X + (t - tStart) * xPerSec;
                double y = ScaleY(envR, env[i], envMaxY);
                var p = new Point(x, y);
                if (!started) { lc.BeginFigure(p, false); started = true; }
                else lc.LineTo(p);
            }
            if (started) lc.EndFigure(false);
        }
        if (started) ctx.DrawGeometry(null, linePen, line);

        // ---- Event bars panel ----
        ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1), barR);
        if (f.Events is not null)
        {
            foreach (var e in f.Events)
            {
                if (e.EndS < tStart || e.StartS > tEnd) continue;
                double x0 = barR.X + Math.Max(0, e.StartS - tStart) * xPerSec;
                double x1 = barR.X + Math.Min(winS, e.EndS - tStart) * xPerSec;
                if (x1 - x0 < 0.5) continue;
                IBrush brush = e.Kind switch
                {
                    "on_dit" => new SolidColorBrush(Color.FromArgb(0xE0, 0x84, 0xFF, 0x6E)),    // green
                    "on_dah" => new SolidColorBrush(Color.FromArgb(0xE0, 0x22, 0xD3, 0xEE)),    // cyan
                    "off_intra" => new SolidColorBrush(Color.FromArgb(0x60, 0x44, 0x55, 0x66)), // dark grey
                    "off_char" => new SolidColorBrush(Color.FromArgb(0xC0, 0x88, 0x99, 0xAA)),  // mid grey
                    "off_word" => new SolidColorBrush(Color.FromArgb(0xE0, 0xFF, 0xAA, 0x44)),  // amber
                    _ => new SolidColorBrush(Color.FromArgb(0x80, 0xFF, 0x44, 0x44)),
                };
                bool isOn = e.Kind == "on_dit" || e.Kind == "on_dah";
                double yTop = isOn ? barR.Y : barR.Y + barR.Height * 0.55;
                double h = isOn ? barR.Height * 0.55 : barR.Height * 0.45;
                ctx.FillRectangle(brush, new Rect(x0, yTop, x1 - x0, h));
            }
        }

        // Time-axis ticks (every 1 second).
        var tickPen = new Pen(new SolidColorBrush(Color.FromArgb(0x60, 0x33, 0x55, 0x77)), 1);
        for (double t = Math.Ceiling(tStart); t <= tEnd; t += 1.0)
        {
            double x = envR.X + (t - tStart) * xPerSec;
            ctx.DrawLine(tickPen, new Point(x, barR.Bottom - 6), new Point(x, barR.Bottom));
            var lbl = new FormattedText(((int)t).ToString(CultureInfo.InvariantCulture),
                CultureInfo.InvariantCulture, FlowDirection.LeftToRight, typeface, 9, labelBrush);
            ctx.DrawText(lbl, new Point(x + 2, barR.Bottom - lbl.Height - 1));
        }
    }

    private void DrawHistogram(DrawingContext ctx, Rect r, VizFrameVm f, Typeface typeface, IBrush labelBrush)
    {
        ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1), r);
        var title = new FormattedText("element duration histogram", CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight, typeface, 10, labelBrush);
        ctx.DrawText(title, new Point(r.X + 6, r.Y + 4));

        var durs = f.OnDurations;
        if (durs is null || durs.Length == 0) return;

        // 30 bins from 0 to max(2*centroid_dah, max(durations)*1.05).
        double maxDur = 0;
        for (int i = 0; i < durs.Length; i++) if (durs[i] > maxDur) maxDur = durs[i];
        double rangeMax = Math.Max(maxDur * 1.05, f.CentroidDah * 2.0);
        if (rangeMax <= 0) rangeMax = 0.5;
        int bins = 30;
        var counts = new int[bins];
        int peak = 0;
        for (int i = 0; i < durs.Length; i++)
        {
            int idx = (int)Math.Floor((durs[i] / rangeMax) * bins);
            if (idx < 0) idx = 0; else if (idx >= bins) idx = bins - 1;
            counts[idx]++;
            if (counts[idx] > peak) peak = counts[idx];
        }

        var plotR = new Rect(r.X + 6, r.Y + 22, r.Width - 12, r.Height - 60);
        ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1), plotR);

        double binW = plotR.Width / bins;
        var barBrush = new SolidColorBrush(Color.FromArgb(0xC0, 0x22, 0xD3, 0xEE));
        for (int i = 0; i < bins; i++)
        {
            if (counts[i] == 0) continue;
            double h = (counts[i] / (double)Math.Max(1, peak)) * plotR.Height;
            var rect = new Rect(plotR.X + i * binW, plotR.Bottom - h, Math.Max(1, binW - 1), h);
            ctx.FillRectangle(barBrush, rect);
        }

        // Centroid markers.
        DrawCentroidMarker(ctx, plotR, f.CentroidDot, rangeMax,
            new SolidColorBrush(Color.FromRgb(0x84, 0xFF, 0x6E)), "dot", typeface);
        DrawCentroidMarker(ctx, plotR, f.CentroidDah, rangeMax,
            new SolidColorBrush(Color.FromRgb(0xFF, 0xCC, 0x44)), "dah", typeface);

        // Locked WPM / dot length readout.
        var stats = $"WPM: {f.Wpm:F1}\ndot: {f.DotSeconds * 1000:F0}ms\npitch: {f.PitchHz:F0}Hz" +
                    (f.LockedWpm.HasValue ? $"\nlocked: {f.LockedWpm:F1}" : "");
        var statsFt = new FormattedText(stats, CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight, typeface, 11,
            new SolidColorBrush(Color.FromRgb(0xE6, 0xF2, 0xFF)));
        ctx.DrawText(statsFt, new Point(r.X + 6, plotR.Bottom + 4));
    }

    private static void DrawCentroidMarker(DrawingContext ctx, Rect plotR, double value, double rangeMax, IBrush brush, string label, Typeface typeface)
    {
        if (value <= 0) return;
        double frac = value / rangeMax;
        if (frac < 0 || frac > 1) return;
        double x = plotR.X + frac * plotR.Width;
        var pen = new Pen(brush, 2, dashStyle: new DashStyle(new double[] { 4, 2 }, 0));
        ctx.DrawLine(pen, new Point(x, plotR.Y), new Point(x, plotR.Bottom));
        var lbl = new FormattedText($"{label} {value * 1000:F0}ms",
            CultureInfo.InvariantCulture, FlowDirection.LeftToRight, typeface, 9, brush);
        ctx.DrawText(lbl, new Point(x + 2, plotR.Y + 2));
    }

    private static void DrawHLine(DrawingContext ctx, Rect r, double y, IBrush brush, double thickness, string label)
    {
        var pen = new Pen(brush, thickness, dashStyle: new DashStyle(new double[] { 3, 3 }, 0));
        ctx.DrawLine(pen, new Point(r.X, y), new Point(r.Right, y));
        var typeface = new Typeface("Consolas");
        var ft = new FormattedText(label, CultureInfo.InvariantCulture,
            FlowDirection.LeftToRight, typeface, 9, brush);
        ctx.DrawText(ft, new Point(r.Right - ft.Width - 4, y - ft.Height - 1));
    }

    private static double ScaleY(Rect r, double v, double max)
    {
        if (max <= 1e-9) return r.Bottom;
        double frac = Math.Clamp(v / max, 0.0, 1.0);
        return r.Bottom - frac * r.Height;
    }
}
