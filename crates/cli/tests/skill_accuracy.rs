//! The bundled agent skill must not teach flags or values the product lacks.
//!
//! The skill is loaded straight into an agent's context and acted on without
//! a second look, so a stale claim there costs every agent that reads it: a
//! usage error for a flag that was renamed, or a branch on an `empty_because`
//! that can no longer arrive. On 2026-10-15 an audit found the skill teaching a
//! two-value `match_field`, an `empty_because` set missing a reason, and a score
//! definition that no longer held. This test makes the checkable half of that
//! drift fail here instead of in a session.
//!
//! Same muscle as `docs_bundle.rs`: flags are read from the REAL binary's
//! `--help`, and value sets from the constants the daemon itself emits, so
//! there is no third list to keep in sync.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The skill exactly as the binary bundles it (`fs3_cli::skill`).
const SKILL: &str = include_str!("../skills/flowspace/SKILL.md");

/// Long flags the skill may name that belong to ANOTHER tool (git, grep,
/// curl…) rather than to flowspace3. Empty today: add an entry only with the
/// owning tool named beside it, never to silence a renamed fs3 flag.
const FOREIGN_FLAGS: &[&str] = &[];

/// A `flowspace3` sealed against the production store (see `docs_bundle.rs`):
/// `--help` opens no pool, and the pin keeps it that way.
fn flowspace3(config_dir: &Path) -> Command {
    fs3_testkit::sealed(
        Path::new(env!("CARGO_BIN_EXE_flowspace3")),
        config_dir,
        fs3_testkit::TestDatabase::Unreachable,
    )
}

/// Every `--flag` the skill names, anywhere in its text.
///
/// A flag starts at `--` that is not part of a longer dash run (a markdown
/// table rule) and is followed by a lowercase letter.
fn flags_named() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (at, _) in SKILL.match_indices("--") {
        let rest = &SKILL[at + 2..];
        let in_dash_run = at > 0 && SKILL.as_bytes()[at - 1] == b'-';
        if in_dash_run || !rest.starts_with(|c: char| c.is_ascii_lowercase()) {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        found.insert(name.trim_end_matches('-').to_string());
    }
    found
}

/// One `--help` page of the real binary, for the subcommand path given.
fn help(config_dir: &Path, path: &[String]) -> String {
    let output = flowspace3(config_dir)
        .args(path)
        .arg("--help")
        .output()
        .expect("the binary should run");
    assert!(
        output.status.success(),
        "`flowspace3 {} --help` failed: {}",
        path.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The subcommands a help page lists: two-space-indented names under
/// `Commands:`, skipping clap's own `help`.
fn subcommands(page: &str) -> Vec<String> {
    page.lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        // A wrapped description continues deeper; only a name sits at two.
        .filter(|line| !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .filter(|word| *word != "help")
        .map(str::to_string)
        .collect()
}

/// The long flags a help page offers: the leading `-x, --name <VALUE>`
/// words of each option line.
fn long_flags(page: &str, into: &mut BTreeSet<String>) {
    for line in page.lines().map(str::trim_start) {
        if !line.starts_with('-') {
            continue;
        }
        for word in line
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|word| !word.is_empty())
            .take_while(|word| word.starts_with('-'))
        {
            if let Some(name) = word.strip_prefix("--")
                && !name.is_empty()
            {
                into.insert(name.to_string());
            }
        }
    }
}

/// Every long flag on every subcommand of the real binary.
fn flags_offered() -> BTreeSet<String> {
    let config = tempfile::tempdir().expect("a temp config directory");
    let mut offered = BTreeSet::new();
    let mut pending: Vec<Vec<String>> = vec![Vec::new()];
    while let Some(path) = pending.pop() {
        let page = help(config.path(), &path);
        long_flags(&page, &mut offered);
        for sub in subcommands(&page) {
            let mut next = path.clone();
            next.push(sub);
            pending.push(next);
        }
    }
    offered
}

/// The first column of the skill table whose header cell is `` `header` ``,
/// backticks stripped.
fn table_column(header: &str) -> BTreeSet<String> {
    let head = format!("| `{header}` |");
    let mut lines = SKILL.lines().skip_while(|line| !line.starts_with(&head));
    assert!(
        lines.next().is_some(),
        "the skill has no table headed {head:?}; this test reads its values from there"
    );
    lines
        .skip(1) // the |---| rule
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cell = line.split('|').nth(1).unwrap_or_default().trim();
            assert!(
                cell.starts_with('`') && cell.ends_with('`'),
                "every row of the `{header}` table must name one backticked value: {line}"
            );
            cell.trim_matches('`').to_string()
        })
        .collect()
}

#[test]
fn every_flag_the_skill_names_is_accepted_by_the_cli() {
    let offered = flags_offered();
    for expected in ["repo", "path", "json", "help"] {
        assert!(
            offered.contains(expected),
            "could not read `--{expected}` from the binary's --help pages; the help parser \
             above needs updating. Read: {offered:?}"
        );
    }

    let named = flags_named();
    assert!(
        named.contains("min-score"),
        "the flag scan read nothing sensible from the skill: {named:?}"
    );
    let unknown: Vec<_> = named
        .iter()
        .filter(|flag| !offered.contains(*flag) && !FOREIGN_FLAGS.contains(&flag.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "the bundled skill names flags no flowspace3 subcommand accepts: {unknown:?}. \
         Either the flag was renamed and SKILL.md needs updating, or it belongs to another \
         tool and goes in FOREIGN_FLAGS with that tool named."
    );
}

#[test]
fn the_skill_lists_exactly_the_match_fields_search_emits() {
    let emitted: BTreeSet<String> = fs3_store::SourceKind::ALL
        .into_iter()
        .map(fs3_store::SourceKind::as_str)
        .chain(
            fs3_store::LexicalMatch::ALL
                .into_iter()
                .map(fs3_store::LexicalMatch::match_field),
        )
        .map(str::to_string)
        .collect();
    assert_eq!(
        table_column("match_field"),
        emitted,
        "SKILL.md's `match_field` table must list exactly the values search emits \
         (semantic: fs3_store::SourceKind, lexical: fs3_store::LexicalMatch::match_field)"
    );
}

#[test]
fn the_skill_lists_exactly_the_empty_because_reasons_search_emits() {
    let emitted: BTreeSet<String> = fs3_daemon::search::EmptyBecause::REASONS
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        table_column("empty_because"),
        emitted,
        "SKILL.md's `empty_because` table must list exactly \
         fs3_daemon::search::EmptyBecause::REASONS"
    );
}
