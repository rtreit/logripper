//! Helpers for treating whole-window ditdah decodes as a causal rolling stream.

use std::collections::VecDeque;

pub fn normalize_snapshot_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
        if normalized.is_empty() {
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

pub fn append_snapshot_text(transcript: &mut String, snapshot_text: &str) -> String {
    let snapshot = normalize_snapshot_text(snapshot_text);
    if snapshot.is_empty() {
        return String::new();
    }

    if transcript.is_empty() {
        transcript.push_str(&snapshot);
        return snapshot;
    }

    if transcript.as_str() == snapshot || transcript.ends_with(&snapshot) || transcript.contains(&snapshot)
    {
        return String::new();
    }

    if let Some(rest) = snapshot.strip_prefix(transcript.as_str()) {
        transcript.push_str(rest);
        return rest.to_string();
    }

    let overlap = longest_suffix_prefix_overlap(transcript, &snapshot);
    if overlap > 0 {
        let appended = snapshot[overlap..].to_string();
        transcript.push_str(&appended);
        return appended;
    }

    if let Some(pos) = snapshot.find(transcript.as_str()) {
        let appended = snapshot[pos + transcript.len()..].to_string();
        transcript.push_str(&appended);
        return appended;
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
    use super::{append_snapshot_text, normalize_snapshot_text, PrefixStabilizer};

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
}
