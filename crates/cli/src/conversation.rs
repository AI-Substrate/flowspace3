//! The conversation importer: a transcript on disk becomes indexed turns.
//!
//! Workshop 005's decision C5 — the import verb ships in v1 and live capture
//! does not — makes this the intake endpoint's FIRST client, and the reason is
//! dogfooding: hand-fed transcripts prove search and windowing on real data
//! with zero live-capture machinery. The live git-ai/harness submitter is a
//! separate future packet against the same endpoint.
//!
//! # Where dialects live
//!
//! Here, and nowhere else (workshop 005, open question 3). The schema knows one
//! shape; a claude or omp transcript is translated INTO it on the way in. A
//! dialect that reached the tables would be a migration every time a harness
//! changed its mind about a field name.
//!
//! # Why re-importing is the normal case
//!
//! A transcript grows while you work. The obvious loop is to import it again,
//! and the intake is idempotent on `(conversation_id, turn_no)` — so a
//! re-import stores only the turns that are new and enqueues enrichment only
//! for those. That only holds if the SAME guid is used each time, which is why
//! a file may carry its own `guid` and why `--guid` exists for files that
//! cannot: a fresh guid per import would store the whole conversation again
//! under a new address and pay for all of it a second time.

use std::io::Read;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

/// One line of our own JSONL shape.
///
/// The FIRST line may be a header (`{"guid": …, "title": …}`); every other line
/// is a turn. A file with no header is legal — the guid is minted, or supplied
/// with `--guid` — because the cheapest thing an agent can write is a stream of
/// turns.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Line {
    /// A header line: anything that carries a title or an anchor and no body.
    Header(Header),
    /// A turn, in our shape or a dialect this module understands.
    Turn(Value),
}

/// The optional first line.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    #[serde(default)]
    guid: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    repo_identity: Option<String>,
    #[serde(default)]
    worktree: Option<String>,
    #[serde(default)]
    base_sha: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
}

/// A transcript, read and shaped, ready to post.
pub struct Import {
    /// The intake request body.
    pub body: Value,
    /// How many turns the file carried.
    pub turns: usize,
}

/// Read a transcript and shape it into an intake request.
///
/// # Errors
/// When the file cannot be read, a line is not JSON, or a turn carries no
/// ordinal and none can be derived.
pub fn read(
    source: &str,
    guid: Option<String>,
    repo: Option<String>,
    worktree: Option<String>,
    title: Option<String>,
) -> Result<Import> {
    let text = if source == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("reading the transcript from stdin")?;
        buffer
    } else {
        std::fs::read_to_string(source)
            .with_context(|| format!("reading the transcript at {source}"))?
    };

    let mut header = Header::default();
    let mut turns: Vec<Value> = Vec::new();

    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Line = serde_json::from_str(line)
            .with_context(|| format!("{source}:{} is not a JSON object", number + 1))?;
        match parsed {
            // Only the FIRST line may be a header. A later one is a malformed
            // file, and it has to say so: a turn carries prose and a role, so
            // nothing that reads as a header is a turn somebody meant to write
            // — silently keeping it would drop content and silently storing it
            // would store a header as a turn.
            Line::Header(found) if turns.is_empty() => header = found,
            Line::Header(_) => anyhow::bail!(
                "{source}:{} is a second header line; only the first line may be a header",
                number + 1
            ),
            Line::Turn(turn) => turns.push(turn),
        }
    }

    let turns: Vec<Value> = turns
        .into_iter()
        .enumerate()
        .map(|(index, turn)| shape_turn(turn, index))
        .collect::<Result<_>>()?;

    // Ordering of precedence, and it is deliberate: what the CALLER typed beats
    // what the FILE says, because the flag is the more recent decision.
    let guid = guid
        .or(header.guid)
        .unwrap_or_else(|| mint_guid(source, &turns));
    let started_at = header
        .started_at
        .or_else(|| first_timestamp(&turns))
        .unwrap_or_else(now);
    let title = title.or(header.title).or_else(|| derive_title(&turns));

    let body = serde_json::json!({
        "guid": guid,
        "repo_identity": repo.or(header.repo_identity),
        "worktree": worktree.or(header.worktree),
        "base_sha": header.base_sha,
        "title": title,
        "started_at": started_at,
        "turns": turns,
    });

    Ok(Import {
        turns: turns_len(&body),
        body,
    })
}

fn turns_len(body: &Value) -> usize {
    body["turns"].as_array().map_or(0, Vec::len)
}

