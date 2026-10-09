//! The `fs3 "<switches> <search>" <message>` grammar: recognising an fs3
//! prompt, splitting the search from the message, and reading the switches.
//!
//! A port of `integrations/claude-code/fs3-prompt-search.py`. It is forgiving
//! on purpose: a near-miss switch is read as the closest real one, and an
//! unplaceable one is reported, never fatal.

/// How many hits when `-n` is not given.
pub const DEFAULT_LIMIT: usize = 8;
/// The most hits `-n` may ask for.
pub const MAX_LIMIT: usize = 25;

/// Opening quote to closing quote: straight or smart.
const CLOSERS: [(char, char); 4] = [('"', '"'), ('\'', '\''), ('“', '”'), ('‘', '’')];

/// Long switch name to the option it sets.
const LONG: [(&str, &str); 8] = [
    ("all", "all"),
    ("code", "code"),
    ("convo", "convo"),
    ("convos", "convo"),
    ("this", "this"),
    ("quick", "quick"),
    ("raw", "raw"),
    ("help", "help"),
];
/// Short switch letter to the option it sets; these may be joined (`-ac`).
const SHORT: [(char, &str); 6] = [
    ('a', "all"),
    ('f', "code"),
    ('c', "convo"),
    ('s', "this"),
    ('q', "quick"),
    ('h', "help"),
];
/// Switches that take the next token as their value.
const VALUED: [&str; 3] = ["n", "r", "repo"];

/// Prompts a harness delivers on another session's behalf. A quoted `"fs3 …"`
/// inside one is a mention, not a request: only the user's own typing searches.
const RELAYED: [&str; 3] = [
    "<cross-session-message",
    "<task-notification",
    "<system-reminder",
];

fn long(name: &str) -> Option<&'static str> {
    LONG.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn short(ch: char) -> Option<&'static str> {
    SHORT.iter().find(|(c, _)| *c == ch).map(|(_, v)| *v)
}

fn valued(name: &str) -> bool {
    VALUED.contains(&name)
}

/// Which content the search covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// Code, docs and conversations.
    Both,
    /// Code and docs.
    Code,
    /// Conversations only.
    Convo,
}

/// The switches as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opts {
    pub all: bool,
    pub repo: Option<String>,
    pub what: What,
    pub this: bool,
    pub quick: bool,
    pub raw: bool,
    pub help: bool,
    pub limit: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            all: false,
            repo: None,
            what: What::Both,
            this: false,
            quick: false,
            raw: false,
            help: false,
            limit: DEFAULT_LIMIT,
        }
    }
}

/// Switches the user fumbled, so the agent can tell them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Issues {
    /// `(typed, read as)` for a near miss.
    pub corrected: Vec<(String, String)>,
    /// Switches that could not be placed.
    pub unknown: Vec<String>,
}

/// Return `(search part, message)`, or `None` when the prompt is not an fs3 prompt.
pub fn split_prompt(prompt: &str) -> Option<(String, String)> {
    let text = prompt.trim();
    let lower = text.to_lowercase();
    if lower == "fs3" {
        return Some((String::new(), String::new()));
    }
    if !lower.starts_with("fs3 ") {
        return embedded(text);
    }
    let rest = text[4..].trim_start();
    let mut chars = rest.chars();
    if let Some(open) = chars.next()
        && let Some(close) = closer(open)
    {
        let after = &rest[open.len_utf8()..];
        return Some(match after.find(close) {
            None => (after.to_string(), String::new()),
            Some(end) => (
                after[..end].to_string(),
                after[end + close.len_utf8()..].trim().to_string(),
            ),
        });
    }
    Some((rest.to_string(), String::new())) // unquoted: the whole remainder is the search
}

fn closer(open: char) -> Option<char> {
    CLOSERS.iter().find(|(o, _)| *o == open).map(|(_, c)| *c)
}

