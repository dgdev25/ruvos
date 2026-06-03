//! Lightweight entity and relation extraction from plain text.
//!
//! No ML model required.  We use two simple but effective heuristics:
//!
//! 1. **Entity extraction** — capitalised noun tokens (Title Case words) that
//!    are not sentence-leading stop words are treated as named entities.
//!    This catches proper nouns like "Alice", "Acme Corp", "PostgreSQL".
//!
//! 2. **Relation extraction** — any two distinct entities that appear in the
//!    same *sentence* (period-split) are linked with a generic "co-occurs"
//!    edge.  This is the same co-occurrence-in-window approach used in early
//!    graphiti versions before LLM extraction was added.
//!
//! Both are intentionally conservative: better to extract fewer confident
//! facts than to pollute the graph with noise.

/// Stop words that are capitalised for grammatical reasons (sentence start)
/// but are not named entities.
const STOPS: &[&str] = &[
    "The", "A", "An", "This", "That", "These", "Those", "It", "He", "She", "They", "We", "I", "My",
    "Our", "His", "Her", "Their", "Its", "In", "On", "At", "By", "To", "Of", "For", "And", "Or",
    "But", "With", "From", "As", "Is", "Are", "Was", "Were", "Be", "Been", "Being", "Has", "Have",
    "Had", "Do", "Does", "Did", "Not", "So", "If", "When", "Where", "Who", "What", "How", "Which",
    "While", "Then", "Each", "Some", "Any", "All", "No", "Also",
];

/// Extract candidate entity names from a block of text.
///
/// Returns deduplicated names in the order first seen.
pub fn extract_entities(text: &str) -> Vec<String> {
    let mut seen: std::collections::LinkedList<String> = std::collections::LinkedList::new();
    let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for word in text.split_whitespace() {
        // Strip trailing punctuation.
        let clean: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '\'')
            .collect();
        if clean.len() < 2 {
            continue;
        }
        // Must start with an uppercase letter.
        let first = clean.chars().next().unwrap();
        if !first.is_uppercase() {
            continue;
        }
        // Must NOT be an all-caps acronym shorter than 3 chars that looks like
        // sentence structure (heuristic: skip single-letter caps like "I").
        if clean.len() == 1 {
            continue;
        }
        // Skip grammatical stop words.
        if STOPS.contains(&clean.as_str()) {
            continue;
        }

        if set.insert(clean.clone()) {
            seen.push_back(clean);
        }
    }

    seen.into_iter().collect()
}

/// Split text into rough sentences on `.`, `!`, `?`.
fn sentences(text: &str) -> Vec<&str> {
    text.split(['.', '!', '?'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// For each sentence, yield all unordered pairs of distinct entities that
/// appear within it.  Returns `(entity_a, entity_b)` pairs.
pub fn extract_co_occurrences(text: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    for sentence in sentences(text) {
        let ents = extract_entities(sentence);
        for i in 0..ents.len() {
            for j in (i + 1)..ents.len() {
                pairs.push((ents[i].clone(), ents[j].clone()));
            }
        }
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_entities_finds_proper_nouns() {
        let ents = extract_entities("Alice works at Acme Corp in London");
        assert!(
            ents.contains(&"Alice".to_string()),
            "expected Alice: {:?}",
            ents
        );
        assert!(
            ents.contains(&"Acme".to_string()),
            "expected Acme: {:?}",
            ents
        );
        assert!(
            ents.contains(&"London".to_string()),
            "expected London: {:?}",
            ents
        );
    }

    #[test]
    fn extract_entities_skips_stop_words() {
        let ents = extract_entities("The quick brown Fox jumped over the lazy Dog");
        assert!(!ents.contains(&"The".to_string()));
    }

    #[test]
    fn co_occurrence_produces_pairs() {
        let pairs = extract_co_occurrences("Alice met Bob. Carol visited Dave and Eve.");
        // Alice-Bob from sentence 1
        assert!(
            pairs.iter().any(|(a, b)| a == "Alice" && b == "Bob"),
            "expected Alice-Bob pair: {:?}",
            pairs
        );
    }

    #[test]
    fn co_occurrence_empty_text_gives_no_pairs() {
        let pairs = extract_co_occurrences("no proper nouns here at all");
        assert!(pairs.is_empty());
    }
}
