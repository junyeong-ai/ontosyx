//! MongoDB data source adapter.
//!
//! Unlike SQL engines, MongoDB has no schema catalog — every primitive
//! is ultimately driven by document sampling. A naive implementation
//! that re-samples for every primitive call would do redundant I/O and
//! produce inconsistent results across calls (each `$sample` draws a
//! different document subset).
//!
//! The adapter therefore materialises a **single source-of-truth**
//! snapshot on first primitive access and serves every subsequent call
//! from it:
//!
//! - `list_tables()` triggers the one-shot sample, returning both real
//!   collection names and any synthesised child-table names from nested
//!   documents.
//! - `describe_table()` / `count_rows()` / `sample_column()` read from
//!   the snapshot's pre-computed structures.
//! - `list_foreign_keys()` returns the FK inferences computed during
//!   snapshotting (`objectId` references + nested-doc parent-child
//!   edges).
//!
//! The snapshot is built exactly once per adapter instance. A fresh
//! introspection requires a fresh adapter — the `IntrospectionKernel`
//! cache + TTL is the right level at which to invalidate.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use futures::StreamExt;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::{ClientOptions, ServerApi, ServerApiVersion};
use tokio::sync::OnceCell;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef};

use crate::DataSourceAdapter;

/// Default number of documents sampled per collection for schema inference.
const DEFAULT_SAMPLE_SIZE: u64 = 100;
/// Maximum distinct values per column to retain as samples.
const MAX_DISTINCT_VALUES: usize = 30;

pub struct MongoAdapter {
    client: mongodb::Client,
    database: String,
    sample_size: u64,
    /// Lazily-built snapshot of the database's inferred schema + samples.
    /// Populated on first primitive call, then every primitive reads from it.
    snapshot: OnceCell<Snapshot>,
}

/// Pre-computed result of a single full sampling pass. Each field exists
/// so that primitives can look up answers in O(1) / O(log n) without
/// re-sampling.
struct Snapshot {
    tables: Vec<SourceTableDef>,
    foreign_keys: Vec<ForeignKeyDef>,
    /// Every collection name the server returned. Distinguishes "real"
    /// collections (which can return a document count) from synthesised
    /// child tables (which cannot).
    real_collections: HashSet<String>,
    /// Approximate doc count per real collection, taken from
    /// `estimatedDocumentCount()` during sampling.
    counts_by_collection: HashMap<String, u64>,
    /// Per-table column stats: `(table, column) → ColumnStats`.
    /// Built by scanning the sampled documents once, then cached.
    stats_by_column: HashMap<(String, String), ColumnStats>,
}

impl MongoAdapter {
    pub async fn connect(uri: &str, database: &str) -> OxResult<Self> {
        Self::connect_with_sample_size(uri, database, DEFAULT_SAMPLE_SIZE).await
    }

    pub async fn connect_with_sample_size(
        uri: &str,
        database: &str,
        sample_size: u64,
    ) -> OxResult<Self> {
        Self::connect_with_config(uri, database, sample_size, crate::AdapterConfig::default()).await
    }

