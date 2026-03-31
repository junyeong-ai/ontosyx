use tracing::info;

use ox_core::design_project::{SourceConfig, SourceTypeKind};
use ox_core::source_analysis::SourceAnalysisReport;
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_source::analyzer::build_analysis_report;
use ox_source::registry::{IntrospectorRegistry, SourceInput};

use crate::error::AppError;

use super::super::types::ProjectSource;
use super::fingerprint::{mongodb_fingerprint, mysql_fingerprint, pg_fingerprint, schema_fingerprint};

/// Analyze a source and return (config, raw_data, schema, profile, report).
///
/// Text sources bypass introspection entirely. Structured sources (CSV, JSON,
/// PostgreSQL, and any custom type) are dispatched through the
/// `IntrospectorRegistry`, making it easy to add new source types without
/// modifying this function.
pub(crate) async fn analyze_source(
    source: ProjectSource,
    registry: &IntrospectorRegistry,
) -> Result<
    (
        SourceConfig,
        Option<String>,
        Option<SourceSchema>,
        Option<SourceProfile>,
        Option<SourceAnalysisReport>,
    ),
    AppError,
> {
    match source {
        ProjectSource::Text { data } => {
            if data.trim().is_empty() {
                return Err(AppError::empty_source_data());
            }
            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Text,
                    schema_name: None,
                    source_fingerprint: None,
                },
                Some(data),
                None,
                None,
                None,
            ))
        }
        ProjectSource::Csv { data } => {
            if data.trim().is_empty() {
                return Err(AppError::empty_source_data());
            }
            let introspector = registry
                .create(
                    "csv",
                    SourceInput {
                        data: Some(data.clone()),
                        connection_string: None,
                        schema_name: None,
                    },
                )
                .await
                .ok_or_else(|| AppError::bad_request("CSV source type is not registered"))?
                .map_err(AppError::from)?;
            let analysis = introspector.analyze().await.map_err(AppError::from)?;
            let fingerprint = schema_fingerprint(&analysis.schema);
            let report = build_analysis_report(&analysis.schema, &analysis.profile);
            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Csv,
                    schema_name: None,
                    source_fingerprint: Some(fingerprint),
                },
                Some(data),
                Some(analysis.schema),
                Some(analysis.profile),
                Some(report),
            ))
        }
        ProjectSource::Json { data } => {
            if data.trim().is_empty() {
                return Err(AppError::empty_source_data());
            }
            let introspector = registry
                .create(
                    "json",
                    SourceInput {
                        data: Some(data.clone()),
                        connection_string: None,
                        schema_name: None,
                    },
                )
                .await
                .ok_or_else(|| AppError::bad_request("JSON source type is not registered"))?
                .map_err(AppError::from)?;
            let analysis = introspector.analyze().await.map_err(AppError::from)?;
            let fingerprint = schema_fingerprint(&analysis.schema);
            let report = build_analysis_report(&analysis.schema, &analysis.profile);
            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Json,
                    schema_name: None,
                    source_fingerprint: Some(fingerprint),
                },
                Some(data),
                Some(analysis.schema),
                Some(analysis.profile),
                Some(report),
            ))
        }
        ProjectSource::CodeRepository { .. } => {
            // CodeRepository analysis requires LLM calls (Brain) and repo infrastructure,
            // which are not available in this function. It is handled directly by the
            // create_project / reanalyze_project handlers.
            Err(AppError::bad_request(
                "CodeRepository source must be handled by the project lifecycle handler",
            ))
        }
        ProjectSource::Postgresql {
            connection_string,
            schema,
        } => {
            info!(schema = %schema, "Connecting to PostgreSQL source");
            let fingerprint = pg_fingerprint(&connection_string, &schema);

            let introspector = registry
                .create(
                    "postgresql",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(schema.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::bad_request("PostgreSQL source type is not registered"))?
                .map_err(AppError::from)?;

            let analysis = introspector.analyze().await.map_err(AppError::from)?;

            let report = build_analysis_report(&analysis.schema, &analysis.profile)
                .with_analysis_warnings(analysis.warnings);

            info!(
                tables = analysis.schema.tables.len(),
                fks = analysis.schema.foreign_keys.len(),
                partial = report.is_partial(),
                "PostgreSQL source introspected"
            );

            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Postgresql,
                    schema_name: Some(schema),
                    source_fingerprint: Some(fingerprint),
                },
                None, // PG: no raw data stored, regenerated from schema+profile
                Some(analysis.schema),
                Some(analysis.profile),
                Some(report),
            ))
        }
        ProjectSource::Mysql {
            connection_string,
            schema,
        } => {
            info!(database = %schema, "Connecting to MySQL source");
            let fingerprint = mysql_fingerprint(&connection_string, &schema);

            let introspector = registry
                .create(
                    "mysql",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(schema.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::bad_request("MySQL source type is not registered"))?
                .map_err(AppError::from)?;

            let analysis = introspector.analyze().await.map_err(AppError::from)?;

            let report = build_analysis_report(&analysis.schema, &analysis.profile)
                .with_analysis_warnings(analysis.warnings);

            info!(
                tables = analysis.schema.tables.len(),
                fks = analysis.schema.foreign_keys.len(),
                partial = report.is_partial(),
                "MySQL source introspected"
            );

            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Mysql,
                    schema_name: Some(schema),
                    source_fingerprint: Some(fingerprint),
                },
                None, // MySQL: no raw data stored, regenerated from schema+profile
                Some(analysis.schema),
                Some(analysis.profile),
                Some(report),
            ))
        }
        ProjectSource::Mongodb {
            connection_string,
            database,
        } => {
            info!(database = %database, "Connecting to MongoDB source");
            let fingerprint = mongodb_fingerprint(&connection_string, &database);

            let introspector = registry
                .create(
                    "mongodb",
                    SourceInput {
                        data: None,
                        connection_string: Some(connection_string),
                        schema_name: Some(database.clone()),
                    },
                )
                .await
                .ok_or_else(|| AppError::bad_request("MongoDB source type is not registered"))?
                .map_err(AppError::from)?;

            let analysis = introspector.analyze().await.map_err(AppError::from)?;

            let report = build_analysis_report(&analysis.schema, &analysis.profile)
                .with_analysis_warnings(analysis.warnings);

            info!(
                collections = analysis.schema.tables.len(),
                fks = analysis.schema.foreign_keys.len(),
                partial = report.is_partial(),
                "MongoDB source introspected"
            );

            Ok((
                SourceConfig {
                    source_type: SourceTypeKind::Mongodb,
                    schema_name: Some(database),
                    source_fingerprint: Some(fingerprint),
                },
                None, // MongoDB: no raw data stored, regenerated from schema+profile
                Some(analysis.schema),
                Some(analysis.profile),
                Some(report),
            ))
        }
    }
}
