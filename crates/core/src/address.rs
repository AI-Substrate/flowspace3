//! Addresses — the universal currency (workshop 003 D7).
//!
//! ```text
//! el:<repo>/<path>::<container>::<name>   element
//! el:<path>::<container>::<name>          element whose repo is not known
//! conv:<guid>                             conversation
//! conv:<guid>#t<ord>                      one turn
//! ```
//!
//! Every search hit carries one; `get` and `tree` accept them. They are stable
//! across re-parses because an element's own address is path-and-name shaped,
//! never line-numbered.
//!
//! # Why the repo half cannot be split by string surgery
//!
//! A repository identity is `git:<host>/<owner>/<repo>` (see [`crate::git`]),
//! so it CONTAINS the separator that is supposed to divide it from the path:
//!
//! ```text
//! el:git:github.com/AI-Substrate/flowspace3/crates/store/src/lib.rs::migrate
//!    └────────── identity ──────────────┘ └───── element address ─────────┘
//! ```
//!
//! Nothing in the text says where the boundary is. It is resolved against the
//! identities the store actually holds ([`ElementAddress::split`]), longest
//! first, which is why parsing is pure here and resolution takes a repo list.
//!
//! A repo-less `el:<address>` is legal and not a defect: search emits one for
//! content no live path holds any more, and `get` has to be able to eat what
//! search emits.

use std::fmt;

use crate::element::ADDRESS_SEGMENT;

/// The prefix of an element address.
pub const ELEMENT_SCHEME: &str = "el:";

/// The prefix of a conversation address.
pub const CONVERSATION_SCHEME: &str = "conv:";

/// The separator between a conversation and one of its turns: `conv:<guid>#t7`.
pub const TURN_SEPARATOR: &str = "#t";

/// One parsed address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Address {
    /// An element: a file, or something declared inside one.
    Element(ElementAddress),
    /// A conversation, or one turn of it.
    Conversation(ConversationAddress),
}

/// Why a string is not an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// Nothing, or only a scheme.
    Empty,
    /// A scheme fs3 has no verb for.
    UnknownScheme(String),
    /// `conv:<guid>#t<ord>` with an ordinal that is not a number.
    InvalidTurn(String),
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressError::Empty => f.write_str("an address cannot be empty"),
            AddressError::UnknownScheme(text) => write!(
                f,
                "{text:?} is not an fs3 address: it must start with `el:` (an element) or \
                 `conv:` (a conversation)"
            ),
            AddressError::InvalidTurn(text) => {
                write!(f, "{text:?} is not a turn ordinal — expected `#t<number>`")
            }
        }
    }
}

impl std::error::Error for AddressError {}

impl Address {
    /// Parse an address, without resolving it against anything.
    ///
    /// # Errors
    /// [`AddressError`] for an empty address, an unknown scheme, or a
    /// malformed turn ordinal.
    pub fn parse(text: &str) -> Result<Self, AddressError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AddressError::Empty);
        }

        if let Some(rest) = text.strip_prefix(ELEMENT_SCHEME) {
            if rest.is_empty() {
                return Err(AddressError::Empty);
            }
            return Ok(Address::Element(ElementAddress {
                locator: rest.to_string(),
            }));
        }

        if let Some(rest) = text.strip_prefix(CONVERSATION_SCHEME) {
            return ConversationAddress::parse(rest).map(Address::Conversation);
        }

        Err(AddressError::UnknownScheme(text.to_string()))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Address::Element(element) => write!(f, "{ELEMENT_SCHEME}{}", element.locator),
            Address::Conversation(conversation) => write!(f, "{conversation}"),
        }
    }
}

/// Everything after `el:` — a repo identity and an element address, or just an
/// element address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementAddress {
    locator: String,
}

/// An [`ElementAddress`] split against the identities a store holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementParts {
    /// The repository identity, when the address named one that exists.
    pub repo: Option<String>,
    /// The element's own address: `<path>` or `<path>::<container>::<name>`.
    pub element: String,
}

impl ElementParts {
    /// The file path the element lives in — the address up to its first `::`.
    #[must_use]
    pub fn path(&self) -> &str {
        element_path(&self.element)
    }

    /// Whether this address names a whole file rather than something inside it.
    #[must_use]
    pub fn is_whole_file(&self) -> bool {
        !self.element.contains(ADDRESS_SEGMENT)
    }
}

