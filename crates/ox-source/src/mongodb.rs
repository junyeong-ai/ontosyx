use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::{ClientOptions, ServerApi, ServerApiVersion};
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_analysis::{
    AnalysisPhase, AnalysisWarning, AnalysisWarningKind, LARGE_SCHEMA_GATE_THRESHOLD, WarningLevel,
};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
    TableProfile,
};

use crate::{AnalysisResult, DataSourceIntrospector};

type ProfileResult = (
    usize,
    String,
    Result<(TableProfile, Vec<AnalysisWarning>), OxError>,
);

/// Default number of documents to sample per collection for schema inference.
const DEFAULT_SAMPLE_SIZE: u64 = 100;
/// Maximum distinct values to collect per field during profiling.
const MAX_DISTINCT_VALUES: usize = 30;
/// Connection timeout for the MongoDB client.
const CONNECT_TIMEOUT_SECS: u64 = 10;
/// Server selection timeout.
const SERVER_SELECTION_TIMEOUT_SECS: u64 = 10;

pub struct MongoIntrospector {
    client: mongodb::Client,
    database: String,
    sample_size: u64,
    /// Collection names discovered via list_collection_names. Populated during
    /// introspect_schema_resilient and used by collect_stats_resilient to
    /// distinguish real collections from synthesized nested-document tables.
    real_collections: std::sync::Mutex<HashSet<String>>,
}

impl MongoIntrospector {
    pub async fn connect(uri: &str, database: &str) -> OxResult<Self> {
        Self::connect_with_sample_size(uri, database, DEFAULT_SAMPLE_SIZE).await
    }

