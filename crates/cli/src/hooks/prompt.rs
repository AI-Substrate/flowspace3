//! `flowspace3 hooks prompt`: run the fs3 search a prompt asked for and build
//! the context the agent reads before its turn.
//!
//! A port of `integrations/claude-code/fs3-prompt-search.py`. The daemon is
//! reached through [`Backend`], so tests drive every path with canned
//! envelopes and no daemon.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::grammar::{self, Issues, Opts, What};

/// What the user sees in the agent's answer when they ask for help.
pub const USAGE_DOC: &str = "\
flowspace3 prompt hook: a prompt that starts with `fs3` runs a flowspace3
search first and hands the hits to the agent with your prompt.

    fs3 \"<switches> <search text>\" <your message to the agent>

Switches (anywhere inside the quotes):

    (none)              this repo, code + docs + conversations
    -all      / -a      every indexed repo
    -repo X   / -r X    one named repo (short name like `pij` or the full identity)
    -code     / -f      code + docs only
    -convo    / -c      conversations only
    -this     / -s      only this chat's own conversation
    -n N                how many hits (default 8, max 25)
    -quick    / -q      answer from the hits alone, no further reading
    -raw                keep tool calls, tool output and fragments (left out by default)
    -help     / -h      the agent explains this grammar

Short switches combine (`-ac`); `--all` style works too. A near-miss switch
(`-thsi`) is read as the closest real one; an unknown one is reported. Search only, never
ask. Any other prompt passes through untouched, and the hook never blocks a
prompt: a failed search is reported to the agent instead.

Full guide: `flowspace3 docs get prompt-search`.
";

/// One daemon call may take this long before the hook reports it as timed out.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(25);
const SNIPPET_CHARS: usize = 500;
/// Stays well inside Claude Code's additionalContext cap and Codex's ~2.5k tokens.
const CONTEXT_BUDGET: usize = 9000;
/// Fetch this many times the limit so dropping noise still leaves enough hits.
const OVERFETCH: usize = 3;
/// `search` has no per-conversation filter: over-fetch and keep this chat's turns.
const SESSION_PAGES: usize = 3;
/// An unsummarised turn shorter than this carries no meaning on its own.
const NOISE_CHARS: usize = 160;
/// This chat's newest turns are tagged, never hidden: after a compaction fs3 may hold the only copy.
const RECENT_TURNS: i64 = 100;
/// A meaning-only best hit under this rarely bears on the search.
const WEAK_SCORE: f64 = 0.55;
const STOPWORDS: [&str; 23] = [
    "the", "and", "for", "what", "how", "why", "did", "does", "was", "were", "with", "from",
    "this", "that", "about", "into", "when", "where", "which", "who", "our", "you", "are",
];
const GUIDE: &str = "`flowspace3 docs get prompt-search`";
const TIP: &str = "To dig further yourself: `flowspace3 search \"<a question>\"` with `--repo all`, \
`--source code|doc|conversation`, `--path <glob>` or `--offset N` for the next page \
(`flowspace3 search --help` lists every flag); `flowspace3 get <address> --before 3 --after 10` \
reads around a turn; `flowspace3 tree conv:<guid>` outlines a whole chat; \
`flowspace3 docs list` lists the guides.";

/// The daemon calls the hook makes. Each answers a JSON envelope, ok or not.
pub trait Backend {
    /// `GET /search` with these query parameters.
    fn search(&self, params: Vec<(String, String)>) -> impl Future<Output = Value>;
    /// `GET /status`.
    fn status(&self) -> impl Future<Output = Value>;
    /// `GET /conversations/verify` for one native session.
    fn verify(&self, harness: &str, session_id: &str) -> impl Future<Output = Value>;
}