/// Translate one line into the shape the intake takes.
///
/// Generous about what it accepts and exact about what it produces, because the
/// dialects differ only in spelling: `content`/`text`/`body` all mean the same
/// thing, and a turn with no ordinal is at the position it was written.
fn shape_turn(turn: Value, index: usize) -> Result<Value> {
    let Value::Object(mut turn) = turn else {
        anyhow::bail!("a turn must be a JSON object, got {turn}");
    };

    // Ordinal: dense from 1, and its own position is the honest default. A file
    // that numbers its turns keeps its numbering, which is what lets a grown
    // file re-import as a delta rather than as a shifted duplicate.
    let turn_no = turn
        .get("turn_no")
        .and_then(Value::as_u64)
        .unwrap_or(index as u64 + 1);
    turn.insert("turn_no".to_string(), Value::from(turn_no));

    // Prose, under whichever name the dialect used.
    let body = ["body", "content", "text", "message"]
        .iter()
        .find_map(|key| turn.get(*key).and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    turn.insert("body".to_string(), Value::from(body));

    // Role: `user` is what most harnesses call a human.
    let role = turn
        .get("role")
        .and_then(Value::as_str)
        .map(|role| match role {
            "user" | "human" => "human",
            _ => "agent",
        })
        .unwrap_or("agent");
    turn.insert("role".to_string(), Value::from(role));

    // Source: measured to matter (workshop 005, C8). Absent means the ordinary
    // case for the harness that wrote the file — a human at a keyboard when the
    // role says human, the harness itself otherwise.
    if !turn.contains_key("source") {
        let source = if role == "human" { "human" } else { "system" };
        turn.insert("source".to_string(), Value::from(source));
    }

    // Timestamp: the intake needs one, and a transcript that omits it is not
    // wrong — it just cannot say when. `now` is honest about being the import
    // time rather than inventing a plausible past.
    if !turn.get("at").is_some_and(Value::is_string) {
        let at = ["at", "timestamp", "time", "created_at"]
            .iter()
            .find_map(|key| turn.get(*key).and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(now);
        turn.insert("at".to_string(), Value::from(at));
    }

    turn.entry("items").or_insert_with(|| Value::Array(vec![]));

    // Fields the schema has no home for are dropped rather than smuggled: the
    // stored form is a contract, and a dialect's extra keys would make it one
    // shape per harness.
    turn.retain(|key, _| {
        matches!(
            key.as_str(),
            "turn_no" | "role" | "source" | "head_sha" | "at" | "body" | "items"
        )
    });

    Ok(Value::Object(turn))
}

/// The conversation's start, taken from its first turn.
fn first_timestamp(turns: &[Value]) -> Option<String> {
    turns
        .first()?
        .get("at")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// A title from the first line of the first turn that has prose.
fn derive_title(turns: &[Value]) -> Option<String> {
    let body = turns
        .iter()
        .find_map(|turn| turn.get("body").and_then(Value::as_str))
        .filter(|body| !body.trim().is_empty())?;
    let line = body.lines().next().unwrap_or(body).trim();
    Some(line.chars().take(120).collect())
}

/// A guid derived from the transcript itself.
///
/// Content-derived rather than random, and that is the whole point: importing
/// the same file twice — or a grown version of it — has to reach the SAME
/// conversation, or the second import stores everything again under a new
/// address and pays for all of it a second time. The seed is the source name
/// plus the first turn, so a file that grows keeps its identity while two
/// genuinely different transcripts do not collide.
fn mint_guid(source: &str, turns: &[Value]) -> String {
    let mut seed = String::from(source);
    if let Some(first) = turns.first() {
        seed.push('\n');
        seed.push_str(&first.to_string());
    }
    let digest = fs3_core::content_hash(seed.as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &digest[0..8],
        &digest[8..12],
        &digest[12..16],
        &digest[16..20],
        &digest[20..32]
    )
}

/// The current time, RFC 3339 in UTC, without a date-time dependency.
///
/// fs3 has no date-time crate on purpose — timestamps are formatted server-side
/// so two machines cannot disagree about what "now" was — and adding one here
/// to stamp an import would be a dependency bought for one string. This is the
/// civil-time conversion, which is arithmetic.
fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since the Unix epoch to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(text: &str) -> Import {
        let file = std::env::temp_dir().join(format!(
            "fs3-import-{}.jsonl",
            fs3_core::content_hash(text.as_bytes())
        ));
        std::fs::write(&file, text).expect("writing the fixture");
        let result =
            read(&file.display().to_string(), None, None, None, None).expect("reading the fixture");
        std::fs::remove_file(&file).ok();
        result
    }

    #[test]
    fn a_bare_stream_of_turns_is_a_legal_transcript() {
        let parsed = import(
            "{\"role\":\"user\",\"content\":\"why is gc eating my jobs\"}\n\
             {\"role\":\"assistant\",\"content\":\"because embed payloads have no raw_hash\"}\n",
        );

        assert_eq!(parsed.turns, 2);
        let turns = parsed.body["turns"].as_array().unwrap();
        assert_eq!(turns[0]["turn_no"], 1, "dense from 1, by position");
        assert_eq!(turns[1]["turn_no"], 2);
        assert_eq!(
            turns[0]["role"], "human",
            "`user` is what a human is called"
        );
        assert_eq!(turns[0]["source"], "human");
        assert_eq!(turns[1]["role"], "agent");
        assert_eq!(turns[1]["source"], "system");
        assert_eq!(turns[0]["body"], "why is gc eating my jobs");
        assert!(turns[0]["at"].is_string(), "the intake needs a timestamp");
        assert_eq!(
            parsed.body["title"], "why is gc eating my jobs",
            "a title is derived from the first prose when none is given"
        );
    }

    /// The property the whole iterative loop rests on: the same file imports to
    /// the same conversation, so a re-import is a delta rather than a copy.
    #[test]
    fn the_same_transcript_mints_the_same_guid() {
        let text = "{\"role\":\"user\",\"content\":\"the same words\"}\n";
        let once = import(text);
        let twice = import(text);
        assert_eq!(once.body["guid"], twice.body["guid"]);

        let other = import("{\"role\":\"user\",\"content\":\"different words\"}\n");
        assert_ne!(once.body["guid"], other.body["guid"]);

        // And it is a guid the daemon will accept.
        assert!(
            fs3_core::ConversationId::new(once.body["guid"].as_str().unwrap().to_string()).is_ok()
        );
    }

    /// A grown file keeps its identity AND its numbering, which is what makes
    /// the second import store only what is new.
    #[test]
    fn a_grown_transcript_keeps_its_guid_and_extends_its_numbering() {
        let first = "{\"guid\":\"6ba7b810-9dad-11d1-80b4-00c04fd430c8\",\"title\":\"a session\"}\n\
                     {\"role\":\"user\",\"content\":\"one\"}\n";
        let grown = format!("{first}{{\"role\":\"assistant\",\"content\":\"two\"}}\n");

        let before = import(first);
        let after = import(&grown);

        assert_eq!(before.body["guid"], "6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_eq!(after.body["guid"], before.body["guid"]);
        assert_eq!(before.turns, 1);
        assert_eq!(after.turns, 2);

        let after_turns = after.body["turns"].as_array().unwrap();
        assert_eq!(after_turns[0]["turn_no"], 1, "the old turn keeps its place");
        assert_eq!(after_turns[1]["turn_no"], 2);
        assert_eq!(after.body["title"], "a session");
    }

    #[test]
    fn a_dialect_carrying_its_own_numbering_and_items_is_kept() {
        let parsed = import(
            "{\"turn_no\":7,\"role\":\"assistant\",\"text\":\"ran it\",\
              \"items\":[{\"kind\":\"tool_call\",\"tool\":\"bash\",\
              \"input\":{\"kind\":\"verbatim\",\"text\":\"cargo test\"}}],\
              \"at\":\"2026-08-27T09:00:00Z\",\"head_sha\":\"abc\",\"extra\":\"dropped\"}\n",
        );

        let turn = &parsed.body["turns"][0];
        assert_eq!(turn["turn_no"], 7, "its own numbering survives");
        assert_eq!(turn["body"], "ran it");
        assert_eq!(turn["at"], "2026-08-27T09:00:00Z");
        assert_eq!(turn["head_sha"], "abc");
        assert_eq!(turn["items"][0]["tool"], "bash");
        assert!(
            turn.get("extra").is_none(),
            "a dialect's extra keys are dropped, not smuggled into the schema"
        );
        assert_eq!(parsed.body["started_at"], "2026-08-27T09:00:00Z");
    }

    #[test]
    fn now_is_rfc_3339_in_utc() {
        let stamp = now();
        assert_eq!(stamp.len(), 20, "{stamp}");
        assert!(stamp.ends_with('Z'), "{stamp}");
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
        // A date this code will not see again, so a broken conversion shows up
        // as a wrong year rather than a plausible one.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
    }
}