    /// Connect with operator-supplied timeouts (MongoDB driver maps only
    /// the `mongo_connect_timeout` and `mongo_server_selection_timeout`
    /// fields of [`crate::AdapterConfig`]; pool size comes from the URI).
    pub async fn connect_with_config(
        uri: &str,
        database: &str,
        sample_size: u64,
        config: crate::AdapterConfig,
    ) -> OxResult<Self> {
        let mut options = ClientOptions::parse(uri)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to parse MongoDB connection string: {e}"),
            })?;

        options.connect_timeout = Some(config.mongo_connect_timeout);
        options.server_selection_timeout = Some(config.mongo_server_selection_timeout);
        options.server_api = Some(ServerApi::builder().version(ServerApiVersion::V1).build());
        options.app_name = Some("ontosyx-introspector".to_string());

        let client = mongodb::Client::with_options(options).map_err(|e| OxError::Runtime {
            message: format!("Failed to create MongoDB client: {e}"),
        })?;

        client
            .database(database)
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to connect to MongoDB database '{database}': {e}"),
            })?;

        info!(database = database, "Connected to MongoDB source");
        Ok(Self {
            client,
            database: database.to_string(),
            sample_size,
            snapshot: OnceCell::new(),
        })
    }

    fn db(&self) -> mongodb::Database {
        self.client.database(&self.database)
    }

    /// Lazily build the full schema + profile snapshot. `OnceCell`
    /// guarantees this runs exactly once — concurrent callers wait for
    /// the in-flight build rather than kicking off duplicate samplings.
    async fn get_snapshot(&self) -> OxResult<&Snapshot> {
        self.snapshot
            .get_or_try_init(|| async { self.build_snapshot().await })
            .await
    }

    async fn build_snapshot(&self) -> OxResult<Snapshot> {
        let db = self.db();

        let raw_names = db
            .list_collection_names()
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to list collections: {e}"),
            })?;

        let mut collection_names: Vec<String> = raw_names
            .into_iter()
            .filter(|name| !name.starts_with("system."))
            .collect();
        collection_names.sort();

        if collection_names.is_empty() {
            return Err(OxError::Runtime {
                message: format!("No collections found in database '{}'", self.database),
            });
        }

        let real_collections: HashSet<String> = collection_names.iter().cloned().collect();

        // Sample each collection + collect its estimated count. Failures
        // return an empty-but-named placeholder so downstream tables can
        // still surface the warning via the kernel.
        let mut tables: Vec<SourceTableDef> = Vec::new();
        let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
        let mut counts: HashMap<String, u64> = HashMap::new();
        let mut stats_by_column: HashMap<(String, String), ColumnStats> = HashMap::new();

        for name in &collection_names {
            match self.sample_collection(name).await {
                Ok(SampledCollection {
                    tables: coll_tables,
                    foreign_keys: coll_fks,
                    count,
                    stats_by_column: coll_stats,
                }) => {
                    tables.extend(coll_tables);
                    foreign_keys.extend(coll_fks);
                    counts.insert(name.clone(), count);
                    stats_by_column.extend(coll_stats);
                }
                Err(err) => {
                    warn!(collection = %name, error = %err, "Skipping inaccessible collection during schema introspection");
                }
            }
        }

        if tables.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "No accessible collections were introspected in database '{}'",
                    self.database
                ),
            });
        }

        // Cross-collection ObjectId references: a field named `user_id`
        // or `userId` whose type is `objectId` is inferred to reference
        // the collection whose name matches `user` / `users`.
        let collection_set: HashSet<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        foreign_keys.extend(infer_objectid_references(&tables, &collection_set));

        Ok(Snapshot {
            tables,
            foreign_keys,
            real_collections,
            counts_by_collection: counts,
            stats_by_column,
        })
    }

    /// Sample a single real collection. Produces every piece of
    /// information the primitives will later need: tables (including
    /// synthesised nested ones), FKs between them, an estimated doc
    /// count, and per-column stats for the sampled documents.
    async fn sample_collection(&self, collection_name: &str) -> OxResult<SampledCollection> {
        let coll = self.db().collection::<Document>(collection_name);

        // Estimated document count — cheap, no full scan.
        let count = coll
            .estimated_document_count()
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to count documents in '{collection_name}': {e}"),
            })?;

        // Draw a fixed-size sample. We use the SAME sample for both
        // schema inference and profiling — consistent view of the data,
        // avoids double the server cost of `$sample`.
        let pipeline = vec![doc! { "$sample": { "size": self.sample_size as i64 } }];
        let mut cursor = coll
            .aggregate(pipeline)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to sample collection '{collection_name}': {e}"),
            })?;

        let mut documents: Vec<Document> = Vec::new();
        while let Some(result) = cursor.next().await {
            match result {
                Ok(doc) => documents.push(doc),
                Err(e) => {
                    warn!(collection = %collection_name, error = %e, "Error reading document during sampling");
                }
            }
        }

        if documents.is_empty() {
            return Ok(SampledCollection {
                tables: vec![SourceTableDef {
                    name: collection_name.to_string(),
                    columns: vec![SourceColumnDef {
                        name: "_id".to_string(),
                        data_type: "objectId".to_string(),
                        nullable: false,
                    }],
                    primary_key: vec!["_id".to_string()],
                }],
                foreign_keys: Vec::new(),
                count,
                stats_by_column: HashMap::new(),
            });
        }

        let mut tables: Vec<SourceTableDef> = Vec::new();
        let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
        extract_tables(collection_name, &documents, &mut tables, &mut foreign_keys);

        // Per-column stats from the same sampled set. Only the top-level
        // collection gets real stats; synthesised child tables (nested
        // docs) keep empty ColumnStats — matches the pre-refactor
        // behaviour where child table profiles were explicit stubs.
        let mut stats_by_column: HashMap<(String, String), ColumnStats> = HashMap::new();
        if let Some(top_table) = tables.iter().find(|t| t.name == collection_name) {
            for col in &top_table.columns {
                let stats = profile_field_over_docs(&col.name, &documents);
                stats_by_column.insert((collection_name.to_string(), col.name.clone()), stats);
            }
        }

        Ok(SampledCollection {
            tables,
            foreign_keys,
            count,
            stats_by_column,
        })
    }
}

