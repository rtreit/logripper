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

All `--json` and `--record` flags are what the Avalonia GUI uses to drive the engine over stdout/stderr NDJSON.

### `eval`

Corpus scorer and sweep harness (`src\bin\eval.rs`).

Current uses:

- exact-window scoring against saved `*.labels.jsonl`
- full-stream scoring by replaying whole recordings causally and intersecting transcript state at label boundaries
- fast parameter sweeps for the causal `ditdah` baseline (`--sweep-ditdah`, optionally `--wide-sweep`)
- a built-in synthetic regression suite (silence, white/bursty/colored noise, clean and noisy synthesized CW at multiple SNRs) when no label flags are supplied

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
- live capture
- optional recording of live audio to `data\cw-recordings\`
- offline replay of the last opened source (live recording or file decode)
- **Replay & Score** live-vs-offline comparison with a visible CER chip
- inline audio playback with a shared transport / progress surface
- a real-time playback signal view driven by the same broad-band profile pipeline used in labeling
- an explicit **CURRENT TONE** readout during live decode and playback
- an experimental **RANGE LOCK** mode for custom streaming, so live/file decode can prefer the strongest tone inside a chosen Hz window
- an optional **SHOW CHAR HZ** overlay so each decoded character can display the tone the streaming decoder had locked when it emitted that symbol

When **RANGE LOCK** is enabled, emitted characters now also pass a short recent-audio tone check near the locked pitch. That keeps broadband transients (for example a finger snap) from being accepted just because they briefly splashed energy into the locked bin.

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

- **6 / 9 exact**
- **avg CER = 0.10**
- **total edit distance = 12**

Interpretation:

- **K1ZZ / DH8BQA** is now exact
- **W1AW** is mostly solved in exact-window mode (5 / 6 exact); the remaining miss is a **leading-edge / warmup** problem (`TUST...` vs `QST...`)
- **80m K5ZD / ZS4TX** labels still both miss, currently classified as one `garbage_decode` and one `near_match`

### Full-stream baseline score

Current scorer result with:

- `mode = full-stream`
- `post-roll = 1500ms`

is:

- **1 / 9 exact**
- **avg CER = 0.85**
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

1. **Add richer label metadata for hard cases**
   - target tone
   - multi-signal flag
   - negative/no-copy labels
2. **Score the custom streaming path against the same corpus**
   The branch now has stronger custom logic, but the corpus README story should eventually include real custom-vs-baseline numbers rather than replay-only intuition.
3. **Improve full-stream commit behavior**
   The baseline full-stream score shows that finalization and region-close behavior are still weak.
4. **Use `probe-fisher` and label metadata together**
   This looks like the right next diagnostic loop for multi-signal contest audio.
5. **Decide whether the next major investment is**
   - target isolation first, or
   - segmentation / commit policy first

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
