//! Cache-first single resolution + sticky user override.

use uuid::Uuid;

use crate::cache::{Cache, CachedTarget, SearchHit};
use crate::error::{Error, ResolveError};
use crate::types::{AliasKind, ResolvedAlias, TargetSource};
use crate::{caldwell, config::ResolverConfig, normalize, Resolver};

/// Why a query could not be resolved to a canonical target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnresolvedReason {
    /// The online resolver was unreachable/timed out/disabled; the caller may
    /// retry later. Cached/known objects still resolve.
    Offline,
    /// The query is unknown (no such object) or malformed.
    Unknown,
    /// The query maps to several distinct physical objects.
    Ambiguous,
}

/// The outcome of a [`SimbadResolver::resolve`] call.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// A canonical target (from cache or freshly resolved + cached).
    Resolved(CachedTarget),
    /// No canonical target; never a fabricated one.
    Unresolved {
        /// The verbatim query.
        query: String,
        /// Why it is unresolved.
        reason: UnresolvedReason,
    },
}

/// The cache-first resolver facade.
///
/// Generic over any [`Resolver`] backend (TAP, Sesame, offline, fake) and any
/// [`Cache`] backend (memory, SQLite, custom).
pub struct SimbadResolver<R: Resolver, C: Cache> {
    resolver: R,
    cache: C,
    config: ResolverConfig,
}

impl<R: Resolver, C: Cache> SimbadResolver<R, C> {
    /// Construct a facade from a resolver, a cache, and config.
    pub fn new(resolver: R, cache: C, config: ResolverConfig) -> Self {
        Self { resolver, cache, config }
    }

    /// Borrow the underlying cache (e.g. for seeding or enumeration).
    pub fn cache(&self) -> &C {
        &self.cache
    }

    /// Borrow the underlying resolver (e.g. to inspect a fake's call count in tests).
    pub fn resolver(&self) -> &R {
        &self.resolver
    }

    /// Local, network-free typeahead search over cached aliases.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, Error> {
        Ok(self.cache.search(query, limit).await?)
    }

    /// Cache-first resolve. On a cache miss and when online is enabled, consult
    /// the resolver, persist the result, and return it; otherwise return a typed
    /// [`Resolution::Unresolved`]. Caldwell designations (`C n`) are translated
    /// to their standard designation first and the original `C n` bound as an
    /// alias. Never fabricates.
    pub async fn resolve(&self, query: &str) -> Result<Resolution, Error> {
        resolve_core(&self.resolver, &self.cache, &self.config, query).await
    }

    /// Bind a chosen canonical target as an authoritative user override, adding
    /// `alias` and making the row sticky (`source = user-override`). Returns the
    /// updated target, or `None` if `target_id` is unknown.
    pub async fn apply_override(
        &self,
        target_id: Uuid,
        alias: &str,
    ) -> Result<Option<CachedTarget>, Error> {
        let Some(existing) = self.cache.get_by_id(target_id).await? else {
            return Ok(None);
        };
        let mut identity = existing.to_identity();
        identity.source = TargetSource::UserOverride;
        if !alias.trim().is_empty() && !identity.aliases.iter().any(|a| a.alias == alias) {
            identity.aliases.push(ResolvedAlias::new(alias.to_owned(), AliasKind::User));
        }
        let (id, _) = self.cache.upsert(&identity, &self.config.namespace).await?;
        Ok(self.cache.get_by_id(id).await?)
    }
}

/// The shared cache-first resolve routine used by both the facade and the batch
/// resolver. Caldwell-translates, checks the cache, then (if online) resolves,
/// persists, and re-reads. Never fabricates.
pub(crate) async fn resolve_core<R: Resolver, C: Cache>(
    resolver: &R,
    cache: &C,
    config: &ResolverConfig,
    query: &str,
) -> Result<Resolution, Error> {
    // Caldwell translation (facade-level so it applies to any resolver).
    let (simbad_query, caldwell_alias) = match caldwell::parse_caldwell_number(query) {
        Some(n) => match caldwell::caldwell_to_designation(n) {
            Some(desig) => (desig.to_owned(), Some(format!("C {n}"))),
            None => {
                return Ok(Resolution::Unresolved {
                    query: query.to_owned(),
                    reason: UnresolvedReason::Unknown,
                })
            }
        },
        None => (query.to_owned(), None),
    };

    // Cache-first: try the original query, then the translated designation.
    for candidate in [normalize::normalize(query), normalize::normalize(&simbad_query)] {
        if candidate.is_empty() {
            continue;
        }
        if let Some(target) = cache.get_by_normalized(&candidate).await? {
            return Ok(Resolution::Resolved(target));
        }
    }

    if !config.online_enabled {
        return Ok(Resolution::Unresolved {
            query: query.to_owned(),
            reason: UnresolvedReason::Offline,
        });
    }

    match resolver.resolve(&simbad_query).await {
        Ok(mut identity) => {
            if let Some(c) = &caldwell_alias {
                if !identity.aliases.iter().any(|a| &a.alias == c) {
                    identity.aliases.push(ResolvedAlias::new(c.clone(), AliasKind::Designation));
                }
            }
            let (id, _) = cache.upsert(&identity, &config.namespace).await?;
            match cache.get_by_id(id).await? {
                Some(target) => Ok(Resolution::Resolved(target)),
                None => Ok(Resolution::Unresolved {
                    query: query.to_owned(),
                    reason: UnresolvedReason::Unknown,
                }),
            }
        }
        Err(e) => Ok(Resolution::Unresolved { query: query.to_owned(), reason: reason_for(&e) }),
    }
}

/// Map a resolver error to an unresolved reason (transient → Offline).
fn reason_for(e: &ResolveError) -> UnresolvedReason {
    if e.is_transient() {
        UnresolvedReason::Offline
    } else if matches!(e, ResolveError::Ambiguous { .. }) {
        UnresolvedReason::Ambiguous
    } else {
        UnresolvedReason::Unknown
    }
}
