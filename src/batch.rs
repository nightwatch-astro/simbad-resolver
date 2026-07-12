//! Async batch resolver over a pluggable [`Queue`].
//!
//! Drains pending items cache-first then online, distinguishing **transient**
//! failures (kept pending, retried later, attempt budget preserved) from
//! **content** misses (marked unresolved). Processing is sequential within a
//! drain (polite to the upstream service); each pending item is processed at
//! most once per [`BatchResolver::drain`] call.

use std::collections::HashSet;
use std::sync::Arc;

use crate::cache::{Cache, Queue};
use crate::{
    config::ResolverConfig,
    error::Error,
    orchestrate::{resolve_core, Resolution, UnresolvedReason},
    Resolver,
};

/// Summary of a [`BatchResolver::drain`] pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainSummary {
    /// Items resolved to a canonical target this pass.
    pub resolved: usize,
    /// Items marked unresolved (content misses) this pass.
    pub unresolved: usize,
    /// Items left pending after a transient failure (to retry later).
    pub still_pending: usize,
}

/// The async batch resolver. Generic over any [`Resolver`]; the cache and queue
/// backends are type-erased so the type does not carry them.
pub struct BatchResolver<R: Resolver> {
    resolver: R,
    cache: Arc<dyn Cache>,
    queue: Arc<dyn Queue>,
    config: ResolverConfig,
    batch_size: usize,
}

impl<R: Resolver> BatchResolver<R> {
    /// Construct a batch resolver from caller-supplied cache and queue backends
    /// — typically a [`Store`](crate::Store)'s `.cache()` and `.queue()` over
    /// one shared database.
    pub fn new(
        resolver: R,
        cache: impl Cache + 'static,
        queue: impl Queue + 'static,
        config: ResolverConfig,
    ) -> Self {
        Self { resolver, cache: Arc::new(cache), queue: Arc::new(queue), config, batch_size: 16 }
    }

    /// Set how many pending items are claimed per round (min 1).
    #[must_use]
    pub fn with_batch_size(mut self, n: usize) -> Self {
        self.batch_size = n.max(1);
        self
    }

    /// Borrow the underlying queue.
    pub fn queue(&self) -> &dyn Queue {
        self.queue.as_ref()
    }

    /// Enqueue an identifier keyed by an opaque caller id (idempotent).
    pub async fn enqueue(&self, id: &str, query: &str) -> Result<(), Error> {
        self.queue.enqueue(id, query).await?;
        Ok(())
    }

    /// Drain the pending queue once: resolve each pending item cache-first then
    /// online, marking resolved/unresolved or leaving it pending on a transient
    /// failure. Returns a [`DrainSummary`]. Terminates once every currently
    /// pending item has been processed (transiently-failed items are retried on
    /// a subsequent call).
    pub async fn drain(&self) -> Result<DrainSummary, Error> {
        let mut summary = DrainSummary::default();
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            let batch = self.queue.claim_pending(self.batch_size).await?;
            let fresh: Vec<_> = batch.into_iter().filter(|it| !seen.contains(&it.id)).collect();
            if fresh.is_empty() {
                break;
            }
            for item in fresh {
                seen.insert(item.id.clone());
                match resolve_core(&self.resolver, self.cache.as_ref(), &self.config, &item.query)
                    .await?
                {
                    Resolution::Resolved(target) => {
                        self.queue.mark_resolved(&item.id, target.id).await?;
                        summary.resolved += 1;
                    }
                    Resolution::Unresolved { reason: UnresolvedReason::Offline, .. } => {
                        self.queue.release(&item.id).await?;
                        summary.still_pending += 1;
                    }
                    Resolution::Unresolved { .. } => {
                        self.queue.mark_unresolved(&item.id).await?;
                        summary.unresolved += 1;
                    }
                }
            }
        }
        Ok(summary)
    }
}
