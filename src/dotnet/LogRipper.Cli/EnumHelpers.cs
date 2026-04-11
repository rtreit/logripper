using LogRipper.Domain;

namespace LogRipper.Cli;

internal static class EnumHelpers
{
    public static Band ParseBand(string input)
    {
        return input.ToUpperInvariant() switch
        {
            "2190M" => Band._2190M,
            "630M" => Band._630M,
            "560M" => Band._560M,
            "160M" => Band._160M,
            "80M" => Band._80M,
            "60M" => Band._60M,
            "40M" => Band._40M,
            "30M" => Band._30M,
            "20M" => Band._20M,
            "17M" => Band._17M,
            "15M" => Band._15M,
            "12M" => Band._12M,
            "10M" => Band._10M,
            "8M" => Band._8M,
            "6M" => Band._6M,
            "5M" => Band._5M,
            "4M" => Band._4M,
            "2M" => Band._2M,
            "1.25M" => Band._125M,
            "70CM" => Band._70Cm,
            "33CM" => Band._33Cm,
            "23CM" => Band._23Cm,
            "13CM" => Band._13Cm,
            "9CM" => Band._9Cm,
            "6CM" => Band._6Cm,
            "3CM" => Band._3Cm,
            "1.25CM" => Band._125Cm,
            "6MM" => Band._6Mm,
            "4MM" => Band._4Mm,
            "2.5MM" => Band._25Mm,
            "2MM" => Band._2Mm,
            "1MM" => Band._1Mm,
            _ => throw new ArgumentException($"Unknown band: {input}. Examples: 20m, 40m, 2m, 70cm"),
        };
    }

    public static Mode ParseMode(string input)
    {
        return input.ToUpperInvariant() switch
        {
            "AM" => Mode.Am,
            "ARDOP" => Mode.Ardop,
            "ATV" => Mode.Atv,
            "CHIP" => Mode.Chip,
            "CLO" => Mode.Clo,
            "CONTESTI" => Mode.Contesti,
            "CW" => Mode.Cw,
            "DIGITALVOICE" => Mode.Digitalvoice,
            "DOMINO" => Mode.Domino,
            "DYNAMIC" => Mode.Dynamic,
            "FAX" => Mode.Fax,
            "FM" => Mode.Fm,
            "FSK" => Mode.Fsk,
            "FT8" => Mode.Ft8,
            "HELL" => Mode.Hell,
            "ISCAT" => Mode.Iscat,
            "JT4" => Mode.Jt4,
            "JT9" => Mode.Jt9,
            "JT44" => Mode.Jt44,
            "JT65" => Mode.Jt65,
            "MFSK" => Mode.Mfsk,
            "MSK144" => Mode.Msk144,
            "MTONE" => Mode.Mtone,
            "OFDM" => Mode.Ofdm,
            "OLIVIA" => Mode.Olivia,
            "OPERA" => Mode.Opera,
            "PAC" => Mode.Pac,
            "PAX" => Mode.Pax,
            "PKT" => Mode.Pkt,
            "PSK" => Mode.Psk,
            "Q15" => Mode.Q15,
            "QRA64" => Mode.Qra64,
            "ROS" => Mode.Ros,
            "RTTY" => Mode.Rtty,
            "RTTYM" => Mode.Rttym,
            "SSB" => Mode.Ssb,
            "SSTV" => Mode.Sstv,
            "T10" => Mode.T10,
            "THOR" => Mode.Thor,
            "THRB" => Mode.Thrb,
            "TOR" => Mode.Tor,
            "V4" => Mode.V4,
            "VOI" => Mode.Voi,
            "WINMOR" => Mode.Winmor,
            "WSPR" => Mode.Wspr,
            _ => throw new ArgumentException($"Unknown mode: {input}. Examples: FT8, CW, SSB, RTTY"),
        };
    }
}
