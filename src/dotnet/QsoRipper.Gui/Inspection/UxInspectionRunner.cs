using System;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Threading;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Gui.Views;

namespace QsoRipper.Gui.Inspection;

internal static class UxInspectionRunner
{
    public static Window CreateWindow(
        IClassicDesktopStyleApplicationLifetime desktop,
        UxInspectionOptions options)
    {
        return options.Surface switch
        {
            UxInspectionSurface.Settings => CreateSettingsWindow(desktop, options),
            _ => CreateMainWindow(desktop, options)
        };
    }

    private static MainWindow CreateMainWindow(
        IClassicDesktopStyleApplicationLifetime desktop,
        UxInspectionOptions options)
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
                await WaitForUiReadyAsync();

                if (!string.IsNullOrWhiteSpace(fixture.SearchText))
                {
                    viewModel.RecentQsos.SearchText = fixture.SearchText;
                }

                if (!string.IsNullOrWhiteSpace(fixture.SelectedLocalId))
                {
                    viewModel.RecentQsos.SelectedQso = viewModel.RecentQsos.VisibleItems
                        .FirstOrDefault(item => string.Equals(item.LocalId, fixture.SelectedLocalId, StringComparison.Ordinal));
                }

                await OpenRequestedSurfaceAsync(viewModel, options.Surface);
            }
            catch (InvalidOperationException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (FileNotFoundException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (IOException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (JsonException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
        };

        return window;
    }

    private static SettingsView CreateSettingsWindow(
        IClassicDesktopStyleApplicationLifetime desktop,
        UxInspectionOptions options)
    {
        var fixture = UxCaptureFixture.Load(options.FixturePath);
        var settingsVm = new SettingsViewModel(new UxFixtureEngineClient(fixture));
        var window = new SettingsView
        {
            DataContext = settingsVm
        };

        window.Opened += async (_, _) =>
        {
            try
            {
                await settingsVm.LoadAsync();
                await WaitForUiReadyAsync();
            }
            catch (InvalidOperationException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (FileNotFoundException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (IOException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (JsonException ex)
            {
                Console.Error.WriteLine($"UX inspection failed: {ex.Message}");
                desktop.Shutdown(1);
            }
        };

        return window;
    }

    private static async Task OpenRequestedSurfaceAsync(
        MainWindowViewModel viewModel,
        UxInspectionSurface surface)
    {
        switch (surface)
        {
            case UxInspectionSurface.MainWindow:
                return;
            case UxInspectionSurface.Settings:
                return;
            case UxInspectionSurface.Wizard:
                if (!viewModel.IsWizardOpen)
                {
                    await viewModel.OpenWizardCommand.ExecuteAsync(null);
                    await WaitForUiReadyAsync();
                }

                return;
            default:
                throw new InvalidOperationException($"Unsupported inspection surface '{surface}'.");
        }
    }

    private static async Task WaitForUiReadyAsync()
    {
        await Dispatcher.UIThread.InvokeAsync(static () => { }, DispatcherPriority.Render);
        await Task.Delay(75);
    }
}