/// A quoted `"fs3 <switches> <search>"` anywhere in the prompt; the rest is the message.
fn embedded(text: &str) -> Option<(String, String)> {
    if RELAYED.iter().any(|tag| text.starts_with(tag)) {
        return None;
    }
    for (open, close) in CLOSERS {
        let mut start = 0;
        while let Some(at) = text[start..].find(open).map(|i| i + start) {
            let from = at + open.len_utf8();
            let Some(end) = text[from..].find(close).map(|i| i + from) else {
                break;
            };
            let inner = text[from..end].trim();
            let lower = inner.to_lowercase();
            if lower.starts_with("fs3 ") && has_words(&inner[3..]) {
                let joined = format!("{} {}", &text[..at], &text[end + close.len_utf8()..]);
                let message = joined.split_whitespace().collect::<Vec<_>>().join(" ");
                return Some((inner[3..].trim().to_string(), message));
            }
            start = end + close.len_utf8();
        }
    }
    None
}

/// Map a switch name to its canonical name, forgiving prefixes and typos.
///
/// Returns `(canonical, exact)`, or `None` when nothing is close.
pub fn match_switch(name: &str) -> Option<(String, bool)> {
    if long(name).is_some() || valued(name) {
        return Some((name.to_string(), true));
    }
    let mut names: Vec<&str> = LONG.iter().map(|(n, _)| *n).collect();
    names.push("repo");
    names.sort_unstable();
    let mut prefixed: Vec<&str> = Vec::new();
    if name.chars().count() >= 2 {
        for &n in &names {
            if n.starts_with(name) {
                let canonical = long(n).unwrap_or(n);
                if !prefixed.contains(&canonical) {
                    prefixed.push(canonical);
                }
            }
        }
    }
    if let [only] = prefixed[..] {
        return Some((only.to_string(), false));
    }
    close_match(name, &names, 0.7).map(|n| (n.to_string(), false))
}

/// `difflib.get_close_matches(word, possibilities, n=1, cutoff)`: the best
/// ratio at or over the cutoff, ties going to the larger string.
fn close_match<'a>(word: &str, possibilities: &[&'a str], cutoff: f64) -> Option<&'a str> {
    possibilities
        .iter()
        .map(|p| (ratio(p, word), *p))
        .filter(|(r, _)| *r >= cutoff)
        .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, p)| p)
}

/// `difflib.SequenceMatcher(None, a, b).ratio()` for short strings with no junk.
fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut matched = 0;
    let mut queue = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = longest_match(&a, &b, alo, ahi, blo, bhi);
        if k == 0 {
            continue;
        }
        matched += k;
        if alo < i && blo < j {
            queue.push((alo, i, blo, j));
        }
        if i + k < ahi && j + k < bhi {
            queue.push((i + k, ahi, j + k, bhi));
        }
    }
    2.0 * matched as f64 / total as f64
}

/// `SequenceMatcher.find_longest_match`: earliest in `a`, then in `b`, on ties.
fn longest_match(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
    let mut prev = vec![0usize; b.len() + 1];
    for (i, ai) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut next = vec![0usize; b.len() + 1];
        for (j, bj) in b.iter().enumerate().take(bhi).skip(blo) {
            if ai == bj {
                let k = if j > 0 { prev[j - 1] } else { 0 } + 1;
                next[j] = k;
                if k > bestsize {
                    (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                }
            }
        }
        prev = next;
    }
    (besti, bestj, bestsize)
}

