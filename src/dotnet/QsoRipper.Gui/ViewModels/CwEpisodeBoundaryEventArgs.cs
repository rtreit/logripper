using System;
using QsoRipper.Domain;

namespace QsoRipper.Gui.ViewModels;

/// <summary>
/// Payload for <see cref="QsoLoggerViewModel.CwEpisodeBoundary"/>. The host
/// (<see cref="MainWindowViewModel"/>) listens and forwards the event to a
/// <see cref="Services.CwDiagnosticsRecorder"/> when one is active so the
/// recorder can finalize the open diagnostic episode.
/// </summary>
internal sealed record CwEpisodeBoundaryEventArgs(
    string Reason,
    QsoRecord? Qso,
    DateTimeOffset UtcStart,
    DateTimeOffset UtcEnd);

/// <summary>
/// Payload for <see cref="QsoLoggerViewModel.CwEpisodeStarted"/>. Fired the
/// moment the operator first types a callsign for a new QSO; the host uses
/// this to begin a diagnostics episode aligned to operator activity rather
/// than to decoder lock state.
/// </summary>
internal sealed record CwEpisodeStartedEventArgs(DateTimeOffset UtcStart);