impl ElementAddress {
    /// The raw text after `el:`.
    #[must_use]
    pub fn locator(&self) -> &str {
        &self.locator
    }

    /// Split the locator into repo and element address, using the identities
    /// that actually exist.
    ///
    /// Longest identity first, so `git:host/org/repo` wins over a hypothetical
    /// `git:host/org` that is also registered — the longer one is the more
    /// specific reading, and the shorter one would leave `repo/...` in the
    /// path where no file lives.
    ///
    /// An address matching no identity keeps its whole locator as the element
    /// address with `repo: None`. That is the repo-less form search emits, and
    /// also what a typo produces: the caller resolves the difference by looking
    /// for the path, which is the only thing that can tell them apart.
    #[must_use]
    pub fn split(&self, identities: &[impl AsRef<str>]) -> ElementParts {
        let mut best: Option<&str> = None;
        for identity in identities {
            let identity = identity.as_ref();
            if identity.is_empty() {
                continue;
            }
            let matches = self.locator == identity
                || (self.locator.len() > identity.len()
                    && self.locator.starts_with(identity)
                    && self.locator.as_bytes()[identity.len()] == b'/');
            if matches && best.is_none_or(|current| identity.len() > current.len()) {
                best = Some(identity);
            }
        }

        match best {
            Some(identity) => ElementParts {
                repo: Some(identity.to_string()),
                element: self.locator[identity.len()..]
                    .trim_start_matches('/')
                    .to_string(),
            },
            None => ElementParts {
                repo: None,
                element: self.locator.clone(),
            },
        }
    }
}

/// A conversation, or one turn of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationAddress {
    /// The conversation's guid.
    pub guid: String,
    /// One turn's ordinal, when the address named one.
    pub turn: Option<u32>,
}

impl ConversationAddress {
    fn parse(rest: &str) -> Result<Self, AddressError> {
        if rest.is_empty() {
            return Err(AddressError::Empty);
        }
        match rest.split_once(TURN_SEPARATOR) {
            None => Ok(ConversationAddress {
                guid: rest.to_string(),
                turn: None,
            }),
            Some((guid, ordinal)) => {
                let turn = ordinal
                    .parse::<u32>()
                    .map_err(|_| AddressError::InvalidTurn(rest.to_string()))?;
                Ok(ConversationAddress {
                    guid: guid.to_string(),
                    turn: Some(turn),
                })
            }
        }
    }
}

impl fmt::Display for ConversationAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{CONVERSATION_SCHEME}{}", self.guid)?;
        match self.turn {
            Some(turn) => write!(f, "{TURN_SEPARATOR}{turn}"),
            None => Ok(()),
        }
    }
}

/// Render an element address the one way fs3 spells it.
///
/// The element's own address already begins with its repo-relative path, so the
/// identity is the only thing prepended. Content whose checkout is gone keeps a
/// bare `el:<address>`: it is real content, and inventing a repository for it
/// would be a lie.
#[must_use]
pub fn element_address(repo: Option<&str>, address: &str) -> String {
    match repo {
        Some(identity) => format!("{ELEMENT_SCHEME}{identity}/{address}"),
        None => format!("{ELEMENT_SCHEME}{address}"),
    }
}