/// POSIX `shlex.split`: quotes group, backslash escapes. `None` on an
/// unclosed quote or a trailing backslash.
pub fn shlex_split(text: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            '\\' => {
                current.push(chars.next()?);
                in_token = true;
            }
            '\'' | '"' => {
                in_token = true;
                loop {
                    let d = chars.next()?;
                    if d == c {
                        break;
                    }
                    if c == '"' && d == '\\' && matches!(chars.peek(), Some('"' | '\\')) {
                        current.push(chars.next()?);
                    } else {
                        current.push(d);
                    }
                }
            }
            _ => {
                current.push(c);
                in_token = true;
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    Some(tokens)
}

/// Return `(opts, query, issues)`.
pub fn parse_switches(search_part: &str) -> (Opts, String, Issues) {
    let mut opts = Opts::default();
    let mut issues = Issues::default();
    let tokens = shlex_split(search_part)
        .unwrap_or_else(|| search_part.split_whitespace().map(str::to_string).collect());
    let mut words: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        i += 1;
        if !tok.starts_with('-') || tok.trim_matches('-').is_empty() {
            words.push(tok.clone());
            continue;
        }
        let name = tok.trim_start_matches('-').to_lowercase();
        let flags: Vec<&str> = if !name.is_empty()
            && long(&name).is_none()
            && !valued(&name)
            && name.chars().all(|ch| short(ch).is_some())
        {
            name.chars().filter_map(short).collect() // joined short switches: -ac
        } else {
            let Some((canonical, exact)) = match_switch(&name) else {
                if words.is_empty() {
                    issues.unknown.push(tok.clone());
                } else {
                    words.push(tok.clone()); // an unplaceable dash-word after the search text is part of it
                }
                continue;
            };
            if !exact {
                issues
                    .corrected
                    .push((tok.clone(), format!("-{canonical}")));
            }
            if valued(&canonical) {
                let value = tokens.get(i).map(String::as_str).unwrap_or("");
                if canonical == "n"
                    && !value.is_empty()
                    && value.chars().all(|c| c.is_ascii_digit())
                {
                    let n = value.parse::<usize>().unwrap_or(MAX_LIMIT);
                    opts.limit = n.clamp(1, MAX_LIMIT);
                    i += 1;
                } else if canonical != "n" && !value.is_empty() && !value.starts_with('-') {
                    opts.repo = Some(value.to_string());
                    i += 1;
                } else {
                    issues
                        .unknown
                        .push(format!("{tok} (it needs a value after it)"));
                }
                continue;
            }
            vec![long(&canonical).unwrap_or("help")]
        };
        for flag in flags {
            match flag {
                "code" => opts.what = What::Code,
                "convo" => opts.what = What::Convo,
                "all" => opts.all = true,
                "this" => opts.this = true,
                "quick" => opts.quick = true,
                "raw" => opts.raw = true,
                _ => opts.help = true,
            }
        }
    }
    (opts, words.join(" ").trim().to_string(), issues)
}

