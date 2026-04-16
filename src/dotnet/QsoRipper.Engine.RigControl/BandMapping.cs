using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Maps a frequency in Hz to the corresponding amateur radio band.
/// </summary>
/// <remarks>
/// Ranges match the Rust <c>band_mapping::frequency_hz_to_band</c> implementation
/// and cover IARU band-plan allocations from 2200 m through 23 cm.
/// </remarks>
public static class BandMapping
{
    /// <summary>
    /// Map a frequency in Hz to the corresponding <see cref="Band"/> enum value.
    /// Returns <see cref="Band.Unspecified"/> when the frequency falls outside
    /// any recognized amateur band.
    /// </summary>
    public static Band FrequencyHzToBand(ulong hz) => hz switch
    {
        // 2200m: 135.7–137.8 kHz
        >= 135_700 and <= 137_800 => Band._2190M,
        // 630m: 472–479 kHz
        >= 472_000 and <= 479_000 => Band._630M,
        // 160m: 1.8–2.0 MHz
        >= 1_800_000 and <= 2_000_000 => Band._160M,
        // 80m: 3.5–4.0 MHz
        >= 3_500_000 and <= 4_000_000 => Band._80M,
        // 60m: 5.06–5.45 MHz (channelized in most countries)
        >= 5_060_000 and <= 5_450_000 => Band._60M,
        // 40m: 7.0–7.3 MHz
        >= 7_000_000 and <= 7_300_000 => Band._40M,
        // 30m: 10.1–10.15 MHz
        >= 10_100_000 and <= 10_150_000 => Band._30M,
        // 20m: 14.0–14.35 MHz
        >= 14_000_000 and <= 14_350_000 => Band._20M,
        // 17m: 18.068–18.168 MHz
        >= 18_068_000 and <= 18_168_000 => Band._17M,
        // 15m: 21.0–21.45 MHz
        >= 21_000_000 and <= 21_450_000 => Band._15M,
        // 12m: 24.89–24.99 MHz
        >= 24_890_000 and <= 24_990_000 => Band._12M,
        // 10m: 28.0–29.7 MHz
        >= 28_000_000 and <= 29_700_000 => Band._10M,
        // 6m: 50.0–54.0 MHz
        >= 50_000_000 and <= 54_000_000 => Band._6M,
        // 2m: 144.0–148.0 MHz
        >= 144_000_000 and <= 148_000_000 => Band._2M,
        // 1.25m: 219–225 MHz
        >= 219_000_000 and <= 225_000_000 => Band._125M,
        // 70cm: 420–450 MHz
        >= 420_000_000 and <= 450_000_000 => Band._70Cm,
        // 33cm: 902–928 MHz
        >= 902_000_000 and <= 928_000_000 => Band._33Cm,
        // 23cm: 1240–1300 MHz
        >= 1_240_000_000 and <= 1_300_000_000 => Band._23Cm,
        _ => Band.Unspecified,
    };
}
