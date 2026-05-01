//! Helpers for treating whole-window ditdah decodes as a causal rolling stream.

use std::collections::VecDeque;

use crate::decoder;

pub fn normalize_snapshot_text(text: &str) -> String {
    repair_common_split_morse(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[derive(Debug, Clone, Copy)]
pub struct CausalBaselineConfig {
    pub window_seconds: f32,
    pub min_window_seconds: f32,
    pub decode_every_ms: u32,
    pub required_confirmations: usize,
}

impl Default for CausalBaselineConfig {
    fn default() -> Self {
        Self {
            window_seconds: 20.0,
            min_window_seconds: 4.0,
            decode_every_ms: 1000,
            required_confirmations: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CausalBaselineOutcome {
    pub transcript: String,
}

#[derive(Debug, Clone)]
pub struct CausalBaselineSnapshot {
    pub end_sample: usize,
    pub transcript: String,
    pub appended: String,
}

#[derive(Debug, Clone)]
pub struct CausalBaselineTrace {
    pub transcript: String,
    pub snapshots: Vec<CausalBaselineSnapshot>,
}

pub fn run_causal_baseline(
    samples: &[f32],
    sample_rate: u32,
    cfg: CausalBaselineConfig,
) -> CausalBaselineOutcome {
    let trace = run_causal_baseline_trace(samples, sample_rate, cfg);
    CausalBaselineOutcome {
        transcript: trace.transcript,
    }
}

pub fn run_causal_baseline_trace(
    samples: &[f32],
    sample_rate: u32,
    cfg: CausalBaselineConfig,
) -> CausalBaselineTrace {
    if samples.is_empty() || sample_rate == 0 {
        return CausalBaselineTrace {
            transcript: String::new(),
            snapshots: Vec::new(),
        };
    }

    let mut streamer = CausalBaselineStreamer::new(sample_rate, cfg);
    let mut snapshots = streamer.feed(samples);
    snapshots.extend(streamer.flush());
    CausalBaselineTrace {
        transcript: streamer.transcript().to_string(),
        snapshots,
    }
}

pub struct CausalBaselineStreamer {
    sample_rate: u32,
    buffer: VecDeque<f32>,
    /// Maximum buffered audio. Buffer GROWS up to this and then slides off
    /// the oldest sample for each new one (FIFO ring). Larger windows give
    /// the prefix stabilizer more opportunity to commit text before it ages
    /// off the front of the buffer; smaller windows give lower latency.
    window_samples: usize,
    min_window_samples: usize,
    decode_every_samples: usize,
    stabilizer: PrefixStabilizer,
    since_last_decode: usize,
    processed_samples: usize,
}

impl CausalBaselineStreamer {
    pub fn new(sample_rate: u32, cfg: CausalBaselineConfig) -> Self {
        let window_samples =
            ((cfg.window_seconds.max(0.5) * sample_rate as f32).round() as usize).max(1);
        let min_window_samples = ((cfg
            .min_window_seconds
            .clamp(0.1, cfg.window_seconds.max(0.5))
            * sample_rate as f32)
            .round() as usize)
            .min(window_samples)
            .max(1);
        let decode_every_samples =
            ((((sample_rate as u64) * cfg.decode_every_ms as u64) / 1000) as usize).max(1);
        Self {
            sample_rate,
            buffer: VecDeque::new(),
            window_samples,
            min_window_samples,
            decode_every_samples,
            stabilizer: PrefixStabilizer::new(cfg.required_confirmations),
            since_last_decode: 0,
            processed_samples: 0,
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> Vec<CausalBaselineSnapshot> {
        let mut snapshots = Vec::new();
        for &sample in samples {
            if self.buffer.len() == self.window_samples {
                self.buffer.pop_front();
            }
            self.buffer.push_back(sample);
            self.since_last_decode += 1;
            self.processed_samples += 1;

            if self.buffer.len() >= self.min_window_samples
                && self.since_last_decode >= self.decode_every_samples
            {
                self.since_last_decode = 0;
                snapshots.push(self.decode_snapshot());
            }
        }
        snapshots
    }

    pub fn flush(&mut self) -> Vec<CausalBaselineSnapshot> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        let mut snapshots = Vec::new();
        if self.buffer.len() >= self.min_window_samples && self.since_last_decode > 0 {
            self.since_last_decode = 0;
            snapshots.push(self.decode_snapshot());
        } else if self.buffer.len() < self.min_window_samples {
            snapshots.push(self.decode_snapshot());
        }

        let appended = self.stabilizer.finalize_latest();
        let needs_final = !snapshots.last().is_some_and(|snapshot| {
            snapshot.end_sample == self.processed_samples
                && snapshot.transcript == self.stabilizer.transcript()
        });
        if needs_final || !appended.is_empty() {
            snapshots.push(CausalBaselineSnapshot {
                end_sample: self.processed_samples,
                transcript: self.stabilizer.transcript().to_string(),
                appended,
            });
        }
        snapshots
    }

    pub fn transcript(&self) -> &str {
        self.stabilizer.transcript()
    }

    pub fn processed_samples(&self) -> usize {
        self.processed_samples
    }

    pub fn window_snapshot(&self) -> Vec<f32> {
        self.buffer.iter().copied().collect()
    }

    pub fn force_stream_anchor(&mut self) {
        self.stabilizer.force_stream_anchor();
    }

    fn decode_snapshot(&mut self) -> CausalBaselineSnapshot {
        let snapshot: Vec<f32> = self.buffer.iter().copied().collect();
        let text = decoder::decode_text(&snapshot, self.sample_rate);
        let appended = self.stabilizer.push_snapshot(&text);
        CausalBaselineSnapshot {
            end_sample: self.processed_samples,
            transcript: self.stabilizer.transcript().to_string(),
            appended,
        }
    }
}

pub struct PrefixStabilizer {
    required_confirmations: usize,
    recent_snapshots: VecDeque<String>,
    committed: String,
    latest_snapshot: String,
    manual_anchor: bool,
}

impl PrefixStabilizer {
    pub fn new(required_confirmations: usize) -> Self {
        Self {
            required_confirmations: required_confirmations.max(1),
            recent_snapshots: VecDeque::new(),
            committed: String::new(),
            latest_snapshot: String::new(),
            manual_anchor: false,
        }
    }

    pub fn force_stream_anchor(&mut self) {
        self.manual_anchor = true;
        self.recent_snapshots.clear();
    }

    pub fn push_snapshot(&mut self, snapshot_text: &str) -> String {
        let mut normalized = normalize_snapshot_text(snapshot_text);
        if normalized.is_empty() || is_noise_dominated_snapshot(&normalized) {
            return String::new();
        }

        let stream_is_active = !self.committed.is_empty() || self.manual_anchor;
        if !stream_is_active {
            let Some(anchored) = anchored_snapshot_text(&normalized) else {
                return String::new();
            };
            normalized = anchored;
        } else if self.manual_anchor && self.committed.is_empty() {
            let Some(anchored) = manual_anchor_snapshot_text(&normalized) else {
                return String::new();
            };
            normalized = anchored;
        } else if !self.committed.is_empty()
            && !has_stream_anchor(&normalized, true)
            && !snapshot_continues_committed(&self.committed, &normalized)
        {
            // Active stream but the new snapshot has no anchor token AND no
            // recognizable token-overlap with the committed transcript tail.
            // That's the signature of a decoder that has wandered off into
            // garbage during silence/QSB. Reject it; PrefixStabilizer's
            // confirmations gate cannot rescue text that does not connect to
            // what we already know.
            return String::new();
        }
        // Once `self.committed` has content the stream is "active". We no
        // longer require every snapshot to contain CQ/DE/73/etc — many real
        // QSOs go a long time between explicit anchor tokens (e.g. an
        // operator running through abbreviations like "BT OM FB PSE NAME
        // QTH RIG ANT WX HR FER ES BK"). Continuity with the committed
        // transcript suffix is enough to accept the snapshot.

        self.latest_snapshot = normalized.clone();
        self.recent_snapshots.push_back(normalized);
        while self.recent_snapshots.len() > self.required_confirmations {
            self.recent_snapshots.pop_front();
        }

        if self.recent_snapshots.len() < self.required_confirmations {
            return String::new();
        }

        let stable_prefix = common_token_prefix(&self.recent_snapshots);
        append_snapshot_text(&mut self.committed, &stable_prefix)
    }

    pub fn finalize_latest(&mut self) -> String {
        append_snapshot_text(&mut self.committed, &self.latest_snapshot)
    }

    pub fn transcript(&self) -> &str {
        self.committed.trim()
    }
}

fn repair_common_split_morse(text: &str) -> String {
    text.split_whitespace()
        .map(|token| token.replace("GT", "Q"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_noise_dominated_snapshot(text: &str) -> bool {
    let mut alnum_count = 0usize;
    let mut single_element_count = 0usize;
    let mut unknown_count = 0usize;

    for ch in text.chars() {
        if ch == '?' {
            unknown_count += 1;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            alnum_count += 1;
            if morse_element_count(ch) == Some(1) {
                single_element_count += 1;
            }
        }
    }

    if alnum_count == 0 {
        return true;
    }

    // Distribution-based filter: noisy decodes are dominated by single-element
    // characters (E/T) or unknown markers ('?'). A clean 20-second window of
    // 30 WPM CW can legitimately contain 100+ alphanumeric characters, so the
    // filter is purely on character composition, not on absolute count.
    let single_fraction = single_element_count as f32 / alnum_count as f32;
    let unknown_fraction = unknown_count as f32 / (alnum_count + unknown_count).max(1) as f32;

    alnum_count > 16 && (single_fraction > 0.45 || unknown_fraction > 0.15)
}

/// Anchor heuristic used when (a) acquiring the stream from cold or (b)
/// validating an active-stream continuation alongside
/// [`snapshot_continues_committed`].
fn has_stream_anchor(text: &str, stream_is_active: bool) -> bool {
    let normalized = text.to_ascii_uppercase();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }

    if tokens.iter().any(|token| {
        *token == "CQ"
            || token.contains("CQ")
            || *token == "DE"
            || *token == "POTA"
            || *token == "TEST"
            || *token == "QSB"
            || *token == "QST"
            || *token == "73"
    }) {
        return true;
    }

    stream_is_active && tokens.iter().any(|token| looks_like_callsign_token(token))
}

fn anchored_snapshot_text(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|token| is_stream_anchor_token(token))?;
    Some(tokens[idx..].join(" "))
}

fn manual_anchor_snapshot_text(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let idx = tokens.iter().position(|token| {
        is_stream_anchor_token(token) || looks_like_strong_callsign_token(token)
    })?;
    Some(tokens[idx..].join(" "))
}

fn is_stream_anchor_token(token: &str) -> bool {
    let token = token.to_ascii_uppercase();
    token == "CQ"
        || token.contains("CQ")
        || token == "DE"
        || token == "POTA"
        || token == "TEST"
        || token == "QSB"
        || token == "QST"
        || token == "73"
}

/// True iff `snapshot` shares at least two contiguous tokens with the tail
/// of `committed`. This is the "continuity" gate that lets active-stream
/// snapshots through without requiring a fresh anchor token, while still
/// rejecting completely unrelated snapshots produced when the decoder
/// wanders off into noise during silence/QSB.
fn snapshot_continues_committed(committed: &str, snapshot: &str) -> bool {
    let committed_tokens: Vec<&str> = committed.split_whitespace().collect();
    let snap_tokens: Vec<&str> = snapshot.split_whitespace().collect();
    if committed_tokens.is_empty() || snap_tokens.is_empty() {
        return false;
    }
    // Look at the last 16 committed tokens and the first 16 snapshot
    // tokens. If any 2-token contiguous run from the snapshot prefix
    // appears verbatim in the committed tail, treat the snapshot as a
    // continuation.
    let tail_len = committed_tokens.len().min(16);
    let head_len = snap_tokens.len().min(16);
    let tail = &committed_tokens[committed_tokens.len() - tail_len..];
    let head = &snap_tokens[..head_len];
    for window_start in 0..head_len.saturating_sub(1) {
        let pair = (head[window_start], head[window_start + 1]);
        for tail_start in 0..tail_len.saturating_sub(1) {
            if tail[tail_start] == pair.0 && tail[tail_start + 1] == pair.1 {
                return true;
            }
        }
    }
    false
}

#[allow(dead_code)]
fn looks_like_callsign_token(token: &str) -> bool {
    looks_like_strong_callsign_token(token)
}

fn looks_like_strong_callsign_token(token: &str) -> bool {
    let len = token.len();
    if !(4..=6).contains(&len) || !token.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return false;
    }

    let chars: Vec<char> = token.chars().map(|ch| ch.to_ascii_uppercase()).collect();
    let digit_positions: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter_map(|(idx, ch)| ch.is_ascii_digit().then_some(idx))
        .collect();
    if digit_positions.len() != 1 {
        return false;
    }

    let digit_idx = digit_positions[0];
    if !(1..=2).contains(&digit_idx) {
        return false;
    }

    let prefix = &chars[..digit_idx];
    let suffix = &chars[digit_idx + 1..];
    if suffix.is_empty()
        || suffix.len() > 3
        || !prefix.iter().all(|ch| ch.is_ascii_alphabetic())
        || !suffix.iter().all(|ch| ch.is_ascii_alphabetic())
    {
        return false;
    }

    prefix.len() == 2 || matches!(prefix[0], 'A' | 'K' | 'N' | 'W')
}

fn morse_element_count(ch: char) -> Option<usize> {
    let n = match ch.to_ascii_uppercase() {
        'A' => 2,
        'B' => 4,
        'C' => 4,
        'D' => 3,
        'E' => 1,
        'F' => 4,
        'G' => 3,
        'H' => 4,
        'I' => 2,
        'J' => 4,
        'K' => 3,
        'L' => 4,
        'M' => 2,
        'N' => 2,
        'O' => 3,
        'P' => 4,
        'Q' => 4,
        'R' => 3,
        'S' => 3,
        'T' => 1,
        'U' => 3,
        'V' => 4,
        'W' => 3,
        'X' => 4,
        'Y' => 4,
        'Z' => 4,
        '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | '0' => 5,
        _ => return None,
    };
    Some(n)
}

pub fn append_snapshot_text(transcript: &mut String, snapshot_text: &str) -> String {
    let snapshot = normalize_snapshot_text(snapshot_text);
    if snapshot.is_empty() {
        return String::new();
    }

    if transcript.is_empty() {
        transcript.push_str(&snapshot);
        return snapshot;
    }

    if transcript.as_str() == snapshot
        || transcript.ends_with(&snapshot)
        || transcript.contains(&snapshot)
    {
        return String::new();
    }

    if let Some(rest) = snapshot.strip_prefix(transcript.as_str()) {
        transcript.push_str(rest);
        return rest.to_string();
    }

    // Try a TOKEN-level suffix/prefix overlap first. Char-level overlap
    // matches single letters too aggressively (committed=" K", snapshot="K
    // RRR N2QLV" → overlap=1 → re-emits "RRR N2QLV", reintroducing N2QLV
    // that's already in the committed text).
    let token_overlap = longest_token_suffix_prefix_overlap(transcript, &snapshot);
    if token_overlap > 0 {
        let appended = snapshot[token_overlap..].to_string();
        if dedup_blocks(transcript, &appended) {
            return String::new();
        }
        transcript.push_str(&appended);
        return appended;
    }

    let overlap = longest_suffix_prefix_overlap(transcript, &snapshot);
    if overlap > 0 {
        let appended = snapshot[overlap..].to_string();
        if dedup_blocks(transcript, &appended) {
            return String::new();
        }
        transcript.push_str(&appended);
        return appended;
    }

    if let Some(pos) = snapshot.find(transcript.as_str()) {
        let appended = snapshot[pos + transcript.len()..].to_string();
        transcript.push_str(&appended);
        return appended;
    }

    if dedup_blocks(transcript, &snapshot) {
        return String::new();
    }
    let separator = if transcript.ends_with(' ') || snapshot.starts_with(' ') {
        ""
    } else {
        " "
    };
    let appended = format!("{separator}{snapshot}");
    transcript.push_str(&appended);
    appended
}

/// Returns true if `appended` should NOT be added to `transcript` because
/// it would re-introduce a recognizable repeat *at the join point*.
///
/// Heuristic: the LAST few tokens of `transcript` already form a contiguous
/// sequence at the START of `appended`. That's the exact failure mode
/// re-segmentation produces — same audio decoded slightly differently so
/// `longest_token_suffix_prefix_overlap` (which requires byte-identical
/// tokens) misses the overlap.
///
/// We deliberately do NOT scan the entire 32-token tail for any repeat of
/// any 4+ char token: callsigns, the operator's name, "TU", "73", etc.
/// legitimately repeat throughout a CW QSO ("CQ CQ CQ DE W7LXN W7LXN K"
/// is one snapshot's worth of new content) and a global dedup drops it
/// all on the floor.
fn dedup_blocks(transcript: &str, appended: &str) -> bool {
    let cand_tokens: Vec<&str> = appended.split_whitespace().collect();
    if cand_tokens.is_empty() {
        return false;
    }
    let trans_tokens: Vec<&str> = transcript.split_whitespace().collect();
    if trans_tokens.is_empty() {
        return false;
    }
    // Look for a contiguous run of 3+ tokens that appears as both the
    // suffix of `transcript` and the prefix of `appended`. This catches
    // re-segmentation duplicates without flagging legitimate within-QSO
    // repetition that happens further down the appended text.
    let max_run = trans_tokens.len().min(cand_tokens.len()).min(8);
    for n in (3..=max_run).rev() {
        let trans_tail = &trans_tokens[trans_tokens.len() - n..];
        let cand_head = &cand_tokens[..n];
        // Allow fuzzy match: ≥80% of token positions must match by
        // case-insensitive equality. Pure-equality would already have
        // been caught by `longest_token_suffix_prefix_overlap`.
        let matches = trans_tail
            .iter()
            .zip(cand_head.iter())
            .filter(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        if matches as f32 / n as f32 >= 0.8 {
            return true;
        }
    }
    false
}

/// Token-level analogue of [`longest_suffix_prefix_overlap`]: returns the
/// byte length (within `right`) of the largest whitespace-token suffix of
/// `left` that equals a prefix of `right`.
fn longest_token_suffix_prefix_overlap(left: &str, right: &str) -> usize {
    let left_tokens: Vec<&str> = left.split_whitespace().collect();
    let right_tokens: Vec<&str> = right.split_whitespace().collect();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0;
    }
    let max = left_tokens.len().min(right_tokens.len());
    for n in (1..=max).rev() {
        let left_tail = &left_tokens[left_tokens.len() - n..];
        let right_head = &right_tokens[..n];
        if left_tail == right_head {
            // Byte length of the joined head tokens (incl. internal spaces).
            return right_head.iter().map(|t| t.len()).sum::<usize>() + n.saturating_sub(1);
        }
    }
    0
}

fn longest_suffix_prefix_overlap(left: &str, right: &str) -> usize {
    let max = left.len().min(right.len());
    for len in (1..=max).rev() {
        if left[left.len() - len..] == right[..len] {
            return len;
        }
    }
    0
}

fn common_token_prefix(values: &VecDeque<String>) -> String {
    if values.is_empty() {
        return String::new();
    }

    let token_lists: Vec<Vec<&str>> = values
        .iter()
        .map(|value| value.split_whitespace().collect::<Vec<_>>())
        .collect();
    let Some(first) = token_lists.first() else {
        return String::new();
    };

    let mut common_len = first.len();
    for tokens in token_lists.iter().skip(1) {
        common_len = common_len.min(tokens.len());
        for idx in 0..common_len {
            if first[idx] != tokens[idx] {
                common_len = idx;
                break;
            }
        }
    }

    first[..common_len].join(" ")
}

// =====================================================================
// LiveCommitCursor — event-driven, sample-indexed transcript commit.
//
// Background: V3's rolling-window decoder re-decodes the latest ~3 s of
// audio every 250 ms. Each cycle's `LiveEnvelopeSnapshot::transcript`
// is a *full* re-decode of that window, not an incremental delta. Doing
// string-level stitching across cycles is fragile: re-segmentation at
// the window boundaries flips the leading character now and then,
// which defeats overlap detection and re-emits already-committed audio
// as ghost text (e.g. "TSA USA EE   SA USA EE   ...").
//
// This cursor moves deduplication out of text space and into audio-time
// space. Each `VizFrame` carries `window_start_sample`/`window_end_sample`
// from `LiveEnvelopeStreamer`, and each `VizEvent` has buffer-relative
// `start_s`/`end_s`. We convert events to absolute sample indices,
// commit only events that lie safely behind the unstable trailing edge,
// and never re-emit text whose audio sits at-or-before
// `committed_until_sample`. The cursor advances monotonically; lock
// release clears the pending Morse buffer but preserves committed text.
// =====================================================================

/// Output of [`LiveCommitCursor::update_from_viz`].
#[derive(Debug, Clone, Default)]
pub struct CommitUpdate {
    /// Append-only committed transcript (entire history so far).
    pub committed_text: String,
    /// Provisional decode of events that are inside the safe interior
    /// but have not yet been flushed (no character/word gap seen). May
    /// change every cycle; should be displayed in a distinct style.
    pub provisional_tail: String,
    /// True when the cursor advanced over a region without producing
    /// committed text (e.g. SNR suppression, lock loss, long stall).
    /// Useful for diagnostics; the cursor itself does not emit
    /// placeholder text into `committed_text`.
    pub committed_gap: bool,
    /// First sample of the gap region (only meaningful when
    /// `committed_gap` is true).
    pub gap_from_sample: u64,
    /// One-past-end sample of the gap region (only meaningful when
    /// `committed_gap` is true).
    pub gap_to_sample: u64,
}

#[derive(Debug, Clone)]
pub struct LiveCommitCursor {
    committed_until_sample: u64,
    committed_text: String,
    pending_morse: String,
    initialized: bool,
    /// Hard cap on `committed_text` length to mirror the 12 000-char
    /// behavior of the legacy `cap_session_transcript`. Trims to ~80 %
    /// on a whitespace boundary when exceeded.
    max_chars: usize,
}

impl Default for LiveCommitCursor {
    fn default() -> Self {
        Self::new(12_000)
    }
}

/// Trailing safety guard, expressed in seconds. Events whose absolute
/// end sample exceeds `window_end_sample - trailing_guard_samples` are
/// considered too close to the rolling-window edge to be stable yet —
/// next cycle's wider context may flip an OffChar boundary into an
/// OffWord, or merge a dit+intra+dit into a single dah. Scales with
/// `dot_seconds` so slow CW (5 WPM ≈ 240 ms dots, ≈ 1.7 s word gaps)
/// gets a longer guard than fast CW.
fn trailing_guard_seconds(dot_s: f32, decode_every_s: f32) -> f32 {
    let dot = dot_s.max(0.001);
    let by_dot = 8.0 * dot;
    let by_cadence = 2.0 * decode_every_s;
    f32_max3(0.50, by_dot, by_cadence)
}

/// Leading safety guard, expressed in seconds. Events whose absolute
/// start sample lies inside the first `leading_guard_samples` of the
/// rolling window may have their leading edge clipped by the window
/// boundary, shortening an OnDah into an OnDit. Skip them.
fn leading_guard_seconds(dot_s: f32) -> f32 {
    let dot = dot_s.max(0.001);
    f32_max2(0.10, 2.0 * dot)
}

#[inline]
fn f32_max2(a: f32, b: f32) -> f32 {
    if a >= b {
        a
    } else {
        b
    }
}
#[inline]
fn f32_max3(a: f32, b: f32, c: f32) -> f32 {
    f32_max2(a, f32_max2(b, c))
}

/// Tiny tolerance for absolute-sample comparisons. Events derived from
/// floating-point window-relative seconds can land ±1 sample off when
/// the same audio region is re-decoded in a later cycle. One frame at
/// the decoder's native step (5 ms = 240 samples @ 48 kHz) is more
/// than enough slack to avoid spurious "different" events while still
/// preventing the cursor from rewinding by any audible amount.
const EPSILON_SAMPLES: u64 = 32;

impl LiveCommitCursor {
    pub fn new(max_chars: usize) -> Self {
        Self {
            committed_until_sample: 0,
            committed_text: String::new(),
            pending_morse: String::new(),
            initialized: false,
            max_chars: max_chars.max(256),
        }
    }

    pub fn committed_text(&self) -> &str {
        &self.committed_text
    }

    pub fn committed_until_sample(&self) -> u64 {
        self.committed_until_sample
    }

    pub fn pending_morse(&self) -> &str {
        &self.pending_morse
    }

    /// Operator manually requested a fresh start (e.g. PR #366's
    /// reset-lock control message). Drops everything; next cycle
    /// initializes from the current safe interior.
    pub fn reset_all(&mut self) {
        self.committed_until_sample = 0;
        self.committed_text.clear();
        self.pending_morse.clear();
        self.initialized = false;
    }

    /// Streamer-internal lock was released (PR #367). Keep committed
    /// history; drop the in-progress Morse pattern so a stale dit/dah
    /// fragment doesn't merge with the next character once a fresh
    /// lock is acquired. Cursor itself stays at its current position.
    pub fn on_lock_lost(&mut self) {
        self.pending_morse.clear();
    }

    /// Drive the cursor from a single `VizFrame`. Idempotent across
    /// cycles that re-decode the same audio region — re-feeding the
    /// same frame produces no new committed text and no cursor motion.
    pub fn update_from_viz(
        &mut self,
        viz: &crate::envelope_decoder::VizFrame,
        decode_every_s: f32,
    ) -> CommitUpdate {
        let sr = viz.sample_rate.max(1) as f32;
        let window_start = viz.window_start_sample;
        let window_end = viz.window_end_sample;
        if window_end <= window_start {
            return self.snapshot_with_provisional();
        }

        // Gate: only commit when the streamer has locked AND the SNR
        // gate didn't suppress this cycle. Same rule the legacy
        // `should_stitch_to_session` enforced; mirrored here so the
        // cursor is self-contained.
        if viz.snr_suppressed || viz.locked_wpm.is_none() {
            // Don't lose the in-progress character: a single gated
            // cycle may bracket a real word with two clean cycles. We
            // simply don't advance and don't compute provisional.
            return self.snapshot_with_provisional();
        }

        let dot_s = if viz.dot_seconds > 0.0 {
            viz.dot_seconds
        } else if viz.wpm > 0.0 {
            1.2 / viz.wpm
        } else {
            0.06
        };
        let trailing_guard_samples = (sr * trailing_guard_seconds(dot_s, decode_every_s)) as u64;
        let leading_guard_samples = (sr * leading_guard_seconds(dot_s)) as u64;

        let safe_start = window_start.saturating_add(leading_guard_samples);
        let safe_end = window_end.saturating_sub(trailing_guard_samples);

        let mut update = CommitUpdate::default();

        if !self.initialized {
            self.committed_until_sample = safe_start;
            self.initialized = true;
        }

        // If the cursor has fallen behind the current safe interior
        // (e.g., a long suppression/unlock window), the audio between
        // committed_until and safe_start is no longer recoverable from
        // the rolling buffer. Skip it; surface a diagnostic gap.
        if self.committed_until_sample + EPSILON_SAMPLES < safe_start {
            update.committed_gap = true;
            update.gap_from_sample = self.committed_until_sample;
            update.gap_to_sample = safe_start;
            self.pending_morse.clear();
            self.committed_until_sample = safe_start;
        }

        // Walk the events in time order. Events are buffer-relative
        // seconds; convert to absolute samples.
        let cursor_start = self.committed_until_sample;
        for ev in &viz.events {
            let start_s = ev.start_s.max(0.0);
            let end_s = ev.end_s.max(start_s);
            let ev_start = window_start.saturating_add((start_s * sr) as u64);
            let ev_end = window_start.saturating_add((end_s * sr) as u64);

            // Already committed.
            if ev_end <= cursor_start.saturating_add(EPSILON_SAMPLES) {
                continue;
            }
            // Outside the safe interior.
            if ev_start < safe_start || ev_end > safe_end {
                continue;
            }
            // Strictly past the current cursor (allow tiny epsilon).
            if ev_start + EPSILON_SAMPLES < self.committed_until_sample {
                continue;
            }

            use crate::envelope_decoder::VizEventKind::*;
            match ev.kind {
                OnDit => {
                    self.pending_morse.push('.');
                }
                OnDah => {
                    self.pending_morse.push('-');
                }
                OffIntra => {
                    // intra-character gap: keep building the same
                    // Morse symbol. Don't advance cursor across the
                    // gap — the symbol it bridges may still extend
                    // into the unstable region next cycle.
                }
                OffChar => {
                    self.flush_pending_char();
                    self.committed_until_sample = ev_end;
                }
                OffWord => {
                    self.flush_pending_char();
                    self.append_word_space();
                    self.committed_until_sample = ev_end;
                }
            }
        }

        update.committed_text = self.committed_text.clone();
        update.provisional_tail = self.compute_provisional_tail(viz);
        update
    }

    fn snapshot_with_provisional(&self) -> CommitUpdate {
        CommitUpdate {
            committed_text: self.committed_text.clone(),
            provisional_tail: String::new(),
            committed_gap: false,
            gap_from_sample: 0,
            gap_to_sample: 0,
        }
    }

    fn compute_provisional_tail(&self, viz: &crate::envelope_decoder::VizFrame) -> String {
        // Decode the events that lie strictly past committed_until but
        // still inside the analyzed window. We deliberately DO NOT use
        // `snap.transcript` here: it is the rolling re-decode of the
        // entire window and would reintroduce string-alignment risk.
        let sr = viz.sample_rate.max(1) as f32;
        let mut morse = self.pending_morse.clone();
        let mut text = String::new();
        for ev in &viz.events {
            let start_s = ev.start_s.max(0.0);
            let end_s = ev.end_s.max(start_s);
            let ev_start = viz
                .window_start_sample
                .saturating_add((start_s * sr) as u64);
            let ev_end = viz.window_start_sample.saturating_add((end_s * sr) as u64);
            if ev_end <= self.committed_until_sample.saturating_add(EPSILON_SAMPLES) {
                continue;
            }
            use crate::envelope_decoder::VizEventKind::*;
            match ev.kind {
                OnDit => morse.push('.'),
                OnDah => morse.push('-'),
                OffIntra => {}
                OffChar => {
                    if !morse.is_empty() {
                        if let Some(c) = crate::envelope_decoder::morse_to_char(&morse) {
                            text.push(c);
                        }
                        morse.clear();
                    }
                }
                OffWord => {
                    if !morse.is_empty() {
                        if let Some(c) = crate::envelope_decoder::morse_to_char(&morse) {
                            text.push(c);
                        }
                        morse.clear();
                    }
                    if !text.ends_with(' ') {
                        text.push(' ');
                    }
                }
            }
            // Stop once we run past `safe_end` would help, but safe_end
            // is local to update_from_viz; for the tail we want to show
            // everything *not yet committed*, including the slightly
            // unstable trailing region.
            let _ = ev_start;
        }
        text
    }

    fn flush_pending_char(&mut self) {
        if self.pending_morse.is_empty() {
            return;
        }
        if let Some(c) = crate::envelope_decoder::morse_to_char(&self.pending_morse) {
            self.committed_text.push(c);
            self.cap_committed();
        }
        self.pending_morse.clear();
    }

    fn append_word_space(&mut self) {
        if !self.committed_text.ends_with(' ') {
            self.committed_text.push(' ');
            self.cap_committed();
        }
    }

    fn cap_committed(&mut self) {
        if self.committed_text.chars().count() <= self.max_chars {
            return;
        }
        let target = (self.max_chars * 4) / 5;
        let total = self.committed_text.chars().count();
        let drop_chars = total.saturating_sub(target);
        let mut byte_cut = 0usize;
        for (i, (b, _)) in self.committed_text.char_indices().enumerate() {
            if i >= drop_chars {
                byte_cut = b;
                break;
            }
        }
        // Snap to whitespace boundary so we don't shear a token.
        if let Some(rel) = self.committed_text[byte_cut..].find(char::is_whitespace) {
            byte_cut += rel + 1;
        }
        self.committed_text.replace_range(..byte_cut, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_snapshot_text_collapses_whitespace() {
        assert_eq!(normalize_snapshot_text("QST\n  QST\tQST "), "QST QST QST");
    }

    #[test]
    fn append_snapshot_text_extends_prefix_match() {
        let mut transcript = "QST QST".to_string();
        let appended = append_snapshot_text(&mut transcript, "QST QST QST");
        assert_eq!(appended, " QST");
        assert_eq!(transcript, "QST QST QST");
    }

    #[test]
    fn append_snapshot_text_uses_suffix_prefix_overlap() {
        let mut transcript = "QST QST QST".to_string();
        let appended = append_snapshot_text(&mut transcript, "ST QST QST DE");
        assert_eq!(appended, " DE");
        assert_eq!(transcript, "QST QST QST DE");
    }

    #[test]
    fn append_snapshot_text_ignores_contained_snapshot() {
        let mut transcript = "QST QST QST DE".to_string();
        let appended = append_snapshot_text(&mut transcript, "QST QST");
        assert!(appended.is_empty());
        assert_eq!(transcript, "QST QST QST DE");
    }

    #[test]
    fn prefix_stabilizer_commits_only_common_tokens() {
        let mut stabilizer = PrefixStabilizer::new(2);
        assert_eq!(stabilizer.push_snapshot("QST QST"), "");
        assert_eq!(stabilizer.push_snapshot("QST QST M"), "QST QST");
        assert_eq!(stabilizer.transcript(), "QST QST");
        assert_eq!(stabilizer.push_snapshot("QST QST QS"), "");
        assert_eq!(stabilizer.transcript(), "QST QST");
    }

    #[test]
    fn prefix_stabilizer_finalizes_with_latest_snapshot() {
        let mut stabilizer = PrefixStabilizer::new(2);
        stabilizer.push_snapshot("QST QST");
        stabilizer.push_snapshot("QST QST QST");
        stabilizer.push_snapshot("QST QST QST DE");
        assert_eq!(stabilizer.transcript(), "QST QST QST");
        assert_eq!(stabilizer.finalize_latest(), " DE");
        assert_eq!(stabilizer.transcript(), "QST QST QST DE");
    }

    #[test]
    fn prefix_stabilizer_accepts_73_as_stream_anchor() {
        let mut stabilizer = PrefixStabilizer::new(1);
        assert_eq!(stabilizer.push_snapshot("73"), "73");
        assert_eq!(stabilizer.transcript(), "73");
    }

    #[test]
    fn prefix_stabilizer_rejects_weak_q_code_fragments_as_anchors() {
        let mut stabilizer = PrefixStabilizer::new(1);

        assert_eq!(stabilizer.push_snapshot("Q OEIIT OI EEE"), "");
        assert_eq!(stabilizer.push_snapshot("SB IZ"), "");
        assert_eq!(stabilizer.transcript(), "");
    }

    #[test]
    fn prefix_stabilizer_accepts_complete_q_code_anchors() {
        let mut stabilizer = PrefixStabilizer::new(1);

        assert_eq!(stabilizer.push_snapshot("QSB"), "QSB");
        assert_eq!(stabilizer.transcript(), "QSB");
    }

    #[test]
    fn prefix_stabilizer_manual_anchor_accepts_callsign_like_mid_qso_text() {
        let mut stabilizer = PrefixStabilizer::new(1);
        assert_eq!(stabilizer.push_snapshot("KK6QZM"), "");

        stabilizer.force_stream_anchor();

        assert_eq!(stabilizer.push_snapshot("KK6QZM"), "KK6QZM");
        assert_eq!(stabilizer.transcript(), "KK6QZM");
    }

    #[test]
    fn prefix_stabilizer_manual_anchor_rejects_e_t_heavy_gibberish() {
        let mut stabilizer = PrefixStabilizer::new(1);
        stabilizer.force_stream_anchor();

        assert_eq!(
            stabilizer.push_snapshot("T R T S E4CTT TN BTE5A I NT EOTG8U EEDN"),
            ""
        );
        assert_eq!(stabilizer.transcript(), "");
    }

    #[test]
    fn prefix_stabilizer_crops_noise_before_automatic_anchor() {
        let mut stabilizer = PrefixStabilizer::new(1);

        assert_eq!(
            stabilizer.push_snapshot("T E I CQ CQ DE K5KV"),
            "CQ CQ DE K5KV"
        );
        assert_eq!(stabilizer.transcript(), "CQ CQ DE K5KV");
    }

    #[test]
    fn prefix_stabilizer_accepts_dense_clean_snapshot_after_anchor() {
        // Regression: a 20-second window of 30 WPM CW can hold ~100+ valid
        // alphanumeric characters. Earlier versions hard-rejected any
        // snapshot with alnum_count > 64 as "noise dominated" which silently
        // dropped clean middles. The decoder must accept dense clean text
        // once the stream is anchored.
        let mut stabilizer = PrefixStabilizer::new(2);
        let head = "CQ DE K1ABC TNX RST 599";
        let mut dense = head.to_string();
        for token in [
            "BT", "OM", "FB", "PSE", "NAME", "QTH", "RIG", "ANT", "WX", "HR", "FER", "ES", "BK",
            "CUL", "AGN", "GUD", "HPE", "NW", "QSL", "QRM", "QRN", "QRP", "DX", "HW", "SRI", "STN",
            "OP", "WKD", "HI", "GA", "GE", "GM", "DR", "GL", "SN", "TMW", "VY", "WID", "XYL", "YL",
        ] {
            dense.push(' ');
            dense.push_str(token);
        }
        assert!(
            dense.chars().filter(|c| c.is_ascii_alphanumeric()).count() > 80,
            "test setup: dense should have >80 alnum chars; was {}",
            dense.chars().filter(|c| c.is_ascii_alphanumeric()).count()
        );
        // Two confirmations of the same clean snapshot should commit a
        // common prefix that is non-empty and contains text past the anchor.
        let _ = stabilizer.push_snapshot(&dense);
        let appended = stabilizer.push_snapshot(&dense);
        assert!(
            !stabilizer.transcript().is_empty(),
            "dense clean snapshot must commit some text; transcript was empty"
        );
        assert!(
            stabilizer.transcript().contains("BT") || appended.contains("BT"),
            "transcript should reach into mid-stream tokens; got {}",
            stabilizer.transcript(),
        );
    }

    #[test]
    fn prefix_stabilizer_active_stream_continues_without_anchor() {
        // Regression: after the stream is anchored, continuing snapshots
        // must not require a fresh CQ/DE/73 anchor token. A real QSO can
        // spend tens of seconds between explicit anchors as the operator
        // sends abbreviations. Continuity with the committed transcript
        // tail is sufficient.
        let mut stabilizer = PrefixStabilizer::new(2);
        let _ = stabilizer.push_snapshot("CQ DE K1ABC TEST QSO");
        let _ = stabilizer.push_snapshot("CQ DE K1ABC TEST QSO BT OM");
        assert!(!stabilizer.transcript().is_empty(), "anchor should commit");
        // Subsequent snapshots in real audio overlap with what's committed
        // because the rolling window slides slowly; the snapshot starts
        // with the same tail tokens that ended the previous decode.
        let _ = stabilizer.push_snapshot("TEST QSO BT OM FB PSE NAME QTH");
        let _ = stabilizer.push_snapshot("TEST QSO BT OM FB PSE NAME QTH RIG ANT");
        assert!(
            stabilizer.transcript().contains("FB"),
            "active stream should accept anchor-less continuations; got {}",
            stabilizer.transcript(),
        );
    }

    #[test]
    fn prefix_stabilizer_active_stream_rejects_unrelated_garbage() {
        // Continuity gate teeth: an active stream must NOT accept a
        // snapshot that has neither an anchor token nor token-overlap with
        // the committed transcript tail.
        let mut stabilizer = PrefixStabilizer::new(2);
        let _ = stabilizer.push_snapshot("CQ DE K1ABC");
        let _ = stabilizer.push_snapshot("CQ DE K1ABC TEST");
        assert!(
            stabilizer.transcript().contains("K1ABC"),
            "anchor should commit before garbage push"
        );
        let before = stabilizer.transcript().to_string();
        let _ = stabilizer.push_snapshot("EEE TT III SSS NNN MMM");
        let _ = stabilizer.push_snapshot("EEE TT III SSS NNN MMM");
        assert_eq!(
            stabilizer.transcript(),
            before,
            "unrelated garbage with no anchor and no overlap must be rejected"
        );
    }

    #[test]
    fn causal_baseline_trace_records_final_transcript_snapshot() {
        let trace = run_causal_baseline_trace(
            &[0.0; 64],
            8_000,
            CausalBaselineConfig {
                window_seconds: 1.0,
                min_window_seconds: 0.1,
                decode_every_ms: 10,
                required_confirmations: 1,
            },
        );
        assert_eq!(
            trace.snapshots.last().map(|snapshot| snapshot.end_sample),
            Some(64)
        );
        assert_eq!(
            trace
                .snapshots
                .last()
                .map(|snapshot| snapshot.transcript.as_str()),
            Some(trace.transcript.as_str())
        );
    }

    // ----- LiveCommitCursor (Approach A+) tests -----

    use crate::envelope_decoder::{VizEvent, VizEventKind, VizFrame};

    /// Build a minimal viz frame with a list of events. `events_secs` is
    /// `(kind, start_s, end_s)` triples relative to `window_start_sample`.
    fn cursor_test_viz(
        sr: u32,
        window_start: u64,
        window_end: u64,
        dot_seconds: f32,
        locked: bool,
        snr_suppressed: bool,
        events_secs: &[(VizEventKind, f32, f32)],
    ) -> VizFrame {
        let events = events_secs
            .iter()
            .map(|(k, s, e)| VizEvent {
                start_s: *s,
                end_s: *e,
                duration_s: (e - s).max(0.0),
                kind: *k,
            })
            .collect();
        VizFrame {
            sample_rate: sr,
            frame_step_s: 0.005,
            buffer_seconds: (window_end - window_start) as f32 / sr as f32,
            pitch_hz: 600.0,
            envelope: Vec::new(),
            envelope_max: 1.0,
            noise_floor: 0.01,
            signal_floor: 0.5,
            snr_db: 30.0,
            snr_suppressed,
            hyst_high: 0.4,
            hyst_low: 0.2,
            events,
            on_durations: Vec::new(),
            dot_seconds,
            wpm: if dot_seconds > 0.0 {
                1.2 / dot_seconds
            } else {
                20.0
            },
            centroid_dot: dot_seconds,
            centroid_dah: dot_seconds * 3.0,
            locked_wpm: if locked { Some(20.0) } else { None },
            window_start_sample: window_start,
            window_end_sample: window_end,
        }
    }

    /// Helper: pattern for "K" = -.- (dah dit dah).
    /// Returns a sequence of (kind, start_s, end_s) inside `[base, end]`.
    /// Caller picks `base` and timing in dot units.
    fn morse_k_events(base: f32, dot: f32) -> Vec<(VizEventKind, f32, f32)> {
        // dah, intra, dit, intra, dah
        let mut t = base;
        let mut out = Vec::new();
        out.push((VizEventKind::OnDah, t, t + 3.0 * dot));
        t += 3.0 * dot;
        out.push((VizEventKind::OffIntra, t, t + dot));
        t += dot;
        out.push((VizEventKind::OnDit, t, t + dot));
        t += dot;
        out.push((VizEventKind::OffIntra, t, t + dot));
        t += dot;
        out.push((VizEventKind::OnDah, t, t + 3.0 * dot));
        out
    }

    #[test]
    fn cursor_does_not_repeat_same_audio_region_across_overlapping_windows() {
        // Two cycles re-decode the SAME absolute sample range; the second
        // call must NOT re-commit the K.
        let sr = 48_000u32;
        let dot = 0.06; // 20 WPM
        let window_start = 0u64;
        // Window is 10 seconds wide so the K (~12 dots ≈ 0.72 s) sits well
        // inside the safe interior.
        let window_end = window_start + sr as u64 * 10;

        let mut events = morse_k_events(2.0, dot);
        // Terminating OffChar so the K commits.
        events.push((VizEventKind::OffChar, 2.6, 2.6 + 3.0 * dot));

        let viz1 = cursor_test_viz(sr, window_start, window_end, dot, true, false, &events);
        let viz2 = cursor_test_viz(sr, window_start, window_end, dot, true, false, &events);

        let mut cursor = LiveCommitCursor::default();
        let u1 = cursor.update_from_viz(&viz1, 0.5);
        assert_eq!(u1.committed_text, "K");

        let u2 = cursor.update_from_viz(&viz2, 0.5);
        assert_eq!(
            u2.committed_text, "K",
            "re-decoding identical events must not duplicate the character"
        );
    }

    #[test]
    fn cursor_handles_legitimate_repetition_in_distinct_audio_regions() {
        // Three Ks at different absolute times must commit as "KKK".
        let sr = 48_000u32;
        let dot = 0.06;
        let window_start = 0u64;
        let window_end = sr as u64 * 10;

        let mut events = Vec::new();
        for base in [2.0_f32, 4.0, 6.0] {
            events.extend(morse_k_events(base, dot));
            events.push((VizEventKind::OffChar, base + 1.0, base + 1.0 + 3.0 * dot));
        }
        let viz = cursor_test_viz(sr, window_start, window_end, dot, true, false, &events);

        let mut cursor = LiveCommitCursor::default();
        let u = cursor.update_from_viz(&viz, 0.5);
        assert_eq!(u.committed_text, "KKK");
    }

    #[test]
    fn cursor_skips_unlocked_cycles_without_advancing() {
        let sr = 48_000u32;
        let dot = 0.06;
        let mut events = morse_k_events(2.0, dot);
        events.push((VizEventKind::OffChar, 2.6, 2.6 + 3.0 * dot));

        // Unlocked: should produce no committed text and not advance cursor.
        let viz_unlocked = cursor_test_viz(sr, 0, sr as u64 * 10, dot, false, false, &events);
        let mut cursor = LiveCommitCursor::default();
        let u = cursor.update_from_viz(&viz_unlocked, 0.5);
        assert_eq!(u.committed_text, "");
        assert_eq!(u.provisional_tail, "");

        // Now a locked cycle covering the same audio commits the K.
        let viz_locked = cursor_test_viz(sr, 0, sr as u64 * 10, dot, true, false, &events);
        let u2 = cursor.update_from_viz(&viz_locked, 0.5);
        assert_eq!(u2.committed_text, "K");
    }

    #[test]
    fn cursor_skips_snr_suppressed_cycles() {
        let sr = 48_000u32;
        let dot = 0.06;
        let mut events = morse_k_events(2.0, dot);
        events.push((VizEventKind::OffChar, 2.6, 2.6 + 3.0 * dot));

        let viz = cursor_test_viz(sr, 0, sr as u64 * 10, dot, true, true, &events);
        let mut cursor = LiveCommitCursor::default();
        let u = cursor.update_from_viz(&viz, 0.5);
        assert_eq!(u.committed_text, "");
    }

    #[test]
    fn cursor_does_not_commit_events_in_trailing_guard() {
        // Place the K so its OffChar lands inside the trailing guard
        // (~8*dot = 0.48s). It should be deferred to provisional, not
        // committed.
        let sr = 48_000u32;
        let dot = 0.06;
        let window_end_s: f32 = 5.0;
        let base = window_end_s - 0.6; // K + OffChar will straddle the guard
        let mut events = morse_k_events(base, dot);
        events.push((VizEventKind::OffChar, base + 1.0, base + 1.0 + 3.0 * dot));

        let viz = cursor_test_viz(
            sr,
            0,
            (sr as f32 * window_end_s) as u64,
            dot,
            true,
            false,
            &events,
        );
        let mut cursor = LiveCommitCursor::default();
        let u = cursor.update_from_viz(&viz, 0.5);
        // Either provisional shows it, or nothing is committed yet.
        assert_eq!(
            u.committed_text, "",
            "events too close to window end must not commit yet"
        );
    }

    #[test]
    fn cursor_advances_past_word_boundary_with_space() {
        let sr = 48_000u32;
        let dot = 0.06;
        let mut events = morse_k_events(2.0, dot);
        // OffWord between K and next K
        events.push((VizEventKind::OffWord, 2.6, 2.6 + 7.0 * dot));
        events.extend(morse_k_events(3.5, dot));
        events.push((VizEventKind::OffChar, 4.1, 4.1 + 3.0 * dot));

        let viz = cursor_test_viz(sr, 0, sr as u64 * 10, dot, true, false, &events);
        let mut cursor = LiveCommitCursor::default();
        let u = cursor.update_from_viz(&viz, 0.5);
        assert_eq!(u.committed_text, "K K");
    }

    #[test]
    fn cursor_reset_all_clears_state() {
        let sr = 48_000u32;
        let dot = 0.06;
        let mut events = morse_k_events(2.0, dot);
        events.push((VizEventKind::OffChar, 2.6, 2.6 + 3.0 * dot));
        let viz = cursor_test_viz(sr, 0, sr as u64 * 10, dot, true, false, &events);

        let mut cursor = LiveCommitCursor::default();
        cursor.update_from_viz(&viz, 0.5);
        assert_eq!(cursor.committed_text(), "K");
        cursor.reset_all();
        assert_eq!(cursor.committed_text(), "");
    }

    #[test]
    fn cursor_reports_committed_gap_after_long_suppression() {
        let sr = 48_000u32;
        let dot = 0.06;
        // First cycle locked: commit K starting at t=1.
        let mut ev1 = morse_k_events(1.0, dot);
        ev1.push((VizEventKind::OffChar, 1.6, 1.6 + 3.0 * dot));
        let viz1 = cursor_test_viz(sr, 0, sr as u64 * 5, dot, true, false, &ev1);

        let mut cursor = LiveCommitCursor::default();
        let u1 = cursor.update_from_viz(&viz1, 0.5);
        assert_eq!(u1.committed_text, "K");

        // Second cycle: window has slid 100 s forward, so committed_until
        // is far behind safe_start. Cursor should report a gap and jump.
        let window_start = sr as u64 * 100;
        let mut ev2 = morse_k_events(2.0, dot);
        ev2.push((VizEventKind::OffChar, 2.6, 2.6 + 3.0 * dot));
        let viz2 = cursor_test_viz(
            sr,
            window_start,
            window_start + sr as u64 * 10,
            dot,
            true,
            false,
            &ev2,
        );
        let u2 = cursor.update_from_viz(&viz2, 0.5);
        assert!(u2.committed_gap, "expected committed_gap after long jump");
        assert!(u2.gap_to_sample > u2.gap_from_sample);
        // K should still commit after the gap.
        assert_eq!(u2.committed_text, "KK");
    }
}
