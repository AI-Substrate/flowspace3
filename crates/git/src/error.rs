//! What can go wrong when reading git, named by what failed and where.
//!
//! Every IO variant carries the path it failed on: "permission denied" without
//! a filename is a message that costs an agent a debugging round trip.

use std::path::PathBuf;

/// A failure reading repository identity or worktree state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The path is not inside a git repository, so there is no worktree to
    /// snapshot. Plain folders index by content hash instead (PRD req 23).
    #[error("{path} is not inside a git worktree")]
    NotAWorktree {
        /// The path that was asked about.
        path: PathBuf,
        /// gitoxide's discovery failure, boxed — it is large and this is the
        /// cold path.
        #[source]
        source: Box<gix::discover::Error>,
    },

    /// The repository has no working directory: a bare repository has nothing
    /// on disk to index.
    #[error("{path} is a bare repository: there is no worktree to snapshot")]
    BareRepository {
        /// The path that was asked about.
        path: PathBuf,
    },

    /// The path could not be resolved to a real location.
    #[error("cannot resolve {path}")]
    Realpath {
        /// The path that could not be resolved.
        path: PathBuf,
        #[source]
        source: gix::path::realpath::Error,
    },

    /// The repository's index could not be read.
    #[error("cannot read the git index")]
    Index(#[from] Box<gix::worktree::open_index::Error>),

    /// The worktree walk could not be configured from repository config.
    #[error("cannot read git config for the worktree walk")]
    WalkOptions(#[from] gix::config::boolean::Error),

    /// The worktree walk failed.
    #[error("cannot walk the worktree")]
    Walk(#[from] Box<gix::dirwalk::Error>),

    /// A file could not be read.
    #[error("cannot read {path}")]
    Io {
        /// The file that could not be read.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A file could not be hashed.
    #[error("cannot hash {path}")]
    Hash {
        /// The file that could not be hashed.
        path: PathBuf,
        #[source]
        source: gix::hash::io::Error,
    },

    /// Git handed back something core refuses — a digest that is not one.
    #[error(transparent)]
    Core(#[from] fs3_core::Error),
}

impl From<gix::dirwalk::Error> for Error {
    fn from(source: gix::dirwalk::Error) -> Self {
        Error::Walk(Box::new(source))
    }
}

impl From<gix::worktree::open_index::Error> for Error {
    fn from(source: gix::worktree::open_index::Error) -> Self {
        Error::Index(Box::new(source))
    }
}