struct SampledCollection {
    tables: Vec<SourceTableDef>,
    foreign_keys: Vec<ForeignKeyDef>,
    count: u64,
    stats_by_column: HashMap<(String, String), ColumnStats>,
}

#[async_trait]
impl DataSourceAdapter for MongoAdapter {
    fn source_type(&self) -> &str {
        "mongodb"
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        let snap = self.get_snapshot().await?;
        Ok(snap.tables.iter().map(|t| t.name.clone()).collect())
    }

    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        let snap = self.get_snapshot().await?;
        snap.tables
            .iter()
            .find(|t| t.name == table)
            .cloned()
            .ok_or_else(|| OxError::NotFound {
                entity: format!("mongo table `{table}`"),
            })
    }

    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        let snap = self.get_snapshot().await?;
        // Real collections carry an estimated doc count. Synthesised
        // child tables (nested documents) report 0 — the pre-refactor
        // behaviour preserved.
        if !snap.real_collections.contains(table) {
            return Ok(0);
        }
        Ok(snap
            .counts_by_collection
            .get(table)
            .copied()
            .unwrap_or_default())
    }

    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        let snap = self.get_snapshot().await?;
        if let Some(stats) = snap
            .stats_by_column
            .get(&(table.to_string(), column.name.clone()))
        {
            return Ok(stats.clone());
        }
        // Child tables (nested docs) don't hold per-column samples —
        // return an empty ColumnStats rather than an error so the
        // kernel's per-column loop continues on the next column.
        Ok(ColumnStats {
            column_name: column.name.clone(),
            null_count: 0,
            distinct_count: 0,
            sample_values: Vec::new(),
            min_value: None,
            max_value: None,
        })
    }

    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        let snap = self.get_snapshot().await?;
        Ok(snap.foreign_keys.clone())
    }
}

// ---------------------------------------------------------------------------
// Sampling helpers (free functions, no `self` borrow)
// ---------------------------------------------------------------------------

