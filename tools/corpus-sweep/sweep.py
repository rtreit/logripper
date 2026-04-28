"""
Sweep cw-decoder over the labeled training-set-a corpus and report CER.
Slices each WAV to its labeled window before decoding.
"""

from __future__ import annotations
import json
import re
import struct
import subprocess
import sys
import tempfile
import wave
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORPUS = REPO / "data" / "cw-samples" / "training-set-a"
DECODER = REPO / "experiments" / "cw-decoder" / "target" / "release" / "cw-decoder.exe"


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
    """CER ignoring all whitespace — isolates character-level errors from
    word-spacing errors."""
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


def _read_pcm16_mono(path: Path) -> tuple[int, list[int]]:
    with wave.open(str(path), "rb") as r:
        sr = r.getframerate()
        ch = r.getnchannels()
        sw = r.getsampwidth()
        nframes = r.getnframes()
        raw = r.readframes(nframes)
    if sw != 2:
        raise ValueError(f"expected 16-bit PCM, got {sw*8}-bit")
    fmt = f"<{nframes * ch}h"
    samples = list(struct.unpack(fmt, raw))
    if ch == 2:
        samples = [(samples[i] + samples[i + 1]) // 2 for i in range(0, len(samples), 2)]
    return sr, samples


def _write_pcm16_mono(path: Path, sr: int, samples: list[int]) -> None:
    clipped = [max(-32768, min(32767, int(s))) for s in samples]
    raw = struct.pack(f"<{len(clipped)}h", *clipped)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(raw)


def normalize_gain(src: Path, dst: Path, peak_dbfs: float = -3.0) -> None:
    """Peak-normalize to peak_dbfs."""
    sr, s = _read_pcm16_mono(src)
    if not s:
        dst.write_bytes(src.read_bytes())
        return
    peak = max(abs(min(s)), abs(max(s))) or 1
    target = int(32767 * (10 ** (peak_dbfs / 20.0)))
    g = target / peak
    _write_pcm16_mono(dst, sr, [int(x * g) for x in s])


def biquad_bandpass(src: Path, dst: Path, center_hz: float, q: float = 4.0) -> None:
    """RBJ biquad bandpass centered on the dominant CW tone — cleans QRN/wideband
    noise outside the keying band so the Goertzel envelope is less polluted."""
    import math
    sr, s = _read_pcm16_mono(src)
    if not s:
        dst.write_bytes(src.read_bytes())
        return
    w0 = 2.0 * math.pi * center_hz / sr
    alpha = math.sin(w0) / (2.0 * q)
    b0 = alpha
    b1 = 0.0
    b2 = -alpha
    a0 = 1.0 + alpha
    a1 = -2.0 * math.cos(w0)
    a2 = 1.0 - alpha
    b0, b1, b2 = b0 / a0, b1 / a0, b2 / a0
    a1, a2 = a1 / a0, a2 / a0
    x1 = x2 = y1 = y2 = 0.0
    out = []
    for x in s:
        xf = float(x)
        y = b0 * xf + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1 = x1, xf
        y2, y1 = y1, y
        out.append(int(y))
    _write_pcm16_mono(dst, sr, out)


def run_decoder(wav: Path, pin_wpm: float | None = None) -> tuple[str, float | None, float | None]:
    args = [str(DECODER), "file", str(wav)]
    if pin_wpm is not None:
        args.extend(["--pin-wpm", str(pin_wpm)])
    res = subprocess.run(
        args,
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=120,
    )
    out = res.stdout
    text = ""
    wpm = pitch = None
    in_text = False
    for line in out.splitlines():
        s = line.strip()
        if s.startswith("== decoded text"):
            in_text = True
            continue
        if s.startswith("== "):
            in_text = False
            continue
        if in_text and s:
            text = s
            in_text = False
        m = re.match(r"WPM:\s*([\d.]+)", s)
        if m:
            wpm = float(m.group(1))
        m = re.match(r"pitch:\s*([\d.]+)", s)
        if m:
            pitch = float(m.group(1))
    return text, wpm, pitch


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
            src = obj["source"]
            wav = (CORPUS / Path(src).name) if not Path(src).is_absolute() else Path(src)
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
    if not DECODER.exists():
        print(f"Decoder not built: {DECODER}", file=sys.stderr)
        return 2
    samples = load_corpus()
    if not samples:
        print("No samples found.", file=sys.stderr)
        return 2

    # variants: each is (name, pin_wpm-or-None)
    variants = [
        ("auto", None),
        ("w22", 22.0),
        ("w25", 25.0),
        ("w28", 28.0),
    ]

    table: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="cw-sweep-") as td:
        td_path = Path(td)
        for s in samples:
            slice_path = td_path / f"{s.name}.slice.wav"
            slice_wav(s.wav, s.start_s, s.end_s, slice_path)
            row: dict = {"sample": s, "results": {}}
            for vname, pin in variants:
                decoded, wpm, pitch = run_decoder(slice_path, pin_wpm=pin)
                row["results"][vname] = {
                    "decoded": decoded,
                    "wpm": wpm,
                    "pitch": pitch,
                    "cer": cer(s.truth, decoded),
                    "cer_chars": cer_charonly(s.truth, decoded),
                }
            table.append(row)

    print()
    header = f"{'sample':<32} {'tlen':>4}"
    for vname, _ in variants:
        header += f"  {vname+'.cer':>8}  {vname+'.charcer':>10}"
    print(header)
    print("-" * len(header))
    for row in table:
        s = row["sample"]
        line = f"{s.name:<32} {len(normalize(s.truth)):>4}"
        for vname, _ in variants:
            r = row["results"][vname]
            line += f"  {r['cer']:>8.2f}  {r['cer_chars']:>10.2f}"
        print(line)
    print("-" * len(header))
    line_w = f"{'weighted':<32} {'':>4}"
    line_m = f"{'mean':<32} {'':>4}"
    for vname, _ in variants:
        total = sum(len(normalize(row["sample"].truth)) for row in table)
        edits = sum(int(round(row["results"][vname]["cer"] * len(normalize(row["sample"].truth)))) for row in table)
        edits_c = sum(int(round(row["results"][vname]["cer_chars"] * len(normalize(row["sample"].truth)))) for row in table)
        wcer = edits / total if total else 0.0
        wcer_c = edits_c / total if total else 0.0
        mcer = sum(row["results"][vname]["cer"] for row in table) / len(table)
        mcer_c = sum(row["results"][vname]["cer_chars"] for row in table) / len(table)
        line_w += f"  {wcer:>8.2f}  {wcer_c:>10.2f}"
        line_m += f"  {mcer:>8.2f}  {mcer_c:>10.2f}"
    print(line_m)
    print(line_w)
    print()
    print("--- per-sample detail ---")
    for row in table:
        s = row["sample"]
        print(f"\n[{s.name}]  truth ({len(normalize(s.truth))} chars): {s.truth[:140]}{'...' if len(s.truth) > 140 else ''}")
        for vname, _ in variants:
            r = row["results"][vname]
            print(f"  {vname:<5} CER={r['cer']:.2f} chars={r['cer_chars']:.2f}  {r['decoded'][:140]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
