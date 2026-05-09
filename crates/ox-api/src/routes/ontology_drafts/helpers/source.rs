use std::sync::Arc;

use tracing::info;

use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_ontology::mapping::refs::SourceId;
use ox_ontology::ontology_draft::{SourceConfig, SourceTypeKind};
use ox_ontology::source_analysis::SourceAnalysisReport;
use ox_source::analyzer::build_analysis_report;
use ox_source::registry::{AdapterRegistry, SourceInput};
use ox_source::{AnalysisResult, AnalyzeSelection, DataSourceAdapter, IntrospectionKernel};

use crate::error::AppError;

use super::super::types::DataSourceSpec;
use super::fingerprint::{
    bigquery_fingerprint, mongodb_fingerprint, mysql_fingerprint, pg_fingerprint,
    schema_fingerprint, snowflake_fingerprint,
};

/// Bundle of derived metadata produced by [`analyze_source`]. Field
/// order mirrors the persistence flow: source identity → raw bytes
/// (when applicable) → schema/profile → ambiguity report.
pub(crate) struct AnalyzedSource {
    pub config: SourceConfig,
    pub raw_data: Option<String>,
    pub schema: Option<SourceSchema>,
    pub profile: Option<SourceProfile>,
    pub report: Option<SourceAnalysisReport>,
}

/// Adapter built from a [`DataSourceSpec`], paired with the partial
/// [`SourceConfig`] derived from the connection (fingerprint resolved
/// later by [`finalize_config`] once schema is known for inline kinds).
pub(crate) struct PreparedAdapter {
    pub adapter: Arc<dyn DataSourceAdapter>,
    /// Source kind, plus `schema_name` and `source_fingerprint` when
    /// they're determinable from the connection alone (database
    /// sources). Inline kinds (CSV/JSON/DuckDB) leave `source_fingerprint`
    /// empty until the schema has been introspected.
    pub config: SourceConfig,
    /// Original raw payload for inline kinds, persisted so the
    /// project row can regenerate the source on demand. `None` for
    /// connection-based kinds.
    pub raw_data: Option<String>,
}

/// Deterministic `(SourceId, source_hash)` pair for analysis-time
/// ambiguity detection. The SourceId is derived through the
/// canonical `SourceId::from_source_config` rule so every call
/// site in the workspace agrees on the `{kind}:{fingerprint}`
/// format.
fn ambiguity_source_handle(kind: &SourceTypeKind, fingerprint: &str) -> (SourceId, String) {
    let config = SourceConfig {
        source_type: kind.clone(),
        schema_name: None,
        source_fingerprint: Some(fingerprint.to_string()),
    };
    (
        SourceId::from_source_config(&config),
        fingerprint.to_string(),
    )
}

