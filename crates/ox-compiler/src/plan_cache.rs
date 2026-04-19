//! Plan cache for compiled queries.
//!
//! The Cypher compile step is cheap per call — format strings and walk a
//! few trees — but in a Foundry-style hot path the same [`QueryIR`] runs
//! thousands of times per minute (dashboards, canned analytical tabs,
//! autopilot queries). Re-compiling the same IR is pure waste, and
//! re-hashing the AST to detect equality is itself the expensive part.
//!
//! [`PlanCache`] wraps any [`GraphCompiler`] and memoizes
//! `compile_query` by a deterministic hash of the input IR. Reads are
//! lock-free via [`DashMap`]; writes are also concurrent. Eviction is a
//! bounded-size, insertion-order sweep: when the cache crosses
//! `capacity`, the oldest 10% of entries are dropped in a single pass.
//! True LRU would require a per-entry timestamp update on every read
//! (doubling the cost we are trying to save); the approximate policy
//! keeps the hot path lock-free and still bounds memory under workloads
//! that churn the key space.
//!
//! # Cache key
//!
//! Keys are a 64-bit SipHash of `serde_json::to_string(query_ir)`. The
//! JSON form deliberately round-trips through serde rather than a
//! `Hash` impl so future IR shape changes (new variants, renamed
//! fields) invalidate the cache on the first deployment — the cached
//! string changes, the hash changes, the old entry is unreachable.
//!
//! HashMap-valued fields (currently just `Analytics.params`) are the
//! one non-deterministic serialization surface; a reorder produces two
//! hashes for the same semantic IR and costs us at most a cache miss.
//! The cost is a single re-compile, not a correctness bug.
//!
//! # Observability
//!
//! `hits` / `misses` counters are atomic and readable via
//! [`PlanCacheStats`]. The runtime caller is expected to surface these
//! on the `/metrics` endpoint — the cache itself has no opinion about
//! where the numbers go.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use ox_core::error::OxResult;
use ox_core::load_plan::LoadPlan;
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::QueryIR;

use crate::{CompiledQuery, GraphCompiler};

/// Default capacity — a few thousand distinct plans fits most
/// dashboard-heavy workloads under ~50 MB of compiled-statement strings
/// (average Cypher statement ≈ 500 bytes).
pub const DEFAULT_PLAN_CACHE_CAPACITY: usize = 4096;

/// Fraction of entries to evict in one pass when the cache overflows.
/// A single-digit percentage amortises the scan cost over many inserts.
const EVICTION_FRACTION_PCT: usize = 10;