/// One prompt as the harness delivered it.
#[derive(Debug, Clone, Default)]
pub struct Event {
    pub prompt: String,
    pub session_id: String,
    pub cwd: String,
    /// The `--harness` name `conversation verify` knows this harness's chats by,
    /// or `None` when flowspace3 does not ingest them.
    pub verify_harness: Option<&'static str>,
    /// The harness's display name, for the notes that name a gap.
    pub harness: &'static str,
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn score(hit: &Value) -> f64 {
    hit.get("score").and_then(Value::as_f64).unwrap_or(0.0)
}

fn recent(hit: &Value) -> bool {
    hit.get("_recent").and_then(Value::as_bool).unwrap_or(false)
}

fn ok(env: &Value) -> bool {
    env.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

fn failure_text(env: &Value) -> String {
    let err = env.get("error").cloned().unwrap_or(Value::Null);
    let parts: Vec<&str> = ["code", "message", "fix"]
        .iter()
        .map(|k| str_at(&err, k))
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        "unknown failure".to_string()
    } else {
        parts.join(" | ")
    }
}

fn results(env: &Value) -> Vec<Value> {
    env.pointer("/data/results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Bounds one daemon call so a slow daemon is reported rather than holding the prompt.
async fn bounded(call: impl Future<Output = Value>, deadline: Instant) -> Value {
    let budget = deadline
        .saturating_duration_since(Instant::now())
        .min(CALL_TIMEOUT);
    match tokio::time::timeout(budget, call).await {
        Ok(env) => env,
        Err(_) => json!({"ok": false, "error": {"message":
            format!("flowspace3 timed out after {:.0}s", budget.as_secs_f64())}}),
    }
}

/// Why an unsummarised turn is left out, or `None` to keep it. Tool calls and
/// their output are commands, not answers: the agent's prose around them says
/// what they meant.
fn is_noise(hit: &Value) -> Option<&'static str> {
    if str_at(hit, "kind") != "turn" || !str_at(hit, "smart").is_empty() {
        return None;
    }
    let text = str_at(hit, "snippet").trim();
    if text.starts_with("[tool-call") {
        return Some("tool call");
    }
    if text.contains("[tool-result") {
        return Some("tool output");
    }
    (text.chars().count() < NOISE_CHARS).then_some("fragment")
}

/// One note naming what was left out, by kind, so the agent knows the hits are filtered.
fn noise_note(dropped: &[&str]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for reason in dropped {
        match counts.iter_mut().find(|(r, _)| r == reason) {
            Some((_, n)) => *n += 1,
            None => counts.push((reason, 1)),
        }
    }
    let parts = counts
        .iter()
        .map(|(reason, n)| format!("{n} {reason}{}", if *n > 1 { "s" } else { "" }))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "left out {parts}: tool calls, tool output and short fragments rarely answer a question, \
         the prose turns around them do. `-raw` keeps them; `flowspace3 search` itself never drops them"
    )
}

/// No hit contains the search words (outside this chat's own echoes) and none scores well.
fn is_weak(hits: &[Value]) -> bool {
    if hits.is_empty() {
        return true;
    }
    let worded = hits
        .iter()
        .any(|h| matches!(str_at(h, "channel"), "lexical" | "both") && !recent(h));
    !worded && hits.iter().map(score).fold(0.0, f64::max) < WEAK_SCORE
}

/// Ready-to-run flowspace3 searches for when the first pass misses.
fn follow_ups(query: &str, message: &str, flags: &[String]) -> Vec<String> {
    let scope = flags.join(" ");
    let mut asks: Vec<String> = Vec::new();
    if !message.is_empty() {
        let flat = message.replace('"', "'");
        let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
        asks.push(flat.chars().take(160).collect());
    }
    let words: BTreeSet<String> = query
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| ".,?!:;'\"()".contains(c))
                .to_lowercase()
        })
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(&w.as_str()))
        .collect();
    let mut terms: Vec<String> = words.into_iter().collect();
    terms.sort_by_key(|w| std::cmp::Reverse(w.chars().count())); // stable: ties stay alphabetical
    asks.extend(terms.into_iter().take(2));
    let mut lines: Vec<String> = asks
        .iter()
        .map(|ask| {
            format!("  flowspace3 search \"{ask}\" {scope}")
                .trim_end()
                .to_string()
        })
        .collect();
    if !flags.iter().any(|f| f == "all") {
        lines.push(format!(
            "  flowspace3 search \"{query}\" --repo all --source all"
        ));
    }
    lines
}

/// Instructions that make the agent tell the user about a fumbled switch.
fn issue_lines(issues: &Issues) -> Vec<String> {
    let mut lines = Vec::new();
    if !issues.corrected.is_empty() {
        let read = issues
            .corrected
            .iter()
            .map(|(typed, meant)| format!("{typed} as {meant}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "The user's fs3 switches were close but not exact; the hook read {read}. \
             Mention that in one short line at the start of your answer."
        ));
    }
    if !issues.unknown.is_empty() {
        lines.push(format!(
            "The user typed fs3 switch(es) the hook does not know: {}. Start your answer by saying so \
             in one line and showing them the real usage below, then answer from any hits.\nUsage:\n{}",
            issues.unknown.join(", "),
            usage()
        ));
    }
    lines
}

