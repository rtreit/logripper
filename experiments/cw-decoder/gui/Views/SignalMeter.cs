using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace CwDecoderGui.Views;

/// <summary>
/// Horizontal "scientific" signal-strength meter. Renders:
///   * a graded background grid (10 ticks)
///   * a power bar that fills cyan -> lime as the level grows
///   * a vertical tick marking the dynamic threshold
///   * a faint glow when the signal is currently above threshold (keying)
/// </summary>
internal sealed class SignalMeter : Control
{
    public static readonly StyledProperty<double> LevelProperty =
        AvaloniaProperty.Register<SignalMeter, double>(nameof(Level));
    public static readonly StyledProperty<double> ThresholdProperty =
        AvaloniaProperty.Register<SignalMeter, double>(nameof(Threshold));
    public static readonly StyledProperty<bool> SignalProperty =
        AvaloniaProperty.Register<SignalMeter, bool>(nameof(Signal));

    static SignalMeter()
    {
        AffectsRender<SignalMeter>(LevelProperty, ThresholdProperty, SignalProperty);
    }

    public double Level { get => GetValue(LevelProperty); set => SetValue(LevelProperty, value); }
    public double Threshold { get => GetValue(ThresholdProperty); set => SetValue(ThresholdProperty, value); }
    public bool Signal { get => GetValue(SignalProperty); set => SetValue(SignalProperty, value); }

    public override void Render(DrawingContext ctx)
    {
        base.Render(ctx);
        var b = Bounds;
        if (b.Width <= 2 || b.Height <= 2) return;

        var bg = new SolidColorBrush(Color.FromRgb(0x0A, 0x12, 0x1B));
        ctx.FillRectangle(bg, b);

        // Outer frame
        var frame = new Pen(new SolidColorBrush(Color.FromRgb(0x22, 0x3C, 0x55)), 1);
        ctx.DrawRectangle(null, frame, b);

        // Tick grid (10 vertical lines)
        var tickPen = new Pen(new SolidColorBrush(Color.FromArgb(0x40, 0x33, 0x55, 0x77)), 1);
        for (int i = 1; i < 10; i++)
        {
            double x = b.X + b.Width * i / 10.0;
            ctx.DrawLine(tickPen, new Point(x, b.Y), new Point(x, b.Y + b.Height));
        }

        // Bar
        double level = Math.Clamp(Level, 0, 1);
        double bw = b.Width * level;
        if (bw > 1)
        {
            var fill = new LinearGradientBrush
            {
                StartPoint = new RelativePoint(0, 0.5, RelativeUnit.Relative),
                EndPoint = new RelativePoint(1, 0.5, RelativeUnit.Relative),
                GradientStops =
                {
                    new GradientStop(Color.FromRgb(0x10, 0xA0, 0xC8), 0.0),
                    new GradientStop(Color.FromRgb(0x22, 0xD3, 0xEE), 0.5),
                    new GradientStop(Color.FromRgb(0x84, 0xFF, 0x6E), 1.0),
                },
            };
            var bar = new Rect(b.X, b.Y + 2, bw, b.Height - 4);
            ctx.FillRectangle(fill, bar);
        }

        // Threshold tick
        double t = Math.Clamp(Threshold, 0, 1);
        if (t > 0)
        {
            double tx = b.X + b.Width * t;
            var thPen = new Pen(new SolidColorBrush(Color.FromRgb(0xFF, 0xB3, 0x47)), 2);
            ctx.DrawLine(thPen, new Point(tx, b.Y - 2), new Point(tx, b.Y + b.Height + 2));
        }

        // Signal indicator: thin top stripe glows magenta when keyed
        if (Signal)
        {
            var glow = new SolidColorBrush(Color.FromRgb(0xFF, 0x5B, 0xD0));
            ctx.FillRectangle(glow, new Rect(b.X, b.Y, b.Width, 2));
        }
    }
}
