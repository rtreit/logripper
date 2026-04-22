# CW Decoder Experiment

This experiment is the current end-to-end workflow for tuning CW decode behavior against labeled off-air audio and then trying the tuned baseline on file or live input from the GUI.

## What is here now

- `cw-decoder` Rust binary
  - custom streaming decoder (`stream-file`, `stream-live`)
  - causal ditdah baseline (`stream-file-ditdah`, `stream-live-ditdah`)
  - harvest / preview / signal-profile helpers for labeling
- `eval` Rust binary
  - exact-window scoring against `*.labels.jsonl`
  - full-stream scoring against the same corpus
  - fast in-process parameter sweeps for the causal ditdah baseline
- Avalonia GUI under `experiments\cw-decoder\gui`
  - **Decode** tab for live/file decode
  - **Labeling** tab for harvest, preview, exact-window editing, and saving labels
  - **Tuning** tab for score/sweep runs and applying top sweep results to the baseline decoder mode

## Decoder modes

The GUI now has two decode modes:

| Mode | Backing path | Tuned by |
|---|---|---|
| Custom streaming | `stream-file` / `stream-live` | Decode-tab sensitivity sliders |
| Baseline ditdah | `stream-file-ditdah` / `stream-live-ditdah` | Tuning-tab baseline settings |

The important consequence is:

- **Custom streaming** is still the original experimental streaming decoder.
- **Baseline ditdah** is the current honest label-driven reference path.
- The **Tuning** tab now applies directly to **Decode** when Decoder Mode = **Baseline ditdah**.

## Recommended workflow

1. Open an audio file on the **Labeling** tab.
2. Click **Harvest** to find candidate CW regions.
3. Adjust the exact window with the signal profile editor, play the slowed preview, and save verified copy to `data\cw-samples\*.labels.jsonl`.
4. Switch to **Tuning**.
5. Run **Score Labels** or **Sweep Baseline**.
6. Click **Apply Top Result** after a sweep.
7. Switch to **Decode**, choose **Baseline ditdah**, then:
   - use **Open File...** for replay/A-B checks, or
   - use **Start** for live audio from the selected input device.

## Harvest caching

Harvest results are cached per selected audio file in the running GUI session.

- Re-selecting the same file restores the cached candidate list, signal profiles, and unsaved draft edits.
- Clicking **Harvest** explicitly re-runs the scan and replaces the cached results for that file.

## Current baseline tuning loop

The baseline settings shared by **Tuning** and **Decode / Baseline ditdah** are:

- rolling window seconds
- minimum warmup window seconds
- decode cadence in milliseconds
- required confirmation count

The current labeled corpus is intentionally small and human-curated. It is meant to answer:

- boundary / warmup problems
- commit/finalization problems
- hard-signal target-isolation problems

Use exact-window scoring first. Use full-stream scoring when you want to understand committed-output lag and end-of-region behavior.

## Current corpus shape

The working corpus currently lives under:

- `data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.labels.jsonl`
- `data\cw-samples\k5zd-zs4tx-80m-qso.labels.jsonl`
- `data\cw-samples\K1ZZ_de_DH8BQA_CQWWCW_CW_20151129_174710Z_14MHz.labels.jsonl`

The present behavior split is roughly:

- W1AW is mostly a boundary / commit problem.
- The harder contest-style samples still look like target-isolation / segmentation problems.

## Build and run

```powershell
cargo build --manifest-path experiments\cw-decoder\Cargo.toml
dotnet build experiments\cw-decoder\gui\CwDecoderGui.csproj
```

Run the GUI:

```powershell
dotnet run --project experiments\cw-decoder\gui\CwDecoderGui.csproj
```

Run scorer/sweep from the shell:

```powershell
cargo run --manifest-path experiments\cw-decoder\Cargo.toml --bin eval -- --all-labels --sweep-ditdah
```

Run the baseline decoder directly on a file:

```powershell
cargo run --manifest-path experiments\cw-decoder\Cargo.toml -- stream-file-ditdah --json --window 20 --min-window 0.5 --decode-every-ms 1000 --confirmations 3 data\cw-samples\W1AW_de_W5WZ_DX_CW_20180623_000422Z_14MHz.mp3
```

## What still needs work

1. Better full-stream scoring and commit/final-flush diagnosis.
2. Better target-isolation tooling for multi-signal contest audio.
3. Optional richer label metadata such as target tone, multi-signal flag, and negative/no-copy labels.
4. Bridging learned behavior from the baseline path back into the custom streaming decoder, once the baseline diagnostics are stable enough to trust.

## Practical note for the radio-attached PC

If the goal is the fastest path to real experimentation on live radio audio, start with:

- **Decoder Mode = Baseline ditdah**
- a tuned setting set from **Tuning**
- the same label corpus pulled from this branch

That gives you the cleanest loop from:

labels -> score/sweep -> apply top result -> live/file decode

without mixing the still-evolving custom streaming path into the evaluation loop too early.