/// Recursively extract table definitions from sampled documents.
/// Nested objects become `{parent}_{field}` child tables; nested
/// arrays-of-objects become the same. Parent-child relationships land
/// as inferred FKs with `from_column = "(nested in {field})"` — a
/// human-readable marker since there's no physical FK column.
fn extract_tables(
    table_name: &str,
    documents: &[Document],
    tables: &mut Vec<SourceTableDef>,
    foreign_keys: &mut Vec<ForeignKeyDef>,
) {
    let mut field_info: BTreeMap<String, FieldMerge> = BTreeMap::new();
    let mut nested_objects: BTreeMap<String, Vec<Document>> = BTreeMap::new();
    let mut nested_arrays: BTreeMap<String, Vec<Document>> = BTreeMap::new();

    let doc_count = documents.len();

    for doc in documents {
        let mut seen_in_doc = HashSet::new();
        for (key, value) in doc {
            seen_in_doc.insert(key.clone());
            match value {
                Bson::Document(nested) => {
                    nested_objects
                        .entry(key.clone())
                        .or_default()
                        .push(nested.clone());
                }
                Bson::Array(arr) if arr.iter().any(|v| matches!(v, Bson::Document(_))) => {
                    for item in arr {
                        if let Bson::Document(nested) = item {
                            nested_arrays
                                .entry(key.clone())
                                .or_default()
                                .push(nested.clone());
                        }
                    }
                }
                _ => {
                    let bson_type = bson_type_name(value);
                    let entry = field_info.entry(key.clone()).or_insert_with(|| FieldMerge {
                        types: BTreeMap::new(),
                        seen_count: 0,
                    });
                    *entry.types.entry(bson_type).or_insert(0) += 1;
                    entry.seen_count += 1;
                }
            }
        }
    }

    let mut columns: Vec<SourceColumnDef> = Vec::new();
    for (field_name, info) in &field_info {
        columns.push(SourceColumnDef {
            name: field_name.clone(),
            data_type: resolve_bson_type(&info.types).to_string(),
            nullable: info.seen_count < doc_count,
        });
    }

    let primary_key = if columns.iter().any(|c| c.name == "_id") {
        vec!["_id".to_string()]
    } else {
        Vec::new()
    };

    if !columns.is_empty() {
        tables.push(SourceTableDef {
            name: table_name.to_string(),
            columns,
            primary_key: primary_key.clone(),
        });
    }

    let parent_pk = primary_key.first().cloned();

    for (field, child_docs) in &nested_objects {
        let child_table = format!("{table_name}_{field}");
        extract_tables(&child_table, child_docs, tables, foreign_keys);
        if let Some(pk_col) = &parent_pk {
            foreign_keys.push(ForeignKeyDef {
                from_table: child_table,
                from_column: format!("(nested in {field})"),
                to_table: table_name.to_string(),
                to_column: pk_col.clone(),
                inferred: true,
            });
        }
    }

    for (field, child_docs) in &nested_arrays {
        let child_table = format!("{table_name}_{field}");
        extract_tables(&child_table, child_docs, tables, foreign_keys);
        if let Some(pk_col) = &parent_pk {
            foreign_keys.push(ForeignKeyDef {
                from_table: child_table,
                from_column: format!("(nested in {field})"),
                to_table: table_name.to_string(),
                to_column: pk_col.clone(),
                inferred: true,
            });
        }
    }
}

/// Infer FK relationships from ObjectId fields whose names match another
/// collection (`user_id`/`userId` → `user` or `users`).
fn infer_objectid_references(
    tables: &[SourceTableDef],
    collection_set: &HashSet<&str>,
) -> Vec<ForeignKeyDef> {
    let mut fks = Vec::new();
    for table in tables {
        for col in &table.columns {
            if col.data_type != "objectId" || col.name == "_id" {
                continue;
            }
            if let Some(base) = extract_reference_name(&col.name) {
                let candidates = [
                    base.clone(),
                    format!("{base}s"),
                    base.trim_end_matches('s').to_string(),
                ];
                for candidate in &candidates {
                    if collection_set.contains(candidate.as_str()) && candidate != &table.name {
                        fks.push(ForeignKeyDef {
                            from_table: table.name.clone(),
                            from_column: col.name.clone(),
                            to_table: candidate.clone(),
                            to_column: "_id".to_string(),
                            inferred: true,
                        });
                        break;
                    }
                }
            }
        }
    }
    fks
}