    pub async fn connect_with_sample_size(
        uri: &str,
        database: &str,
        sample_size: u64,
    ) -> OxResult<Self> {
        let mut options = ClientOptions::parse(uri).await.map_err(|e| OxError::Runtime {
            message: format!("Failed to parse MongoDB connection string: {e}"),
        })?;

        options.connect_timeout = Some(Duration::from_secs(CONNECT_TIMEOUT_SECS));
        options.server_selection_timeout = Some(Duration::from_secs(SERVER_SELECTION_TIMEOUT_SECS));
        options.server_api = Some(ServerApi::builder().version(ServerApiVersion::V1).build());
        options.app_name = Some("ontosyx-introspector".to_string());

        let client = mongodb::Client::with_options(options).map_err(|e| OxError::Runtime {
            message: format!("Failed to create MongoDB client: {e}"),
        })?;

        // Verify connectivity by pinging the database
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
            real_collections: std::sync::Mutex::new(HashSet::new()),
        })
    }

    fn db(&self) -> mongodb::Database {
        self.client.database(&self.database)
    }

    // -----------------------------------------------------------------------
    // Schema introspection
    // -----------------------------------------------------------------------

    pub async fn introspect_schema_resilient(
        &self,
    ) -> OxResult<(SourceSchema, Vec<AnalysisWarning>)> {
        let db = self.db();

        // 1. List collections (excluding system collections)
        let collection_names = db
            .list_collection_names()
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to list collections: {e}"),
            })?;

        let mut collection_names: Vec<String> = collection_names
            .into_iter()
            .filter(|name| !name.starts_with("system."))
            .collect();
        collection_names.sort();

        if collection_names.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "No collections found in database '{}'",
                    self.database
                ),
            });
        }

        // Store real collection names for later use in profiling
        {
            let mut lock = self.real_collections.lock().unwrap();
            *lock = collection_names.iter().cloned().collect();
        }

        if collection_names.len() >= LARGE_SCHEMA_GATE_THRESHOLD {
            warn!(
                collection_count = collection_names.len(),
                threshold = LARGE_SCHEMA_GATE_THRESHOLD,
                "Large database detected. Schema inference may take significant time.",
            );
        }

        let mut warnings = Vec::new();

        // 2. Introspect collections concurrently
        let mut futures = FuturesUnordered::new();
        for (idx, name) in collection_names.iter().enumerate() {
            let name = name.clone();
            futures.push(async move {
                let result = self.introspect_collection(&name).await;
                (idx, name, result)
            });
        }

        type CollectionResult = (usize, String, Result<(Vec<SourceTableDef>, Vec<ForeignKeyDef>), OxError>);
        let mut indexed_results: Vec<CollectionResult> = Vec::with_capacity(collection_names.len());
        while let Some(item) = futures.next().await {
            indexed_results.push(item);
        }

        indexed_results.sort_by_key(|(idx, _, _)| *idx);

        let mut tables = Vec::new();
        let mut foreign_keys = Vec::new();
        for (_, coll_name, result) in indexed_results {
            match result {
                Ok((coll_tables, coll_fks)) => {
                    tables.extend(coll_tables);
                    foreign_keys.extend(coll_fks);
                }
                Err(err) => {
                    warn!(collection = %coll_name, error = %err, "Skipping inaccessible collection during schema introspection");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::SchemaIntrospection,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: coll_name,
                        message: err.to_string(),
                    });
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

        // 3. Infer cross-collection ObjectId references
        let collection_set: HashSet<&str> =
            tables.iter().map(|t| t.name.as_str()).collect();
        let objectid_fks = self.infer_objectid_references(&tables, &collection_set);
        foreign_keys.extend(objectid_fks);

        Ok((
            SourceSchema {
                source_type: "mongodb".to_string(),
                tables,
                foreign_keys,
            },
            warnings,
        ))
    }

    /// Sample documents from a collection and infer its schema.
    /// Nested objects are extracted as child "tables" following the same pattern as JSON sources.
    async fn introspect_collection(
        &self,
        collection_name: &str,
    ) -> OxResult<(Vec<SourceTableDef>, Vec<ForeignKeyDef>)> {
        let coll = self.db().collection::<Document>(collection_name);

        // Sample documents using $sample aggregation
        let pipeline = vec![doc! { "$sample": { "size": self.sample_size as i64 } }];
        let mut cursor = coll
            .aggregate(pipeline)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to sample collection '{collection_name}': {e}"),
            })?;

        let mut documents = Vec::new();
        while let Some(result) = cursor.next().await {
            match result {
                Ok(doc) => documents.push(doc),
                Err(e) => {
                    warn!(collection = %collection_name, error = %e, "Error reading document during sampling");
                }
            }
        }

        if documents.is_empty() {
            // Empty collection: create a table with just _id
            return Ok((
                vec![SourceTableDef {
                    name: collection_name.to_string(),
                    columns: vec![SourceColumnDef {
                        name: "_id".to_string(),
                        data_type: "objectId".to_string(),
                        nullable: false,
                    }],
                    primary_key: vec!["_id".to_string()],
                }],
                Vec::new(),
            ));
        }

        let mut tables = Vec::new();
        let mut foreign_keys = Vec::new();

        self.extract_collection_tables(
            collection_name,
            &documents,
            &mut tables,
            &mut foreign_keys,
        );

        Ok((tables, foreign_keys))
    }

    /// Recursively extract table definitions from sampled documents.
    /// Mirrors the JSON source pattern: nested objects become `{parent}_{field}` child tables.
    fn extract_collection_tables(
        &self,
        table_name: &str,
        documents: &[Document],
        tables: &mut Vec<SourceTableDef>,
        foreign_keys: &mut Vec<ForeignKeyDef>,
    ) {
        // Track all fields, their types, and nullability across documents
        let mut field_info: BTreeMap<String, FieldMerge> = BTreeMap::new();
        let mut nested_objects: BTreeMap<String, Vec<Document>> = BTreeMap::new();
        let mut nested_arrays: BTreeMap<String, Vec<Document>> = BTreeMap::new();

        let doc_count = documents.len();

        for doc in documents {
            let mut seen_in_doc = HashSet::new();
            for (key, value) in doc {
                seen_in_doc.insert(key.clone());

                match value {
                    // Nested document -> extract as child table
                    Bson::Document(nested) => {
                        nested_objects
                            .entry(key.clone())
                            .or_default()
                            .push(nested.clone());
                    }
                    // Array of documents -> extract as child table
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
                    // Scalar or scalar array -> track as column
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

            // Fields not present in this document are nullable
            for (key, info) in &mut field_info {
                if !seen_in_doc.contains(key) {
                    // Don't increment seen_count — absence means nullable
                    let _ = info; // just noting the absence
                }
            }
        }

        // Build column definitions
        let mut columns = Vec::new();
        for (field_name, info) in &field_info {
            let data_type = resolve_bson_type(&info.types);
            let nullable = info.seen_count < doc_count;
            columns.push(SourceColumnDef {
                name: field_name.clone(),
                data_type: data_type.to_string(),
                nullable,
            });
        }

        // Primary key: _id is always the PK for top-level collections
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

        // Recursively extract nested object tables
        for (field, child_docs) in &nested_objects {
            let child_table = format!("{table_name}_{field}");
            self.extract_collection_tables(&child_table, child_docs, tables, foreign_keys);

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

        // Recursively extract nested array-of-objects tables
        for (field, child_docs) in &nested_arrays {
            let child_table = format!("{table_name}_{field}");
            self.extract_collection_tables(&child_table, child_docs, tables, foreign_keys);

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

    /// Infer FK relationships from ObjectId fields that likely reference other collections.
    /// A field named `{collection}_id` or `{collection}Id` whose type is ObjectId
    /// is treated as a reference to that collection's `_id` field.
    fn infer_objectid_references(
        &self,
        tables: &[SourceTableDef],
        collection_set: &HashSet<&str>,
    ) -> Vec<ForeignKeyDef> {
        let mut fks = Vec::new();

        for table in tables {
            for col in &table.columns {
                if col.data_type != "objectId" || col.name == "_id" {
                    continue;
                }

                // Try to match field name to a collection:
                // - "user_id" -> "users" (plural) or "user" (singular)
                // - "userId" -> "users" or "user"
                // - "author_id" -> "authors" or "author"
                let base_name = extract_reference_name(&col.name);
                if let Some(base) = base_name {
                    let candidates = [
                        base.clone(),
                        format!("{base}s"),   // singular -> plural
                        base.trim_end_matches('s').to_string(), // plural -> singular
                    ];

                    for candidate in &candidates {
                        if collection_set.contains(candidate.as_str())
                            && candidate != &table.name
                        {
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

    // -----------------------------------------------------------------------
    // Data profiling
    // -----------------------------------------------------------------------

    pub async fn collect_stats_resilient(
        &self,
        schema: &SourceSchema,
    ) -> OxResult<(SourceProfile, Vec<AnalysisWarning>)> {
        // Only profile top-level collections (not child "tables" from nested docs).
        // Child tables were profiled from sampled data during introspection.
        let real_colls = self.real_collections.lock().unwrap().clone();

        let mut futures = FuturesUnordered::new();
        for (idx, table) in schema.tables.iter().enumerate() {
            let is_top_level = real_colls.contains(&table.name);
            futures.push(async move {
                let result = if is_top_level {
                    self.profile_collection(&table.name, &table.columns).await
                } else {
                    // Child tables from nested documents: provide stub profile
                    Ok((
                        TableProfile {
                            table_name: table.name.clone(),
                            row_count: 0,
                            column_stats: Vec::new(),
                        },
                        Vec::new(),
                    ))
                };
                (idx, table.name.clone(), result)
            });
        }

        let mut indexed_results: Vec<ProfileResult> = Vec::with_capacity(schema.tables.len());
        while let Some(item) = futures.next().await {
            indexed_results.push(item);
        }

        indexed_results.sort_by_key(|(idx, _, _)| *idx);

        let mut table_profiles = Vec::new();
        let mut warnings = Vec::new();

        for (_, coll_name, result) in indexed_results {
            match result {
                Ok((table_profile, mut coll_warnings)) => {
                    table_profiles.push(table_profile);
                    warnings.append(&mut coll_warnings);
                }
                Err(err) => {
                    warn!(collection = %coll_name, error = %err, "Skipping collection during data profiling");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::DataProfiling,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: coll_name,
                        message: err.to_string(),
                    });
                }
            }
        }

        if table_profiles.is_empty() && !schema.tables.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "Failed to collect stats for any collection in database '{}'",
                    self.database
                ),
            });
        }

        Ok((SourceProfile { table_profiles }, warnings))
    }

    async fn profile_collection(
        &self,
        collection_name: &str,
        columns: &[SourceColumnDef],
    ) -> OxResult<(TableProfile, Vec<AnalysisWarning>)> {
        let coll = self.db().collection::<Document>(collection_name);

        // Get document count
        let row_count = coll
            .estimated_document_count()
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to count documents in '{collection_name}': {e}"),
            })?;

        // Profile fields by sampling
        let pipeline = vec![doc! { "$sample": { "size": self.sample_size as i64 } }];
        let mut cursor = coll
            .aggregate(pipeline)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to sample '{collection_name}' for profiling: {e}"),
            })?;

        let mut documents = Vec::new();
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result { documents.push(doc) }
        }

        let mut column_stats = Vec::new();
        let mut warnings = Vec::new();

        for col in columns {
            match self.profile_field(collection_name, &col.name, &documents) {
                Ok((stats, sample_warning)) => {
                    column_stats.push(stats);
                    if let Some(w) = sample_warning {
                        warnings.push(w);
                    }
                }
                Err(err) => {
                    warn!(
                        collection = %collection_name,
                        field = %col.name,
                        error = %err,
                        "Skipping field during data profiling"
                    );
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::DataProfiling,
                        kind: AnalysisWarningKind::ColumnSkipped,
                        location: format!("{collection_name}.{}", col.name),
                        message: err.to_string(),
                    });
                }
            }
        }

        Ok((
            TableProfile {
                table_name: collection_name.to_string(),
                row_count,
                column_stats,
            },
            warnings,
        ))
    }

    fn profile_field(
        &self,
        _collection_name: &str,
        field_name: &str,
        documents: &[Document],
    ) -> OxResult<(ColumnStats, Option<AnalysisWarning>)> {
        let mut null_count: u64 = 0;
        let mut distinct_set: HashSet<String> = HashSet::new();
        let mut sample_values: Vec<String> = Vec::new();
        let mut sample_seen: HashSet<String> = HashSet::new();
        let mut min_value: Option<String> = None;
        let mut max_value: Option<String> = None;

        for doc in documents {
            match doc.get(field_name) {
                None | Some(Bson::Null) => {
                    null_count += 1;
                }
                Some(value) => {
                    let str_val = bson_to_string(value);
                    distinct_set.insert(str_val.clone());

                    // Track min/max
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

                    if sample_seen.insert(str_val.clone())
                        && sample_values.len() < MAX_DISTINCT_VALUES
                    {
                        sample_values.push(str_val);
                    }
                }
            }
        }

        let distinct_count = distinct_set.len() as u64;

        // Only keep sample values if distinct count is manageable
        let (final_samples, sample_warning) = if distinct_count > MAX_DISTINCT_VALUES as u64 {
            (Vec::new(), None)
        } else {
            (sample_values, None)
        };

        Ok((
            ColumnStats {
                column_name: field_name.to_string(),
                null_count,
                distinct_count,
                sample_values: final_samples,
                min_value,
                max_value,
            },
            sample_warning,
        ))
    }
}

