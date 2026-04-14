using System;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Media.Imaging;
using Avalonia.Threading;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Gui.Views;

namespace QsoRipper.Gui.Inspection;

internal static class UxCaptureRunner
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true
    };

    public static MainWindow CreateWindow(IClassicDesktopStyleApplicationLifetime desktop, UxCaptureOptions options)
    {
        var fixture = UxCaptureFixture.Load(options.FixturePath);
        var viewModel = new MainWindowViewModel(new UxFixtureEngineClient(fixture));
        var window = new MainWindow
        {
            IsInspectionMode = true,
            DataContext = viewModel
        };

        window.Opened += async (_, _) =>
        {
            try
            {
                await viewModel.CheckFirstRunAsync(focusSearch: false);

                if (!string.IsNullOrWhiteSpace(fixture.SearchText))
                {
                    viewModel.RecentQsos.SearchText = fixture.SearchText;
                }

                if (!string.IsNullOrWhiteSpace(fixture.SelectedLocalId))
                {
                    viewModel.RecentQsos.SelectedQso = viewModel.RecentQsos.VisibleItems
                        .FirstOrDefault(item => string.Equals(item.LocalId, fixture.SelectedLocalId, StringComparison.Ordinal));
                }

                await WaitForRenderAsync();
                await CaptureAsync(window, viewModel, fixture, options);
                desktop.Shutdown(0);
            }
            catch (InvalidOperationException ex)
            {
                Console.Error.WriteLine($"UX capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (FileNotFoundException ex)
            {
                Console.Error.WriteLine($"UX capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (IOException ex)
            {
                Console.Error.WriteLine($"UX capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (JsonException ex)
            {
                Console.Error.WriteLine($"UX capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
        };

        return window;
    }

    private static async Task CaptureAsync(
        MainWindow window,
        MainWindowViewModel viewModel,
        UxCaptureFixture fixture,
        UxCaptureOptions options)
    {
        var target = ResolveTarget(window, options.TargetName);
        Directory.CreateDirectory(Path.GetDirectoryName(options.OutputPath)!);

        var logicalSize = ResolveTargetSize(target);
        if (logicalSize.Width <= 0 || logicalSize.Height <= 0)
        {
            throw new InvalidOperationException($"Capture target '{options.TargetName}' has no rendered size.");
        }

        var pixelSize = new PixelSize(
            Math.Max(1, (int)Math.Ceiling(logicalSize.Width * window.RenderScaling)),
            Math.Max(1, (int)Math.Ceiling(logicalSize.Height * window.RenderScaling)));

        using var bitmap = new RenderTargetBitmap(
            pixelSize,
            new Vector(96 * window.RenderScaling, 96 * window.RenderScaling));
        bitmap.Render(target);
        bitmap.Save(options.OutputPath);

        var summary = new UxCaptureSummary
        {
            Surface = "avalonia",
            Scenario = options.Scenario,
            OutputPath = options.OutputPath,
            FixturePath = options.FixturePath,
            TargetName = options.TargetName,
            ThemeVariant = options.ThemeVariant.ToString(),
            LogicalWidth = logicalSize.Width,
            LogicalHeight = logicalSize.Height,
            PixelWidth = pixelSize.Width,
            PixelHeight = pixelSize.Height,
            RenderScaling = window.RenderScaling,
            RecentQsoCount = viewModel.RecentQsos.VisibleItems.Count,
            SearchText = fixture.SearchText,
            CapturedAtUtc = DateTimeOffset.UtcNow
        };

        var summaryPath = Path.ChangeExtension(options.OutputPath, ".json");
        using (var stream = File.Create(summaryPath))
        {
            JsonSerializer.Serialize(stream, summary, JsonOptions);
        }

        var currentReportPath = Path.Combine(
            Directory.GetCurrentDirectory(),
            "artifacts",
            "ux",
            "current",
            "report.json");
        Directory.CreateDirectory(Path.GetDirectoryName(currentReportPath)!);
        using var reportStream = File.Create(currentReportPath);
        JsonSerializer.Serialize(reportStream, summary, JsonOptions);
    }

    private static async Task WaitForRenderAsync()
    {
        await Dispatcher.UIThread.InvokeAsync(static () => { }, DispatcherPriority.Render);
        await Task.Delay(75);
        await Dispatcher.UIThread.InvokeAsync(static () => { }, DispatcherPriority.Background);
    }

    private static Visual ResolveTarget(MainWindow window, string targetName)
    {
        if (string.Equals(targetName, "MainWindow", StringComparison.OrdinalIgnoreCase))
        {
            return window;
        }

        return window.FindControl<Control>(targetName)
            ?? throw new InvalidOperationException($"Could not find capture target '{targetName}' in MainWindow.");
    }

    private static Size ResolveTargetSize(Visual target) =>
        target is Window window
            ? new Size(window.Bounds.Width, window.Bounds.Height)
            : new Size(target.Bounds.Width, target.Bounds.Height);
}

internal sealed class UxCaptureSummary
{
    public string Surface { get; init; } = string.Empty;

    public string Scenario { get; init; } = string.Empty;

    public string OutputPath { get; init; } = string.Empty;

    public string? FixturePath { get; init; }

    public string TargetName { get; init; } = string.Empty;

    public string ThemeVariant { get; init; } = string.Empty;

    public double LogicalWidth { get; init; }

    public double LogicalHeight { get; init; }

    public int PixelWidth { get; init; }

    public int PixelHeight { get; init; }

    public double RenderScaling { get; init; }

    public int RecentQsoCount { get; init; }

    public string SearchText { get; init; } = string.Empty;

    public DateTimeOffset CapturedAtUtc { get; init; }
}
