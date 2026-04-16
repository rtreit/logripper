using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Result of mapping a Hamlib/rigctld mode string to project-owned types.
/// </summary>
/// <param name="Mode">Normalized project-owned mode.</param>
/// <param name="Submode">Optional submode (e.g., <c>"USB"</c> for SSB).</param>
public readonly record struct ModeMappingResult(Mode Mode, string? Submode);

/// <summary>
/// Maps Hamlib/rigctld mode strings to project-owned <see cref="Mode"/> and submode values.
/// </summary>
/// <remarks>
/// The mapping is case-insensitive and matches the Rust
/// <c>mode_mapping::hamlib_mode_to_proto</c> implementation.
/// </remarks>
public static class ModeMapping
{
    /// <summary>
    /// Map a raw Hamlib mode string to a <see cref="ModeMappingResult"/>.
    /// Unknown modes map to <see cref="Mode.Unspecified"/> with a <c>null</c> submode.
    /// </summary>
    public static ModeMappingResult HamlibModeToProto(string raw)
    {
        ArgumentNullException.ThrowIfNull(raw);

        return raw.ToUpperInvariant() switch
        {
            "USB" => new ModeMappingResult(Mode.Ssb, "USB"),
            "LSB" => new ModeMappingResult(Mode.Ssb, "LSB"),
            "CW" => new ModeMappingResult(Mode.Cw, null),
            "CWR" => new ModeMappingResult(Mode.Cw, "CWR"),
            "AM" => new ModeMappingResult(Mode.Am, null),
            "FM" or "WFM" => new ModeMappingResult(Mode.Fm, null),
            "RTTY" => new ModeMappingResult(Mode.Rtty, null),
            "RTTYR" => new ModeMappingResult(Mode.Rtty, "RTTYR"),
            "PKTUSB" => new ModeMappingResult(Mode.Pkt, "PKTUSB"),
            "PKTLSB" => new ModeMappingResult(Mode.Pkt, "PKTLSB"),
            "PKTFM" => new ModeMappingResult(Mode.Pkt, "PKTFM"),
            "FT8" => new ModeMappingResult(Mode.Ft8, null),
            "PSK" or "PSK31" => new ModeMappingResult(Mode.Psk, null),
            _ => new ModeMappingResult(Mode.Unspecified, null),
        };
    }
}
