use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::source_analysis::ImpliedRelationship;
use crate::source_schema::{ForeignKeyDef, SourceSchema};

/// A cluster of related tables with their internal and cross-cluster FK relationships.
#[derive(Debug, Clone)]
pub struct TableCluster {
    pub id: usize,
    pub tables: Vec<String>,
    pub internal_fks: Vec<ForeignKeyDef>,
    /// FKs where at least one side belongs to a different cluster.
    pub cross_fks: Vec<ForeignKeyDef>,
}

/// Result of table clustering: clusters grouped into parallel execution levels.
/// Each level contains clusters with no unresolved dependencies — safe to run concurrently.
#[derive(Debug, Clone)]
pub struct ClusterPlan {
    pub clusters: Vec<TableCluster>,
    /// Parallel execution levels. Each inner Vec contains cluster IDs that can run concurrently.
    pub levels: Vec<Vec<usize>>,
}

/// Partition source tables into clusters using FK connectivity, then topologically sort.
///
/// Algorithm:
/// 1. Build FK adjacency graph (declared FKs + confirmed implied relationships)
/// 2. Union-Find to find connected components
/// 3. Split oversized components via BFS from highest-degree nodes
/// 4. Topological sort (referenced clusters first) with cycle fallback
pub fn cluster_tables(
    schema: &SourceSchema,
    implied_rels: &[ImpliedRelationship],
    max_cluster_size: usize,
) -> ClusterPlan {
    let table_names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
    if table_names.is_empty() {
        return ClusterPlan {
            clusters: vec![],
            levels: vec![],
        };
    }

    let name_to_idx: HashMap<&str, usize> = table_names
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, i))
        .collect();
    let n = table_names.len();

    // -------------------------------------------------------------------------
    // Step 1: Build undirected FK adjacency graph for connectivity clustering
    // -------------------------------------------------------------------------
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];

    let all_fks = collect_all_fks(&schema.foreign_keys, implied_rels, &name_to_idx);

    for &(from, to) in &all_fks {
        adj[from].insert(to);
        adj[to].insert(from);
    }

    // -------------------------------------------------------------------------
    // Step 2: Union-Find connected components
    // -------------------------------------------------------------------------
    let mut uf = UnionFind::new(n);
    for &(a, b) in &all_fks {
        uf.union(a, b);
    }

    // Group tables by component root (sorted for determinism)
    let mut components: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        components.entry(uf.find(i)).or_default().push(i);
    }
    let mut component_groups: Vec<Vec<usize>> = components.into_values().collect();
    component_groups.sort_by_key(|g| g[0]); // deterministic order

    // -------------------------------------------------------------------------
    // Step 3: Split oversized components + group isolated tables
    // -------------------------------------------------------------------------
    let mut partitions: Vec<Vec<usize>> = Vec::new();

    for group in component_groups {
        if group.len() <= max_cluster_size {
            partitions.push(group);
        } else {
            // BFS-based balanced split from highest-degree hub
            let sub = bfs_split(&group, &adj, max_cluster_size);
            partitions.extend(sub);
        }
    }

    // -------------------------------------------------------------------------
    // Step 4: Build TableCluster structs
    // -------------------------------------------------------------------------
    // Map each table index → partition index
    let mut table_partition: Vec<usize> = vec![0; n];
    for (pid, part) in partitions.iter().enumerate() {
        for &tidx in part {
            table_partition[tidx] = pid;
        }
    }

    let clusters: Vec<TableCluster> = partitions
        .iter()
        .enumerate()
        .map(|(pid, part)| {
            let tables: Vec<String> = part.iter().map(|&i| table_names[i].to_string()).collect();
            let part_set: HashSet<usize> = part.iter().copied().collect();

            let mut internal_fks = Vec::new();
            let mut cross_fks = Vec::new();

            for fk in &schema.foreign_keys {
                let from_idx = name_to_idx.get(fk.from_table.as_str());
                let to_idx = name_to_idx.get(fk.to_table.as_str());
                match (from_idx, to_idx) {
                    (Some(&fi), Some(&ti)) => {
                        let from_in = part_set.contains(&fi);
                        let to_in = part_set.contains(&ti);
                        if from_in && to_in {
                            internal_fks.push(fk.clone());
                        } else if from_in || to_in {
                            cross_fks.push(fk.clone());
                        }
                    }
                    _ => {}
                }
            }
            // Classify confirmed implied rels the same way as declared FKs
            for rel in implied_rels {
                if rel.confidence < 0.8 {
                    continue;
                }
                let from_idx = name_to_idx.get(rel.from_table.as_str());
                let to_idx = name_to_idx.get(rel.to_table.as_str());
                if let (Some(&fi), Some(&ti)) = (from_idx, to_idx) {
                    let from_in = part_set.contains(&fi);
                    let to_in = part_set.contains(&ti);
                    let implied_fk = ForeignKeyDef {
                        from_table: rel.from_table.clone(),
                        from_column: rel.from_column.clone(),
                        to_table: rel.to_table.clone(),
                        to_column: rel.to_column.clone(),
                        inferred: true,
                    };
                    if from_in && to_in {
                        internal_fks.push(implied_fk);
                    } else if from_in || to_in {
                        cross_fks.push(implied_fk);
                    }
                }
            }

            TableCluster {
                id: pid,
                tables,
                internal_fks,
                cross_fks,
            }
        })
        .collect();

    // -------------------------------------------------------------------------
    // Step 5: Compute parallel execution levels
    // -------------------------------------------------------------------------
    let cluster_count = clusters.len();
    let levels = compute_parallel_levels(&clusters, &name_to_idx, &table_partition, cluster_count);

    // Flatten levels into ordered cluster list and reassign IDs
    let id_order: Vec<usize> = levels.iter().flat_map(|l| l.iter().copied()).collect();
    let reordered: Vec<TableCluster> = id_order
        .iter()
        .enumerate()
        .map(|(new_id, &old_id)| {
            let mut c = clusters[old_id].clone();
            c.id = new_id;
            c
        })
        .collect();

    // Remap level indices from old IDs to new IDs
    let old_to_new: HashMap<usize, usize> = id_order
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();
    let remapped_levels: Vec<Vec<usize>> = levels
        .iter()
        .map(|level| level.iter().map(|&old| old_to_new[&old]).collect())
        .collect();

    ClusterPlan {
        clusters: reordered,
        levels: remapped_levels,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Collect all FK edges as (from_idx, to_idx) pairs from declared FKs and confirmed implied rels.
fn collect_all_fks<'a>(
    foreign_keys: &[ForeignKeyDef],
    implied_rels: &[ImpliedRelationship],
    name_to_idx: &HashMap<&'a str, usize>,
) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();

    for fk in foreign_keys {
        if let (Some(&fi), Some(&ti)) = (
            name_to_idx.get(fk.from_table.as_str()),
            name_to_idx.get(fk.to_table.as_str()),
        ) {
            if fi != ti && seen.insert((fi, ti)) {
                edges.push((fi, ti));
            }
        }
    }

    for rel in implied_rels {
        if rel.confidence < 0.8 {
            continue;
        }
        if let (Some(&fi), Some(&ti)) = (
            name_to_idx.get(rel.from_table.as_str()),
            name_to_idx.get(rel.to_table.as_str()),
        ) {
            if fi != ti && seen.insert((fi, ti)) {
                edges.push((fi, ti));
            }
        }
    }

    edges
}