/// Build a live adapter for a `DataSourceSpec` without performing any
/// introspection. Used by both the preview endpoint (cheap table
/// listing) and the full analysis flow ([`analyze_source`]).
///
/// `Text` and `CodeRepository` kinds are not introspected via an
/// adapter and are rejected here — the lifecycle handler routes
/// them through their own paths.
pub(crate) async fn build_adapter(
    source: DataSourceSpec,
    registry: &AdapterRegistry,
) -> Result<PreparedAdapter, AppError> {
    match source {
        DataSourceSpec::Text { .. } => Err(AppError::internal(
            "build_adapter called with Text source — Text routes through the project lifecycle path",
        )),
        DataSourceSpec::CodeRepository { .. } => Err(AppError::internal(
            "build_adapter called with CodeRepository source — \
             CodeRepository routes through the project lifecycle path",
        )),

        DataSourceSpec::Csv { data } => {
            if data.trim().is_empty() {
                return Err(AppError::empty_source_data());
            }
            let adapter = registry
                .create(
                    "csv",
                    SourceInput {
                        data: Some(data.clone()),
                        connection_string: None,
                        schema_name: None,
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_csv"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Csv,
                    schema_name: None,
                    source_fingerprint: None,
                },
                raw_data: Some(data),
            })
        }

        DataSourceSpec::Json { data } => {
            if data.trim().is_empty() {
                return Err(AppError::empty_source_data());
            }
            let adapter = registry
                .create(
                    "json",
                    SourceInput {
                        data: Some(data.clone()),
                        connection_string: None,
                        schema_name: None,
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_json"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Json,
                    schema_name: None,
                    source_fingerprint: None,
                },
                raw_data: Some(data),
            })
        }

        DataSourceSpec::Postgresql {
            connection_string,
            schema,
        } => {
            info!(schema = %schema, "Connecting to PostgreSQL source");
            let fingerprint = pg_fingerprint(&connection_string, &schema);
            let adapter = registry
                .create(
                    "postgresql",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(schema.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_postgresql"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Postgresql,
                    schema_name: Some(schema),
                    source_fingerprint: Some(fingerprint),
                },
                raw_data: None,
            })
        }

        DataSourceSpec::Mysql {
            connection_string,
            schema,
        } => {
            info!(database = %schema, "Connecting to MySQL source");
            let fingerprint = mysql_fingerprint(&connection_string, &schema);
            let adapter = registry
                .create(
                    "mysql",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(schema.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_mysql"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Mysql,
                    schema_name: Some(schema),
                    source_fingerprint: Some(fingerprint),
                },
                raw_data: None,
            })
        }

        DataSourceSpec::Mongodb {
            connection_string,
            database,
        } => {
            info!(database = %database, "Connecting to MongoDB source");
            let fingerprint = mongodb_fingerprint(&connection_string, &database);
            let adapter = registry
                .create(
                    "mongodb",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(database.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_mongodb"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Mongodb,
                    schema_name: Some(database),
                    source_fingerprint: Some(fingerprint),
                },
                raw_data: None,
            })
        }

        DataSourceSpec::Snowflake {
            account,
            user,
            password,
            warehouse,
            database,
            schema,
        } => {
            info!(account = %account, database = %database, schema = %schema, "Connecting to Snowflake source");
            let fingerprint = snowflake_fingerprint(&account, &database, &schema);
            let connection_string = format!(
                "snowflake://{account}/{database}/{schema}?user={user}&password={password}&warehouse={warehouse}"
            );
            let adapter = registry
                .create(
                    "snowflake",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(schema.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_snowflake"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Snowflake,
                    schema_name: Some(schema),
                    source_fingerprint: Some(fingerprint),
                },
                raw_data: None,
            })
        }

        DataSourceSpec::Bigquery {
            project_id,
            dataset,
            billing_project_id,
            credentials_path,
        } => {
            info!(
                project_id = %project_id,
                dataset = %dataset,
                billing_project_id = ?billing_project_id,
                "Connecting to BigQuery source"
            );
            let fingerprint = bigquery_fingerprint(&project_id, &dataset);
            let mut connection_string = format!("bigquery://{project_id}/{dataset}");
            let mut params: Vec<(&str, &str)> = Vec::new();
            if let Some(billing) = &billing_project_id {
                params.push(("billing_project_id", billing));
            }
            if let Some(creds) = &credentials_path {
                params.push(("credentials_path", creds));
            }
            if !params.is_empty() {
                let query: String = params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&");
                connection_string.push('?');
                connection_string.push_str(&query);
            }
            let adapter = registry
                .create(
                    "bigquery",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(dataset.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_bigquery"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::Bigquery,
                    schema_name: Some(dataset),
                    source_fingerprint: Some(fingerprint),
                },
                raw_data: None,
            })
        }

        DataSourceSpec::Duckdb { file_path } => {
            info!(file_path = %file_path, "Connecting to DuckDB file source");
            let adapter = registry
                .create(
                    "duckdb",
                    SourceInput {
                        data: Some(file_path.clone()),
                        connection_string: None,
                        schema_name: None,
                    },
                )
                .await
                .ok_or_else(|| AppError::feature_not_configured("source_duckdb"))?
                .map_err(AppError::from)?;
            Ok(PreparedAdapter {
                adapter,
                config: SourceConfig {
                    source_type: SourceTypeKind::DuckDb,
                    schema_name: None,
                    source_fingerprint: None,
                },
                raw_data: None,
            })
        }
    }
}

