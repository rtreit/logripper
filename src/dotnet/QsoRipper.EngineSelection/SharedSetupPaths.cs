namespace QsoRipper.EngineSelection;

public static class SharedSetupPaths
{
    public const string DefaultConfigFileName = "config.toml";

    public static string GetDefaultConfigPath()
    {
        if (OperatingSystem.IsWindows())
        {
            var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            if (string.IsNullOrWhiteSpace(appData))
            {
                throw new InvalidOperationException("APPDATA is not set; cannot resolve the default config path.");
            }

            return Path.GetFullPath(Path.Combine(appData, "qsoripper", DefaultConfigFileName));
        }

        var xdgConfigHome = Environment.GetEnvironmentVariable("XDG_CONFIG_HOME");
        if (!string.IsNullOrWhiteSpace(xdgConfigHome))
        {
            return Path.GetFullPath(Path.Combine(xdgConfigHome, "qsoripper", DefaultConfigFileName));
        }

        var home = Environment.GetEnvironmentVariable("HOME");
        if (string.IsNullOrWhiteSpace(home))
        {
            throw new InvalidOperationException("HOME is not set; cannot resolve the default config path.");
        }

        return Path.GetFullPath(Path.Combine(home, ".config", "qsoripper", DefaultConfigFileName));
    }
}
