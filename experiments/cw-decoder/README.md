# CW Decoder Experiment

This folder is the current sandbox for improving QsoRipper CW decoding on real off-air audio.

The project has converged on two parallel goals:

1. keep a **boring, label-driven reference path** that we can score and tune honestly, and
2. keep iterating on a **faster custom streaming path** for eventual live use.

Today, the reference path is the causal `ditdah` baseline. The custom streaming decoder has improved substantially, but it is still not the only truth source and should not replace corpus-driven evaluation yet.

## Current architecture

### Core binaries

#### `cw-decoder`

Main experiment executable. It currently exposes several surfaces:

- **Offline decode**
  - `file` — single-pass or sliding-window whole-file decode through `ditdah`
- **Live capture**
  - `devices` — list available CPAL input devices
  - `live` — TUI-driven capture + rolling-window `ditdah` decode (legacy interactive surface)
- **Custom streaming decoder**
  - `stream-file` — file-driven streaming decode with optional NDJSON event output and live `--stdin-control` config updates
  - `stream-live` — live capture through the streaming decoder, with optional `--record` WAV mirror and `--stdin-control`
- **Causal ditdah baseline**
  - `stream-file-ditdah` — file-driven causal whole-window `ditdah` replay
  - `stream-live-ditdah` — live capture through the causal baseline, with optional `--record` WAV mirror
- **Labeling helpers**
  - `harvest-file` — find candidate "golden copy" windows by intersecting offline `ditdah` and the streaming decoder, optional `--needle` anchors
  - `preview-window` — render a slowed WAV preview of a window for human verification
  - `profile-window` — emit a tone-energy profile for the labeling UI's signal-profile editor
- **Playback helper**
  - `play-file` — play an audio file through the default output device and emit JSON progress for the GUI's inline transport
- **Tone diagnostics**
  - `probe-fisher` — sweep candidate pitches across an audio file and rank them by trial-decode Fisher score
- **Cold-start + lock-stability benchmark**
  - `bench-latency` — feed a deterministic synthetic scenario matrix (silence/noise/voice lead-ins + long-clean-CW lock-stability stress) or a real recording (`--from-file --truth --cw-onset-ms`) through the streaming decoder and report two classes of metrics: cold-start *acquisition latency* (time from CW onset to first stable-N-correct decoded run) and *lock stability* once locked (post-first-lock uptime ratio, `PitchLost` count, relock cycles, longest non-Locked gap). Headline metric is `lat_ms = t_stable_N - cw_onset_ms`. Use `--label <name>` and `--json` to collect comparison runs across decoder configurations.

All `--json` and `--record` flags are what the Avalonia GUI uses to drive the engine over stdout/stderr NDJSON.

### `eval`

Corpus scorer and sweep harness (`src\bin\eval.rs`).

Current uses:

- exact-window scoring against saved `*.labels.jsonl`
- full-stream scoring by replaying whole recordings causally and intersecting transcript state at label boundaries
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

### 2. Causal ditdah baseline

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

It is the current reference path for label-driven tuning.

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

Using the current reference baseline settings:

- `window = 20.0s`
- `min-window = 0.5s`
- `decode-every = 1000ms`
- `confirmations = 3`

### Exact-window baseline score

Current scorer result:

- **7 / 10 exact**
- **avg CER = 0.09**
- **total edit distance = 12**

Interpretation:

- **K1ZZ / DH8BQA** is now exact
- **W1AW** is mostly solved in exact-window mode (6 / 7 exact); the remaining miss is a **leading-edge / warmup** problem (`TUST...` vs `QST...`)
- **80m K5ZD / ZS4TX** labels still both miss, currently classified as one `garbage_decode` and one `near_match`
- The new **TONE PURITY** gate is a no-op on this corpus — exact-window numbers are unchanged whether `--min-tone-purity 3.0` (default) or `--min-tone-purity 0` is used. The gate fires only on broadband impulses, which the labeled CW does not contain.

### Full-stream baseline score

Current scorer result with:

- `mode = full-stream`
- `post-roll = 1500ms`

is:

- **1 / 10 exact**
- **avg CER = 0.92**
- dominated by `empty_output` and `garbage_decode`

Interpretation:

- the baseline is much better as an **exact-window decode reference** than as a fully solved streaming-commit path
- commit timing, final flush, and segmentation are still major open problems
- this is a useful result, not a failure: it tells us the corpus is measuring something real

