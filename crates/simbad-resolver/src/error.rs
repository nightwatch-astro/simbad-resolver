//! Facade error type.

use simbad_resolver_cache::{CacheError, QueueError};
use simbad_resolver_core::ResolveError;

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