/// Whether a search has anything to search for: `…` or `?` alone does not
/// (Python's `\w`).
pub fn has_words(query: &str) -> bool {
    query.chars().any(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(p: &str) -> Option<(String, String)> {
        split_prompt(p)
    }

    fn s(a: &str, b: &str) -> Option<(String, String)> {
        Some((a.to_string(), b.to_string()))
    }

    #[test]
    fn plain_prompts_pass_through() {
        assert_eq!(split("how does ingest work"), None);
        assert_eq!(split("fs3x nope"), None);
    }

    #[test]
    fn leading_fs3_splits_quoted_search_from_message() {
        assert_eq!(
            split(r#"fs3 "-all retry" what changed?"#),
            s("-all retry", "what changed?")
        );
        assert_eq!(split("FS3 “smart quotes” msg"), s("smart quotes", "msg"));
        assert_eq!(split("fs3 'single' m"), s("single", "m"));
        assert_eq!(split(r#"fs3 "unclosed search"#), s("unclosed search", ""));
        assert_eq!(split("fs3 -this nas backup"), s("-this nas backup", ""));
        assert_eq!(split("  fs3  "), s("", ""));
    }

    #[test]
    fn quoted_fs3_anywhere_is_found_and_the_rest_is_the_message() {
        assert_eq!(
            split(r#"tell me   "fs3 -c weekly backup" where   do dumps go"#),
            s("-c weekly backup", "tell me where do dumps go")
        );
        // A quoted fs3 with no search words in it is a mention, not a search.
        assert_eq!(split(r#"a "not it" b "fs3" c"#), None);
        assert_eq!(split(r#"my PR adds "fs3 …" anywhere"#), None);
    }

    #[test]
    fn relayed_messages_never_search() {
        let peer = "<cross-session-message from=\"x\">\nmy PR adds \"fs3 …\" quoted ANYWHERE\n</cross-session-message>";
        assert_eq!(split(peer), None);
        assert_eq!(
            split("<task-notification>\"fs3 x\"</task-notification>"),
            None
        );
    }

    #[test]
    fn has_words_rejects_punctuation_only_searches() {
        assert!(!has_words("…"));
        assert!(!has_words(" ?! "));
        assert!(has_words("nas"));
    }

    #[test]
    fn shlex_matches_posix_python() {
        assert_eq!(shlex_split(r#"a "b c" d"#).unwrap(), ["a", "b c", "d"]);
        assert_eq!(shlex_split(r#"x"y z"w"#).unwrap(), ["xy zw"]);
        assert_eq!(shlex_split(r#""a\"b" 'c\d'"#).unwrap(), ["a\"b", "c\\d"]);
        assert_eq!(shlex_split(r"a\ b").unwrap(), ["a b"]);
        assert_eq!(shlex_split(r#""""#).unwrap(), [""]);
        assert_eq!(shlex_split("don't"), None);
        assert_eq!(shlex_split("trailing\\"), None);
    }

    #[test]
    fn ratio_matches_difflib() {
        // Values from Python's difflib.SequenceMatcher(None, a, b).ratio().
        assert!((ratio("this", "thsi") - 0.75).abs() < 1e-9);
        assert!((ratio("repo", "repos") - 0.888_888_888_9).abs() < 1e-9);
        assert!((ratio("abcd", "bcda") - 0.75).abs() < 1e-9);
        assert!((ratio("convo", "cnovo") - 0.8).abs() < 1e-9);
        assert_eq!(ratio("", ""), 1.0);
    }

    #[test]
    fn switches_forgive_prefixes_and_typos() {
        assert_eq!(match_switch("thsi"), Some(("this".into(), false)));
        assert_eq!(match_switch("con"), Some(("convo".into(), false)));
        assert_eq!(match_switch("repos"), Some(("repo".into(), false)));
        assert_eq!(match_switch("co"), None); // code or convo
        assert_eq!(match_switch("sdfjkls"), None);
        assert_eq!(match_switch("all"), Some(("all".into(), true)));
        assert_eq!(match_switch("n"), Some(("n".into(), true)));
    }

    #[test]
    fn parse_reads_every_switch_shape() {
        let (o, q, i) = parse_switches("-ac -n 40 copilot ask");
        assert!(o.all && o.what == What::Convo && o.limit == MAX_LIMIT);
        assert_eq!(q, "copilot ask");
        assert_eq!(i, Issues::default());

        let (o, q, _) = parse_switches("--repo pij -q -raw retry -x");
        assert_eq!(o.repo.as_deref(), Some("pij"));
        assert!(o.quick && o.raw);
        assert_eq!(q, "retry -x"); // a dash-word after the text is part of it

        let (o, q, i) = parse_switches("-thsi --sdfjkls nas");
        assert!(o.this);
        assert_eq!(q, "nas");
        assert_eq!(i.corrected, [("-thsi".to_string(), "-this".to_string())]);
        assert_eq!(i.unknown, ["--sdfjkls"]);

        let (o, _, i) = parse_switches("-n x -r -c");
        assert_eq!(o.limit, DEFAULT_LIMIT);
        assert_eq!(o.repo, None);
        assert_eq!(
            i.unknown,
            [
                "-n (it needs a value after it)",
                "-r (it needs a value after it)"
            ]
        );

        let (o, _, i) = parse_switches("-convos -n 0");
        assert_eq!(o.what, What::Convo);
        assert_eq!(o.limit, 1);
        assert!(i.corrected.is_empty());
    }
}
