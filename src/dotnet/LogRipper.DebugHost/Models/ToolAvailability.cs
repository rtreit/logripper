namespace LogRipper.DebugHost.Models;

public sealed record ToolAvailability(string Name, bool IsAvailable, string? Details = null);
