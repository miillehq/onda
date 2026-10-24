//! Indexed dictionary retrieval and deterministic Unicode-aware phrase correction.
use crate::{
    contracts::{DictionaryEntry, DictionaryKind, Snippet},
    error::{AppError, AppResult},
    MAX_TEXT_BYTES,
};
use aho_corasick::AhoCorasick;
use std::collections::{BTreeMap, HashMap, HashSet};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use unicode_segmentation::UnicodeSegmentation;

const MAX_PATTERN_CHARS: usize = 160;
const MAX_CANDIDATES: usize = 32;
const MAX_RETRIEVAL_POOL: usize = 256;
const MAX_MATCHES: usize = 2_000_000;
const MAX_STARRED_CANDIDATES: usize = 8;
/// Whisper keeps roughly the last 224 prompt tokens; stay well inside that.
const MAX_PROMPT_TERMS: usize = 48;
const MAX_PROMPT_CHARS: usize = 600;
const MAX_POSTING_VISITS: usize = 16_384;
const MAX_QUERY_GRAMS: usize = 1024;
const MAX_FUZZY_QUERY_TOKENS: usize = 64;
const MIN_FUZZY_TOKEN_CHARACTERS: usize = 4;
const MIN_FUZZY_SIMILARITY: f64 = 0.65;

/// Normalization used for duplicates/retrieval, never for emitted spelling.
pub fn lookup_key(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x9fff | 0x20000..=0x3134f | 0xf900..=0xfaff | 0x3040..=0x30ff | 0x1100..=0x11ff | 0xa960..=0xa97f | 0xac00..=0xd7af | 0xd7b0..=0xd7ff)
}

/// Mapping normalized bytes back to source graphemes preserves Unicode expansions.
struct NormalizedText {
    text: String,
    ranges: Vec<(usize, usize)>,
}

impl NormalizedText {
    // Time O(n), space O(n), where n is source UTF-8 bytes.
    fn new(source: &str) -> Self {
        let mut text = String::with_capacity(source.len());
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(source.len());
        for (start, grapheme) in source.grapheme_indices(true) {
            let normalized: String = grapheme.nfkc().flat_map(char::to_lowercase).collect();
            for character in normalized.chars() {
                let value = if character.is_whitespace() && character != '\n' {
                    ' '
                } else {
                    character
                };
                if value == ' ' && text.ends_with(' ') {
                    if let Some(last) = ranges.last_mut() {
                        last.1 = start + grapheme.len();
                    }
                    continue;
                }
                text.push(value);
                ranges.extend(std::iter::repeat_n(
                    (start, start + grapheme.len()),
                    value.len_utf8(),
                ));
            }
        }
        Self { text, ranges }
    }
}

pub(crate) fn bounded(source: &str, start: usize, end: usize) -> bool {
    let first = source[start..end].chars().next();
    let last = source[start..end]
        .chars()
        .rev()
        .find(|character| !is_combining_mark(*character));
    let previous = source[..start].chars().next_back();
    let next = source[end..].chars().next();
    let word = |value: char| value.is_alphanumeric() || value == '_' || is_combining_mark(value);
    let left = first.is_some_and(is_cjk) || !first.is_some_and(word) || !previous.is_some_and(word);
    let right = last.is_some_and(is_cjk) || !last.is_some_and(word) || !next.is_some_and(word);
    left && right
}

/// A compiled immutable index is replaced after each committed dictionary edit.
pub struct PhraseMatcher {
    automaton: Option<AhoCorasick>,
    expansions: Vec<String>,
}

impl PhraseMatcher {
    pub fn new(patterns: Vec<(String, String)>) -> AppResult<Self> {
        let mut seen = HashSet::new();
        let mut keys = Vec::new();
        let mut expansions = Vec::new();
        for (pattern, expansion) in patterns {
            let key = lookup_key(&pattern);
            if key.is_empty()
                || pattern.chars().count() > MAX_PATTERN_CHARS
                || pattern.contains('\0')
                || expansion.is_empty()
                || expansion.len() > MAX_TEXT_BYTES
                || expansion.contains('\0')
            {
                return Err(AppError::invalid(
                    "A phrase or expansion is invalid or too long.",
                ));
            }
            if !seen.insert(key.clone()) {
                return Err(AppError::invalid(
                    "Duplicate phrase triggers are ambiguous.",
                ));
            }
            keys.push(key);
            expansions.push(expansion);
        }
        let automaton = if keys.is_empty() {
            None
        } else {
            Some(
                AhoCorasick::new(keys)
                    .map_err(|_| AppError::invalid("Dictionary index could not be built."))?,
            )
        };
        Ok(Self {
            automaton,
            expansions,
        })
    }