/// Wrapper that caches `compile_query` results for an inner compiler.
///
/// Other trait methods (`compile_schema`, `compile_load`, `name`) delegate
/// straight through — schema DDL and batch loads don't benefit from
/// per-call caching (each is emitted once per deploy).
pub struct PlanCache<C> {
    inner: C,
    cache: DashMap<u64, CachedPlan>,
    capacity: usize,
    /// Monotonic counter used as a cheap insertion-order stamp for the
    /// eviction scan. Wraps at `u64::MAX` — in practice a machine
    /// compiling 1M queries/sec would take ~585,000 years to overflow.
    sequence: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

struct CachedPlan {
    compiled: CompiledQuery,
    inserted_seq: u64,
}

/// Read-only snapshot of cache counters. Cheap to produce; safe to
/// expose on the `/metrics` endpoint.
#[derive(Debug, Clone, Copy)]
pub struct PlanCacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl PlanCacheStats {
    /// Ratio of hits to total lookups, or `None` if the cache has never
    /// been queried. A `None` in metrics is cleaner than a divide-by-zero
    /// `0.0` sentinel.
    pub fn hit_rate(self) -> Option<f64> {
        let total = self.hits + self.misses;
        if total == 0 {
            None
        } else {
            Some(self.hits as f64 / total as f64)
        }
    }
}

impl<C> PlanCache<C>
where
    C: GraphCompiler,
{
    /// Wrap `inner` with a cache of the given capacity. A capacity of
    /// 0 disables caching — every call falls through to the inner
    /// compiler. This is useful in tests and benches that want to
    /// exercise the wrapper plumbing without memoization interference.
    pub fn new(inner: C, capacity: usize) -> Self {
        Self {
            inner,
            cache: DashMap::new(),
            capacity,
            sequence: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Wrap `inner` with a cache sized to [`DEFAULT_PLAN_CACHE_CAPACITY`].
    pub fn with_default_capacity(inner: C) -> Self {
        Self::new(inner, DEFAULT_PLAN_CACHE_CAPACITY)
    }

    pub fn stats(&self) -> PlanCacheStats {
        PlanCacheStats {
            entries: self.cache.len(),
            capacity: self.capacity,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }

    /// Clear every entry; leaves counters untouched so long-running
    /// metrics still make sense across invalidations. Intended for
    /// ontology-level invalidation (schema change → every compiled plan
    /// referencing the old labels is stale).
    pub fn invalidate_all(&self) {
        self.cache.clear();
    }

    fn key(query: &QueryIR) -> Option<u64> {
        // JSON round-trip is the one serialization that every IR type
        // already supports. A fallible map lets a caller with an
        // unserializable QueryIR (none today) still compile via the
        // cache-miss path instead of panicking.
        let json = serde_json::to_string(query).ok()?;
        let mut hasher = DefaultHasher::new();
        json.hash(&mut hasher);
        Some(hasher.finish())
    }

    fn maybe_evict(&self) {
        if self.capacity == 0 || self.cache.len() <= self.capacity {
            return;
        }
        // One pass: find the sequence threshold that drops
        // `EVICTION_FRACTION_PCT`% of entries, then `retain`. The scan
        // runs under concurrent writers because `DashMap::retain`
        // shards-lock internally; a racing insert might win against an
        // evict, which only delays reclamation to the next overflow.
        let to_evict = (self.capacity * EVICTION_FRACTION_PCT / 100).max(1);
        let mut seqs: Vec<u64> = self.cache.iter().map(|e| e.value().inserted_seq).collect();
        if seqs.len() <= to_evict {
            return;
        }
        seqs.sort_unstable();
        let threshold = seqs[to_evict - 1];
        let mut removed = 0u64;
        self.cache.retain(|_, v| {
            if v.inserted_seq <= threshold {
                removed += 1;
                false
            } else {
                true
            }
        });
        self.evictions.fetch_add(removed, Ordering::Relaxed);
    }
}

impl<C> GraphCompiler for PlanCache<C>
where
    C: GraphCompiler,
{
    fn compile_schema(&self, ontology: &OntologyIR) -> OxResult<Vec<String>> {
        self.inner.compile_schema(ontology)
    }

    fn compile_query(&self, query: &QueryIR) -> OxResult<CompiledQuery> {
        if self.capacity == 0 {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return self.inner.compile_query(query);
        }

        let Some(key) = Self::key(query) else {
            // Unserializable IR — bypass the cache entirely.
            self.misses.fetch_add(1, Ordering::Relaxed);
            return self.inner.compile_query(query);
        };

        if let Some(entry) = self.cache.get(&key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(entry.compiled.clone());
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        let compiled = self.inner.compile_query(query)?;
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(
            key,
            CachedPlan {
                compiled: compiled.clone(),
                inserted_seq: seq,
            },
        );
        self.maybe_evict();
        Ok(compiled)
    }

    fn compile_load(&self, plan: &LoadPlan) -> OxResult<Vec<String>> {
        self.inner.compile_load(plan)
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cypher::CypherCompiler;
    use ox_core::graph_label::GraphLabel;
    use ox_core::query_ir::{
        GraphPattern, Projection, QUERY_IR_SCHEMA_VERSION, QueryOp,
    };
    use ox_core::variable_name::VariableName;

    fn vn(s: &'static str) -> VariableName {
        VariableName::new(s).expect("test variable")
    }

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label")
    }

    fn simple_query(label: &'static str) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("n"),
                    label: Some(gl(label)),
                    property_filters: vec![],
                }],
                filter: None,
                projections: vec![Projection::Variable {
                    variable: vn("n"),
                    alias: None,
                }],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        }
    }

    #[test]
    fn second_compile_of_same_ir_is_a_hit() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 16);
        let q = simple_query("Person");

        let a = cache.compile_query(&q).expect("compile");
        let b = cache.compile_query(&q).expect("compile");
        assert_eq!(a.statement, b.statement);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1, "second call must hit");
        assert_eq!(stats.misses, 1, "first call is the miss");
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn distinct_irs_produce_distinct_entries() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 16);
        cache.compile_query(&simple_query("Person")).expect("c1");
        cache.compile_query(&simple_query("Company")).expect("c2");
        cache.compile_query(&simple_query("Person")).expect("c3");

        let stats = cache.stats();
        assert_eq!(stats.entries, 2, "two distinct IRs → two entries");
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn capacity_zero_disables_caching() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 0);
        cache.compile_query(&simple_query("Person")).expect("ok");
        cache.compile_query(&simple_query("Person")).expect("ok");
        let stats = cache.stats();
        assert_eq!(stats.hits, 0, "capacity=0 never hits");
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.entries, 0);
    }

    #[test]
    fn overflow_evicts_oldest_entries() {
        // capacity 10 → eviction drops max(1, 10%) = 1 oldest entry per overflow.
        let cache = PlanCache::new(CypherCompiler::neo4j(), 10);
        // 20 distinct queries push through the cache; by the end we must
        // still be at or under capacity.
        for i in 0..20 {
            let label = match i {
                0 => "L0",
                1 => "L1",
                2 => "L2",
                3 => "L3",
                4 => "L4",
                5 => "L5",
                6 => "L6",
                7 => "L7",
                8 => "L8",
                9 => "L9",
                10 => "L10",
                11 => "L11",
                12 => "L12",
                13 => "L13",
                14 => "L14",
                15 => "L15",
                16 => "L16",
                17 => "L17",
                18 => "L18",
                _ => "L19",
            };
            cache.compile_query(&simple_query(label)).expect("compile");
        }

        let stats = cache.stats();
        assert!(
            stats.entries <= 10,
            "cache must respect capacity bound, got {} entries",
            stats.entries
        );
        assert!(stats.evictions > 0, "some entries must have been evicted");
    }

    #[test]
    fn invalidate_all_clears_entries_but_keeps_counters() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 16);
        cache.compile_query(&simple_query("Person")).expect("c1");
        cache.compile_query(&simple_query("Person")).expect("c2");
        let before = cache.stats();
        cache.invalidate_all();
        let after = cache.stats();
        assert_eq!(after.entries, 0);
        assert_eq!(after.hits, before.hits, "counters survive invalidation");
        assert_eq!(after.misses, before.misses);
    }

    #[test]
    fn hit_rate_returns_none_without_traffic() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 16);
        assert!(cache.stats().hit_rate().is_none());
    }

    #[test]
    fn hit_rate_reflects_traffic() {
        let cache = PlanCache::new(CypherCompiler::neo4j(), 16);
        cache.compile_query(&simple_query("Person")).expect("c1");
        cache.compile_query(&simple_query("Person")).expect("c2");
        cache.compile_query(&simple_query("Person")).expect("c3");
        // 1 miss, 2 hits → 0.667 rate
        let rate = cache.stats().hit_rate().expect("rate available");
        assert!((rate - 2.0 / 3.0).abs() < 1e-9, "hit_rate = {rate}");
    }
}
