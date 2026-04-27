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
}

impl PrefixStabilizer {
    pub fn new(required_confirmations: usize) -> Self {
        Self {
            required_confirmations: required_confirmations.max(1),
            recent_snapshots: VecDeque::new(),
            committed: String::new(),
            latest_snapshot: String::new(),
        }
    }

    pub fn push_snapshot(&mut self, snapshot_text: &str) -> String {
        let normalized = normalize_snapshot_text(snapshot_text);
        if normalized.is_empty()
            || is_noise_dominated_snapshot(&normalized)
            || !has_stream_anchor(&normalized, !self.committed.is_empty())
        {
            return String::new();
        }

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

    if alnum_count > 64 {
        return true;
    }

    let single_fraction = single_element_count as f32 / alnum_count as f32;
    let unknown_fraction = unknown_count as f32 / (alnum_count + unknown_count).max(1) as f32;

    alnum_count > 16 && (single_fraction > 0.45 || unknown_fraction > 0.15)
}

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
            || *token == "Q"
            || *token == "SB"
            || *token == "QSB"
            || *token == "QST"
            || *token == "73"
    }) {
        return true;
    }

    stream_is_active && tokens.iter().any(|token| looks_like_callsign_token(token))
}

fn looks_like_callsign_token(token: &str) -> bool {
    let len = token.len();
    (4..=8).contains(&len)
        && token.chars().any(|ch| ch.is_ascii_digit())
        && token.chars().any(|ch| ch.is_ascii_alphabetic())
        && token.chars().all(|ch| ch.is_ascii_alphanumeric())
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

#[cfg(test)]
mod tests {
    use super::{
        append_snapshot_text, normalize_snapshot_text, run_causal_baseline_trace,
        CausalBaselineConfig, PrefixStabilizer,
    };

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
}