// ---------------------------------------------------------------------------
// DataSourceIntrospector trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DataSourceIntrospector for MongoIntrospector {
    fn source_type(&self) -> &str {
        "mongodb"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        let (schema, warnings) = self.introspect_schema_resilient().await?;
        for warning in warnings {
            warn!(
                phase = ?warning.phase,
                kind = ?warning.kind,
                location = %warning.location,
                message = %warning.message,
                "MongoDB schema introspection completed with warnings"
            );
        }
        Ok(schema)
    }

    async fn collect_stats(&self, schema: &SourceSchema) -> OxResult<SourceProfile> {
        let (profile, warnings) = self.collect_stats_resilient(schema).await?;
        for warning in warnings {
            warn!(
                phase = ?warning.phase,
                kind = ?warning.kind,
                location = %warning.location,
                message = %warning.message,
                "MongoDB data profiling completed with warnings"
            );
        }
        Ok(profile)
    }

    async fn analyze(&self) -> OxResult<AnalysisResult> {
        let (schema, mut warnings) = self.introspect_schema_resilient().await?;
        let (profile, profile_warnings) = self.collect_stats_resilient(&schema).await?;
        warnings.extend(profile_warnings);
        Ok(AnalysisResult {
            schema,
            profile,
            warnings,
        })
    }
}

