//! Cache-first single resolution + sticky user override.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use crate::cache::redb::Store;
use crate::cache::{Cache, CachedTarget, SearchHit, RANK_FUZZY};
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

/// How a [`SimbadResolver`] obtains its cache backend.
///
/// The built-in variants select the redb-backed [`Store`](crate::Store) at
/// construction; [`Custom`](Self::Custom) accepts any [`Cache`]. Backend tuning
/// lives on the variant that owns it (e.g. [`FileCache`]), so the constructor
/// signature never grows.
///
/// There is deliberately no "disabled" variant: the cache is load-bearing (the
/// resolve pipeline returns cached rows), so [`InMemory`](Self::InMemory) is the
/// ephemeral, nothing-persisted choice and is what [`Default`] selects. For a
/// truly uncached one-shot lookup, call a bare [`Resolver`] directly instead of
/// the facade.
pub enum CacheBackend {
    /// Ephemeral in-memory redb store (nothing persisted to disk).
    InMemory,
    /// Durable, file-backed redb store.
    File(FileCache),
    /// A caller-supplied cache backend.
    Custom(Arc<dyn Cache>),
}

impl Default for CacheBackend {
    /// The zero-config default: an ephemeral in-memory store.
    fn default() -> Self {
        Self::InMemory
    }
}

impl CacheBackend {
    /// A file-backed backend at `path` with default options.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(FileCache::new(path))
    }

    /// Wrap a caller-supplied cache backend.
    #[must_use]
    pub fn custom(cache: impl Cache + 'static) -> Self {
        Self::Custom(Arc::new(cache))
    }

    /// Materialise the selection into a shared [`Cache`] handle.
    fn into_cache(self) -> Result<Arc<dyn Cache>, Error> {
        match self {
            Self::InMemory => Ok(Arc::new(Store::in_memory()?.cache())),
            Self::File(f) => Ok(Arc::new(Store::open(f.path)?.cache())),
            Self::Custom(c) => Ok(c),
        }
    }
}

/// Options for the built-in file-backed cache.
///
/// Future tunables (cache size, durability, …) are added here, defaulted —
/// never as new constructor arguments.
#[derive(Clone, Debug)]
pub struct FileCache {
    /// Path to the redb database file (created if missing).
    pub path: PathBuf,
}

impl FileCache {
    /// A file cache at `path` with default options.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// The cache-first resolver facade.
///
/// Generic over any [`Resolver`] backend (TAP, Sesame, offline, fake); the cache
/// backend is chosen at construction via [`CacheBackend`] and type-erased, so
/// the facade type does not carry it.
pub struct SimbadResolver<R: Resolver> {
    resolver: R,
    cache: Arc<dyn Cache>,
    config: ResolverConfig,
}

impl<R: Resolver> SimbadResolver<R> {
    /// Construct a facade from a resolver, a [`CacheBackend`], and config.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if a built-in ([`CacheBackend::InMemory`] /
    /// [`CacheBackend::File`]) store cannot be opened or initialised.
    /// [`CacheBackend::Custom`] never fails here.
    pub fn new(resolver: R, cache: CacheBackend, config: ResolverConfig) -> Result<Self, Error> {
        Ok(Self { resolver, cache: cache.into_cache()?, config })
    }

    /// Borrow the underlying cache (e.g. for seeding or enumeration).
    pub fn cache(&self) -> &dyn Cache {
        self.cache.as_ref()
    }

    /// Borrow the underlying resolver (e.g. to inspect a fake's call count in tests).
    pub fn resolver(&self) -> &R {
        &self.resolver
    }

    /// Local, network-free typeahead search over cached aliases.
    ///
    /// Returns exact/prefix/substring matches (see [`Cache::search`]). When fuzzy
    /// matching is enabled via [`ResolverConfig::with_fuzzy`] and fewer than
    /// `limit` of those are found, the remaining slots are filled with token-set
    /// similarity matches ([`crate::RANK_FUZZY`]), best score first. [`Self::resolve`]
    /// is never affected — it stays exact-normalized and never fabricates.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, Error> {
        let mut hits = self.cache.search(query, limit).await?;

        if let Some(min_score) = self.config.fuzzy_min_score {
            let q = normalize::normalize(query);
            if !q.is_empty() && hits.len() < limit {
                let already: HashSet<Uuid> = hits.iter().map(|h| h.target.id).collect();

                // Score each not-yet-matched target by its best-matching alias.
                let mut scored: Vec<(f32, usize, SearchHit)> = Vec::new();
                for target in self.cache.list().await? {
                    if already.contains(&target.id) {
                        continue;
                    }
                    let mut best: Option<(f32, String, usize)> = None;
                    for alias in &target.aliases {
                        let score = normalize::jaccard_normalized(&q, &alias.normalized);
                        if score >= min_score
                            && best.as_ref().is_none_or(|(prev, _, _)| score > *prev)
                        {
                            best = Some((score, alias.alias.clone(), alias.normalized.len()));
                        }
                    }
                    if let Some((score, matched_alias, normalized_len)) = best {
                        scored.push((
                            score,
                            normalized_len,
                            SearchHit { target, matched_alias, rank: RANK_FUZZY },
                        ));
                    }
                }

                // Best score first; ties break on the shorter matched alias, then
                // the target's primary designation (stable, deterministic order).
                scored.sort_by(|a, b| {
                    b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then_with(|| {
                        a.2.target.primary_designation.cmp(&b.2.target.primary_designation)
                    })
                });
                for (_, _, hit) in scored.into_iter().take(limit - hits.len()) {
                    hits.push(hit);
                }
            }
        }

        Ok(hits)
    }

    /// Cache-first resolve. On a cache miss and when online is enabled, consult
    /// the resolver, persist the result, and return it; otherwise return a typed
    /// [`Resolution::Unresolved`]. Caldwell designations (`C n`) are translated
    /// to their standard designation first and the original `C n` bound as an
    /// alias. Never fabricates.
    pub async fn resolve(&self, query: &str) -> Result<Resolution, Error> {
        resolve_core(&self.resolver, self.cache.as_ref(), &self.config, query).await
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
pub(crate) async fn resolve_core<R: Resolver, C: Cache + ?Sized>(
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
