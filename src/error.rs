//! Error types: [`ResolveError`] (the resolver seam) and [`Error`] (the facade).

use crate::cache::{CacheError, QueueError};

/// Errors produced by a [`crate::Resolver`].
///
/// `Network`/`Timeout`/`Disabled` are transient — degrade to seed+cache and
/// retry later ([`ResolveError::is_transient`]). `NotFound`/`Ambiguous`/`Parse`
/// are content misses: the query itself did not resolve, so an identical retry
/// produces the same result. A resolver reports a content miss as one of these
/// variants rather than returning a best-guess result in their place.
///
/// `Clone`/`Eq` let callers (e.g. a durable retry queue) retain the error
/// across attempts without re-running the request.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ResolveError {
    /// Network/transport failure reaching the resolver backend; degrade to
    /// seed+cache.
    #[error("resolver backend unreachable: {0}")]
    Network(String),
    /// The request exceeded the configured timeout (seconds); degrade to
    /// seed+cache.
    #[error("resolver request timed out after {0}s")]
    Timeout(u64),
    /// Online resolution is disabled by configuration; seed+cache only.
    #[error("online resolution is disabled")]
    Disabled,
    /// The query did not resolve to any known object (unknown/garbled).
    #[error("no object resolved for query: {0}")]
    NotFound(String),
    /// The query resolved to multiple distinct physical objects; callers
    /// leave the item unresolved rather than guessing.
    #[error("query '{query}' is ambiguous ({count} distinct objects)")]
    Ambiguous {
        /// The verbatim query.
        query: String,
        /// Number of distinct physical objects matched.
        count: usize,
    },
    /// The backend response could not be parsed into a canonical identity.
    #[error("failed to parse resolver response: {0}")]
    Parse(String),
}

impl ResolveError {
    /// Whether this error is transient (worth retrying later) as opposed to a
    /// content miss (the query itself does not resolve).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout(_) | Self::Disabled)
    }
}

/// Errors surfaced by the [`crate::SimbadResolver`] facade and
/// [`crate::BatchResolver`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A cache backend operation failed.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// A queue backend operation failed.
    #[error(transparent)]
    Queue(#[from] QueueError),
    /// A resolver produced an unexpected error not modelled as an unresolved
    /// outcome (the normal not-found/ambiguous/offline cases are represented by
    /// [`crate::Resolution::Unresolved`], not this variant).
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

#[cfg(test)]
mod tests {
    use super::ResolveError;

    #[test]
    fn transient_variants() {
        assert!(ResolveError::Network("down".to_owned()).is_transient());
        assert!(ResolveError::Timeout(10).is_transient());
        assert!(ResolveError::Disabled.is_transient());
    }

    #[test]
    fn content_miss_variants_are_not_transient() {
        assert!(!ResolveError::NotFound("x".to_owned()).is_transient());
        assert!(!ResolveError::Ambiguous { query: "x".to_owned(), count: 2 }.is_transient());
        assert!(!ResolveError::Parse("bad".to_owned()).is_transient());
    }
}