// ---------------------------------------------------------------------------
// BSON type helpers
// ---------------------------------------------------------------------------

/// Tracks type occurrences and presence count for a field across sampled documents.
struct FieldMerge {
    /// BSON type name -> occurrence count
    types: BTreeMap<&'static str, usize>,
    /// Number of documents where this field was present (not absent)
    seen_count: usize,
}

/// Map a BSON value to a type name string for schema inference.
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

    // Filter out null — it affects nullable, not the column type
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

    // Numeric promotion: int + double/decimal -> double
    let has_int = non_null.iter().any(|(t, _)| *t == "int");
    let has_double = non_null.iter().any(|(t, _)| *t == "double");
    let has_decimal = non_null.iter().any(|(t, _)| *t == "decimal");
    if non_null.len() == 2 && has_int && (has_double || has_decimal) {
        return "double";
    }

    // Mixed types -> string (safe fallback)
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
        Bson::DateTime(dt) => {
            // Try human-readable format, fall back to millis
            dt.try_to_rfc3339_string().unwrap_or_else(|_| dt.timestamp_millis().to_string())
        }
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
/// - "userId" -> Some("user")
/// - "author" -> None (not an ID-like field)
fn extract_reference_name(field_name: &str) -> Option<String> {
    // Pattern: xxx_id
    if let Some(base) = field_name.strip_suffix("_id")
        && !base.is_empty() {
            return Some(base.to_lowercase());
        }

    // Pattern: xxxId (camelCase)
    if field_name.ends_with("Id") && field_name.len() > 2 {
        let base = &field_name[..field_name.len() - 2];
        if !base.is_empty() {
            // Convert camelCase to lowercase
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
        assert_eq!(bson_to_string(&Bson::Null), "null");
    }
}
