use super::*;

/// Mapping normalized bytes back to source graphemes preserves Unicode expansions.
pub(super) struct NormalizedText {
    pub(super) text: String,
    pub(super) ranges: Vec<(usize, usize)>,
}

impl NormalizedText {
    // Time O(n), space O(n), where n is source UTF-8 bytes.
    pub(super) fn new(source: &str) -> Self {
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
    pub(super) automaton: Option<AhoCorasick>,
    pub(super) expansions: Vec<String>,
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