## Current thinking

## What we know with reasonable confidence

1. **Labeling was the right move.**
   It exposed boundary failures vs target-isolation failures much more clearly than raw listening or eyeballing waveforms.
2. **The baseline remains the best tuning reference.**
   It is simpler, sweepable, and already performs well enough on the easier labels to be meaningful.
3. **The custom streaming decoder has improved materially.**
   The new Fisher-based tone selection, adaptive thresholding, and lock watchdogs are promising, especially for live operation.
4. **Hard contest audio is still the frontier.**
   The remaining hard cases do not look like “just simplify the streamer” problems.

## What this implies for next steps

### Keep pursuing labeling, but evolve it carefully

Do **not** stop investing in labels.

Instead:

1. keep the current exact-window label corpus active
2. add richer metadata only for hard cases
3. add some negative / no-copy examples
4. add target-tone hints where multiple CW signals are present

### Keep the baseline as the main evaluation reference

For corpus work, the current order should stay:

1. exact-window baseline score
2. full-stream baseline score
3. custom live/offline replay comparison
4. future custom-streaming corpus score once instrumentation is ready

### Use the custom streaming path as the algorithm sandbox

The custom decoder is where more aggressive work belongs:

- better target isolation
- better lock retention / drop policy
- better segmentation under contest-style pacing
- lower ghost output

But improvements there should be measured back against:

- the label corpus
- replay CER
- exact-window vs full-stream deltas

## Recommended next steps

1. **Close the acquisition-latency gap on cold-start CW after a long voice lead-in.**
   On the YouTube reference clip the decoder now correctly silences the bogus voice lock and recovers when real CW arrives, but the first ~10 characters (`CQ DE K UR ...`) are missed because the pre-lock Fisher search needs ~12 s of CW audio to commit a lock once a stale lock has been dropped. The fix is on the acquisition side, not the confidence machine: faster Fisher convergence, shorter `RELOCK_SECONDS` for the cold-start case, or a reduced `PITCH_LOCK_SECONDS` window once the previous lock has been explicitly rejected as bogus.
2. **Add richer label metadata for hard cases**
   - target tone (Phase 1A oracle-tone eval)
   - multi-signal flag
   - negative/no-copy labels (Phase 2 false-chars/min metric)
3. **Score the custom streaming path against the same corpus end-to-end**
   The branch now has stronger custom logic, but the corpus README story should eventually include real custom-vs-baseline numbers rather than replay-only intuition.
4. **Improve full-stream commit behavior**
   The baseline full-stream score shows that finalization and region-close behavior are still weak.
5. **Use `probe-fisher` and label metadata together**
   This looks like the right next diagnostic loop for multi-signal contest audio.
6. **Top-K candidate tracker (Phase 3)** — replace single-pitch lock with a CFAR-scored ridge tracker over 350–1500 Hz so multi-signal contest audio can present per-track candidates instead of one winner-takes-all lock.

Current evidence suggests:

- **W1AW-like misses** -> boundary / commit / warmup
- **80m contest misses** -> target isolation + segmentation

## Practical workflow today

If the goal is the fastest useful loop on another PC with live radio audio:

1. pull this branch
2. build `experiments\cw-decoder`
3. run the GUI
4. use **Baseline ditdah** for honest tuning
5. record live audio and use **Replay & Score**
6. keep using the label corpus to decide whether improvements are real

If the goal is custom-streaming research:

- keep the GUI default at **Custom streaming**
- use live recording + replay CER for quick iteration
- keep the label corpus as the harder regression gate

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

List live audio devices and run the legacy TUI:

```powershell
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- devices
cargo run --release --manifest-path experiments\cw-decoder\Cargo.toml -- live --device "USB Audio CODEC"
```

## Repo-local artifacts

- `gui-screenshot*.png` — historical GUI screenshots tracking visual iteration on the Decode tab
- `screenshots\sensitivity-panel.png` — close-up of the sensitivity / threshold panel
- `target\` — local Cargo build output (debug + release) for `cw-decoder` and `eval`
- `gui\bin\`, `gui\obj\` — local .NET build output for the Avalonia GUI

These are not committed-meaningful build artifacts; they exist to make the GUI runnable without an extra build step on the developer machine.
