namespace QsoRipper.Gui.Services;

/// <summary>
/// Confidence/lock state reported by the cw-decoder via its
/// <c>confidence</c> NDJSON event. Mirrors the Rust
/// <c>ConfidenceState</c> enum: see
/// <c>experiments/cw-decoder/src/streaming.rs</c>.
/// </summary>
/// <remarks>
/// The GUI uses this to gate the "live WPM" displays — when the
/// decoder is not <see cref="Locked"/>, the WPM/decoded-text panels
/// are stale by definition (the decoder emits no further wpm/char
/// events while hunting) and must be visually distinguished from a
/// fresh in-progress decode.
/// </remarks>
internal enum CwLockState
{
    /// <summary>No <c>confidence</c> event has been seen yet.</summary>
    Unknown = 0,

    /// <summary>Decoder is searching the spectrum for a CW tone.
    /// No <c>char</c>/<c>wpm</c>/<c>word</c> events will arrive.</summary>
    Hunting = 1,

    /// <summary>Pitch lock acquired but not yet promoted; decoded
    /// events are buffered upstream until the watchdog confirms
    /// or rejects the lock.</summary>
    Probation = 2,

    /// <summary>Decoder is actively emitting decoded characters
    /// and WPM updates against a confirmed pitch lock.</summary>
    Locked = 3,
}