/// The file path part of an element address — everything before the first `::`.
#[must_use]
pub fn element_path(address: &str) -> &str {
    match address.find(ADDRESS_SEGMENT) {
        Some(at) => &address[..at],
        None => address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITIES: &[&str] = &[
        "git:github.com/AI-Substrate/flowspace3",
        "git:github.com/AI-Substrate",
        "path:/srv/notes",
    ];

    #[test]
    fn an_element_address_parses_and_round_trips() {
        let text = "el:git:github.com/AI-Substrate/flowspace3/crates/store/src/lib.rs::migrate";
        let address = Address::parse(text).expect("an element address");
        assert_eq!(address.to_string(), text);
    }

    /// The whole reason resolution takes a repo list: the identity contains the
    /// separator, so the longest registered identity is the boundary — a
    /// shorter one that also matches would leave `flowspace3/…` in the path,
    /// where no file lives.
    #[test]
    fn the_repo_half_is_the_longest_identity_that_matches() {
        let Address::Element(element) = Address::parse(
            "el:git:github.com/AI-Substrate/flowspace3/crates/store/src/lib.rs::migrate",
        )
        .expect("an element address") else {
            panic!("that is an element address");
        };
        let parts = element.split(IDENTITIES);
        assert_eq!(
            parts.repo.as_deref(),
            Some("git:github.com/AI-Substrate/flowspace3")
        );
        assert_eq!(parts.element, "crates/store/src/lib.rs::migrate");
        assert_eq!(parts.path(), "crates/store/src/lib.rs");
        assert!(!parts.is_whole_file());
    }

    /// A prefix that is not followed by `/` is a different repository, not this
    /// one: `git:github.com/AI-Substrate-Labs/x` must not resolve against
    /// `git:github.com/AI-Substrate`.
    #[test]
    fn an_identity_prefix_only_matches_on_a_segment_boundary() {
        let Address::Element(element) =
            Address::parse("el:git:github.com/AI-Substrate-Labs/x/src/main.rs").expect("parses")
        else {
            panic!("that is an element address");
        };
        assert_eq!(element.split(IDENTITIES).repo, None);
    }

    /// Search emits this form for content no live path holds; `get` has to eat
    /// what search emits.
    #[test]
    fn a_repo_less_address_keeps_its_whole_locator() {
        let Address::Element(element) =
            Address::parse("el:crates/store/src/lib.rs::migrate").expect("parses")
        else {
            panic!("that is an element address");
        };
        let parts = element.split(IDENTITIES);
        assert_eq!(parts.repo, None);
        assert_eq!(parts.element, "crates/store/src/lib.rs::migrate");
    }

    #[test]
    fn a_whole_file_address_says_so() {
        let Address::Element(element) =
            Address::parse("el:git:github.com/AI-Substrate/flowspace3/README.md").expect("parses")
        else {
            panic!("that is an element address");
        };
        let parts = element.split(IDENTITIES);
        assert_eq!(parts.element, "README.md");
        assert!(parts.is_whole_file());
    }

    #[test]
    fn a_repo_address_with_no_path_resolves_to_an_empty_element() {
        let Address::Element(element) =
            Address::parse("el:git:github.com/AI-Substrate/flowspace3").expect("parses")
        else {
            panic!("that is an element address");
        };
        let parts = element.split(IDENTITIES);
        assert_eq!(
            parts.repo.as_deref(),
            Some("git:github.com/AI-Substrate/flowspace3")
        );
        assert_eq!(parts.element, "");
    }

    #[test]
    fn conversation_addresses_parse_with_and_without_a_turn() {
        let Address::Conversation(conversation) = Address::parse("conv:abc-123").expect("parses")
        else {
            panic!("that is a conversation address");
        };
        assert_eq!(conversation.guid, "abc-123");
        assert_eq!(conversation.turn, None);

        let Address::Conversation(turn) = Address::parse("conv:abc-123#t42").expect("parses")
        else {
            panic!("that is a conversation address");
        };
        assert_eq!(turn.turn, Some(42));
        assert_eq!(turn.to_string(), "conv:abc-123#t42");
    }

    /// The dispatch arm the conversations plan will fill in has to be reachable
    /// as a conversation, not rejected as a parse error — otherwise "not yet"
    /// is indistinguishable from "you typed it wrong".
    #[test]
    fn a_bad_turn_ordinal_is_a_turn_error_not_an_unknown_scheme() {
        assert_eq!(
            Address::parse("conv:abc#tlast"),
            Err(AddressError::InvalidTurn("abc#tlast".to_string()))
        );
    }

    #[test]
    fn anything_without_a_known_scheme_is_refused() {
        assert_eq!(
            Address::parse("crates/store/src/lib.rs"),
            Err(AddressError::UnknownScheme(
                "crates/store/src/lib.rs".to_string()
            ))
        );
        assert_eq!(Address::parse("   "), Err(AddressError::Empty));
        assert_eq!(Address::parse("el:"), Err(AddressError::Empty));
    }

    #[test]
    fn rendering_matches_what_search_emits() {
        assert_eq!(
            element_address(Some("git:host/org/repo"), "src/lib.rs::migrate"),
            "el:git:host/org/repo/src/lib.rs::migrate"
        );
        assert_eq!(
            element_address(None, "src/lib.rs::migrate"),
            "el:src/lib.rs::migrate"
        );
    }
}