/// Compute per-field `ColumnStats` from an already-sampled document set.
/// Pure function on `documents` so it's trivially testable and can run
/// synchronously — no DB round-trip required after the initial sample.
fn profile_field_over_docs(field_name: &str, documents: &[Document]) -> ColumnStats {
    let mut null_count: u64 = 0;
    let mut distinct_set: HashSet<String> = HashSet::new();
    let mut sample_values: Vec<String> = Vec::new();
    let mut sample_seen: HashSet<String> = HashSet::new();
    let mut min_value: Option<String> = None;
    let mut max_value: Option<String> = None;

    for doc in documents {
        match doc.get(field_name) {
            None | Some(Bson::Null) => null_count += 1,
            Some(value) => {
                let str_val = bson_to_string(value);
                distinct_set.insert(str_val.clone());
                match &min_value {
                    None => min_value = Some(str_val.clone()),
                    Some(current) if str_val < *current => min_value = Some(str_val.clone()),
                    _ => {}
                }
                match &max_value {
                    None => max_value = Some(str_val.clone()),
                    Some(current) if str_val > *current => max_value = Some(str_val.clone()),
                    _ => {}
                }
                if sample_seen.insert(str_val.clone()) && sample_values.len() < MAX_DISTINCT_VALUES
                {
                    sample_values.push(str_val);
                }
            }
        }
    }

    let distinct_count = distinct_set.len() as u64;
    let final_samples = if distinct_count > MAX_DISTINCT_VALUES as u64 {
        Vec::new()
    } else {
        sample_values
    };

    ColumnStats {
        column_name: field_name.to_string(),
        null_count,
        distinct_count,
        sample_values: final_samples,
        min_value,
        max_value,
    }
}

// ---------------------------------------------------------------------------
// BSON type helpers
// ---------------------------------------------------------------------------

struct FieldMerge {
    types: BTreeMap<&'static str, usize>,
    seen_count: usize,
}

/// Map a BSON value to a type-name string for schema inference.
fn bson_type_name(value: &Bson) -> &'static str {
    match value {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "document",
        Bson::Boolean(_) => "bool",
        Bson::Null => "null",
        Bson::RegularExpression(_) => "string",
        Bson::JavaScriptCode(_) => "string",
        Bson::JavaScriptCodeWithScope(_) => "string",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "int",
        Bson::Timestamp(_) => "timestamp",
        Bson::Binary(_) => "binary",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        Bson::Symbol(_) => "string",
        Bson::Decimal128(_) => "decimal",
        Bson::Undefined => "null",
        Bson::MaxKey => "string",
        Bson::MinKey => "string",
        Bson::DbPointer(_) => "objectId",
    }
}