/// BFS-based balanced splitting of a large component.
fn bfs_split(
    group: &[usize],
    adj: &[BTreeSet<usize>],
    max_size: usize,
) -> Vec<Vec<usize>> {
    let group_set: HashSet<usize> = group.iter().copied().collect();
    let mut assigned: HashSet<usize> = HashSet::new();
    let mut partitions: Vec<Vec<usize>> = Vec::new();

    // Precompute degree within the group, then sort descending
    let degrees: HashMap<usize, usize> = group
        .iter()
        .map(|&node| {
            let deg = adj[node].iter().filter(|x| group_set.contains(x)).count();
            (node, deg)
        })
        .collect();

    let mut by_degree: Vec<usize> = group.to_vec();
    by_degree.sort_by(|&a, &b| {
        degrees[&b].cmp(&degrees[&a]).then_with(|| a.cmp(&b))
    });

    for &start in &by_degree {
        if assigned.contains(&start) {
            continue;
        }
        let mut partition = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        assigned.insert(start);
        partition.push(start);

        while let Some(node) = queue.pop_front() {
            if partition.len() >= max_size {
                break;
            }
            for &neighbor in &adj[node] {
                if partition.len() >= max_size {
                    break;
                }
                if group_set.contains(&neighbor) && !assigned.contains(&neighbor) {
                    assigned.insert(neighbor);
                    partition.push(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        partitions.push(partition);
    }

    partitions
}

/// Group clusters into parallel execution levels using Kahn's algorithm.
/// Each level contains clusters with no unresolved dependencies — safe to run concurrently.
/// Returns Vec of levels, each containing cluster indices that can execute in parallel.
fn compute_parallel_levels(
    clusters: &[TableCluster],
    name_to_idx: &HashMap<&str, usize>,
    table_partition: &[usize],
    cluster_count: usize,
) -> Vec<Vec<usize>> {
    // Build DAG: referenced cluster → referencing cluster
    let mut in_degree: Vec<usize> = vec![0; cluster_count];
    let mut dag: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); cluster_count];
    let mut inbound_cross_count: Vec<usize> = vec![0; cluster_count];

    for cluster in clusters {
        for fk in &cluster.cross_fks {
            let from_part = name_to_idx
                .get(fk.from_table.as_str())
                .map(|&i| table_partition[i]);
            let to_part = name_to_idx
                .get(fk.to_table.as_str())
                .map(|&i| table_partition[i]);

            if let (Some(fp), Some(tp)) = (from_part, to_part) {
                if fp != tp {
                    if dag[tp].insert(fp) {
                        in_degree[fp] += 1;
                    }
                    inbound_cross_count[tp] += 1;
                }
            }
        }
    }

    // Level-by-level Kahn's: collect all in_degree=0 nodes as one level,
    // then reduce in_degrees and repeat.
    let mut levels: Vec<Vec<usize>> = Vec::new();
    let mut processed = 0;

    loop {
        let mut level: Vec<usize> = (0..cluster_count)
            .filter(|&i| in_degree[i] == 0)
            .collect();

        if level.is_empty() {
            break;
        }

        // Sort within level: most-referenced first, then by index for determinism
        level.sort_by(|&a, &b| {
            inbound_cross_count[b]
                .cmp(&inbound_cross_count[a])
                .then_with(|| a.cmp(&b))
        });

        // Mark processed by setting in_degree to sentinel
        for &node in &level {
            in_degree[node] = usize::MAX;
            for &succ in &dag[node] {
                if in_degree[succ] != usize::MAX {
                    in_degree[succ] -= 1;
                }
            }
        }

        processed += level.len();
        levels.push(level);
    }

    // Handle cycles: remaining nodes as final level
    if processed < cluster_count {
        let mut remaining: Vec<usize> = (0..cluster_count)
            .filter(|&i| in_degree[i] != usize::MAX)
            .collect();
        remaining.sort_by(|&a, &b| {
            inbound_cross_count[b]
                .cmp(&inbound_cross_count[a])
                .then_with(|| a.cmp(&b))
        });
        levels.push(remaining);
    }

    levels
}

// ---------------------------------------------------------------------------
// Union-Find with path compression and union by rank
// ---------------------------------------------------------------------------

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_schema::SourceTableDef;

    fn make_schema(table_names: &[&str], fks: Vec<ForeignKeyDef>) -> SourceSchema {
        SourceSchema {
            source_type: "postgresql".to_string(),
            tables: table_names
                .iter()
                .map(|n| SourceTableDef {
                    name: n.to_string(),
                    columns: vec![],
                    primary_key: vec!["id".to_string()],
                })
                .collect(),
            foreign_keys: fks,
        }
    }

    fn fk(from: &str, to: &str) -> ForeignKeyDef {
        ForeignKeyDef {
            from_table: from.to_string(),
            from_column: format!("{to}_id"),
            to_table: to.to_string(),
            to_column: "id".to_string(),
            inferred: false,
        }
    }

    #[test]
    fn single_component_fits_in_one_cluster() {
        let schema = make_schema(&["a", "b", "c"], vec![fk("b", "a"), fk("c", "a")]);
        let plan = cluster_tables(&schema, &[],10);
        assert_eq!(plan.clusters.len(), 1);
        assert_eq!(plan.clusters[0].tables.len(), 3);
        assert!(plan.clusters[0].cross_fks.is_empty());
    }

    #[test]
    fn disconnected_components_become_separate_clusters() {
        let schema = make_schema(
            &["a", "b", "c", "d"],
            vec![fk("b", "a"), fk("d", "c")],
        );
        let plan = cluster_tables(&schema, &[],10);
        assert_eq!(plan.clusters.len(), 2);
    }

    #[test]
    fn large_component_gets_split() {
        // Chain: a→b→c→d→e (5 tables, max_cluster_size=3)
        let schema = make_schema(
            &["a", "b", "c", "d", "e"],
            vec![fk("b", "a"), fk("c", "b"), fk("d", "c"), fk("e", "d")],
        );
        let plan = cluster_tables(&schema, &[],3);
        assert!(plan.clusters.len() >= 2);
        // All tables accounted for
        let all: HashSet<String> = plan.clusters
            .iter()
            .flat_map(|c| c.tables.iter().cloned())
            .collect();
        assert_eq!(all.len(), 5);
        // No cluster exceeds max
        for c in &plan.clusters {
            assert!(c.tables.len() <= 3);
        }
    }

    #[test]
    fn isolated_tables_grouped() {
        let schema = make_schema(&["a", "b", "c", "d", "e"], vec![]);
        let plan = cluster_tables(&schema, &[],3);
        let total: usize = plan.clusters.iter().map(|c| c.tables.len()).sum();
        assert_eq!(total, 5);
        for c in &plan.clusters {
            assert!(c.tables.len() <= 3);
        }
    }

    #[test]
    fn topo_sort_referenced_first() {
        // b→a, c→b → expected order: a first (most referenced)
        let schema = make_schema(&["c", "b", "a"], vec![fk("b", "a"), fk("c", "b")]);
        // max_size=1 → each table is its own cluster
        let plan = cluster_tables(&schema, &[],1);
        assert_eq!(plan.clusters.len(), 3);
        // a should be in the first cluster (most referenced)
        assert!(plan.clusters[0].tables.contains(&"a".to_string()));
    }

    #[test]
    fn cross_fks_populated() {
        // Split a→b into separate clusters (max_size=1)
        let schema = make_schema(&["a", "b"], vec![fk("b", "a")]);
        let plan = cluster_tables(&schema, &[],1);
        assert_eq!(plan.clusters.len(), 2);
        let total_cross: usize = plan.clusters.iter().map(|c| c.cross_fks.len()).sum();
        assert!(total_cross > 0);
    }

    #[test]
    fn empty_schema() {
        let schema = make_schema(&[], vec![]);
        let plan = cluster_tables(&schema, &[],10);
        assert!(plan.clusters.is_empty());
    }

    #[test]
    fn all_tables_accounted_for_invariant() {
        // Create a realistic scenario: 20 tables, various FK patterns
        let mut tables = Vec::new();
        let mut fks = Vec::new();
        for i in 0..20 {
            tables.push(format!("t{i}"));
        }
        // Hub: t0 referenced by t1..t10
        for i in 1..=10 {
            fks.push(fk(&format!("t{i}"), "t0"));
        }
        // Chain: t11→t12→t13
        fks.push(fk("t12", "t11"));
        fks.push(fk("t13", "t12"));
        // t14..t19 isolated

        let table_strs: Vec<&str> = tables.iter().map(|s| s.as_str()).collect();
        let schema = make_schema(&table_strs, fks);
        let plan = cluster_tables(&schema, &[],5);

        let all: HashSet<String> = plan.clusters
            .iter()
            .flat_map(|c| c.tables.iter().cloned())
            .collect();
        assert_eq!(all.len(), 20, "All tables must be accounted for");

        for c in &plan.clusters {
            assert!(c.tables.len() <= 5, "No cluster exceeds max_cluster_size");
        }
    }
}
