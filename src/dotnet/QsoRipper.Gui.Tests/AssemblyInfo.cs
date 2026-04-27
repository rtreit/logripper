using System;
using System.Runtime.CompilerServices;
using System.Threading;

// Avalonia's Dispatcher.UIThread is thread-affine: the first thread to call
// Dispatcher.UIThread.RunJobs() (or otherwise drive its job loop) becomes the
// owner forever in that AppDomain. xUnit dispatches tests across pool threads
// non-deterministically, so on Linux CI a test that calls
// Dispatcher.UIThread.Invoke(body) from a non-owner thread blocks forever
// (the owner thread isn't pumping).
//
// To make Dispatcher.UIThread.Invoke + RunJobs reliable across all GUI tests,
// we spin up a single long-lived "UI thread" in a module initializer (runs
// before any test). That thread:
//   1. Pumps Dispatcher.UIThread.RunJobs() to claim ownership.
//   2. Stays alive servicing the job queue so foreign-thread Invoke()s
//      (and VM-internal Dispatcher.UIThread.Post() callbacks) get drained.
//
// Also serialize at the collection level so an assertion failure can't race
// other tests through the dispatcher queue.
[assembly: Xunit.CollectionBehavior(DisableTestParallelization = true)]

namespace QsoRipper.Gui.Tests;

internal static class AvaloniaDispatcherBootstrap
{
    private static Thread? s_uiThread;
    private static readonly ManualResetEventSlim s_ready = new(false);

    [ModuleInitializer]
    internal static void Init()
    {
        s_uiThread = new Thread(RunUiLoop)
        {
            IsBackground = true,
            Name = "QsoRipper.Gui.Tests.UIThread",
        };
        s_uiThread.Start();
        s_ready.Wait(TimeSpan.FromSeconds(5));
    }

    private static void RunUiLoop()
    {
        // Touch Dispatcher.UIThread from this thread first so it becomes the
        // owning thread. Any Invoke from another thread now routes here.
        Avalonia.Threading.Dispatcher.UIThread.RunJobs();
        s_ready.Set();

        // Continuously drain queued dispatcher jobs so foreign-thread
        // Dispatcher.UIThread.Invoke(...) calls complete promptly.
        while (true)
        {
            try
            {
                Avalonia.Threading.Dispatcher.UIThread.RunJobs();
            }
#pragma warning disable CA1031 // Pump must never die; surface via per-test assertions
            catch
            {
                // Swallow; individual test bodies surface their own failures.
            }
#pragma warning restore CA1031
            Thread.Sleep(5);
        }
    }
}
