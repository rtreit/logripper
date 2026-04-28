"""
Sweep cw-decoder corpus at multiple pinned WPMs via the ditdah pin-wpm-test
binary (which calls the patched ditdah library directly).
"""

from __future__ import annotations
import json
import re
import subprocess
import sys
import tempfile
import wave
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORPUS = REPO / "data" / "cw-samples" / "training-set-a"
PIN_BIN = REPO / "tools" / "ditdah-direct" / "target" / "release" / "pin-wpm-test.exe"

WPM_GRID = [None, 15, 18, 20, 22, 25, 28, 30, 35]


def levenshtein(a: str, b: str) -> int:
    if len(a) < len(b):
        a, b = b, a
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(cur[j - 1] + 1, prev[j] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def normalize(s: str) -> str:
    return re.sub(r"\s+", " ", s.strip().upper())


def cer(truth: str, hyp: str) -> float:
    t = normalize(truth)
    h = normalize(hyp)
    if not t:
        return 0.0 if not h else 1.0
    return levenshtein(t, h) / len(t)


def cer_charonly(truth: str, hyp: str) -> float:
    t = normalize(truth).replace(" ", "")
    h = normalize(hyp).replace(" ", "")
    if not t:
        return 0.0 if not h else 1.0
    return levenshtein(t, h) / len(t)


def slice_wav(src: Path, start_s: float, end_s: float, dst: Path) -> None:
    with wave.open(str(src), "rb") as r:
        sr = r.getframerate()
        nframes = r.getnframes()
        ch = r.getnchannels()
        sw = r.getsampwidth()
        start_f = max(0, int(round(start_s * sr)))
        end_f = min(nframes, int(round(end_s * sr)))
        r.setpos(start_f)
        data = r.readframes(end_f - start_f)
    with wave.open(str(dst), "wb") as w:
        w.setnchannels(ch)
        w.setsampwidth(sw)
        w.setframerate(sr)
        w.writeframes(data)


def run_pin_grid(wav: Path) -> dict[int | None, str]:
    """Returns {wpm_or_None: decoded_text}.
    pin-wpm-test prints `=== auto WPM ===` then text, then for each pin_wpm in
    [15,18,20,22,25,28,arg] prints `=== pin_wpm=N (...) ===` then text."""
    arg_wpm = "35"  # forces final extra slot we ignore
    res = subprocess.run(
        [str(PIN_BIN), str(wav), arg_wpm],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=120,
    )
    out = res.stdout.splitlines()
    results: dict[int | None, str] = {}
    cur_key: int | None | object = None  # None => auto, int => pinned
    SENTINEL = object()
    cur_key = SENTINEL
    text_lines: list[str] = []

    def flush():
        nonlocal text_lines
        if cur_key is not SENTINEL:
            text = " ".join(t for t in text_lines if t.strip()).strip()
            results[cur_key] = text  # type: ignore
        text_lines = []

    for line in out:
        s = line.strip()
        if s.startswith("=== auto WPM"):
            flush()
            cur_key = None
            continue
        m = re.match(r"=== pin_wpm=(\d+)", s)
        if m:
            flush()
            cur_key = int(m.group(1))
            continue
        if s.startswith("Finished") or s.startswith("Running"):
            continue
        text_lines.append(s)
    flush()
    return results


@dataclass
class Sample:
    name: str
    wav: Path
    start_s: float
    end_s: float
    truth: str


def load_corpus() -> list[Sample]:
    samples: list[Sample] = []
    for jl in sorted(CORPUS.glob("*.labels.jsonl")):
        for line in jl.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            obj = json.loads(line)
            wav = (CORPUS / Path(obj["source"]).name)
            if not wav.exists():
                continue
            samples.append(Sample(
                name=wav.stem,
                wav=wav,
                start_s=float(obj["start_s"]),
                end_s=float(obj["end_s"]),
                truth=obj["correct_copy"],
            ))
    return samples


def main() -> int:
    if not PIN_BIN.exists():
        print(f"pin-wpm-test not built: {PIN_BIN}", file=sys.stderr)
        return 2
    samples = load_corpus()
    if not samples:
        print("No samples found.", file=sys.stderr)
        return 2

    table: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="cw-pin-") as td:
        td_path = Path(td)
        for s in samples:
            slice_path = td_path / f"{s.name}.slice.wav"
            slice_wav(s.wav, s.start_s, s.end_s, slice_path)
            decoded_by_wpm = run_pin_grid(slice_path)
            row = {"sample": s, "decoded": decoded_by_wpm,
                   "cer": {k: cer(s.truth, v) for k, v in decoded_by_wpm.items()},
                   "cer_chars": {k: cer_charonly(s.truth, v) for k, v in decoded_by_wpm.items()}}
            table.append(row)

    # Header: WPM grid
    grid = [None, 15, 18, 20, 22, 25, 28, 35]
    label = lambda k: "auto" if k is None else f"w{k}"
    header = f"{'sample':<32} " + " ".join(f"{label(k):>5}" for k in grid)
    print()
    print(header)
    print("-" * len(header))
    for row in table:
        s = row["sample"]
        line = f"{s.name:<32} " + " ".join(
            f"{row['cer'].get(k, float('nan')):>5.2f}" for k in grid
        )
        print(line)
    print("-" * len(header))

    # Best pinned WPM per sample (by CER)
    print()
    print("--- best WPM per sample ---")
    print(f"{'sample':<32} {'auto.cer':>9} {'best.wpm':>9} {'best.cer':>9} {'gain':>7}")
    for row in table:
        s = row["sample"]
        auto = row["cer"].get(None, float("inf"))
        pinned = {k: v for k, v in row["cer"].items() if k is not None}
        if not pinned:
            continue
        best_w = min(pinned, key=lambda k: pinned[k])
        best = pinned[best_w]
        gain = auto - best
        print(f"{s.name:<32} {auto:>9.2f} {best_w:>9} {best:>9.2f} {gain:>+7.2f}")

    # Aggregate weighted CER
    print()
    print("--- aggregate weighted CER (by truth-len) per WPM ---")
    print(f"{'wpm':<8} {'wcer':>6} {'wcer_chars':>11}")
    total = sum(len(normalize(r["sample"].truth)) for r in table)
    for k in grid:
        edits = sum(int(round(r["cer"].get(k, 1.0) * len(normalize(r["sample"].truth)))) for r in table)
        edits_c = sum(int(round(r["cer_chars"].get(k, 1.0) * len(normalize(r["sample"].truth)))) for r in table)
        wcer = edits / total if total else 0.0
        wcer_c = edits_c / total if total else 0.0
        print(f"{label(k):<8} {wcer:>6.2f} {wcer_c:>11.2f}")

    print()
    print("--- per-sample detail ---")
    for row in table:
        s = row["sample"]
        print(f"\n[{s.name}]  truth: {s.truth[:140]}{'...' if len(s.truth) > 140 else ''}")
        for k in grid:
            t = row["decoded"].get(k, "")
            print(f"  {label(k):<5} CER={row['cer'].get(k, float('nan')):.2f} chars={row['cer_chars'].get(k, float('nan')):.2f}  {t[:140]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