/// Resolve the most common BSON type from a frequency map.
/// If types are mixed, falls back to "string" (except numeric promotions).
fn resolve_bson_type(types: &BTreeMap<&'static str, usize>) -> &'static str {
    if types.is_empty() {
        return "string";
    }
    let non_null: Vec<(&'static str, usize)> = types
        .iter()
        .filter(|(t, _)| **t != "null")
        .map(|(t, c)| (*t, *c))
        .collect();
    if non_null.is_empty() {
        return "string";
    }
    if non_null.len() == 1 {
        return non_null[0].0;
    }
    let has_int = non_null.iter().any(|(t, _)| *t == "int");
    let has_double = non_null.iter().any(|(t, _)| *t == "double");
    let has_decimal = non_null.iter().any(|(t, _)| *t == "decimal");
    if non_null.len() == 2 && has_int && (has_double || has_decimal) {
        return "double";
    }
    "string"
}

/// Convert a BSON value to a string representation for profiling.
fn bson_to_string(value: &Bson) -> String {
    match value {
        Bson::String(s) => s.clone(),
        Bson::Int32(n) => n.to_string(),
        Bson::Int64(n) => n.to_string(),
        Bson::Double(n) => n.to_string(),
        Bson::Boolean(b) => b.to_string(),
        Bson::ObjectId(oid) => oid.to_hex(),
        Bson::DateTime(dt) => dt
            .try_to_rfc3339_string()
            .unwrap_or_else(|_| dt.timestamp_millis().to_string()),
        Bson::Null => "null".to_string(),
        Bson::Decimal128(d) => d.to_string(),
        Bson::Binary(b) => format!("<{} bytes>", b.bytes.len()),
        Bson::Array(arr) => format!("[{} items]", arr.len()),
        Bson::Document(_) => "{...}".to_string(),
        Bson::Timestamp(ts) => format!("Timestamp({}, {})", ts.time, ts.increment),
        Bson::RegularExpression(re) => format!("/{}/", re.pattern),
        _ => format!("{value}"),
    }
}

/// Extract a potential collection reference name from a field name.
/// - "user_id" -> Some("user")
/// - "userId"  -> Some("user")
/// - "author"  -> None (not an ID-like field)
fn extract_reference_name(field_name: &str) -> Option<String> {
    if let Some(base) = field_name.strip_suffix("_id")
        && !base.is_empty()
    {
        return Some(base.to_lowercase());
    }
    if field_name.ends_with("Id") && field_name.len() > 2 {
        let base = &field_name[..field_name.len() - 2];
        if !base.is_empty() {
            return Some(base.to_lowercase());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_reference_name_patterns() {
        assert_eq!(extract_reference_name("user_id"), Some("user".to_string()));
        assert_eq!(extract_reference_name("userId"), Some("user".to_string()));
        assert_eq!(
            extract_reference_name("author_id"),
            Some("author".to_string())
        );
        assert_eq!(
            extract_reference_name("authorId"),
            Some("author".to_string())
        );
        assert_eq!(extract_reference_name("_id"), None);
        assert_eq!(extract_reference_name("name"), None);
        assert_eq!(extract_reference_name("id"), None);
    }

    #[test]
    fn resolve_bson_type_single() {
        let mut types = BTreeMap::new();
        types.insert("string", 10);
        assert_eq!(resolve_bson_type(&types), "string");
    }

    #[test]
    fn resolve_bson_type_numeric_promotion() {
        let mut types = BTreeMap::new();
        types.insert("int", 5);
        types.insert("double", 3);
        assert_eq!(resolve_bson_type(&types), "double");
    }

    #[test]
    fn resolve_bson_type_mixed_fallback() {
        let mut types = BTreeMap::new();
        types.insert("string", 5);
        types.insert("int", 3);
        assert_eq!(resolve_bson_type(&types), "string");
    }

    #[test]
    fn resolve_bson_type_ignores_null() {
        let mut types = BTreeMap::new();
        types.insert("int", 5);
        types.insert("null", 2);
        assert_eq!(resolve_bson_type(&types), "int");
    }

    #[test]
    fn bson_type_name_mapping() {
        assert_eq!(bson_type_name(&Bson::String("x".into())), "string");
        assert_eq!(bson_type_name(&Bson::Int32(1)), "int");
        assert_eq!(bson_type_name(&Bson::Int64(1)), "int");
        assert_eq!(bson_type_name(&Bson::Double(1.0)), "double");
        assert_eq!(bson_type_name(&Bson::Boolean(true)), "bool");
        assert_eq!(bson_type_name(&Bson::Null), "null");
        assert_eq!(
            bson_type_name(&Bson::ObjectId(mongodb::bson::oid::ObjectId::new())),
            "objectId"
        );
    }

    #[test]
    fn bson_to_string_formats() {
        assert_eq!(bson_to_string(&Bson::String("hello".into())), "hello");
        assert_eq!(bson_to_string(&Bson::Int32(42)), "42");
        assert_eq!(bson_to_string(&Bson::Int64(123)), "123");
        assert_eq!(bson_to_string(&Bson::Boolean(true)), "true");
    }

    #[test]
    fn profile_field_counts_nulls_and_distincts() {
        let docs = vec![
            doc! { "status": "active" },
            doc! { "status": "active" },
            doc! { "status": "paused" },
            doc! { "other": 1 }, // status absent → null
        ];
        let stats = profile_field_over_docs("status", &docs);
        assert_eq!(stats.null_count, 1);
        assert_eq!(stats.distinct_count, 2);
        assert!(stats.sample_values.iter().any(|v| v == "active"));
        assert!(stats.sample_values.iter().any(|v| v == "paused"));
    }
}
