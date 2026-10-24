use crate::contracts::LearningProposal;
use unicode_segmentation::UnicodeSegmentation;

const MAX_OBSERVED_BYTES: usize = 16 * 1024;
const MIN_TERM_CHARACTERS: usize = 3;
const MAX_TERM_CHARACTERS: usize = 60;
const MAX_NORMALIZED_DISTANCE: f64 = 0.34;
const COMMON_TERMS: &[&str] = &[
    "a", "an", "about", "above", "after", "again", "all", "also", "am", "and", "any", "are", "as",
    "ask", "at", "away", "back", "bad", "be", "because", "been", "before", "being", "below",
    "between", "both", "but", "by", "call", "came", "can", "come", "could", "day", "did", "do",
    "does", "done", "down", "each", "even", "ever", "every", "few", "first", "for", "from", "gave",
    "get", "give", "go", "going", "good", "got", "great", "had", "has", "have", "he", "hello",
    "her", "here", "hers", "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just",
    "keep", "know", "last", "later", "left", "less", "let", "like", "little", "long", "look",
    "made", "make", "many", "may", "me", "might", "more", "most", "much", "must", "my", "name",
    "need", "new", "next", "no", "nor", "not", "now", "of", "off", "often", "on", "once", "one",
    "only", "or", "other", "our", "out", "over", "own", "please", "put", "rather", "really",
    "right", "said", "same", "say", "see", "send", "she", "should", "since", "small", "so", "some",
    "still", "such", "take", "tell", "than", "thanks", "that", "the", "their", "them", "then",
    "there", "these", "they", "thing", "think", "this", "those", "through", "time", "to", "today",
    "together", "tomorrow", "too", "under", "until", "up", "us", "use", "used", "using", "very",
    "want", "was", "way", "we", "well", "went", "were", "what", "when", "where", "which", "while",
    "who", "why", "will", "with", "without", "work", "would", "year", "yes", "yet", "you", "your",
];

/// Common English prose is excluded from fuzzy name guidance and proposals.
pub(crate) fn is_common_term(term: &str) -> bool {
    COMMON_TERMS
        .iter()
        .any(|common| term.eq_ignore_ascii_case(common))
}

/// Propose, never silently add. Exactly one token may change in otherwise identical scope.
// Time O(n + l²), space O(n), l<=60 edited term characters.
pub fn propose_correction(inserted: &str, observed: &str) -> Option<LearningProposal> {
    if inserted.len() > MAX_OBSERVED_BYTES
        || observed.len() > MAX_OBSERVED_BYTES
        || inserted.contains('\0')
        || observed.contains('\0')
    {
        return None;
    }
    if crate::text::protected_tokens(inserted) != crate::text::protected_tokens(observed) {
        return None;
    }
    let before: Vec<&str> = inserted.split_word_bounds().collect();
    let after: Vec<&str> = observed.split_word_bounds().collect();
    if before.len() != after.len() {
        return None;
    }
    let changes: Vec<_> = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| a != b)
        .take(2)
        .collect();
    if changes.len() != 1 {
        return None;
    }
    let (old, new) = changes[0];
    let count = new.chars().count();
    let previous_count = old.chars().count();
    if !(MIN_TERM_CHARACTERS..=MAX_TERM_CHARACTERS).contains(&count)
        || !(MIN_TERM_CHARACTERS..=MAX_TERM_CHARACTERS).contains(&previous_count)
        || !new.chars().all(char::is_alphabetic)
        || !old.chars().all(char::is_alphabetic)
    {
        return None;
    }
    if is_common_term(new) || is_common_term(old) {
        return None;
    }
    let old_key = old.to_lowercase();
    let new_key = new.to_lowercase();
    if !new.chars().any(char::is_uppercase) || old_key == new_key {
        return None;
    }
    let distance = 1.0 - strsim::normalized_levenshtein(&old_key, &new_key);
    if distance > MAX_NORMALIZED_DISTANCE {
        return None;
    }
    Some(LearningProposal {
        id: uuid::Uuid::new_v4().to_string(),
        original: (*old).into(),
        corrected: (*new).into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_name_correction_is_proposed_but_rewrites_are_ignored() {
        assert!(propose_correction("Ask Monvi today.", "Ask Manvi today.").is_some());
        assert!(propose_correction("Ask Monvi today.", "Please ask Manvi now.").is_none());
        assert!(propose_correction("their name", "there name").is_none());
        assert!(propose_correction("Call Alice", "Call Bob").is_none());
    }
    #[test]
    fn changed_urls_emails_code_and_common_prose_do_not_become_name_proposals() {
        for (before, after) in [
            ("https://example.com/Monvi", "https://example.com/Manvi"),
            ("Monvi@example.com", "Manvi@example.com"),
            ("`Monvi`", "`Manvi`"),
            ("```\nMonvi\n```", "```\nManvi\n```"),
            ("He Wept away", "He Went away"),
        ] {
            assert!(propose_correction(before, after).is_none(), "{before}");
        }
        assert!(propose_correction(&"M".repeat(MAX_TERM_CHARACTERS + 1), "Manvi").is_none());
        assert!(propose_correction("Ask Monvi", "Ask Manvi").is_some());
    }
}