/// The grammar part of the doc: from the example line to the guide pointer.
fn usage() -> &'static str {
    let body = USAGE_DOC.split("Full guide:").next().unwrap_or(USAGE_DOC);
    body.split_once("prompt.")
        .map_or(body, |(_, rest)| rest)
        .trim()
}

/// Map a short repo name to its indexed identity. Returns `(identity, warning)`.
async fn resolve_repo<B: Backend>(
    backend: &B,
    name: &str,
    deadline: Instant,
) -> (Option<String>, Option<String>) {
    if name.contains(':') || name.contains('/') {
        return (Some(name.to_string()), None);
    }
    let env = bounded(backend.status(), deadline).await;
    if !ok(&env) {
        return (
            Some(name.to_string()),
            Some(format!(
                "could not list repos to resolve -repo {name}: {}",
                failure_text(&env)
            )),
        );
    }
    let identities: BTreeSet<String> = env
        .pointer("/data/roots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|root| str_at(root, "identity").to_string())
        .filter(|i| !i.is_empty())
        .collect();
    let leaf = |i: &str| i.rsplit('/').next().unwrap_or(i).to_lowercase();
    let wanted = name.to_lowercase();
    let exact: Vec<&String> = identities.iter().filter(|i| leaf(i) == wanted).collect();
    if let [only] = exact[..] {
        return (Some(only.clone()), None);
    }
    let partial: Vec<&String> = identities
        .iter()
        .filter(|i| leaf(i).contains(&wanted))
        .collect();
    if let [only] = partial[..] {
        return (Some(only.clone()), None);
    }
    let listed: Vec<&String> = if !exact.is_empty() {
        exact.clone()
    } else if !partial.is_empty() {
        partial.clone()
    } else {
        identities.iter().collect()
    };
    let shorts = listed
        .iter()
        .map(|i| i.rsplit('/').next().unwrap_or(i))
        .collect::<Vec<_>>()
        .join(", ");
    let how = if exact.is_empty() && partial.is_empty() {
        "no"
    } else {
        "several"
    };
    (
        None,
        Some(format!(
            "-repo {name} matches {how} indexed repos ({shorts}); searched all repos instead"
        )),
    )
}

/// This chat's conversation guid and turn count, or the reason it has none.
async fn this_chat<B: Backend>(
    backend: &B,
    event: &Event,
    deadline: Instant,
) -> Result<Value, String> {
    let Some(harness) = event.verify_harness else {
        return Err(format!(
            "flowspace3 does not index {} chats, so this chat cannot be searched or tagged",
            event.harness
        ));
    };
    if event.session_id.is_empty() {
        return Err(format!("{} did not say which chat this is", event.harness));
    }
    let env = bounded(backend.verify(harness, &event.session_id), deadline).await;
    if ok(&env) {
        Ok(env["data"].clone())
    } else {
        Err(format!(
            "this chat is not indexed yet: {}",
            failure_text(&env)
        ))
    }
}

/// Hits, the scope they came from, notes for the agent, and the flags to search again with.
pub struct Found {
    pub hits: Vec<Value>,
    pub scope: String,
    pub problems: Vec<String>,
    pub flags: Vec<String>,
}

fn param(name: &str, value: impl ToString) -> (String, String) {
    (name.to_string(), value.to_string())
}

async fn search_this_chat<B: Backend>(
    backend: &B,
    query: &str,
    opts: &Opts,
    event: &Event,
    deadline: Instant,
) -> Found {
    let flags = vec![
        "--repo".into(),
        "all".into(),
        "--source".into(),
        "conversation".into(),
    ];
    let verify = match this_chat(backend, event, deadline).await {
        Ok(verify) => verify,
        Err(why) => {
            return Found {
                hits: Vec::new(),
                scope: "this chat".into(),
                problems: vec![why],
                flags,
            };
        }
    };
    let guid = str_at(&verify, "guid");
    let prefix = format!("conv:{guid}#");
    let (mut hits, mut problems, mut dropped) = (Vec::new(), Vec::new(), Vec::new());
    for page in 0..SESSION_PAGES {
        let env = bounded(
            backend.search(vec![
                param("q", query),
                param("repo", "all"),
                param("source", "conversation"),
                param("limit", 100),
                param("offset", page * 100),
                param("cwd", &event.cwd),
            ]),
            deadline,
        )
        .await;
        if !ok(&env) {
            problems.push(failure_text(&env));
            break;
        }
        for hit in results(&env) {
            if !str_at(&hit, "address").starts_with(&prefix) {
                continue;
            }
            match (!opts.raw).then(|| is_noise(&hit)).flatten() {
                Some(reason) => dropped.push(reason),
                None => hits.push(hit),
            }
        }
        if hits.len() >= opts.limit || env.pointer("/data/next_offset").is_none_or(Value::is_null) {
            break;
        }
    }
    if !dropped.is_empty() {
        problems.push(noise_note(&dropped));
    }
    let through = verify
        .get("last_turn_at")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    hits.truncate(opts.limit);
    Found {
        hits,
        scope: format!("this chat only (conv:{guid}, indexed through {through})"),
        problems,
        flags,
    }
}

/// Run the search the switches describe.
pub async fn search<B: Backend>(
    backend: &B,
    query: &str,
    opts: &Opts,
    event: &Event,
    deadline: Instant,
) -> Found {
    if opts.this {
        return search_this_chat(backend, query, opts, event, deadline).await;
    }
    let mut problems = Vec::new();
    let fetch = if opts.raw {
        opts.limit
    } else {
        (opts.limit * OVERFETCH).min(100)
    };
    let mut base = vec![
        param("q", query),
        param("limit", fetch),
        param("cwd", &event.cwd),
    ];
    let mut flags: Vec<String> = Vec::new();
    let mut place = |repo: &str, base: &mut Vec<(String, String)>| {
        base.push(param("repo", repo));
        flags.extend(["--repo".to_string(), repo.to_string()]);
    };
    let where_ = if let Some(repo) = &opts.repo {
        let (identity, warning) = resolve_repo(backend, repo, deadline).await;
        problems.extend(warning);
        place(identity.as_deref().unwrap_or("all"), &mut base);
        identity.unwrap_or_else(|| "all repos".into())
    } else if opts.all {
        place("all", &mut base);
        "all repos".into()
    } else {
        "this repo".to_string()
    };
    let with = |source: &str| {
        let mut params = base.clone();
        params.push(param("source", source));
        bounded(backend.search(params), deadline)
    };
    let envs = match opts.what {
        What::Both => vec![with("all").await],
        What::Convo => vec![with("conversation").await],
        What::Code => {
            let (code, doc) = tokio::join!(with("code"), with("doc"));
            vec![code, doc]
        }
    };
    let mut hits = Vec::new();
    for env in envs {
        if ok(&env) {
            hits.extend(results(&env));
        } else {
            problems.push(failure_text(&env));
        }
    }
    hits.sort_by(|a, b| score(b).total_cmp(&score(a)));
    if opts.what != What::Code
        && let Ok(verify) = this_chat(backend, event, deadline).await
    {
        let prefix = format!("conv:{}#t", str_at(&verify, "guid"));
        let floor = verify.get("turns").and_then(Value::as_i64).unwrap_or(0) - RECENT_TURNS;
        for hit in &mut hits {
            let is_recent = str_at(hit, "address")
                .strip_prefix(&prefix)
                .and_then(|rest| rest.split('-').next())
                .and_then(|turn| turn.parse::<i64>().ok())
                .is_some_and(|turn| turn > floor);
            if let Some(fields) = hit.as_object_mut() {
                fields.insert("_recent".into(), Value::Bool(is_recent));
            }
        }
    }
    if !opts.raw {
        let dropped: Vec<&str> = hits.iter().filter_map(is_noise).collect();
        if !dropped.is_empty() {
            problems.push(noise_note(&dropped));
        }
        hits.retain(|h| is_noise(h).is_none());
    }
    hits.truncate(opts.limit);
    let tagged = hits.iter().filter(|h| recent(h)).count();
    if tagged > 0 {
        problems.push(format!(
            "{tagged} hit(s) are from this chat's last {RECENT_TURNS} turns (tagged THIS CHAT): \
             you may already have them in context, unless a compaction removed them"
        ));
    }
    let what = match opts.what {
        What::Both => "code + docs + conversations",
        What::Code => "code + docs",
        What::Convo => "conversations",
    };
    flags.extend(
        match opts.what {
            What::Both => &[][..],
            What::Code => &["--source", "code"][..],
            What::Convo => &["--source", "conversation"][..],
        }
        .iter()
        .map(|s| s.to_string()),
    );
    Found {
        hits,
        scope: format!("{where_}, {what}"),
        problems,
        flags,
    }
}

/// Keep rank order, but put every hit from one conversation under its first.
fn grouped(hits: &[Value]) -> Vec<(String, Vec<(usize, &Value)>)> {
    let mut groups: Vec<(String, Vec<(usize, &Value)>)> = Vec::new();
    for (rank, hit) in hits.iter().enumerate() {
        let address = str_at(hit, "address");
        let key = if address.starts_with("conv:") {
            address.split('#').next().unwrap_or(address)
        } else {
            address
        };
        match groups.iter_mut().find(|(k, _)| k == key) {
            Some((_, members)) => members.push((rank + 1, hit)),
            None => groups.push((key.to_string(), vec![(rank + 1, hit)])),
        }
    }
    groups
}

fn turn_of(address: &str) -> &str {
    address.split_once('#').map_or("", |(_, turn)| turn)
}

/// The context block the agent reads.
pub fn render(
    query: &str,
    found: &Found,
    message: &str,
    quick: bool,
    fumbles: &[String],
) -> String {
    let hits = &found.hits;
    let mut lines: Vec<String> = vec![
        "flowspace3 search results the user asked for with an `fs3` prompt.".into(),
        "The user typed `fs3` because they want this answered from flowspace3 (they build it and use it on \
         purpose): use flowspace3 (`search`, `get`, `tree`) before the web, memory or other tools."
            .into(),
        format!("Search: \"{query}\" | scope: {} | {} hit(s).", found.scope, hits.len()),
        format!("A hook ran `flowspace3 search` before your turn; you did not run it. Grammar: {GUIDE}."),
        if message.is_empty() {
            "The user added no message: summarise what the results say about the search, citing addresses."
                .into()
        } else {
            format!("The user's message about these results: {message}")
        },
        "Judge relevance first: build on the hits that bear on the search and say in one line which you ignored. \
         If none bear on it, the search missed, not the index: search flowspace3 again before anything else."
            .into(),
    ];
    lines.extend(fumbles.iter().cloned());
    if is_weak(hits) {
        let best = hits.iter().map(score).fold(0.0, f64::max);
        let why = if hits.is_empty() {
            "no hits".to_string()
        } else {
            format!(
                "no hit contains the search words and the best meaning match scores only {best:.2}"
            )
        };
        if quick {
            lines.push(format!(
                "WEAK RESULTS: {why}. The user asked for a quick answer (-quick): say plainly \
                 that these hits miss rather than guessing, and offer to search further."
            ));
        } else {
            lines.push(format!(
                "WEAK RESULTS: {why}, so these probably miss. Do not stop here or go to the web: \
                 run at least 2-3 more flowspace3 searches first, then read around any hit that fits. \
                 Rephrase as a full question and try each key word alone, for example:"
            ));
            lines.extend(follow_ups(query, message, &found.flags));
            lines.push(
                "Only when those miss too, tell the user flowspace3 had nothing on it, then use other sources."
                    .into(),
            );
            lines.push(TIP.into());
        }
    } else if quick {
        lines.push(
            "The user asked for a quick answer (-quick): answer from these hits alone, no further reading."
                .into(),
        );
    } else {
        lines.push(
            "Before answering, read around the strongest hit(s) with `flowspace3 get <address>` \
             (add `--before 3 --after 10` for a conversation turn): one turn is rarely the whole story. \
             Conversations are history; check current code before calling a past decision still true."
                .into(),
        );
        lines.push(TIP.into());
    }
    lines.extend(found.problems.iter().map(|p| format!("NOTE: {p}")));
    lines.push(String::new());
    let mut used: usize = lines.iter().map(|l| l.chars().count() + 1).sum();
    let mut shown = 0;
    for (key, members) in grouped(hits) {
        let conv = key.starts_with("conv:");
        let first = members[0].1;
        let repo = str_at(first, "repo").rsplit('/').next().unwrap_or("");
        let repo = if repo.is_empty() { "(none)" } else { repo };
        let mut block: Vec<String> = Vec::new();
        if conv {
            let turns = members
                .iter()
                .map(|(rank, hit)| format!("[{rank}] #{}", turn_of(str_at(hit, "address"))))
                .collect::<Vec<_>>()
                .join(", ");
            let agent = first.get("agent").cloned().unwrap_or(Value::Null);
            let models = agent
                .get("models")
                .and_then(Value::as_array)
                .map(|m| {
                    m.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let who = [
                str_at(&agent, "seat"),
                str_at(&agent, "harness"),
                models.as_str(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            let who = if who.is_empty() {
                String::new()
            } else {
                format!("  agent={who}")
            };
            block.push(format!("{key}  repo={repo}{who}  turns: {turns}"));
        }
        for (rank, hit) in &members {
            let mut kind = str_at(hit, "kind").to_string();
            let subkind = str_at(hit, "subkind");
            if !subkind.is_empty() {
                kind = format!("{kind}/{subkind}");
            }
            if recent(hit) {
                kind.push_str(" THIS CHAT");
            }
            let address = str_at(hit, "address");
            let head = if conv {
                format!(
                    "  [{rank}] #{}  {kind} score={:.2}",
                    turn_of(address),
                    score(hit)
                )
            } else {
                let path = str_at(hit, "path");
                let path = if path.is_empty() {
                    String::new()
                } else {
                    format!(" {path}")
                };
                format!(
                    "[{rank}] {address}  repo={repo}{path}  {kind} score={:.2}",
                    score(hit)
                )
            };
            let smart = str_at(hit, "smart");
            let mut body = (if smart.is_empty() {
                str_at(hit, "snippet")
            } else {
                smart
            })
            .trim()
            .to_string();
            if body.chars().count() > SNIPPET_CHARS {
                body = body.chars().take(SNIPPET_CHARS).collect::<String>() + " …";
            }
            block.push(head);
            block.push(format!("    {}", body.replace('\n', "\n    ")));
        }
        let text = block.join("\n") + "\n";
        let size = text.chars().count();
        if used + size > CONTEXT_BUDGET {
            lines.push(format!(
                "({} more hit(s) cut to fit the context budget)",
                hits.len() - shown
            ));
            break;
        }
        lines.push(text);
        used += size;
        shown += members.len();
    }
    lines.join("\n")
}

/// The context for one prompt, or `None` when it is not an fs3 prompt.
pub async fn build_context<B: Backend>(backend: &B, event: &Event) -> Option<String> {
    let (search_part, message) = grammar::split_prompt(&event.prompt)?;
    let (opts, query, issues) = grammar::parse_switches(&search_part);
    if opts.help || !grammar::has_words(&query) {
        let mut fumbled = issues.unknown.clone();
        fumbled.extend(issues.corrected.iter().map(|(typed, _)| typed.clone()));
        let why = if !fumbled.is_empty() && !opts.help {
            format!("used switch(es) {} with no search text", fumbled.join(", "))
        } else if opts.help {
            "asked how it works".into()
        } else {
            "gave no search text".into()
        };
        return Some(format!(
            "The user typed an `fs3` prompt but {why}, so no search ran. \
             Tell them that in one line and show them the usage plainly; the full guide is {GUIDE}.\n\n{USAGE_DOC}"
        ));
    }
    let deadline = Instant::now() + CALL_TIMEOUT;
    let found = search(backend, &query, &opts, event, deadline).await;
    Some(render(
        &query,
        &found,
        &message,
        opts.quick,
        &issue_lines(&issues),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Canned daemon answers; records every search it was asked for.
    #[derive(Default)]
    struct Fake {
        search: Vec<Value>,
        status: Option<Value>,
        verify: Option<Value>,
        asked: Mutex<Vec<Vec<(String, String)>>>,
        slow: bool,
    }

    impl Backend for Fake {
        async fn search(&self, params: Vec<(String, String)>) -> Value {
            if self.slow {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            let source = params
                .iter()
                .find(|(k, _)| k == "source")
                .map(|(_, v)| v.clone());
            self.asked.lock().unwrap().push(params);
            let results: Vec<Value> = self
                .search
                .iter()
                .filter(|h| match source.as_deref() {
                    Some("code") => h["kind"] != "turn" && h["kind"] != "doc",
                    Some("doc") => h["kind"] == "doc",
                    Some("conversation") => h["kind"] == "turn",
                    _ => true,
                })
                .cloned()
                .collect();
            json!({"ok": true, "data": {"results": results}})
        }
        async fn status(&self) -> Value {
            self.status
                .clone()
                .unwrap_or(json!({"ok": false, "error": {"message": "down"}}))
        }
        async fn verify(&self, _: &str, _: &str) -> Value {
            self.verify
                .clone()
                .unwrap_or(json!({"ok": false, "error": {"code": "E", "message": "not indexed"}}))
        }
    }

    fn event(prompt: &str) -> Event {
        Event {
            prompt: prompt.into(),
            session_id: "s1".into(),
            cwd: "/repo".into(),
            verify_harness: Some("claude"),
            harness: "Claude Code",
        }
    }

    fn code_hit(address: &str, score: f64) -> Value {
        json!({"address": address, "kind": "symbol", "repo": "git:github.com/o/fs3",
               "path": "src/a.rs", "score": score, "snippet": "fn retry() {}", "channel": "both"})
    }

    fn turn(address: &str, score: f64, snippet: &str) -> Value {
        json!({"address": address, "kind": "turn", "repo": "git:github.com/o/fs3",
               "score": score, "snippet": snippet, "channel": "semantic",
               "agent": {"seat": "pij-x", "harness": "claude", "models": ["opus"]}})
    }

    async fn context(fake: &Fake, prompt: &str) -> Option<String> {
        build_context(fake, &event(prompt)).await
    }

    #[tokio::test]
    async fn non_fs3_prompts_pass_through() {
        assert_eq!(context(&Fake::default(), "hello").await, None);
    }

    #[tokio::test]
    async fn help_and_empty_searches_show_usage_without_searching() {
        let fake = Fake::default();
        let help = context(&fake, "fs3 \"-help\"").await.unwrap();
        assert!(help.contains("asked how it works") && help.contains("-convo    / -c"));
        let empty = context(&fake, "fs3 \"-thsi\"").await.unwrap();
        assert!(empty.contains("used switch(es) -thsi with no search text"));
        let dots = context(&fake, "fs3 \"…\"").await.unwrap();
        assert!(dots.contains("gave no search text"));
        assert!(fake.asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn hits_are_ranked_grouped_and_noise_is_left_out() {
        let long_prose = "x".repeat(200);
        let fake = Fake {
            search: vec![
                code_hit("el:a", 0.70),
                turn("conv:g1#t5", 0.80, &long_prose),
                turn("conv:g1#t9", 0.60, &long_prose),
                turn("conv:g2#t1", 0.90, "[tool-call bash] ls"),
                turn("conv:g2#t2", 0.85, "short"),
            ],
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 \"retry policy\" why?").await.unwrap();
        assert!(ctx.contains(
            "Search: \"retry policy\" | scope: this repo, code + docs + conversations | 3 hit(s)."
        ));
        assert!(ctx.contains("The user's message about these results: why?"));
        assert!(
            ctx.contains("conv:g1  repo=fs3  agent=pij-x · claude · opus  turns: [1] #t5, [3] #t9")
        );
        assert!(ctx.contains("[2] el:a  repo=fs3 src/a.rs  symbol score=0.70"));
        assert!(ctx.contains("NOTE: left out 1 tool call, 1 fragment: "));
        assert!(!ctx.contains("WEAK RESULTS"));
        assert!(ctx.contains("Before answering, read around"));
        let asked = fake.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert!(asked[0].contains(&param("limit", 24)));
        assert!(asked[0].contains(&param("source", "all")));
        assert!(asked[0].contains(&param("cwd", "/repo")));
    }

    #[tokio::test]
    async fn code_switch_searches_code_and_docs_in_parallel() {
        let fake = Fake {
            search: vec![
                code_hit("el:a", 0.4),
                json!({"address": "el:d", "kind": "doc", "score": 0.9}),
            ],
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 \"-f -n 2 x\"").await.unwrap();
        assert!(ctx.contains("[1] el:d") && ctx.contains("[2] el:a"));
        assert_eq!(fake.asked.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn weak_results_suggest_follow_up_searches() {
        let fake = Fake {
            search: vec![turn("conv:g#t1", 0.30, &"y".repeat(300))],
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 \"-c where does the backup go\" the nas one")
            .await
            .unwrap();
        assert!(ctx.contains("WEAK RESULTS: no hit contains the search words and the best meaning match scores only 0.30"));
        assert!(ctx.contains("  flowspace3 search \"the nas one\" --source conversation"));
        assert!(ctx.contains("  flowspace3 search \"backup\" --source conversation"));
        assert!(
            ctx.contains(
                "  flowspace3 search \"where does the backup go\" --repo all --source all"
            )
        );
        let quick = context(&fake, "fs3 \"-q x\"").await.unwrap();
        assert!(quick.contains("say plainly that these hits miss"));
    }

    #[tokio::test]
    async fn this_chat_turns_are_tagged_never_hidden() {
        let prose = "z".repeat(200);
        let fake = Fake {
            search: vec![
                turn("conv:me#t150", 0.9, &prose),
                turn("conv:me#t20", 0.8, &prose),
            ],
            verify: Some(
                json!({"ok": true, "data": {"guid": "me", "turns": 200, "last_turn_at": "t"}}),
            ),
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 \"x\"").await.unwrap();
        assert!(ctx.contains("#t150  turn THIS CHAT"));
        assert!(ctx.contains("#t20  turn score"));
        assert!(ctx.contains("1 hit(s) are from this chat's last 100 turns"));
    }

    #[tokio::test]
    async fn this_switch_keeps_only_this_chat() {
        let prose = "z".repeat(200);
        let fake = Fake {
            search: vec![
                turn("conv:other#t1", 0.9, &prose),
                turn("conv:me#t3", 0.5, &prose),
            ],
            verify: Some(
                json!({"ok": true, "data": {"guid": "me", "turns": 5, "last_turn_at": "2026-10-09"}}),
            ),
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 -this nas").await.unwrap();
        assert!(
            ctx.contains("scope: this chat only (conv:me, indexed through 2026-10-09) | 1 hit(s)")
        );
        assert!(!ctx.contains("conv:other"));
    }

    #[tokio::test]
    async fn unindexed_harnesses_name_the_gap() {
        let fake = Fake::default();
        let mut ev = event("fs3 -this nas");
        ev.verify_harness = None;
        ev.harness = "Codex";
        let ctx = build_context(&fake, &ev).await.unwrap();
        assert!(ctx.contains(
            "NOTE: flowspace3 does not index Codex chats, so this chat cannot be searched"
        ));
    }

    #[tokio::test]
    async fn repo_short_names_resolve_through_status() {
        let fake = Fake {
            status: Some(json!({"ok": true, "data": {"roots": [
                {"identity": "git:github.com/o/pij"}, {"identity": "git:github.com/o/pij-rs"},
                {"identity": "git:github.com/o/fs3"}]}})),
            ..Fake::default()
        };
        context(&fake, "fs3 \"-r pij x\"").await.unwrap();
        assert!(fake.asked.lock().unwrap()[0].contains(&param("repo", "git:github.com/o/pij")));
        let ctx = context(&fake, "fs3 \"-r nope x\"").await.unwrap();
        assert!(ctx.contains(
            "-repo nope matches no indexed repos (fs3, pij, pij-rs); searched all repos instead"
        ));
    }

    #[tokio::test]
    async fn fumbled_switches_are_reported() {
        let ctx = context(&Fake::default(), "fs3 \"-thsi --zzz nas\"")
            .await
            .unwrap();
        assert!(ctx.contains("the hook read -thsi as -this"));
        assert!(ctx.contains("does not know: --zzz") && ctx.contains("Usage:\nfs3 \"<switches>"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_daemon_is_reported_not_waited_on() {
        let fake = Fake {
            slow: true,
            ..Fake::default()
        };
        let ctx = context(&fake, "fs3 \"-c x\"").await.unwrap();
        assert!(ctx.contains("NOTE: flowspace3 timed out after 25s"));
    }

    #[test]
    fn the_budget_cuts_whole_groups() {
        let hits: Vec<Value> = (0..25).map(|i| code_hit(&format!("el:{i}"), 0.9)).collect();
        let mut big = hits.clone();
        for h in &mut big {
            h["snippet"] = json!("w".repeat(600));
        }
        let found = Found {
            hits: big,
            scope: "s".into(),
            problems: vec![],
            flags: vec![],
        };
        let ctx = render("q", &found, "", false, &[]);
        assert!(ctx.chars().count() <= CONTEXT_BUDGET + 200);
        assert!(ctx.contains("more hit(s) cut to fit the context budget"));
        assert!(ctx.contains("w …"));
    }
}