    /// One non-cascading pass; same-position matches prefer the longest phrase.
    // Time O(n + m log(1+p) + k log k), space O(n + p + k);
    // m matches, p protected spans, k distinct match starts.
    pub fn apply(&self, source: &str) -> AppResult<String> {
        if source.len() > MAX_TEXT_BYTES {
            return Err(AppError::invalid("Text exceeds the operation limit."));
        }
        let Some(automaton) = &self.automaton else {
            return Ok(source.to_owned());
        };
        let normalized = NormalizedText::new(source);
        let mut matches: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        let protected = crate::text::protected_ranges(source);
        for (count, found) in automaton
            .find_overlapping_iter(&normalized.text)
            .enumerate()
        {
            if count >= MAX_MATCHES {
                return Err(AppError::invalid("Dictionary match limit exceeded."));
            }
            let start = normalized.ranges[found.start()].0;
            let end = normalized.ranges[found.end() - 1].1;
            // A compatibility character may expand into several normalized
            // characters. Partial matches must never consume its entire grapheme.
            if (found.start() > 0
                && normalized.ranges[found.start() - 1] == normalized.ranges[found.start()])
                || (found.end() < normalized.ranges.len()
                    && normalized.ranges[found.end() - 1] == normalized.ranges[found.end()])
            {
                continue;
            }
            if !bounded(source, start, end) {
                continue;
            }
            let protected_index = protected.partition_point(|span| span.end <= start);
            if protected
                .get(protected_index)
                .is_some_and(|span| span.start < end)
            {
                continue;
            }
            let candidate = matches
                .entry(start)
                .or_insert((end, found.pattern().as_usize()));
            if end > candidate.0 {
                *candidate = (end, found.pattern().as_usize());
            }
        }
        let mut output = String::with_capacity(source.len());
        let mut consumed = 0;
        for (start, (end, index)) in matches {
            if start < consumed {
                continue;
            }
            if output.len() + start - consumed + self.expansions[index].len() > MAX_TEXT_BYTES {
                return Err(AppError::invalid(
                    "Expanded text exceeds the operation limit.",
                ));
            }
            output.push_str(&source[consumed..start]);
            output.push_str(&self.expansions[index]);
            consumed = end;
        }
        output.push_str(&source[consumed..]);
        if output.len() > MAX_TEXT_BYTES {
            return Err(AppError::invalid(
                "Expanded text exceeds the operation limit.",
            ));
        }
        Ok(output)
    }
}

fn trigrams(value: &str) -> Vec<String> {
    let characters: Vec<char> = value.chars().take(MAX_PATTERN_CHARS).collect();
    if characters.len() < 3 {
        return vec![value.to_owned()];
    }
    characters
        .windows(3)
        .map(|window| window.iter().collect())
        .collect()
}

pub struct DictionaryIndex {
    pub replacements: PhraseMatcher,
    pub snippets: PhraseMatcher,
    vocabulary: Vec<String>,
    keys: Vec<String>,
    postings: HashMap<String, Vec<usize>>,
    starred: Vec<usize>,
    lexical_automaton: Option<AhoCorasick>,
    lexical_indices: Vec<usize>,
}

