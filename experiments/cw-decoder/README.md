# CW Decoder Experiment

This folder is the current sandbox for improving QsoRipper CW decoding on real off-air audio.

The project has converged on two parallel goals:

1. keep a **simple append-only event-stream foundation** that behaves like what the operator actually hears and sees, and
2. layer more ambitious signal-processing experiments on top without allowing them to regress that foundation.

The current breakthrough is that the best live behavior did **not** come from rolling transcript windows, overlap stitching, or commit heuristics. It came from consuming the same stable dit/dah/gap event stream that paints the Visualizer bars and appending each matured event once in audio order. That path is now the "line in the sand": Decode, Labeling, Tuning, Bench, and Visualizer should all key off it first, while experimental decoders are compared against it rather than silently replacing it.

> **Integration status (round 1, issue #321)**: the production GUI now hosts the
> `cw-decoder` binary as a subprocess and auto-fills `QsoRecord.cw_decode_rx_wpm`
> on logged CW QSOs (time-weighted mean over the QSO start/end window, ADIF
> field `APP_QSORIPPER_RX_WPM`). See
> `src\dotnet\QsoRipper.Gui\Services\CwDecoderProcessSampleSource.cs`,
> `src\dotnet\QsoRipper.Gui\Services\CwQsoWpmAggregator.cs`, and the
> **Settings → Display → Monitor radio** section in the main window.
> Round 2 will move the decoder behind an engine-side `CwDecodeService`
> so all clients can consume the same stream.
>
> **Fresh-user prerequisites for the radio monitor to do anything:**
>
> 1. Build the decoder once: from `experiments\cw-decoder\` run
>    `cargo build --release` (this folder is a stand-alone Cargo workspace and
>    is **not** built by the main `cargo build` in `src\rust\`). Built outputs
>    land in `experiments\cw-decoder\target\release\cw-decoder.exe`.
> 2. Either leave the binary at that default location (the GUI walks up from
>    its base directory looking for it) or set the `CW_DECODER_EXE` env var to
>    an absolute path.
> 3. Allow the application access to a capture device. The decoder uses the
>    OS default capture device unless you pick a specific one in the dropdown.
> 4. Open **Settings → Display** in the GUI and tick *Enable radio monitor
>    (auto-fills CW WPM on logged QSOs)*. Pick a capture device from the
>    dropdown — physical inputs (microphones, USB Audio CODEC dongles) appear
>    as plain device names; system output devices that can be tapped via
>    WASAPI loopback appear with a `(system output / loopback)` suffix so you
>    can validate without a radio by playing audio through your speakers.
>    Save. Optionally tick *Show CW WPM in the status bar* (toggle live with
>    `Ctrl+Shift+W`) to see the live WPM readout, which dims when the monitor
>    is off as a reminder. If the decoder gets "stuck" on a wrong baseline
>    (e.g. a slow station hands off to a fast one and the dot/dash estimator
>    doesn't follow), press `Ctrl+Alt+W` to restart the decoder process and
>    let the confidence state machine re-acquire from scratch.
>
> If the binary cannot be found, the monitor toggle silently flips back off
> and the status row reports `CW WPM: decoder not built`. If the binary is
> found but launching fails (e.g. cpal cannot open the capture device), the
> status row reports the underlying error and the toggle flips off.
>
> **Validating the GUI without a radio (loopback / file playback):**
>
> 1. Build the decoder (`cargo build --release` in `experiments\cw-decoder`).
> 2. Open **Settings → Display → Monitor radio**, enable the monitor, and
>    pick a `(system output / loopback)` entry from the *Capture device*
>    dropdown — for example *Speakers (Realtek)  (system output / loopback)*.
>    The dropdown auto-detects the loopback case so you don't need to know
>    the underlying WASAPI plumbing.
> 3. Play a CW practice clip through your speakers. The status row (when
>    enabled with `Ctrl+Shift+W`) reports a live WPM.
> 4. Cross-platform alternative: install VB-Audio Cable or similar, route
>    system output to the cable, and pick the cable's *input* entry from the
>    dropdown (the plain non-loopback variant).
> 5. List candidate device names from the command line with
>    `experiments\cw-decoder\target\release\cw-decoder.exe devices` (add
>    `--json` for machine-readable output that mirrors the GUI dropdown) —
>    this prints both input devices and the output devices that are usable
>    as loopback targets.
> 6. Log a CW QSO during playback and confirm `cw_decode_rx_wpm` is
>    populated on the new row in the recent-QSO grid.

## Current architecture

### Core binaries

#### `cw-decoder`

Main experiment executable. It currently exposes several surfaces:

- **Offline decode**
  - `file` — single-pass or sliding-window whole-file decode through `ditdah`
- **Live capture**
  - `devices` — list available CPAL input devices (add `--json` for machine-readable output that includes both inputs and loopback-capable outputs)
  - `live` — TUI-driven capture + rolling-window `ditdah` decode (legacy interactive surface)
- **Custom streaming decoder**
  - `stream-file` — file-driven streaming decode with optional NDJSON event output and live `--stdin-control` config updates
  - `stream-live` — live capture through the streaming Goertzel decoder, with optional `--record` WAV mirror and `--stdin-control`
  - `stream-live-v2` — whole-growing-buffer `ditdah` replay. This was a valuable intermediate reference because it proved that re-decoding the whole accumulated buffer and replacing the displayed transcript was far better than sliding-window `ditdah` stitching. It is no longer the GUI default, but remains useful for A/B comparison.
  - `stream-live-v3` — **current GUI foundation.** In-house envelope decoder that drives Decode, Labeling, Tuning, Bench, and the **VISUALIZER** tab. Goertzel envelope → percentile-based noise/signal floors → hysteresis state machine → k-means dot/dah classifier. Emits NDJSON `viz` frames (envelope curve, noise/signal floors, hysteresis bands, classified events, on-duration histogram, k-means centroids, current/locked WPM, SNR) so the operator can *see* exactly what the decoder is reacting to. Its transcript is produced by the append-only event-stream decoder in `src\append_decode.rs`: each matured `on_dit`, `on_dah`, `off_char`, and `off_word` bar is consumed once in sample-time order, appended to a raw Morse stream (`.` / `-` / `/` / `//`), and decoded into a single growing text line with real spaces for word gaps. Optional `--pin-wpm` (hard-pins the streamer's `locked_wpm` so the first decode honors the operator), `--pin-hz` (bypasses the auto pitch detector when it locks onto a noise/harmonic peak), and `--min-snr-db` (default 6.0; below this floor the decoder still emits viz frames but suppresses text — kills the "noise-locked dit-spam" failure mode where the auto-pitch detector locks onto a high-tone harmonic). The streamer also enforces a default dynamic-range bimodality gate (`(signal_floor - noise_floor) / envelope_max >= 0.55`) which catches high-variance noise that sneaks past the SNR ratio gate. When either gate fires the visualizer overlays a red `LOW SNR` badge so the suppression is visible. Live captures auto-save to `experiments\cw-decoder\captures\viz-yyyyMMdd-HHmmss.wav` for later labeling. For file replay, `stream-live-v3 --file --play` clocks decoder feeding from the output playback cursor so the bars/transcript stay aligned with the audio instead of drifting in a separate process.
- **Causal ditdah baseline**
  - `stream-file-ditdah` — file-driven causal whole-window `ditdah` replay
  - `stream-live-ditdah` — live capture through the rolling-window causal baseline, with optional `--record` WAV mirror. **Deprecated** — kept only for A/B comparison; the GUI now uses the `stream-live-v3` append foundation.
- **Labeling helpers**
  - `harvest-file` — find candidate "golden copy" windows by intersecting offline `ditdah` and the streaming decoder, optional `--needle` anchors
  - `preview-window` — render a slowed WAV preview of a window for human verification
  - `profile-window` — emit a tone-energy profile for the labeling UI's signal-profile editor
- **Playback helper**
  - `play-file` — play an audio file through the default output device and emit JSON progress for the GUI's inline transport
- **Tone diagnostics**
  - `probe-fisher` — sweep candidate pitches across an audio file and rank them by trial-decode Fisher score
- **Cold-start + lock-stability benchmark**
  - `bench-latency` — feed a deterministic synthetic scenario matrix (silence/noise/voice lead-ins + long-clean-CW lock-stability stress) or a real recording (`--from-file --truth --cw-onset-ms`) through the streaming decoder and report two classes of metrics: cold-start *acquisition latency* (time from CW onset to first stable-N-correct decoded run) and *lock stability* once locked (post-first-lock uptime ratio, `PitchLost` count, relock cycles, longest non-Locked gap). Headline metric is `lat_ms = t_stable_N - cw_onset_ms`. Add `--foundation` to score the append-only event-stream transcript path used by the GUI; in that mode latency-specific lock metrics are intentionally empty and the output is a transcript-quality/regression record.

All `--json` and `--record` flags are what the Avalonia GUI uses to drive the engine over stdout/stderr NDJSON.

### `eval`

Corpus scorer and sweep harness (`src\bin\eval.rs`).

Current uses:

- exact-window scoring against saved `*.labels.jsonl`
- full-stream scoring by replaying whole recordings causally and intersecting transcript state at label boundaries
- foundation strategy scoring (`--strategy-sweep --strategies foundation`) so tuning can compare experimental modes against the same append-only path the GUI uses
- fast parameter sweeps for the causal `ditdah` baseline (`--sweep-ditdah`, optionally `--wide-sweep`)
- a built-in synthetic regression suite (silence, white/bursty/colored noise, clean and noisy synthesized CW at multiple SNRs) when no label flags are supplied

### Stress-test harness (`scripts\stress-gen.ps1` + `scripts\stress-eval.ps1`)

Generates a deterministic matrix of "stressed" copies of a clean baseline WAV via `ffmpeg`, then runs the cw-decoder over every variant and emits a degradation summary + CSV. Useful for catching regressions when changing acquisition or decoding logic, and for honestly measuring how far down the SNR ladder the current implementation can still find and decode a known signal.

The matrix currently covers (per baseline):

- **clean** passthrough at the decoder's native 12 kHz mono s16
- **attenuation ladder**: -6, -12, -18, -24, -30 dB (no added noise)
- **white noise** SNR ladder: 20, 10, 6, 3, 0 dB
- **pink noise** SNR ladder: 20, 10, 6, 3, 0 dB (closer to band hiss)
- **brown / red noise**: 10, 6, 3 dB (atmospheric / QRN-like)
- **narrow IF**: 250–1100 Hz bandpass (simulates a narrow CW filter)
- **QRM**: steady carrier at 850 Hz mixed at -16 dB
- **combined weak-signal presets**: `weak_pink_snr6` (-18 dB signal + pink @6 dB SNR), `weak_pink_snr3` (-24 dB + pink @3 dB SNR)

Generate and score:

```powershell
# Produces 23 .wav variants in data\cw-stress\30wpm\ (gitignored).
.\experiments\cw-decoder\scripts\stress-gen.ps1 `
    -InputWav   data\cw-samples\cw_30wpm_youtube_70s_2min_12k.wav `
    -OutputDir  data\cw-stress\30wpm

# Decodes every variant with the current decoder (default purity 3.0) and
# prints colored per-variant {pitch, WPM, char count, transcript preview}
# plus a stress-results.csv next to the inputs. Add -Truth "..." to also
# get CER vs an operator-supplied ground-truth string.
.\experiments\cw-decoder\scripts\stress-eval.ps1 -StressDir data\cw-stress\30wpm
```

Current observed behavior on the 30 WPM youtube baseline (sender at ~30 WPM):

- decoder reports **29.5 WPM** on `clean` and remains at 28–30 WPM down through the entire attenuation ladder (including -30 dB), through brown/pink/white noise SNR ≥ 6 dB, through the narrow IF, and through QRM at +250 Hz from the CW pitch
- pitch lock starts to wander to a side-bin (~574 Hz) at pink_snr6, white_snr6, weak_pink_snr3 — these are the cases where the next experimental work (top-K candidate tracking, oracle-tone eval) should pay off

Stress audio is large and reproducible from the script, so `data/cw-stress/` is gitignored. Commit only the script changes and any operator-curated `TRUTH.txt` files.

## Decoder families

### 1. Custom streaming decoder

Implemented in `src\streaming.rs`.

Current shape:

- live/file audio is resampled to 12 kHz
- band-limited
- pitch is selected from candidate tones
- Goertzel power is tracked at the chosen tone
- adaptive thresholding + SNR gating produce key-up/key-down state
- on/off durations are classified into dits, dahs, letters, and words

Recent custom-streaming changes on this branch added:

- **keying-aware pitch picking** to resist strong continuous carriers
- **trial-decode Fisher scoring** to rank candidate tones
- **auto-threshold tuning** from running SNR margin so threshold follows QSB
- **post-lock quality watchdog** so weak/dirty locks can be dropped instead of drifting forever
- **adjacent-bin tone purity gate** to suppress broadband impulses (finger snaps, key clicks, splatter) at the source
- **wide-bin sniff** (`--wide-bin-count`) to integrate energy across ±N Goertzel bins for acoustically re-captured CW
- **force-pitch override** (`--force-pitch-hz`) that bypasses acquisition when the operator already knows the target
- **min-pulse / min-gap dot-fraction filters** that reject sub-dot blips and fill sub-dot gaps in the keying envelope (mic-mode default off, file-mode default off, mic preset turns them on)
- **WASAPI loopback capture** (`stream-live --loopback`) for same-machine digital playback (YouTube, browsers, local files) — separates "speaker→mic acoustic recapture" (research) from "render→loopback digital pipe" (operational)
- **centroid pitch picking** as a tiebreaker so locks centre on the energy ridge instead of edge-locking on a side bin
- **mic-mode preset** that bundles wider bins, lower purity, and the min-pulse/min-gap filters in one toggle
- **lockstep decode-and-play** (`stream-file --decode-and-play` / GUI **DECODE+PLAY**) so the audio you hear is exactly the audio being decoded, with a single cursor controlling pause / seek / region trim
- **confidence state machine + held-event buffer** (`hunting` / `probation` / `locked`) so decoded characters never reach the operator until the lock has cleared its first quality watchdog. Bogus locks made on voice formants or impulse noise are silently discarded; genuine CW that started streaming during the verification window is buffered and flushed in order at the moment the lock is confirmed. The GUI surfaces this as a prominent **● LOCKED / ◐ VERIFYING SIGNAL / ○ ACQUIRING TARGET** badge in the status bar.

This path is the more ambitious live decoder, but it still needs better corpus-driven measurement.

### 2. Append-only event-stream foundation

Implemented in `src\append_decode.rs` and surfaced by `stream-live-v3`.

This is intentionally simple:

- the envelope streamer classifies bars as `on_dit`, `on_dah`, `off_intra`, `off_char`, or `off_word`
- each event is anchored to the audio sample range that produced it
- a short trailing stability guard lets gaps mature before they are emitted
- each event is consumed once in audio order
- dits/dahs accumulate into one pending Morse character
- `off_char` flushes that character
- `off_word` flushes that character and appends a real space

The raw debug representation is the same thing without Morse lookup: `.` for a dit, `-` for a dah, `/` for a character gap, and `//` for a word gap. This proved crucial because it made the decoder's output comparable to the colored Visualizer bars without being coupled to Avalonia redraws. Redraw-level logging repeated rolling windows and produced false text; event-stream logging exposed the actual heard sequence.

This is now the current reference path for live decoding and regression prevention. More complex pipelines may improve the event classifier, pitch selection, preprocessing, or spacing policy, but they should preserve this append-only contract or prove a measurable improvement against it.

### 3. Causal ditdah baseline

Implemented in `src\ditdah_streaming.rs`.

This is intentionally simpler:

- keep a rolling audio window
- repeatedly run whole-window `ditdah`
- commit only the prefix that stabilizes across repeated snapshots

This baseline exists because it is:

- understandable
- reproducible
- easier to sweep
- easier to score against human labels

It remains a useful historical reference and comparison strategy for label-driven tuning, but it is no longer the GUI foundation.

## Signal processing architecture

The custom streaming decoder (`src\streaming.rs`) is a chain of stages, each addressing a specific failure mode the project has hit on real off-air audio. Read top-to-bottom — each stage assumes the previous one has done its job.

```
                    raw input audio (file or capture device)
                                    │
                                    ▼
                     ┌───────────────────────────────┐
                     │  resample to 12 kHz mono f32  │  rubato
                     └───────────────┬───────────────┘
                                     ▼
                     ┌───────────────────────────────┐
                     │   HP / LP biquad chain        │  300–1500 Hz CW band
                     └───────────────┬───────────────┘
                                     ▼
                ┌────────────────────┴────────────────────┐
                │ pitch_locked == None?                   │
                └────┬──────────────────────────┬─────────┘
                     │ yes                      │ no
                     ▼                          ▼
        ┌──────────────────────┐   ┌────────────────────────┐
        │ ACQUISITION          │   │ TRACKING               │
        │  • emit "hunting"    │   │  • Goertzel @ pitch    │
        │  • build pre-lock    │   │  • + side bins for     │
        │    audio buffer      │   │    instantaneous       │
        │    (PITCH_LOCK_S or  │   │    tone-purity ratio   │
        │    RELOCK_S after a  │   │  • + wide-bin sniff    │
        │    recent loss)      │   │    (mic-mode integ.)   │
        │  • try_acquire:      │   │                        │
        │     trial_decode     │   │  per-sample gates:     │
        │     Fisher per cand  │   │    amplitude > thr ∧   │
        │     pitch (ditdah    │   │    smoothed SNR ok ∧   │
        │     re-decode)       │   │    tone_purity > k ∧   │
        │  • centroid tiebreak │   │    not impulse         │
        │  • commit best ≥     │   │                        │
        │    MIN_LOCK_FISHER   │   │  on/off → durations →  │
        │  • emit "probation"  │   │  ditdah symbol classif │
        └──────────┬───────────┘   │  → Char / Word events  │
                   │ lock          └─────────┬──────────────┘
                   ▼                         │
                                             ▼
                              ┌──────────────────────────────┐
                              │ CONFIDENCE FILTER            │
                              │   hunting   → drop chars     │
                              │   probation → hold chars     │
                              │   locked    → pass chars     │
                              └──────────┬───────────────────┘
                                         ▼
                              ┌──────────────────────────────┐
                              │ QUALITY WATCHDOG             │
                              │  every QUALITY_CHECK_S over  │
                              │  QUALITY_WINDOW_S buffer:    │
                              │    Fisher < FAST_DROP        │
                              │      → drop, "hunting",      │
                              │        discard held          │
                              │    Fisher in [FAST_DROP,     │
                              │      MIN_HOLD) for N checks  │
                              │      → drop, hysteresis      │
                              │    Fisher ≥ MIN_HOLD         │
                              │      → if probation:         │
                              │          promote, flush held │
                              │        else: keep            │
                              └──────────┬───────────────────┘
                                         ▼
                                  StreamEvent stream
                            (PitchUpdate, Char, Word, Garbled,
                             WpmUpdate, Power, PitchLost,
                             Confidence)
```

### Key design properties

- **Two-stage detection.** Acquisition uses `trial_decode_score` (a real
  ditdah pass on a candidate window) so we only lock on tones that
  actually look like CW — not just strong tones. Tracking is a much
  cheaper per-sample Goertzel + gates path so the steady-state CPU
  cost is small.
- **Acquisition-first hypothesis.** The custom decoder was originally
  the bottleneck; it now isn't. The remaining hard cases on the
  9-label corpus are dominated by **wrong-tone lock**, **late lock**,
  and **lock on noise**, not by symbol classification errors. The
  oracle-tone eval mode and the planned top-K tracker (Phase 3) target
  these directly.
- **Confidence machine = first-class operator UX.** The decoder's
  internal lifecycle (`Hunting` / `Probation` / `Locked`) is exposed
  to the GUI as a coloured status badge, and char-class events are
  gated by it. Bogus locks made on voice formants or transient impulses
  never reach the transcript, even when the watchdog needs several
  seconds of accumulated audio to confirm them as bogus.
- **Held-event buffer keeps probation honest.** While in `Probation`,
  decoded characters are held (not dropped). If the lock survives,
  the held buffer is flushed in order so genuine CW that started
  during the verification window is preserved. If the lock is rejected,
  the held buffer is discarded.
- **Two acquisition surfaces, one engine.** Voice-acoustic capture
  (mic) and digital same-machine playback (loopback) flow through the
  same streaming decoder; the mic path layers on the wide-bin /
  min-pulse / min-gap / lower-purity preset, while loopback uses the
  default file-mode tuning because the audio is bit-identical to the
  source.

### Confidence state machine

```
       (start)
          │
          ▼
   ┌─────────────┐    pitch_lock acquired
   │  Hunting    ├──────────────────────────┐
   │ (red badge) │                          │
   └─────▲───────┘                          ▼
         │                            ┌──────────────┐
         │                            │  Probation   │
         │ watchdog drop              │ (amber badge)│
         │ (Fisher < FAST_DROP, or    └──────┬───────┘
         │  MIN_HOLD failed N times)         │
         │                                   │ first watchdog check
         │                                   │ Fisher ≥ MIN_HOLD
         │ ←─────────── watchdog drop ───────┤
         │                                   ▼
         │                            ┌──────────────┐
         │                            │   Locked     │
         │                            │ (green badge)│
         │                            └──────┬───────┘
         │                                   │
         └─── watchdog drop ─────────────────┘
```

Char-class events (`Char`, `Garbled`, `Word`, `WpmUpdate`):

| State      | Treatment of decoded chars |
|------------|----------------------------|
| Hunting    | Dropped at the gate (lock not even attempted yet, or just lost) |
| Probation  | Buffered in `held_events`, awaiting verdict |
| Locked     | Passed through unchanged; held buffer is flushed on the transition |

Confidence transitions emit `StreamEvent::Confidence { state }`, which serializes as `{"type":"confidence","state":"hunting|probation|locked"}` over NDJSON. The Avalonia GUI reads this on the Decode tab and updates the status badge plus colour scheme.

This was added specifically in response to the YouTube reference clip (`cw_30wpm_youtube_12k.wav`) where the pre-CW voice section was producing decoded garbage like `MI U I EIE N` and the decoder was missing the actual `CQ DE K UR` because the bad voice lock had to age out via the slow steady-state watchdog. With the confidence machine: the bad lock now goes through Probation silently, the watchdog rejects it, the held buffer is discarded, and the operator sees nothing until the real lock at ~604 Hz survives its check and flushes the genuine `73 TNX RST R TU = OM FB ...` transcript.

## GUI architecture

The Avalonia app under `gui\` (titled **CW SCOPE**) is the main operator surface. It launches the Rust `cw-decoder` and `eval` binaries from `experiments\cw-decoder\target\{release,debug}\`, walking up from `AppContext.BaseDirectory` to find them — either build flavor works, with release preferred when both are present. The key constraint is that the GUI does **not** rebuild the Rust engine on its own; if no binary is found it throws with a hint to run `cargo build` (typically `--release`) in `experiments\cw-decoder` first.

The GUI is organized into three tabs: **Decode**, **Labeling**, and **Tuning**.

### Decode tab

The GUI now supports two decoder modes:

| Mode | Backing path | Primary use |
|---|---|---|
| `Custom streaming` | `stream-file` / `stream-live` | primary live-decoder experiment |
| `Baseline ditdah` | `stream-file-ditdah` / `stream-live-ditdah` | honest label-driven A/B reference |

Current decode-tab workflow also includes:

- explicit source control semantics:
  - **DECODE FILE...** opens and immediately decodes a chosen file
  - **START LIVE** captures from the selected input device
  - **DECODE+PLAY** opens a file, decodes it, and plays the same audio in lockstep so what you hear is exactly what is being processed
- live capture
- optional recording of live audio to `data\cw-recordings\`
- offline replay of the last opened source (live recording or file decode)
- **Replay & Score** live-vs-offline comparison with a visible CER chip
- inline audio playback with a shared transport / progress surface
- a real-time playback signal view driven by the same broad-band profile pipeline used in labeling
- an explicit **CURRENT TONE** readout during live decode and playback
- a prominent confidence badge — **● LOCKED** (green), **◐ VERIFYING SIGNAL** (amber), or **○ ACQUIRING TARGET** (red) — that surfaces the streaming decoder's confidence machine to the operator. Decoded characters do not appear in the transcript until the badge is green; while the badge is amber the engine is buffering candidate output and waiting for a quality watchdog confirmation.
- a **Mic mode** preset toggle that bundles wide-bin sniff, lower tone-purity threshold, and the min-pulse/min-gap dot filters into one click for acoustically re-captured CW
- **WASAPI loopback** capture (`stream-live --loopback`) — for same-machine playback decode (YouTube, browsers, local files) the audio is taken from the system render endpoint instead of a microphone, bypassing the speaker→room→mic chain entirely
- an experimental **RANGE LOCK** mode for custom streaming, so live/file decode can prefer the strongest tone inside a chosen Hz window
- an experimental **TONE PURITY** gate that compares each instantaneous target-bin power against the off-band noise bins (q25 of bins at ±150/300/500/700 Hz) at the *same* sample. A real CW tone scores 5–20+ instantaneous purity; a 5 ms broadband impulse (finger snap, key click, lightning, switching ground) lights up *all* bins together so the ratio collapses to ~1 and is rejected at the source. Default `min_tone_purity = 3.0`; set to 0 to disable. Reuses the existing noise bins (no new Goertzels), and runs *before* smoothing so the gate fires faster than the 200 ms noise smoother can equalize.
- an optional **SHOW CHAR HZ** overlay so each decoded character can display the tone the streaming decoder had locked when it emitted that symbol; a companion **SHOW PURITY** toggle adds the per-character peak tone-purity ratio under the Hz line, so spurious characters from broadband impulses (typically `purity ~1`) are visually distinguishable from real CW (`purity 5-20+`)
- a **FORCE PITCH (Hz)** acquisition override that locks the streaming decoder to an exact pitch instead of running auto-acquisition (0 = auto). The Fisher quality watchdog AND the confidence machine are both bypassed when forced — the decoder goes straight to Locked and stays there. Useful when the operator already knows the target tone, or as a diagnostic ("does the decoder fail because of acquisition or downstream?")
- a **WIDE BINS** wide-bin sniff (0–8) that adds companion Goertzels at `pitch ± k * bin_width` and sums their power into the main signal estimate. `0` = single 40 Hz bin (default). `N=2` ≈ 200 Hz of integration bandwidth. Built specifically for **acoustically re-captured CW** (speaker → mic round-trip) where speaker frequency response, room reverb, and slight pitch drift smear the tone across many Goertzel bins; without this gate a single 40 Hz slice catches only ~30% of the signal energy and the keying envelope flickers within elements. CLI: `--wide-bin-count <N>` on `stream-file` and `stream-live`. NDJSON: `"wide_bin_count": <N>`. Combine with `--force-pitch-hz` for live mic capture: e.g. `--force-pitch-hz 620 --wide-bin-count 2`.

The tone-purity gate replaces an earlier "recent-audio re-detection" guard that ran at character emission time. That earlier guard could not catch transient impulses because by the time it re-ran pitch detection the impulse was already history; the new gate runs per Goertzel power sample and ANDs with the existing amplitude / smoothed-SNR gates so a sample only counts as key-down when the locked bin is meaningfully louder than its closely-spaced neighbors.

That replay path is useful for answering: _“what did the live path think happened, and what does an offline rerun on the same captured audio think happened?”_

### Labeling tab

The labeling workflow is now built around exact-window truth:

- harvest candidate regions from recordings
- if harvest finds no regions, fall back to a single whole-file candidate so faint recordings can still be labeled
- preview slowed audio inline inside CW SCOPE instead of shelling out to an external player
- view signal profile
- drag exact start/end handles
- save uppercase verified copy to JSONL

Saved labels retain:

- exact adjusted window
- original harvested window
- `clip_start`
- `clip_end`
- decoder snapshots used during labeling

Signal-profile rendering now also works without a usable pitch lock by falling back to a broadband activity profile. That keeps the editor usable on faint files where neither decoder can confidently lock a tone yet.

Harvest caching is now both:

- in-memory while you stay in the current GUI session, and
- persisted under the local app-data cache so previously harvested files reopen with their cached candidate list after restarting the app, unless you explicitly click **HARVEST** again.

The strong-signal W1AW path also now uses warmup-aware harvest windows for the streaming side, so short-window harvest is no longer forced into whole-file fallback just because the streaming decoder starts cold on every 4-second slice.

### Tuning tab

The tuning workflow is now first-class in the GUI:

- score the current label file, the full corpus, or any checked subset of available `*.labels.jsonl` files
- run parameter sweeps with a coarse pass plus a local refinement pass around the best baseline candidate
- inspect score cards, failure-breakdown bars, and per-label truth-vs-decoded detail instead of raw console text
- inspect sweep rankings with exact-match progress bars plus average / worst CER
- **Apply Top Result**
- score the experimental custom-streaming range-lock path against labels by enabling **RANGE LOCK** on the Tuning tab

Baseline sweep still tunes the causal `ditdah` reference only. When **RANGE LOCK** is enabled, use **Score Labels** to measure the streaming experiment rather than **Sweep Baseline**.

When Decode mode = **Baseline ditdah**, the Decode tab uses the same shared tuning settings as the Tuning tab.

That gives the branch an honest loop:

`label -> score/sweep -> apply top result -> decode tab file/live A/B`

## Labeling model and whether it still makes sense

Yes — **the labeling approach still makes sense and is still worth pursuing**.

The current exact-window + clipped-edge scheme has already paid off because it separated two very different classes of problems:

- **boundary / warmup / commit issues**
- **hard-signal isolation / segmentation issues**

Without the labels, most misses just looked like “decoder bad.”

With the labels, we can already see that:

- strong W1AW-style copy is mostly recoverable
- some misses are specifically leading-edge or commit-policy failures
- the harder contest-style recordings are a different problem class

That said, the current label schema is **necessary but not sufficient** for the hardest recordings.

The next label additions should be optional, not a schema reset:

- target tone estimate / confidence
- multiple-signal flag
- copy-confidence flag
- negative / no-copy labels
- short notes for “weaker target under stronger adjacent station” style cases

So the answer is:

- **keep labeling**
- **keep exact-window truth**
- **do not throw away the current corpus**
- **expand metadata only where hard signals need more context**

## Experiments to date

## Phase 1: custom streaming decoder and real-audio harvest

Initial work established:

- real-file decode
- live audio capture
- an early custom streaming path
- harvest from offline/stream agreement
- pause-bounded region snapping

This gave the project real off-air candidate regions instead of synthetic clean-CW toy cases.

## Phase 2: exact-window human labeling

The GUI labeling workflow added:

- slowed preview audio
- signal-profile editing
- exact-window saved truth
- clipped-edge flags

This is the key change that turned the experiment into a measurable loop.

## Phase 3: simplified causal baseline

Whole-window `ditdah` succeeded on the strong W1AW sample where the earlier streaming path missed copy. That led to the simplified causal baseline:

- repeated whole-window `ditdah`
- prefix stabilization
- streaming-style transcript commit

This baseline became the reference scorer target.

## Phase 4: scorer and parameter sweep

`eval` now supports:

- label discovery via `--labels-dir` / `--all-labels`
- repeated `--labels <file>` arguments so scoring/sweeping can target an arbitrary subset of label files
- exact-window scoring
- full-stream scoring
- wide and interactive sweeps
- sweep ranking by exact matches, total edit distance, average CER, and worst-case CER
- failure classification such as:
  - `exact`
  - `leading_edge_error`
  - `near_match`
  - `spacing_only_error`
  - `garbage_decode`
  - `empty_output`

## Phase 5: GUI integration

The experiment no longer depends on shell-only tuning:

- Tuning tab exposes score/sweep
- Apply Top Result connects sweeps to baseline decode
- harvest results are cached per file and persisted across app restarts
- Decode tab can record live audio and replay it offline for CER comparison
- Decode and Labeling now share inline audio playback instead of launching an external media player
- CW SCOPE now shows a moving signal profile/playhead during playback

### Integration with the QsoRipper GUI (Round 1, PR #324)

The QsoRipper desktop GUI (`src\dotnet\QsoRipper.Gui`) hosts `cw-decoder.exe`
as a subprocess (`stream-live --json`) and consumes the NDJSON event stream
to drive two operator-facing surfaces:

- **CW WPM auto-fill**: when a CW QSO is logged, the time-weighted mean WPM
  over the QSO start→end window is written to `QsoRecord.cw_decode_rx_wpm`.
- **F9 CW Stats pane**: live overlay showing the current confidence/lock
  state, signal pitch (Hz), instantaneous WPM, last decoded characters and
  the most recent garbled symbol. Driven entirely by the same NDJSON stream
  the CW Scope tooling uses.

The episode boundary for both surfaces is **operator activity, not decoder
lock**: the QSO clock starts the moment the operator first types a callsign
and ends on save or clear. The decoder process itself runs continuously
whenever Radio Monitor is enabled.

### Advanced diagnostics mode

Enable **Settings → Advanced CW diagnostics** to capture an offline-debug
bundle for every QSO. Each radio-monitor session writes to:

```text
%LOCALAPPDATA%\QsoRipper\diagnostics\session-<UTC>\
  session.json                 startup metadata (binary, device, loopback)
  session.wav                  continuous mirror of decoder input audio
  session-events.ndjson        every raw NDJSON line emitted by the decoder
  episodes\episode-NNN\
    events.ndjson              decoder events between callsign-typed and save/clear
    ux-snapshot.json           comparison: aggregator mean vs displayed UI
                               WPM vs in-window samples + the QsoRecord +
                               a copy/paste repro command
```

The repro command in each `ux-snapshot.json` re-runs the decoder against the
captured WAV over the same time window. Sample form:

```text
cw-decoder decode-and-play --json --start 12.4 --end 47.9 "session.wav"
```

Use this to compare what the operator saw in the status bar against what the
decoder would emit in a deterministic offline replay — the canonical way to
debug round 1 WPM regressions without trying to reproduce live propagation.

WAV size is roughly 330 MB/hour (48 kHz mono, 16-bit) and is not rotated in
round 1. Disable diagnostics or prune `%LOCALAPPDATA%\QsoRipper\diagnostics`
manually between debug sessions.

### WPM emission smoothing (#326)

The first live capture made with the diagnostics bundle revealed a failure
mode in `current_wpm()`: a sustained signal degradation produced a
monotonically drifting raw WPM (11.3 → 6.75 over ~6 s on a real on-air QSO)
while the pitch lock was still nominally healthy. The pitch-quality
watchdog only fired ~6 s after the WPM had already collapsed, so the
operator-facing speed dropped from a correct ~13 WPM to ~6 WPM mid-QSO.

`StreamingDecoder` now emits a smoothed value instead of the raw
`current_wpm()` in `StreamEvent::WpmUpdate`:

1. **Median over the last `WPM_SMOOTH_WINDOW` (=7) raw samples.** Rejects
   single degenerate calibration windows where one mis-classified
   character produces a wild dot-length estimate.
2. **Rate cap of `WPM_MAX_REL_DELTA_PER_EMIT` (=3%) per emit.** Real
   operators cannot physically alter keying speed faster than this between
   adjacent character emits; anything larger is the dit-cluster
   calibration tracking the noise instead of the operator. Genuine WPM
   changes still converge in ~3 s; a crashing calibration gets stretched
   far enough that the watchdog drops the lock first.

The internal `current_wpm()` is unchanged and is still what end-of-run
summaries and the harvest output use. Replaying the original captured
session through the fixed decoder shows the displayed WPM staying above
9.5 WPM across the same crash window where the pre-fix value bottomed at
6.75 WPM.

## Current labeled corpus

Label files live at the **repo root** under `data\cw-samples\`, not under `experiments\cw-decoder\`. `--all-labels` resolves `data\cw-samples\` relative to the current working directory, so it works fine when `eval` is invoked from the repo root and silently finds nothing when invoked from elsewhere. The examples below use `--labels-dir data\cw-samples` to make the path explicit, but `--all-labels` is equivalent when the cwd is the repo root.

Current corpus files and label counts:

| File | Labels |
|---|---|
| `data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.labels.jsonl` | 6 |
| `data\cw-samples\k5zd-zs4tx-80m-qso.labels.jsonl` | 2 |
| `data\cw-samples\K1ZZ_de_DH8BQA_CQWWCW_CW_20151129_174710Z_14MHz.labels.jsonl` | 1 |

Current size: **9 labels**

Companion `.mp3` recordings (including `K1ZZ_de_LA8OM_*`, `k5zd-ey8mm-40m-qso`, and ad-hoc `radio-*` captures) are present but not yet labeled.

## Current results

### Label corpus reference scores

The corpus currently has **9 labels**. The safest command form is explicit about the repo-root label directory:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels-dir data\cw-samples
```

`--all-labels` is equivalent when run from the repo root, but it resolves `data\cw-samples\` relative to the current working directory and is easier to misuse from `experiments\cw-decoder`.

Current exact-window score:

- **6 / 9 exact**
- **avg CER = 0.10**
- **total edit distance = 12**

Current full-stream score:

- **1 / 9 exact**
- **avg CER = 0.85**

Interpretation:

- exact-window scoring still tells us what the classifier can do when the target audio is bounded correctly
- full-stream scoring tells us that acquisition, segmentation, gap maturity, and finalization are the hard live problems
- the append-only event-stream foundation is the current best live-facing compromise because it removes rolling-window string replacement/stitching from the critical path while staying directly measurable against labels and replay transcripts

### Append-foundation smoke evidence

The foundation path now runs through the same Rust core in every surface:

- `stream-live-v3` emits the append transcript as primary `text` / `transcript`
- `cursor_transcript` keeps the older event-cursor transcript for diagnostics
- `raw_morse` exposes the raw event stream for bar-level debugging
- `eval --strategy-sweep --strategies foundation` compares the foundation against other strategies
- `bench-latency --foundation --json` emits transcript-quality rows for the append foundation
- the GUI defaults Decode/Labeling file and live runs to **Append event stream (foundation)**
- the Visualizer still has an **APPEND DECODE** view/debug path, but it is now aligned with the same underlying append contract

On synthetic PARIS bench scenarios the foundation clean/noise transcripts are recognizable immediately (`PARIS PARIS ...`) while voice-lead-in scenarios still show garbage before the target appears. That is useful: it confirms the foundation is simple and honest rather than hiding acquisition/target-isolation failures behind post-hoc stitching.

## Current thinking

## What we know with reasonable confidence

1. **The simple append-event stream is working much better than the rolling-window transcript machinery.**
   The important shift was moving from "decode a rolling window, then stitch text" to "classify bars, then append matured events once." That removes a whole class of ghost characters, repeated prefixes, disappearing/replacing text, and overlapping-window artifacts.
2. **Visualizer truth is event truth, not redraw truth.**
   The colored bars are a rolling display. Logging every redraw records repeated partial windows (`..`, then `..-`, then `..- ...`) and creates fake Morse. The useful debug layer is the audio-time event stream beneath the redraw.
3. **Spacing is now visible and testable.**
   The raw stream (`.` / `-` / `/` / `//`) made it obvious when a word gap was being emitted where a character gap was expected, for example `...//.-` (`S A`) instead of `.../.-` (`SA`). Future spacing work can now target that exact failure instead of guessing from final text.
4. **Audio/playback synchronization matters.**
   The old Visualizer file path decoded in one process and played audio in another, so visual bars could lead or lag what the operator heard. `stream-live-v3 --file --play` fixes this by using one process and feeding the decoder from the output playback cursor.
5. **Hard contest audio is still the frontier.**
   The foundation does not magically solve target isolation, voice lead-ins, same-band QRM, or weak/noisy spacing. It gives us a stable place to measure those failures without rolling-window artifacts obscuring them.

## What this implies for next steps

### Keep the append foundation as the non-regression line

The append-only path is now the default contract:

```text
audio -> envelope/viz events -> append event decoder -> transcript
```

Regressions should be caught at several levels:

1. **Unit level:** `AppendEventDecoder` tests should cover repeated `viz` frames, character gaps, word gaps, and final pending-character flush.
2. **CLI level:** `stream-live-v3 --json` transcript events must keep `transcript` / `text` as the append-foundation text, with `cursor_transcript` only as diagnostics.
3. **GUI level:** Decode, Labeling, Bench, Tuning, and Visualizer should default to or explicitly include `foundation`; any future mode should be labeled as experimental.
4. **Corpus level:** every future algorithm should report against `--labels-dir data\cw-samples` and include `foundation` in strategy sweeps.
5. **Bench level:** `bench-latency --foundation --json` should remain a quick smoke that emits recognizable transcript rows before deeper latency metrics are trusted.

The rule of thumb: improvements may change how events are detected, filtered, or classified, but they should not reintroduce rolling text stitching as the primary live transcript path.

### Keep pursuing labeling, but evolve it carefully

Do **not** stop investing in labels.

Instead:

1. keep the current exact-window label corpus active
2. add richer metadata only for hard cases
3. add some negative / no-copy examples
4. add target-tone hints where multiple CW signals are present

### Use foundation-first evaluation

For corpus work, the current order should stay:

1. append-foundation score / replay transcript
2. exact-window label score as the upper-bound classifier check
3. full-stream score as the live segmentation/finalization check
4. experimental strategy sweeps that always include `foundation`

### Use experiments as layers above the stable base

The promising future work is no longer "replace the foundation." It is "make better events for the foundation to append":

- **spacing classifier:** tune char-vs-word gap thresholds, make gap maturity explicit, and score raw Morse gaps against labels where possible
- **target isolation:** track multiple tone ridges and choose or present candidates instead of winner-takes-all pitch lock
- **preprocessing:** bandpass-around-pitch and dynamic compression helped real radio clips, but must be gated so clean synthetic CW does not regress
- **matched element scoring:** replace hard threshold chatter with soft scores over candidate 1-dot, 3-dot, and 7-dot windows
- **lock/acquisition policy:** speed up acquisition after voice/noise lead-ins without allowing noise-locked dit spam
- **region segmentation:** detect active CW spans and compare region-local decode against the append live transcript
- **multi-surface diagnostics:** keep raw Morse, transcript, bars, WPM, pitch, SNR, and label CER tied to the same audio-time cursor

But every improvement must be measured back against:

- the label corpus
- replay CER
- raw Morse gap fidelity
- foundation-vs-experiment deltas

## Recommended next steps

1. **Promote foundation regression checks.**
   Add/keep tests around `src\append_decode.rs`, require `foundation` in strategy sweeps, and preserve `raw_morse`/`cursor_transcript` diagnostics so future changes can explain differences instead of only showing final text.
2. **Quantify spacing failures.**
   The current foundation exposed word-gap mistakes cleanly. The next useful scorer should classify failures as character substitution vs char-gap vs word-gap errors.
3. **Close the acquisition gap after voice/noise lead-ins.**
   Synthetic bench results show the append foundation is honest: clean/noise PARIS is recognizable, but voice lead-ins still create pre-target garbage. That points to better target detection and lock admission, not transcript stitching.
4. **Layer preprocessing carefully.**
   Real-radio bandpass+compander preprocessing can help dramatically, but it previously broke clean synthetic CW in some paths. Treat preprocessing as an optional layer above the foundation with explicit A/B coverage.
5. **Add richer label metadata for hard cases.**
   Target tone, multi-signal flag, negative/no-copy regions, and gap annotations will make future experiments much easier to judge.
6. **Top-K candidate tracker.**
   Replace single-pitch lock with a CFAR-scored ridge tracker over 350-1500 Hz so multi-signal contest audio can present per-track candidates instead of one winner-takes-all lock.

Current evidence suggests:

- **clean/noisy single-target misses** -> spacing maturity and final pending-character flush
- **voice lead-in misses** -> acquisition / lock admission
- **contest/multi-signal misses** -> target isolation + segmentation

## Practical workflow today

If the goal is the fastest useful loop on another PC with live radio audio:

1. pull this branch
2. build `experiments\cw-decoder`
3. run the GUI
4. use **Append event stream (foundation)** for Decode / Labeling / Visualizer
5. record live audio and use replay/label scoring to compare against ground truth
6. keep `foundation` in every strategy sweep so improvements are real and regressions are obvious

If the goal is custom-streaming research:

- keep the GUI default at **Append event stream (foundation)**
- use live recording + replay CER for quick iteration
- keep the label corpus as the harder regression gate
- treat other modes as experiments layered above the stable base

## Build and run

The Avalonia GUI launches whichever Rust binaries it finds under `experiments\cw-decoder\target\{release,debug}\` (release preferred) but does **not** rebuild them. A debug build is enough to make the GUI run; release is recommended for realistic decode latency. Build the engine first, then the GUI:

```powershell
cargo build --release --manifest-path experiments\cw-decoder\Cargo.toml
dotnet build experiments\cw-decoder\gui\CwDecoderGui.csproj
```

Run the GUI:

```powershell
dotnet run --project experiments\cw-decoder\gui\CwDecoderGui.csproj
```

If you want to smoke-test inline playback directly from the CLI:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- play-file data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.mp3 --json
```

Run the scorer on the full corpus (from the repo root, since labels live under `data\cw-samples\`):

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels-dir data\cw-samples --window 20 --min-window 0.5 --decode-every-ms 1000 --confirmations 3
```

Run the full-stream scorer:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels-dir data\cw-samples --mode full-stream --window 20 --min-window 0.5 --decode-every-ms 1000 --confirmations 3 --post-roll-ms 1500
```

Run the foundation strategy in the same sweep harness used by the Tuning tab:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels-dir data\cw-samples --strategy-sweep --strategies foundation
```

Run the experimental range-lock scorer against a focused label subset:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.labels.jsonl --experimental-range-lock --range-lock-min-hz 550 --range-lock-max-hz 850
```

Run a baseline `ditdah` parameter sweep against the corpus:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels-dir data\cw-samples --sweep-ditdah --wide-sweep --top 10
```

Run scoring or sweeping against a hand-picked subset of labels:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --labels data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.labels.jsonl --labels data\cw-samples\k5zd-zs4tx-80m-qso.labels.jsonl --sweep-ditdah --top 5
```

Without any `--labels` / `--labels-dir` / `--all-labels` flag, `eval` falls back to its built-in synthetic suite (silence, noise, clean/noisy synthesized CW) instead of label scoring.

On the debug binary, label sweeps can still take a few minutes because each config replays the selected corpus. For practical tuning loops, prefer `cargo build --release --bins` so CW SCOPE launches the faster release `eval.exe` / `cw-decoder.exe`.

Probe likely target tones by Fisher score:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- probe-fisher data\cw-samples\k5zd-zs4tx-80m-qso.mp3 --min-hz 350 --max-hz 1500 --step-hz 10 --top 8
```

Run the cold-start + lock-stability benchmark on the synthetic scenario matrix (silence/noise/voice/long-clean-CW lead-ins; latency `lat_ms = t_stable_N - cw_onset_ms`, plus post-lock uptime / drops / relock cycles / longest non-Locked gap):

```powershell
.\experiments\cw-decoder\target\release\cw-decoder.exe bench-latency
```

Same benchmark on a real recording (operator supplies CW onset + truth):

```powershell
.\experiments\cw-decoder\target\release\cw-decoder.exe bench-latency `
    --from-file data\cw-recordings\live-20260422-220247.wav `
    --cw-onset-ms 0 `
    --truth "W7LXN DE WA?FBSA K" `
    --stable-n 3
```

Compare two configurations by tagging each run with `--label`. Combine with `--json` to capture machine-readable rows for offline comparison:

```powershell
.\experiments\cw-decoder\target\release\cw-decoder.exe bench-latency --label baseline    --json > bench-baseline.ndjson
.\experiments\cw-decoder\target\release\cw-decoder.exe bench-latency --label no-purity   --purity 0 --json > bench-no-purity.ndjson
```

Run the append-foundation bench smoke. This records transcript quality for the GUI foundation; latency and lock fields are intentionally empty in this mode:

```powershell
.\experiments\cw-decoder\target\release\cw-decoder.exe bench-latency --foundation --json > bench-foundation.ndjson
```

List live audio devices and run the legacy TUI:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- devices
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- live --device "USB Audio CODEC"
```

Run the 30 WPM abbreviation bench across the full real-audio variant matrix (12 scenarios from clean to chaos):

```powershell
# One-time: regenerate the variant WAVs (gitignored, ~50 MB total).
.\experiments\cw-decoder\scripts\gen-30wpm-variants.ps1

# Then bench:
.\experiments\cw-decoder\scripts\bench-30wpm.ps1 -Label default
```

The variant matrix is intentionally tiered:

| Tier | Variants | What stresses the decoder |
|---|---|---|
| baseline | `clean`, `weak`, `qrn`, `qsb`, `weak_qsb` | mild SNR / fade — decoder should pass cleanly |
| extreme | `extreme_qrn`, `crushed`, `deep_qsb`, `buried` | heavy brown noise (mostly killed by the 300 Hz HP) + deep slow QSB; `buried` combines all three and is where the decoder first cracks |
| harsh   | `harsh_white`, `inband_qrm`, `chaos` | white / CW-band-bandpassed noise the front end *cannot* filter away; locks acquire instantly on the right pitch but symbol classification fails — this is where the decoder's downstream gating becomes the bottleneck rather than acquisition |

The `harsh_white` / `inband_qrm` / `chaos` variants currently expose a real weakness: even with a healthy lock and a passed Fisher confidence check, the keying envelope chatters in dense in-band noise and the ditdah classifier emits long runs of garbage characters. Improving the `false_chars_before_stable` metric on these three is the next concrete bench target. Tracked in [#320](https://github.com/rtreit/qsoripper/issues/320).

### Downstream classifier hardening for #320 (chatter-merge + duration sanity + rescue suppression)

The first hysteresis-only patch killed the long ghost-character stream on `harsh_white` but did not change the underlying truth: the dense-noise variants chatter the keying envelope, and the dot/dah classifier was happy to interpret the chatter as a stream of `E`s and `T`s. A second pass landed four cooperating fixes (all opt-in, all exposed via CLI / JSON config / bench script):

1. **Hard ON-duration sanity gate** (always on). After the dot length is known, ON intervals shorter than `0.4 · dot` or longer than `4.8 · dot` are dropped *and* the in-progress letter is cleared. This stops both threshold chatter from being classified as a dit and giant QRM blobs from being classified as a dah. Tracked by the `invalid_on_duration_dropped` counter.
2. **Single-element rescue suppression**. The previous code rescued any `valid_morse` letter when the rhythm gate was closed, which let single-element `E` (`.`) and `T` (`-`) ghosts leak through every short blip. The rescue is now restricted to multi-element patterns, gated by `RhythmGate::was_recently_mature`. Tracked by `single_element_rescue_suppressed`.
3. **Real merge for `min_gap_dot_fraction`**. Previously a short OFF was just dropped, leaving the surrounding ON runs to be classified separately (so a real dah broken by a tiny noise dip could become `. .` instead of `-`). The new sanitizer (`sanitize_interval` in `streaming.rs`) actually fuses the surrounding ON intervals into one element before classification. Tracked by `short_gaps_bridged` and `on_runs_merged`.
4. **Hysteresis wired through every streaming path**. `--hysteresis-fraction` is now accepted by `cw-decoder stream-file`, `stream-live`, `decode-and-play`, `bench-latency`, and `eval` (previously only `bench-latency` and JSON-stdin took it). The `bench-30wpm.ps1` script gained `-Hysteresis`, `-MinGap`, and `-MinPulse` parameters so the full sweep is one command.

Bench JSON now also carries a `decoder_counters` block (`raw_edges_total`, `short_pulses_dropped`, `short_gaps_bridged`, `on_runs_merged`, `invalid_on_duration_dropped`, `single_element_rescue_suppressed`, `chars_emitted`, etc.) so a future tuning pass can prove a config improves CER instead of just suppressing output.

Best-known bench config on the 12-variant matrix is `-Hysteresis 0.3 -MinGap 0.2 -MinPulse 0.3`:

| variant       | baseline ghost | best-config ghost | baseline lat_ms | best-config lat_ms |
|---------------|---------------:|------------------:|----------------:|-------------------:|
| clean         | 0              | 0                 | 15700           | 15700              |
| weak          | 0              | 0                 | 14000           | 14000              |
| qsb           | 2              | 0                 | 14700           | 14700              |
| weak_qsb      | 2              | 1                 | 14000           | 14000              |
| crushed       | 6              | 0                 | 17800           | 17800              |
| deep_qsb      | 3              | 1                 | 14000           | 14000              |
| harsh_white   | **343**        | **0**             | 110500          | (never stable)     |
| inband_qrm    | 0              | 0                 | (never stable)  | (never stable)     |
| chaos         | 0              | 0                 | (never stable)  | (never stable)     |

So the four-patch combination reduces ghost output across the whole baseline tier (zero on most, one on the two QSB-heavy variants) without regressing latency, and it completely silences the harsh-tier ghost flood. The remaining gap — getting `harsh_white` / `inband_qrm` / `chaos` to a *stable* lock at all — is no longer a downstream-classifier problem; it is acquisition under continuous in-band carriers, which is what the next iteration (CFAR-style local-contrast detection or envelope-against-slow-carrier-floor) needs to address. The acceptance criteria in #320 are still not fully met, but the ghost-character class of failure that the issue opened with is resolved.

### Opt-in CFAR keying (#322)

A first cut of the CFAR keying idea ships behind an opt-in flag. With `--cfar-keying` (or `-CfarKeying` in `bench-30wpm.ps1`) the on/off threshold state machine is fed the dimensionless ratio `smoothed / noise` instead of raw `smoothed` Goertzel power, and the global `snr_ok` gate is bypassed (the rolling-quantile threshold over the ratio supplies its own discrimination). This is the only per-frame variant from the #322 experiment matrix that produced any stable transcript on `harsh_white` (stable @72.9 s with 38 ghost characters before lock). It costs the `clean` and `qsb` scenarios in exchange, so the flag stays off by default; the production decoder behavior across all 12 baseline scenarios is unchanged.

| Mode | PASS | WARN | FAIL | Total ghost | `harsh_white` |
| --- | --- | --- | --- | --- | --- |
| default (no flag) | 6 | 3 | 3 | 2 | no stable |
| `-CfarKeying` | 5 | 3 | 4 | 42 | **stable @72.9 s** |

The empirical conclusion (recorded on issue #322): per-frame normalization alone cannot crack the harsh tier without regressing the clean tier. The `--cfar-keying` substrate is the foundation for the next iteration — soft element-window matched scoring that integrates the ratio metric over candidate ~1-dot, ~3-dot, and ~7-dot windows once the dot estimate is primed.

## Repo-local artifacts

- `gui-screenshot*.png` — historical GUI screenshots tracking visual iteration on the Decode tab
- `screenshots\sensitivity-panel.png` — close-up of the sensitivity / threshold panel
- `target\` — local Cargo build output (debug + release) for `cw-decoder` and `eval`
- `gui\bin\`, `gui\obj\` — local .NET build output for the Avalonia GUI
- `bench-runs\` — per-label JSON results from `bench-30wpm.ps1`
- `artifacts\run\cw-debug-bars-*.txt` — Visualizer append-debug raw Morse streams (`.` / `-` / `/` / `//`) flushed when a clip stops

These are not committed-meaningful build artifacts; they exist to make the GUI runnable without an extra build step on the developer machine.
