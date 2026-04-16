using QsoRipper.Domain;

namespace QsoRipper.Gui.ViewModels;

internal sealed record BandOption(string Label, Band ProtoBand, double DefaultFrequencyMhz);

internal sealed record ModeOption(string Label, Mode ProtoMode, string? Submode, string DefaultRst);

internal static class OperatorOptions
{
    public static BandOption[] Bands { get; } =
    [
        new("160m", Band._160M, 1.900),
        new("80m", Band._80M, 3.750),
        new("60m", Band._60M, 5.330),
        new("40m", Band._40M, 7.150),
        new("30m", Band._30M, 10.125),
        new("20m", Band._20M, 14.225),
        new("17m", Band._17M, 18.100),
        new("15m", Band._15M, 21.200),
        new("12m", Band._12M, 24.940),
        new("10m", Band._10M, 28.400),
        new("6m", Band._6M, 50.125),
        new("2m", Band._2M, 146.520),
        new("70cm", Band._70Cm, 446.000),
    ];

    public static ModeOption[] Modes { get; } =
    [
        new("SSB", Mode.Ssb, null, "59"),
        new("CW", Mode.Cw, null, "599"),
        new("FT8", Mode.Ft8, null, "599"),
        new("FT4", Mode.Mfsk, "FT4", "599"),
        new("RTTY", Mode.Rtty, null, "599"),
        new("PSK31", Mode.Psk, "PSK31", "599"),
        new("AM", Mode.Am, null, "59"),
        new("FM", Mode.Fm, null, "59"),
    ];

    public static int FindBandIndex(Band band)
    {
        for (int i = 0; i < Bands.Length; i++)
        {
            if (Bands[i].ProtoBand == band)
            {
                return i;
            }
        }

        return 0;
    }

    public static int FindModeIndex(Mode mode, string? submode)
    {
        for (int i = 0; i < Modes.Length; i++)
        {
            var opt = Modes[i];
            if (opt.ProtoMode == mode && string.Equals(opt.Submode, submode, StringComparison.OrdinalIgnoreCase))
            {
                return i;
            }
        }

        // Fall back to just matching proto mode
        for (int i = 0; i < Modes.Length; i++)
        {
            if (Modes[i].ProtoMode == mode)
            {
                return i;
            }
        }

        return 0;
    }
}