impl DictionaryIndex {
    /// Compile aliases once; retrieval never scans the entire vocabulary.
    // Time O(T + d log d), space O(T + d), T total pattern characters and d entries.
    pub fn build(entries: &[DictionaryEntry], snippets: &[Snippet]) -> AppResult<Self> {
        let mut corrections = Vec::new();
        let mut vocabulary = Vec::new();
        let mut keys = Vec::new();
        let mut output_indices: HashMap<String, usize> = HashMap::new();
        let mut starred_indices = HashSet::new();
        let mut lexical_aliases: BTreeMap<String, usize> = BTreeMap::new();
        for entry in entries.iter().filter(|entry| entry.enabled) {
            let expansion = if entry.kind == DictionaryKind::Replacement {
                entry
                    .replacement
                    .clone()
                    .ok_or_else(|| AppError::invalid("Replacement needs an output phrase."))?
            } else {
                entry.phrase.clone()
            };
            corrections.push((entry.phrase.clone(), expansion.clone()));
            let output_key = lookup_key(&expansion);
            let index = if let Some(index) = output_indices.get(&output_key).copied() {
                if entry.kind == DictionaryKind::Vocabulary {
                    vocabulary[index] = expansion.clone();
                }
                index
            } else {
                let index = vocabulary.len();
                output_indices.insert(output_key.clone(), index);
                vocabulary.push(expansion);
                keys.push(output_key.clone());
                index
            };
            if entry.starred {
                starred_indices.insert(index);
            }
            lexical_aliases
                .entry(lookup_key(&entry.phrase))
                .or_insert(index);
            lexical_aliases.entry(output_key).or_insert(index);
        }
        let starred = (0..vocabulary.len())
            .filter(|index| starred_indices.contains(index))
            .collect();
        let mut postings: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, key) in keys.iter().enumerate() {
            let mut distinct = HashSet::new();
            for trigram in trigrams(key) {
                if distinct.insert(trigram.clone()) {
                    postings.entry(trigram).or_default().push(index);
                }
            }
        }
        let lexical_indices = lexical_aliases.values().copied().collect();
        let lexical_automaton =
            if lexical_aliases.is_empty() {
                None
            } else {
                Some(AhoCorasick::new(lexical_aliases.keys()).map_err(|_| {
                    AppError::invalid("Vocabulary retrieval index could not be built.")
                })?)
            };
        Ok(Self {
            replacements: PhraseMatcher::new(corrections)?,
            snippets: PhraseMatcher::new(
                snippets
                    .iter()
                    .filter(|item| item.enabled)
                    .map(|item| (item.trigger.clone(), item.expansion.clone()))
                    .collect(),
            )?,
            vocabulary,
            keys,
            postings,
            starred,
            lexical_automaton,
            lexical_indices,
        })
    }

    /// Retrieval is lexical guidance only; fuzzy matches never rewrite local text.
    // Time O(n + m + p + c*t*l²), space O(n + d); all query matches, posting visits,
    // candidate pool c, query tokens t and phrase length l have explicit bounds.
    pub fn candidates(&self, transcript: &str, context: &str) -> Vec<String> {
        if transcript.len() > MAX_TEXT_BYTES || context.len() > MAX_TEXT_BYTES {
            return Vec::new();
        }
        let query = lookup_key(&format!("{} {}", transcript, context));
        let mut exact: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut exact_words = HashSet::new();
        if let Some(automaton) = &self.lexical_automaton {
            for found in automaton.find_overlapping_iter(&query).take(MAX_MATCHES) {
                if !bounded(&query, found.start(), found.end()) {
                    continue;
                }
                let index = self.lexical_indices[found.pattern().as_usize()];
                exact
                    .entry(index)
                    .and_modify(|position| {
                        if found.start() < position.0
                            || (found.start() == position.0 && found.len() > position.1)
                        {
                            *position = (found.start(), found.len());
                        }
                    })
                    .or_insert((found.start(), found.len()));
                exact_words.extend(query[found.start()..found.end()].unicode_words());
            }
        }
        let mut exact_ranked: Vec<_> = exact.into_iter().collect();
        exact_ranked.sort_unstable_by(|a, b| {
            a.1 .0
                .cmp(&b.1 .0)
                .then(b.1 .1.cmp(&a.1 .1))
                .then(a.0.cmp(&b.0))
        });
        let mut selected: Vec<usize> = exact_ranked
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|(index, _)| index)
            .collect();
        for index in self.starred.iter().take(MAX_STARRED_CANDIDATES) {
            if selected.len() >= MAX_CANDIDATES {
                break;
            }
            if !selected.contains(index) {
                selected.push(*index);
            }
        }
        if selected.len() >= MAX_CANDIDATES {
            return selected
                .into_iter()
                .map(|index| self.vocabulary[index].clone())
                .collect();
        }
        let tokens: Vec<&str> = query
            .unicode_words()
            .filter(|token| {
                let length = token.chars().count();
                !exact_words.contains(token)
                    && !crate::learning::is_common_term(token)
                    && (MIN_FUZZY_TOKEN_CHARACTERS..=MAX_PATTERN_CHARS).contains(&length)
            })
            .take(MAX_FUZZY_QUERY_TOKENS)
            .collect();
        let mut counts: HashMap<usize, usize> = HashMap::new();
        let mut unique_grams = HashSet::new();
        let mut visited_postings = 0;
        'query: for token in &tokens {
            for gram in trigrams(token) {
                if unique_grams.len() >= MAX_QUERY_GRAMS {
                    break 'query;
                }
                if !unique_grams.insert(gram.clone()) {
                    continue;
                }
                if let Some(postings) = self.postings.get(&gram) {
                    for index in postings {
                        if visited_postings >= MAX_POSTING_VISITS {
                            break 'query;
                        }
                        visited_postings += 1;
                        if selected.contains(index) {
                            continue;
                        }
                        *counts.entry(*index).or_default() += 1;
                    }
                }
            }
        }
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.truncate(MAX_RETRIEVAL_POOL);
        let mut scored: Vec<_> = ranked
            .into_iter()
            .filter_map(|(index, _)| {
                if self.keys[index].chars().count() > MAX_PATTERN_CHARS {
                    return None;
                }
                let score = tokens
                    .iter()
                    .map(|token| strsim::normalized_levenshtein(token, &self.keys[index]))
                    .fold(0.0_f64, f64::max);
                if score >= MIN_FUZZY_SIMILARITY {
                    Some((index, score))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(MAX_CANDIDATES - selected.len());
        selected.extend(scored.into_iter().map(|(index, _)| index));
        selected
            .into_iter()
            .map(|index| self.vocabulary[index].clone())
            .collect()
    }

    /// Spellings that bias local recognition before anything is transcribed: starred terms,
    /// then terms relevant to nearby context, then the rest of the vocabulary, within the
    /// speech model's prompt budget. Without this only starred terms reached the model.
    pub fn prompt_terms(&self, context: &str) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        let mut characters = 0;
        let preferred = self.candidates("", context);
        for term in preferred.iter().chain(self.vocabulary.iter()) {
            if terms.len() >= MAX_PROMPT_TERMS {
                break;
            }
            if terms.iter().any(|chosen| chosen == term) {
                continue;
            }
            let cost = term.chars().count() + 2;
            if characters + cost > MAX_PROMPT_CHARS {
                continue;
            }
            characters += cost;
            terms.push(term.clone());
        }
        terms
    }

    pub fn normalize(&self, source: &str) -> AppResult<String> {
        self.snippets.apply(
            &self
                .replacements
                .apply(&crate::text::normalize_whitespace(source))?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_terms_include_unstarred_vocabulary_with_starred_first() {
        let entries = vec![
            vocabulary_entry(0, "Kubernetes", false),
            vocabulary_entry(1, "miille", true),
            vocabulary_entry(2, "Onda", false),
        ];
        let index = DictionaryIndex::build(&entries, &[]).unwrap();
        let terms = index.prompt_terms("");
        assert_eq!(terms.first().map(String::as_str), Some("miille"));
        assert!(terms.iter().any(|term| term == "Kubernetes"));
        assert!(terms.iter().any(|term| term == "Onda"));
        assert_eq!(terms.len(), 3);
    }

    #[test]
    fn prompt_terms_respect_the_prompt_budget() {
        let entries: Vec<_> = (0..200)
            .map(|index| vocabulary_entry(index, &format!("Term{index:03}"), false))
            .collect();
        let index = DictionaryIndex::build(&entries, &[]).unwrap();
        let terms = index.prompt_terms("");
        assert!(terms.len() <= MAX_PROMPT_TERMS);
        assert!(
            terms
                .iter()
                .map(|term| term.chars().count() + 2)
                .sum::<usize>()
                <= MAX_PROMPT_CHARS
        );
    }

    #[test]
    fn longest_phrase_and_unicode_offsets_and_boundaries() {
        let matcher = PhraseMatcher::new(vec![
            ("post grass".into(), "Postgres".into()),
            ("post".into(), "mail".into()),
            ("café".into(), "Café".into()),
            ("你好".into(), "您好".into()),
        ])
        .unwrap();
        assert_eq!(
            matcher
                .apply("post grass postage cafe\u{301} 你好世界 🦅")
                .unwrap(),
            "Postgres postage Café 您好世界 🦅"
        );
    }
    #[test]
    fn replacements_never_cascade_and_case_expansion_keeps_offsets() {
        let matcher = PhraseMatcher::new(vec![
            ("a".into(), "b".into()),
            ("b".into(), "c".into()),
            ("ffi".into(), "ligature".into()),
        ])
        .unwrap();
        assert_eq!(matcher.apply("a b ﬃ!").unwrap(), "b c ligature!");
    }
    #[test]
    fn snippets_reject_duplicate_normalized_triggers() {
        assert!(PhraseMatcher::new(vec![
            ("HELLO".into(), "one".into()),
            ("hello".into(), "two".into())
        ])
        .is_err());
    }
    #[test]
    fn compatibility_expansions_require_whole_source_graphemes() {
        let partial = PhraseMatcher::new(vec![
            ("f".into(), "changed".into()),
            ("fi".into(), "changed".into()),
            ("i".into(), "changed".into()),
        ])
        .unwrap();
        assert_eq!(partial.apply("ﬃ İ").unwrap(), "ﬃ İ");
        let complete = PhraseMatcher::new(vec![
            ("ffi".into(), "ligature".into()),
            ("i\u{307}".into(), "dotted".into()),
        ])
        .unwrap();
        assert_eq!(complete.apply("ﬃ İ").unwrap(), "ligature dotted");
    }
    #[test]
    fn combining_accents_do_not_turn_word_interiors_into_boundaries() {
        let matcher = PhraseMatcher::new(vec![
            ("café".into(), "Coffee".into()),
            ("가".into(), "나".into()),
        ])
        .unwrap();
        assert_eq!(
            matcher
                .apply("cafe\u{301}teria caféteria cafe\u{301}!")
                .unwrap(),
            "cafe\u{301}teria caféteria Coffee!"
        );
        assert_eq!(matcher.apply("\u{1100}\u{1161}나다").unwrap(), "나나다");
    }
    #[test]
    fn corrections_preserve_urls_emails_numbers_and_code_spans() {
        let matcher = PhraseMatcher::new(vec![
            ("monvi".into(), "Manvi".into()),
            ("42".into(), "forty two".into()),
            ("example".into(), "changed".into()),
        ])
        .unwrap();
        let protected = "https://example.com/monvi! monvi@example.com -42 `monvi` ```\nmonvi\n```";
        assert_eq!(matcher.apply(protected).unwrap(), protected);
        assert_eq!(matcher.apply("Ask monvi today").unwrap(), "Ask Manvi today");
        for code in [
            r"C:\monvi\notes.txt",
            r"\\example\monvi\notes.txt",
            "object.monvi",
            "project_monvi",
            "module::monvi",
        ] {
            assert_eq!(matcher.apply(code).unwrap(), code);
        }
    }
    fn vocabulary_entry(index: usize, word: &str, starred: bool) -> DictionaryEntry {
        DictionaryEntry {
            id: index.to_string(),
            kind: DictionaryKind::Vocabulary,
            phrase: word.into(),
            replacement: None,
            enabled: true,
            starred,
            source: crate::contracts::EntrySource::Manual,
        }
    }
    #[test]
    fn exact_retrieval_survives_common_gram_posting_limits_and_precedes_stars() {
        let mut entries: Vec<_> = (0..2000)
            .map(|index| vocabulary_entry(index, &format!("ProjectTerm{index}"), false))
            .collect();
        entries.push(vocabulary_entry(2000, "StarredName", true));
        let index = DictionaryIndex::build(&entries, &[]).unwrap();
        let candidates = index.candidates("Ask ProjectTerm1999 today.", "");
        assert_eq!(candidates[0], "ProjectTerm1999");
        assert_eq!(candidates[1], "StarredName");
        assert_eq!(candidates.len(), 2);
    }
    #[test]
    fn replacement_aliases_retrieve_outputs_without_fuzzy_local_rewrites() {
        let entries = [
            DictionaryEntry {
                id: "rule".into(),
                kind: DictionaryKind::Replacement,
                phrase: "post grass".into(),
                replacement: Some("Postgres".into()),
                enabled: true,
                starred: false,
                source: crate::contracts::EntrySource::Manual,
            },
            vocabulary_entry(2, "Thea", false),
            vocabulary_entry(3, "Manvi", false),
        ];
        let index = DictionaryIndex::build(&entries, &[]).unwrap();
        assert_eq!(index.candidates("post grass", ""), vec!["Postgres"]);
        assert!(index.candidates("the", "").is_empty());
        assert_eq!(index.candidates("Monvi", ""), vec!["Manvi"]);
        assert_eq!(
            index.normalize("the Monvi um UM very very").unwrap(),
            "the Monvi um very very"
        );
    }
    #[test]
    fn equivalent_outputs_are_deduplicated_and_star_status_is_retained() {
        let entries = [
            DictionaryEntry {
                id: "rule".into(),
                kind: DictionaryKind::Replacement,
                phrase: "post grass".into(),
                replacement: Some("postgres".into()),
                enabled: true,
                starred: true,
                source: crate::contracts::EntrySource::Manual,
            },
            vocabulary_entry(2, "Postgres", false),
        ];
        let index = DictionaryIndex::build(&entries, &[]).unwrap();
        assert_eq!(index.candidates("post grass", ""), vec!["Postgres"]);
        assert_eq!(index.candidates("", ""), vec!["Postgres"]);
    }
}
