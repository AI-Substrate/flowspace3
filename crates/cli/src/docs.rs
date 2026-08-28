//! Documentation bundled into the binary (PRD reqs 44, 45).
//!
//! `flowspace3 docs list` and `flowspace3 docs get <topic>` answer offline,
//! with no daemon, no network and no files on disk. That is the whole point:
//! an agent that has just installed fs3 can ask fs3 how to use fs3, before the
//! stack is up and before it knows whether it is up.
//!
//! # Why the pages live in this crate
//!
//! `include_str!` resolves relative to the source file, so `crates/cli/docs/`
//! travels with the crate. Pointing at the repository's `docs/` would compile
//! on a developer's machine and break the moment the crate is vendored or
//! published — the bundle has to be part of the package, not near it.
//!
//! # These are not copies
//!
//! `docs/services/*.md` stay the long-form pages a human reads while working on
//! fs3. These are condensed for an agent that wants an answer in its context
//! window: the loop, the shapes, and the things that will otherwise cost a
//! wrong turn. Duplicating the long pages here would be a second copy nobody
//! updates; condensing them is a different artifact with a different job.
//!
//! # Keeping them honest
//!
//! Self-teaching docs that teach the wrong thing are worse than none, because
//! the reader trusts them. `tests/docs_bundle.rs` extracts every
//! `flowspace3 <verb>` string in the bundle and fails if it is not a real
//! subcommand, so a renamed verb cannot leave the pages quietly lying.

use fs3_core::catalog;
use fs3_core::envelope::{Envelope, Failure};
use serde::{Deserialize, Serialize};

/// One bundled page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Topic {
    /// The name `docs get` takes.
    pub name: &'static str,
    /// One line for `docs list`.
    pub title: &'static str,
    /// The page itself, compiled in.
    pub text: &'static str,
    /// Topics a reader of this one usually wants next.
    pub related: &'static [&'static str],
}

/// Every bundled topic, in the order `docs list` prints them.
///
/// Ordered by the sequence someone actually needs them in — install, then the
/// operating loop, then the two configuration subjects — rather than
/// alphabetically. `agents` is first because it is the one page that answers
/// "what is this and how do I drive it" in full (PRD req 45).
pub const TOPICS: &[Topic] = &[
    Topic {
        name: "agents",
        title: "The agent operating guide: the loop, the envelope, the gotchas",
        text: include_str!("../docs/agents.md"),
        related: &["search", "read", "conversations", "doctor", "config"],
    },
    Topic {
        name: "install",
        title: "Install and first run",
        text: include_str!("../docs/install.md"),
        related: &["doctor", "config"],
    },
    Topic {
        name: "doctor",
        title: "What doctor checks, what it repairs, and what it refuses to",
        text: include_str!("../docs/doctor.md"),
        related: &["install", "daemon"],
    },
    Topic {
        name: "daemon",
        title: "Running the indexer: boot, the queue, and its log stream",
        text: include_str!("../docs/daemon.md"),
        related: &["doctor", "config"],
    },
    Topic {
        name: "search",
        title: "The query surface: flags, hit shape, and how ranking works",
        text: include_str!("../docs/search.md"),
        related: &["agents", "providers"],
    },
    Topic {
        name: "ddocs",
        title: "Deterministic-document rows: filters, addresses, and state truth",
        text: include_str!("../docs/ddocs.md"),
        related: &["search", "read", "agents"],
    },
    Topic {
        name: "read",
        title: "Fetch by address: get, tree, and what scoping means",
        text: include_str!("../docs/read.md"),
        related: &["search", "agents"],
    },
    Topic {
        name: "conversations",
        title: "Storing the WHY: importing transcripts, and asking for them later",
        text: include_str!("../docs/conversations.md"),
        related: &["search", "read", "agents"],
    },
    Topic {
        name: "config",
        title: "Configuration files, layering, and the secrets chain",
        text: include_str!("../docs/config.md"),
        related: &["providers", "install"],
    },
    Topic {
        name: "providers",
        title: "The provider registry: fake, OpenAI, Azure, and the row key",
        text: include_str!("../docs/providers.md"),
        related: &["config", "search"],
    },
];

