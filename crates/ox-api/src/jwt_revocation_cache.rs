//! In-memory cache layered in front of the JWT revocation store.
//!
//! `require_auth` runs on every protected request. A round-trip to
//! Postgres for two narrow lookups (`revoked_jwts.find` plus
//! `users.token_version`) on every call would dominate request
//! latency and pin a pool slot per request. The 30-second TTL bounds
//! the worst-case window between an admin clicking "revoke" and the
//! client losing access — the chosen tradeoff between freshness and
//! request throughput.
//!
//! Two independent lookup axes:
//!
//! - **`revoked_jti`** — boolean per-jti, populated on miss. Negative
//!   results (token not revoked) are cached too; that is the entire
//!   point of caching the lookup, since the overwhelming majority of
//!   requests carry an unrevoked token.
//! - **`token_version`** — per-user `i64`, populated on miss. Mirrors
//!   `users.token_version`; bulk invalidation calls
//!   [`invalidate_user`] so the next lookup re-reads from the DB.
//!
//! [`invalidate_jti`] and [`invalidate_user`] are exported so the
//! revoke / role-change handlers can drop stale cache entries
//! immediately, sidestepping the TTL window for the operator who
//! just made the change.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

/// 30-second cache window. See module docs for the freshness vs.
/// throughput tradeoff.
pub const JWT_REVOCATION_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy)]
struct CachedRevocation {
    revoked: bool,
    inserted_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct CachedTokenVersion {
    version: i64,
    inserted_at: Instant,
}

#[derive(Default)]
pub struct JwtRevocationCache {
    revoked: DashMap<Uuid, CachedRevocation>,
    token_versions: DashMap<Uuid, CachedTokenVersion>,
    ttl: Duration,
}

impl JwtRevocationCache {
    pub fn with_default_ttl() -> Arc<Self> {
        Self::new(JWT_REVOCATION_CACHE_TTL)
    }

    pub fn new(ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            revoked: DashMap::new(),
            token_versions: DashMap::new(),
            ttl,
        })
    }

    /// `Some(revoked)` on a hit; `None` when the caller must consult
    /// the store and then fill the cache via [`record_revocation`].
    pub fn lookup_revocation(&self, jti: Uuid) -> Option<bool> {
        let entry = self.revoked.get(&jti)?;
        if entry.inserted_at.elapsed() < self.ttl {
            Some(entry.revoked)
        } else {
            None
        }
    }

    /// Cache the result of a `find_revoked_jwt` lookup. Negative
    /// results (token not revoked) are cached too — that's the
    /// path the overwhelming majority of authed requests take, and
    /// avoiding the round-trip is the entire point of the layer.
    pub fn record_revocation(&self, jti: Uuid, revoked: bool) {
        self.revoked.insert(
            jti,
            CachedRevocation {
                revoked,
                inserted_at: Instant::now(),
            },
        );
    }

    pub fn lookup_token_version(&self, user_id: Uuid) -> Option<i64> {
        let entry = self.token_versions.get(&user_id)?;
        if entry.inserted_at.elapsed() < self.ttl {
            Some(entry.version)
        } else {
            None
        }
    }

    pub fn record_token_version(&self, user_id: Uuid, version: i64) {
        self.token_versions.insert(
            user_id,
            CachedTokenVersion {
                version,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Drop a cached jti entry — call after writing to
    /// `revoked_jwts` so the next request sees the revocation
    /// without waiting out the TTL.
    pub fn invalidate_jti(&self, jti: Uuid) {
        self.revoked.remove(&jti);
    }

    /// Drop a cached `token_version` entry — call after incrementing
    /// `users.token_version` so the next request sees the bulk
    /// invalidation immediately.
    pub fn invalidate_user(&self, user_id: Uuid) {
        self.token_versions.remove(&user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_lookup_misses_until_recorded() {
        let cache = JwtRevocationCache::with_default_ttl();
        let jti = Uuid::new_v4();
        assert!(cache.lookup_revocation(jti).is_none());
        cache.record_revocation(jti, true);
        assert_eq!(cache.lookup_revocation(jti), Some(true));
    }

    #[test]
    fn negative_revocation_results_are_cached() {
        let cache = JwtRevocationCache::with_default_ttl();
        let jti = Uuid::new_v4();
        cache.record_revocation(jti, false);
        // The hot-path token: not revoked, cache hit. Avoiding a DB
        // round-trip on the common case is the whole point.
        assert_eq!(cache.lookup_revocation(jti), Some(false));
    }

    #[test]
    fn token_version_lookup_misses_until_recorded() {
        let cache = JwtRevocationCache::with_default_ttl();
        let user = Uuid::new_v4();
        assert!(cache.lookup_token_version(user).is_none());
        cache.record_token_version(user, 7);
        assert_eq!(cache.lookup_token_version(user), Some(7));
    }

    #[test]
    fn ttl_expiry_falls_through_to_miss() {
        let cache = JwtRevocationCache::new(Duration::from_millis(1));
        let jti = Uuid::new_v4();
        cache.record_revocation(jti, false);
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            cache.lookup_revocation(jti).is_none(),
            "expired entry must surface as a miss so the next lookup \
             refreshes from the store"
        );
    }

    #[test]
    fn invalidate_jti_drops_entry_before_ttl() {
        let cache = JwtRevocationCache::with_default_ttl();
        let jti = Uuid::new_v4();
        cache.record_revocation(jti, false);
        cache.invalidate_jti(jti);
        assert!(cache.lookup_revocation(jti).is_none());
    }

    #[test]
    fn invalidate_user_drops_only_that_users_version() {
        let cache = JwtRevocationCache::with_default_ttl();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        cache.record_token_version(alice, 1);
        cache.record_token_version(bob, 2);
        cache.invalidate_user(alice);
        assert!(cache.lookup_token_version(alice).is_none());
        assert_eq!(
            cache.lookup_token_version(bob),
            Some(2),
            "invalidation must be scoped to the named user"
        );
    }
}