/// Analyze a source against the user-supplied [`AnalyzeSelection`].
///
/// `Text` sources bypass introspection entirely and are returned with
/// raw data only. Every other kind dispatches through
/// [`build_adapter`] so the same connection logic services both the
/// preview endpoint and the analysis path.
///
/// `baseline` is only consulted when `selection` is
/// [`AnalyzeSelection::Extend`] — the caller passes the project's
/// previously stored `AnalysisResult` so extension dedupes against
/// it.
pub(crate) async fn analyze_source(
    source: DataSourceSpec,
    registry: &AdapterRegistry,
    selection: AnalyzeSelection,
    baseline: Option<&AnalysisResult>,
) -> Result<AnalyzedSource, AppError> {
    if let DataSourceSpec::Text { data } = &source {
        if data.trim().is_empty() {
            return Err(AppError::empty_source_data());
        }
        return Ok(AnalyzedSource {
            config: SourceConfig {
                source_type: SourceTypeKind::Text,
                schema_name: None,
                source_fingerprint: None,
            },
            raw_data: Some(data.clone()),
            schema: None,
            profile: None,
            report: None,
        });
    }

    let prepared = build_adapter(source, registry).await?;
    let analysis = IntrospectionKernel::new(prepared.adapter)
        .analyze(selection, baseline)
        .await
        .map_err(AppError::from)?;

    // Fingerprint for inline kinds (Csv/Json/DuckDb) is resolvable
    // only after introspection — derive it from the schema.
    let fingerprint = prepared
        .config
        .source_fingerprint
        .clone()
        .unwrap_or_else(|| schema_fingerprint(&analysis.schema));

    let (src_id, src_hash) = ambiguity_source_handle(&prepared.config.source_type, &fingerprint);
    let report = build_analysis_report(&src_id, &src_hash, &analysis.schema, &analysis.profile)
        .with_analysis_warnings(analysis.warnings.clone());

    info!(
        source_type = %prepared.config.source_type,
        tables = analysis.schema.tables.len(),
        fks = analysis.schema.foreign_keys.len(),
        "Source introspected"
    );

    Ok(AnalyzedSource {
        config: SourceConfig {
            source_type: prepared.config.source_type,
            schema_name: prepared.config.schema_name,
            source_fingerprint: Some(fingerprint),
        },
        raw_data: prepared.raw_data,
        schema: Some(analysis.schema.clone()),
        profile: Some(analysis.profile.clone()),
        report: Some(report),
    })
}

/// Φ12.4 — persist a [`SourceContractDef`] row per introspected
/// table so the commit-path validator
/// (`OntologyIR::validate_against_source_contracts`) has the
/// authoritative `(source_id, relation, columns, primary_key)`
/// shape to check mappings against.
///
/// No-op on `Text` / `CodeRepository` sources (no `schema` to
/// promote). The store call is workspace-scoped via the calling
/// task's `WORKSPACE_ID` task-local — the route handler that owns
/// the request context is the only valid caller.
///
/// Errors propagate so that introspection success without contract
/// capture cannot silently leave the bank stale. The store layer
/// recomputes the fingerprint server-side, so the inbound
/// `SourceContractDef::new` is canonical by construction.
pub(crate) async fn capture_source_contracts(
    state: &crate::state::AppState,
    analyzed: &AnalyzedSource,
) -> Result<(), AppError> {
    let Some(schema) = analyzed.schema.as_ref() else {
        return Ok(());
    };
    let source_id = SourceId::from_source_config(&analyzed.config);
    for table in &schema.tables {
        let columns: Vec<ox_ontology::ColumnSpec> = table
            .columns
            .iter()
            .map(|c| ox_ontology::ColumnSpec::new(&c.name, &c.data_type, c.nullable))
            .collect();
        let contract = ox_ontology::SourceContractDef::new(
            source_id.clone(),
            &table.name,
            columns,
            table.primary_key.clone(),
        );
        state
            .store
            .upsert_source_contract(&contract)
            .await
            .map_err(AppError::from)?;
    }
    Ok(())
}