/// A topic as `docs list` reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicSummary {
    /// The name to pass to `docs get`.
    pub name: String,
    /// One line describing it.
    pub title: String,
    /// Size of the page, so a caller can budget its context before fetching.
    pub bytes: usize,
}

/// What `docs list` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicList {
    /// Every bundled topic.
    pub topics: Vec<TopicSummary>,
}

/// What `docs get` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicPage {
    /// The topic that was asked for.
    pub topic: String,
    /// Its one-line description.
    pub title: String,
    /// The whole page, as markdown.
    ///
    /// One string, not chunked and not paginated: the consumer is an agent that
    /// wants the topic in its context, and making it fetch pages would be a
    /// worse version of the file read it is avoiding.
    pub text: String,
    /// Topics a reader of this one usually wants next, so a caller never has to
    /// guess a topic name.
    pub related: Vec<String>,
}

/// List every bundled topic.
#[must_use]
pub fn list() -> Envelope<TopicList> {
    let topics = TOPICS
        .iter()
        .map(|topic| TopicSummary {
            name: topic.name.to_string(),
            title: topic.title.to_string(),
            bytes: topic.text.len(),
        })
        .collect();

    Envelope::ok("docs", TopicList { topics }).with_next_action(
        "`flowspace3 docs get agents` is the whole operating guide; the others are detail",
    )
}

/// Fetch one bundled topic.
///
/// An unknown name is a 404 whose `fix` LISTS the valid ones — the whole point
/// of a fixed topic set is that the available names are knowable, and making a
/// caller guess twice is the failure this avoids.
#[must_use]
pub fn get(name: &str) -> Envelope<TopicPage> {
    let wanted = name.trim();
    let Some(topic) = TOPICS.iter().find(|topic| topic.name == wanted) else {
        let available: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();
        return Envelope::failed(
            "docs",
            Failure::new(
                &catalog::USAGE_TOPIC_NOT_FOUND,
                format!("no bundled topic named {wanted:?}"),
            )
            .with_fix(format!(
                "pick one of: {}. `flowspace3 docs list` prints them with descriptions.",
                available.join(", ")
            ))
            .with_detail("available", available),
        );
    };

    Envelope::ok(
        "docs",
        TopicPage {
            topic: topic.name.to_string(),
            title: topic.title.to_string(),
            text: topic.text.to_string(),
            related: topic
                .related
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        },
    )
    .with_next_action(format!("related topics: {}", topic.related.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_is_listed_with_a_real_size() {
        let listed = list().data.expect("list always succeeds");
        assert_eq!(listed.topics.len(), TOPICS.len());
        for summary in &listed.topics {
            assert!(
                summary.bytes > 500,
                "{} is suspiciously short at {} bytes — a stub page is worse than \
                 no page, because a reader trusts it",
                summary.name,
                summary.bytes
            );
            assert!(!summary.title.is_empty());
        }
    }

    #[test]
    fn a_known_topic_comes_back_whole() {
        let page = get("agents").data.expect("agents is bundled");
        assert_eq!(page.topic, "agents");
        assert!(page.text.contains("flowspace3 search"));
        assert!(!page.related.is_empty(), "every page points somewhere next");
    }

    /// The point of a fixed topic set is that the valid names are knowable, so
    /// a wrong guess must not require a second one.
    #[test]
    fn an_unknown_topic_is_a_404_that_lists_the_real_ones() {
        let answer = get("embeddings");
        assert!(!answer.ok);
        assert_eq!(answer.http_status(), 404);

        let error = answer.error.expect("a failure carries an error");
        assert_eq!(error.code, "FS3-E-USAGE-TOPIC-NOT-FOUND");
        for topic in TOPICS {
            assert!(
                error.fix.contains(topic.name),
                "the fix must name every available topic, missing {}",
                topic.name
            );
        }
    }

    #[test]
    fn topic_names_are_unique_and_related_links_resolve() {
        let mut names: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two topics share a name");

        for topic in TOPICS {
            for related in topic.related {
                assert!(
                    names.binary_search(related).is_ok(),
                    "{} points at {related:?}, which is not a topic",
                    topic.name
                );
                assert_ne!(&topic.name, related, "{} points at itself", topic.name);
            }
        }
    }
}
