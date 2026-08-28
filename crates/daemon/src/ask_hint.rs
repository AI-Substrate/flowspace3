//! A conservative steer from semantic search to question answering.
//!
//! Search is still the right verb for identifiers, concepts, and keywords;
//! teaching every query about `ask` would turn a useful next action into noise.
//! False positives therefore cost more than misses here. A query without a
//! question mark only qualifies when it starts with a common interrogative and
//! contains more than that one word. This catches ordinary natural-language
//! questions without treating symbols such as `is_ready` or short topic
//! searches such as `retry policy` as questions.

/// The extra next-action guidance for a question sent to `search`.
pub(crate) const HINT: &str = "this looks like a question — `flowspace3 ask \"<question>\"` answers questions by searching the index for you";

/// Whether a search query has enough question shape to warrant [`HINT`].
#[must_use]
pub(crate) fn looks_like_question(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    if query.ends_with('?') {
        return true;
    }

    let mut words = query.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    words.next().is_some()
        && [
            "how", "why", "where", "what", "when", "which", "who", "does", "is", "can", "should",
        ]
        .iter()
        .any(|interrogative| first.eq_ignore_ascii_case(interrogative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_interrogatives_mark_natural_language_questions() {
        for query in [
            "how does retry work",
            "why is the queue stalled",
            "where is debounce owned",
            "what parses Rust",
            "when does indexing finish",
            "which provider is active",
            "who owns the watcher",
            "does scan deduplicate jobs",
            "is the daemon ready",
            "can search filter repositories",
            "should this return a warning",
        ] {
            assert!(looks_like_question(query), "{query:?}");
        }
    }

    #[test]
    fn a_trailing_question_mark_marks_a_question() {
        assert!(looks_like_question("retry policy?"));
        assert!(looks_like_question("WatcherSupervisor?"));
    }

    #[test]
    fn interrogatives_are_case_insensitive() {
        assert!(looks_like_question("HOW does retry work"));
        assert!(looks_like_question("Where is WatcherSupervisor"));
    }

    #[test]
    fn identifiers_keywords_and_bare_interrogatives_stay_searches() {
        for query in [
            "WatcherSupervisor",
            "retry policy",
            "is_ready",
            "how",
            "WHERE",
        ] {
            assert!(!looks_like_question(query), "{query:?}");
        }
    }

    #[test]
    fn empty_and_whitespace_only_queries_are_not_questions() {
        assert!(!looks_like_question(""));
        assert!(!looks_like_question("   \n\t"));
    }
}
