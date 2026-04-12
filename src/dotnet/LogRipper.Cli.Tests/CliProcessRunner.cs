using System.Diagnostics;

namespace LogRipper.Cli.Tests;

internal static class CliProcessRunner
{
    public static async Task<CliProcessResult> RunAsync(params string[] args)
    {
        var assemblyPath = typeof(CliArgumentParser).Assembly.Location;
        var startInfo = new ProcessStartInfo
        {
            FileName = "dotnet",
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false
        };

        startInfo.ArgumentList.Add(assemblyPath);
        foreach (var arg in args)
        {
            startInfo.ArgumentList.Add(arg);
        }

        using var process = new Process { StartInfo = startInfo };
        process.Start();

        var standardOutput = process.StandardOutput.ReadToEndAsync();
        var standardError = process.StandardError.ReadToEndAsync();

        await process.WaitForExitAsync();

        return new CliProcessResult(
            process.ExitCode,
            await standardOutput,
            await standardError);
    }
}

internal sealed record CliProcessResult(int ExitCode, string StandardOutput, string StandardError);
