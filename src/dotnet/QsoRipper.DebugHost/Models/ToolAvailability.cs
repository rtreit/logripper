namespace QsoRipper.DebugHost.Models;

internal sealed record ToolAvailability(string Name, bool IsAvailable, string? Details = null);
