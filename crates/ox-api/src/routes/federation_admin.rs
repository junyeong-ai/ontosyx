//! Admin routes for the federation (VOL) adapter resolver.
//!
//! Every workspace owns a small registry of `DataSourceAdapter`s,
//! each keyed by a stable `SourceId`. These endpoints are the admin
//! CRUD surface against that registry:
//!
//! - `POST /api/admin/federation/adapters` — register / replace.
//! - `POST /api/admin/federation/adapters/preview` — build the
//!   adapter and return its inferred schema without persisting.
//! - `GET  /api/admin/federation/adapters` — list.
//! - `GET  /api/admin/federation/adapters/{source_id}` — detail.
//! - `DELETE /api/admin/federation/adapters/{source_id}` — deregister.
//! - `POST /api/admin/federation/adapters/refresh` — rehydrate the
//!   live resolver from the persistent store (for out-of-band edits).
//! - `GET  /api/admin/federation/health` — drift snapshot.
//!
//! Registrations are persisted in `data_sources` (see
//! `0011_data_sources.sql`). A restart does not lose them — the first
//! federation query per workspace lazily rehydrates that workspace's
//! resolver from the store via
//! [`crate::federation_resolver::ensure_workspace_resolver`].
//!
//! The wire and stored JSONB shapes are unified — [`RegisterAdapterKind`]
//! and [`crate::credential::Credential`] round-trip through serde
//! unchanged, so the admin API, the preview path, and the hydration
//! path all consume the same type. See
//! [`RegisterAdapterKind::build_adapter`] for the single place that
//! turns a kind + credential into a live adapter.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::credential::{Credential, SecretResolver};
use crate::error::AppError;
use crate::federation_resolver::{
    refresh_workspace_resolver, remove_workspace_adapter, upsert_workspace_adapter,
};
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::FederationState;
use crate::workspace::WorkspaceContext;

use ox_source::DataSourceAdapter;
use ox_source::bigquery::BigQueryAdapter;
use ox_source::mysql::MysqlAdapter;
use ox_source::postgres::PostgresAdapter;
use ox_source::sample::{CsvAdapter, JsonAdapter};
use ox_source::AnalyzeSelection;

// ---------------------------------------------------------------------------
// Wire / stored types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/admin/federation/adapters`.
///
/// Wire shape:
/// ```json
/// {
///   "source_id": "sales_csv",
///   "kind": "csv",
///   "credential": { "kind": "inline", "value": "id,x\n1,2\n" }
/// }
/// ```
///
/// `kind` is the outer tag; the body of each variant carries a
/// `Credential` plus adapter-specific options (schema name,
/// connection extras). The same struct deserialises both fresh API
/// requests and rows replayed from the `data_sources` table, so
/// register / preview / hydrate share the one source of truth.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterAdapterRequest {
    /// Opaque identifier the planner looks up via
    /// `ObjectMappingDef::source_id`. Stable — ontology authors
    /// reference it by string in mappings.
    pub source_id: String,
    /// Adapter kind + its credential + kind-specific options.
    #[serde(flatten)]
    pub kind: RegisterAdapterKind,
}

/// Adapter kind + its credential + any kind-specific options.
///
/// The outer `kind` tag on the wire is a `serde(tag)` discriminator,
/// so `{"kind": "csv", "credential": {...}}` matches `Csv`. Every
/// variant carries a [`Credential`]; a future `vault:` / `aws-sm:`
/// secret scheme is a resolver change, not an enum change.
#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegisterAdapterKind {
    /// Register a CSV payload. Columns / types are inferred from the
    /// header row; everything lives in one implicit `records`
    /// relation (see `ox_source::sample::CsvAdapter`).
    Csv { credential: Credential },
    /// Register a JSON payload. Nested objects / arrays of objects
    /// become child relations per `ox_source::sample::JsonAdapter`.
    Json { credential: Credential },
    /// Register a PostgreSQL adapter. `schema_name` defaults to
    /// `"public"` when omitted — matches the adapter's default.
    Postgres {
        credential: Credential,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_name: Option<String>,
    },
    /// Register a MySQL adapter. `schema_name` is *required* — MySQL
    /// has no default-database equivalent of Postgres' `public`, so
    /// the register call must name the target database up-front.
    Mysql {
        credential: Credential,
        schema_name: String,
    },
    /// Register a BigQuery adapter. The connection string's own
    /// `?credentials_path=...` carries BigQuery's auth path; no
    /// `schema_name` because the dataset is already in the URI.
    Bigquery { credential: Credential },
}

// ---------------------------------------------------------------------------
// Deserializer body types.
//
// One per distinct stored shape (credential-only / postgres / mysql).
// The field types on these drive serde's validation at decode time,
// replacing runtime presence checks:
//
// - `StoredCredOnlyBody` — covers CSV, JSON, BigQuery. One
//   required field.
// - `StoredPostgresBody` — `schema_name: Option<String>`. Optional
//   because `PostgresAdapter::connect` defaults to `"public"` when
//   the field is absent.
// - `StoredMysqlBody` — `schema_name: String`. Required because
//   MySQL has no default-database analogue of Postgres' `public`,
//   and we'd rather surface the missing field as a serde error at
//   decode time than a connect-time failure against the wrong
//   database.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StoredCredOnlyBody {
    credential: Credential,
}

#[derive(Deserialize)]
struct StoredPostgresBody {
    credential: Credential,
    #[serde(default)]
    schema_name: Option<String>,
}

#[derive(Deserialize)]
struct StoredMysqlBody {
    credential: Credential,
    schema_name: String,
}

impl RegisterAdapterKind {
    /// Short lowercase tag matching the `kind` column in
    /// `data_sources`. Exhaustive match — a new variant surfaces as
    /// a compile error here, not as a drift at runtime.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Self::Csv { .. } => "csv",
            Self::Json { .. } => "json",
            Self::Postgres { .. } => "postgres",
            Self::Mysql { .. } => "mysql",
            Self::Bigquery { .. } => "bigquery",
        }
    }

    /// Serialise the variant's body (credential + options, without
    /// the outer `kind` tag) into the JSON shape the `data_sources`
    /// row persists.
    ///
    /// The outer `kind` tag is not emitted here because `data_sources`
    /// stores the tag in a separate column. The JSONB carries only
    /// what the Rust enum variant holds beyond the discriminator.
    ///
    /// Uses `serde_json::json!` macro — every value in these
    /// constructions is trivially serialisable (`String`,
    /// `Credential`'s derive-Serialize of two `String` variants),
    /// so there is no error path to propagate. Postgres splits into
    /// two match arms (`schema_name: Some` / `None`) rather than
    /// conditionally inserting into a `serde_json::Map`, so the
    /// emitted shape matches the deserializer body exactly (absent
    /// field vs explicit `null`).
    pub fn to_stored_config(&self) -> serde_json::Value {
        match self {
            Self::Csv { credential }
            | Self::Json { credential }
            | Self::Bigquery { credential } => {
                serde_json::json!({ "credential": credential })
            }
            Self::Postgres {
                credential,
                schema_name: Some(schema_name),
            } => serde_json::json!({
                "credential": credential,
                "schema_name": schema_name,
            }),
            Self::Postgres {
                credential,
                schema_name: None,
            } => serde_json::json!({ "credential": credential }),
            Self::Mysql {
                credential,
                schema_name,
            } => serde_json::json!({
                "credential": credential,
                "schema_name": schema_name,
            }),
        }
    }

    /// Reconstruct a `RegisterAdapterKind` from a stored
    /// `data_sources` row — the inverse of [`to_stored_config`].
    ///
    /// Each kind has its own deserializer body type. `schema_name`
    /// is `Option<String>` for postgres (optional — defaults to
    /// `"public"` at adapter-build time) and a required `String`
    /// for mysql — that way a mysql row missing `schema_name`
    /// surfaces as a serde "missing field" error automatically,
    /// instead of requiring a runtime presence check.
    ///
    /// A malformed row (unknown kind, missing credential, missing
    /// required `schema_name` for mysql, etc.) surfaces as
    /// `AppError::internal`; the hydration path treats that as
    /// "skip this row with a warn".
    pub fn from_stored(kind: &str, config: &serde_json::Value) -> Result<Self, AppError> {
        fn decode<T: for<'de> Deserialize<'de>>(
            kind: &str,
            config: &serde_json::Value,
        ) -> Result<T, AppError> {
            serde_json::from_value(config.clone()).map_err(|e| {
                AppError::internal(format!(
                    "data_source kind '{kind}' has an invalid stored config: {e}"
                ))
            })
        }

        match kind {
            "csv" => {
                let StoredCredOnlyBody { credential } = decode("csv", config)?;
                Ok(Self::Csv { credential })
            }
            "json" => {
                let StoredCredOnlyBody { credential } = decode("json", config)?;
                Ok(Self::Json { credential })
            }
            "postgres" => {
                let StoredPostgresBody {
                    credential,
                    schema_name,
                } = decode("postgres", config)?;
                Ok(Self::Postgres {
                    credential,
                    schema_name,
                })
            }
            "mysql" => {
                // `schema_name` is a required field on the
                // deserializer body, so a mysql row missing it
                // surfaces as "missing field `schema_name`"
                // directly from serde — no manual presence check.
                let StoredMysqlBody {
                    credential,
                    schema_name,
                } = decode("mysql", config)?;
                Ok(Self::Mysql {
                    credential,
                    schema_name,
                })
            }
            "bigquery" => {
                let StoredCredOnlyBody { credential } = decode("bigquery", config)?;
                Ok(Self::Bigquery { credential })
            }
            other => Err(AppError::invalid_enum_value(
                "kind",
                other.to_string(),
                &["csv", "json", "postgres", "mysql", "bigquery"],
            )),
        }
    }

    /// Construct the live `DataSourceAdapter` for this kind.
    ///
    /// Resolves the credential through the supplied [`SecretResolver`],
    /// then forwards to the per-adapter `connect` / `new`. A
    /// malformed payload, a bad connection URL, or a missing env var
    /// all surface as a typed `ApiErrorCode` so the register
    /// handler can return a 400 without touching the store.
    pub async fn build_adapter(
        &self,
        resolver: &dyn SecretResolver,
    ) -> Result<Arc<dyn DataSourceAdapter>, AppError> {
        match self {
            // Inline payloads: hand the `Arc<str>` straight to the
            // adapter ctor — no .to_string(), no memcpy.
            Self::Csv { credential } => {
                let data = credential.resolve(resolver).await?;
                let adapter = CsvAdapter::new(data).map_err(AppError::from)?;
                Ok(Arc::new(adapter))
            }
            Self::Json { credential } => {
                let data = credential.resolve(resolver).await?;
                let adapter = JsonAdapter::new(data).map_err(AppError::from)?;
                Ok(Arc::new(adapter))
            }
            // DB adapters consume the connection string as `&str`
            // (parse + discard pattern — they don't store it). Deref
            // the Arc for the call; the Arc itself drops when this
            // scope ends.
            Self::Postgres {
                credential,
                schema_name,
            } => {
                let connection_string = credential.resolve(resolver).await?;
                let schema = schema_name.as_deref().unwrap_or("public");
                let adapter = PostgresAdapter::connect(&connection_string, schema)
                    .await
                    .map_err(AppError::from)?;
                Ok(Arc::new(adapter))
            }
            Self::Mysql {
                credential,
                schema_name,
            } => {
                let connection_string = credential.resolve(resolver).await?;
                let adapter = MysqlAdapter::connect(&connection_string, schema_name)
                    .await
                    .map_err(AppError::from)?;
                Ok(Arc::new(adapter))
            }
            Self::Bigquery { credential } => {
                let connection_string = credential.resolve(resolver).await?;
                let adapter = BigQueryAdapter::connect(&connection_string)
                    .await
                    .map_err(AppError::from)?;
                Ok(Arc::new(adapter))
            }
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterSummary {
    pub source_id: String,
    pub source_type: String,
    /// `true` when the adapter implements `DataSourceAdapter::scan`
    /// — i.e. it can back federated link mappings (cross-source
    /// joins via DataFusion). Adapters returning `false` are
    /// introspection-only; mapping a federated link onto them is a
    /// configuration mistake the admin UI should surface up-front
    /// rather than discovering it at query time.
    pub supports_scan: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterAdapterResponse {
    /// `true` when a previous registration under the same `source_id`
    /// was replaced. Useful so the admin UI can decide whether to
    /// trigger downstream cache invalidation.
    pub replaced: bool,
    /// The inserted row, echoed so the client does not round-trip
    /// its own input.
    pub adapter: AdapterSummary,
}

// ---------------------------------------------------------------------------
// POST /api/admin/federation/adapters
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/admin/federation/adapters",
    request_body = RegisterAdapterRequest,
    responses(
        (status = 200, description = "Adapter registered", body = RegisterAdapterResponse),
        (status = 400, description = "Invalid payload for the declared kind",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn register_adapter(
    State(state): State<FederationState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<Json<ApiResponse<RegisterAdapterResponse>>, AppError> {
    principal.require_admin()?;

    if req.source_id.trim().is_empty() {
        return Err(AppError::required_field_empty("source_id"));
    }

    let source_type = req.kind.kind_tag().to_string();

    // Probe the adapter's capability surface up-front so the admin
    // UI can warn ("this source is introspection-only — federated
    // links won't work against it") instead of discovering the
    // limit deep in a planner failure at query time. The probe runs
    // a single connect-and-`supports_scan()` call; the connection
    // is dropped immediately afterwards.
    let probe_adapter = req.kind.build_adapter(state.secret_resolver.as_ref()).await?;
    let supports_scan = probe_adapter.supports_scan();
    drop(probe_adapter);

    // Delegates the build + store-upsert + memory-register flow to
    // `upsert_workspace_adapter`, which holds the workspace's
    // resolver write lock across the three steps so concurrent
    // registers on the same `source_id` cannot leave the store and
    // memory pinned to different versions. See that function's
    // doc-comment for the atomicity contract.
    let outcome =
        upsert_workspace_adapter(&state, ws.workspace_id, &req.source_id, &req.kind).await?;

    Ok(ApiResponse::of(RegisterAdapterResponse {
        replaced: outcome.replaced,
        adapter: AdapterSummary {
            source_id: req.source_id,
            source_type,
            supports_scan,
        },
    }))
}

// ---------------------------------------------------------------------------
// POST /api/admin/federation/adapters/preview
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PreviewAdapterRequest {
    #[serde(flatten)]
    pub kind: RegisterAdapterKind,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewAdapterResponse {
    pub source_type: String,
    pub tables: Vec<PreviewTable>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewTable {
    pub name: String,
    pub columns: Vec<PreviewColumn>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[utoipa::path(
    post,
    path = "/api/admin/federation/adapters/preview",
    request_body = PreviewAdapterRequest,
    responses(
        (status = 200, description = "Preview of the adapter's schema", body = PreviewAdapterResponse),
        (status = 400, description = "Adapter config failed to build",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn preview_adapter(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<PreviewAdapterRequest>,
) -> Result<Json<ApiResponse<PreviewAdapterResponse>>, AppError> {
    principal.require_admin()?;

    let source_type = req.kind.kind_tag().to_string();
    let adapter = req.kind.build_adapter(state.secret_resolver.as_ref()).await?;

    let table_names = adapter.list_tables().await.map_err(AppError::from)?;
    let mut tables = Vec::with_capacity(table_names.len());
    for name in &table_names {
        let def = adapter
            .describe_table(name)
            .await
            .map_err(AppError::from)?;
        tables.push(PreviewTable {
            name: def.name,
            columns: def
                .columns
                .into_iter()
                .map(|c| PreviewColumn {
                    name: c.name,
                    data_type: c.data_type,
                    nullable: c.nullable,
                })
                .collect(),
        });
    }
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ApiResponse::of(PreviewAdapterResponse {
        source_type,
        tables,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/admin/federation/adapters
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/admin/federation/adapters",
    responses(
        (status = 200, description = "Registered federation adapters",
            body = Vec<AdapterSummary>),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
pub(crate) async fn list_adapters(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
) -> Result<Json<ApiResponse<Vec<AdapterSummary>>>, AppError> {
    principal.require_admin()?;
    let rows = state.store.list_data_sources().await.map_err(AppError::from)?;
    let summaries = rows
        .into_iter()
        .map(|row| AdapterSummary {
            supports_scan: source_type_supports_scan(&row.kind),
            source_id: row.source_id,
            source_type: row.kind,
        })
        .collect();
    Ok(ApiResponse::of(summaries))
}

/// Static capability lookup keyed on the adapter's `source_type`.
/// Mirrors the per-adapter `DataSourceAdapter::supports_scan`
/// override: every backend that returns `true` from that method is
/// listed here. Listed centrally so the admin list / bulk health
/// endpoints don't have to build a live adapter per row just to
/// learn one boolean.
///
/// Adding a new federation-capable backend = add the new
/// `supports_scan` override on its `impl DataSourceAdapter` block,
/// then list its `source_type` here. The compiler doesn't enforce
/// the pair, so the integration test
/// `register_adapter_reports_supports_scan_consistently_across_backends`
/// (federation_admin tests) cross-checks every registered kind.
fn source_type_supports_scan(kind: &str) -> bool {
    matches!(kind, "postgresql" | "mysql" | "bigquery" | "csv" | "json")
}

// ---------------------------------------------------------------------------
// GET /api/admin/federation/adapters/{source_id}
// ---------------------------------------------------------------------------

/// Detail view of one registered adapter.
///
/// Wire shape matches [`RegisterAdapterRequest`] exactly, except
/// `credential` is the redacted [`CredentialSource`] instead of
/// the raw [`Credential`]. This symmetry means admin clients can
/// render the GET result with the same form UI that drives POST —
/// only the credential field needs a "re-enter inline value"
/// affordance (since the raw inline bytes are not echoed).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterDetail {
    pub source_id: String,
    /// Kind-specific body. The `kind` tag + the variant's fields
    /// appear flat at this struct's top level via `serde(flatten)`.
    #[serde(flatten)]
    pub kind: AdapterDetailKind,
}

/// Per-kind detail body. Mirrors [`RegisterAdapterKind`] with
/// [`CredentialSource`] in place of [`Credential`].
///
/// `schema_name` appears on the variants that actually use it —
/// `Option<String>` on `Postgres` (defaults to `"public"` at build
/// time), required `String` on `Mysql`, absent on the other three.
/// The old flat `Option<String>` field on `AdapterDetail` is gone;
/// csv / json / bigquery detail responses no longer carry a
/// nullable `schema_name: null` that meant nothing.
///
/// Adding a new adapter kind updates both this enum and
/// `RegisterAdapterKind`. The `From<&RegisterAdapterKind>`
/// conversion below uses an exhaustive `match` (no wildcard), so
/// forgetting the new arm surfaces as a compile error — the two
/// enums stay in lockstep.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterDetailKind {
    Csv { credential: CredentialSource },
    Json { credential: CredentialSource },
    Postgres {
        credential: CredentialSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        schema_name: Option<String>,
    },
    Mysql {
        credential: CredentialSource,
        schema_name: String,
    },
    Bigquery { credential: CredentialSource },
}

impl From<&RegisterAdapterKind> for AdapterDetailKind {
    fn from(r: &RegisterAdapterKind) -> Self {
        match r {
            RegisterAdapterKind::Csv { credential } => Self::Csv {
                credential: credential.into(),
            },
            RegisterAdapterKind::Json { credential } => Self::Json {
                credential: credential.into(),
            },
            RegisterAdapterKind::Postgres {
                credential,
                schema_name,
            } => Self::Postgres {
                credential: credential.into(),
                schema_name: schema_name.clone(),
            },
            RegisterAdapterKind::Mysql {
                credential,
                schema_name,
            } => Self::Mysql {
                credential: credential.into(),
                schema_name: schema_name.clone(),
            },
            RegisterAdapterKind::Bigquery { credential } => Self::Bigquery {
                credential: credential.into(),
            },
        }
    }
}

/// Redacted view of a [`Credential`] for GET responses.
///
/// `Inline` credentials have their raw value stripped and reported
/// as a single discriminator — the admin API never echoes an inline
/// secret back out. `SecretRef`s carry only a reference string, so
/// we do echo those (operators need to see which env var their
/// registration points at).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialSource {
    /// A raw value is stored; the raw value is NOT returned.
    Inline,
    /// A reference string — safe to echo. `Arc<str>` to match
    /// [`Credential::SecretRef`]; the view is built by cloning the
    /// Arc (refcount bump), not by duplicating bytes.
    SecretRef {
        #[schema(value_type = String)]
        value: Arc<str>,
    },
}

impl From<&Credential> for CredentialSource {
    fn from(c: &Credential) -> Self {
        match c {
            Credential::Inline { .. } => CredentialSource::Inline,
            Credential::SecretRef { value } => CredentialSource::SecretRef {
                value: Arc::clone(value),
            },
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/federation/adapters/{source_id}",
    params(("source_id" = String, Path, description = "SourceId to look up")),
    responses(
        (status = 200, description = "Adapter detail", body = AdapterDetail),
        (
            status = 403,
            description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)
        ),
        (
            status = 404,
            description = "No adapter registered for that source_id",
            body = inline(crate::openapi::ErrorResponse)
        ),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
pub(crate) async fn get_adapter(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(source_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterDetail>>, AppError> {
    principal.require_admin()?;
    let row = state
        .store
        .find_data_source_by_source_id(&source_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::not_found(&format!(
                "federation adapter for source_id '{source_id}'"
            ))
        })?;

    // Round-trip the stored config through `RegisterAdapterKind`
    // so the detail view is built from the same typed representation
    // the register / preview handlers use. A malformed stored row
    // surfaces as a 500 — it means the stored shape drifted from
    // the Rust enum, which is a server bug, not a client bug.
    let kind = RegisterAdapterKind::from_stored(&row.kind, &row.config).map_err(|e| {
        AppError::internal(format!(
            "federation adapter for source_id '{source_id}' has a stored config \
             this server cannot decode: {e:?}"
        ))
    })?;

    Ok(ApiResponse::of(AdapterDetail {
        source_id: row.source_id,
        kind: (&kind).into(),
    }))
}

// ---------------------------------------------------------------------------
// DELETE /api/admin/federation/adapters/{source_id}
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/admin/federation/adapters/{source_id}",
    params(("source_id" = String, Path, description = "SourceId to deregister")),
    responses(
        (status = 204, description = "Adapter removed"),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "No adapter registered for that source_id",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
pub(crate) async fn delete_adapter(
    State(state): State<FederationState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let store_removed = state
        .store
        .delete_data_source_by_source_id(&source_id)
        .await
        .map_err(AppError::from)?;
    let memory_removed = remove_workspace_adapter(&state, ws.workspace_id, &source_id).await;
    if store_removed || memory_removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found(&format!(
            "federation adapter for source_id '{source_id}'"
        )))
    }
}

// ---------------------------------------------------------------------------
// POST /api/admin/federation/adapters/refresh
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RefreshAdaptersResponse {
    pub refreshed: bool,
    pub count: usize,
}

#[utoipa::path(
    post,
    path = "/api/admin/federation/adapters/refresh",
    responses(
        (
            status = 200,
            description = "Workspace resolver rehydrated from the store",
            body = RefreshAdaptersResponse
        ),
        (
            status = 403,
            description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)
        ),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn refresh_adapters(
    State(state): State<FederationState>,
    principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApiResponse<RefreshAdaptersResponse>>, AppError> {
    principal.require_admin()?;
    let count = refresh_workspace_resolver(&state, ws.workspace_id).await?;
    Ok(ApiResponse::of(RefreshAdaptersResponse {
        refreshed: true,
        count,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/admin/federation/health
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FederationHealthResponse {
    pub workspace_id: Uuid,
    pub resolver_hydrated: bool,
    pub resolver_count: usize,
    pub store_count: usize,
    pub in_sync: bool,
    pub orphans_in_resolver: Vec<String>,
    pub missing_from_resolver: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/admin/federation/health",
    responses(
        (status = 200, description = "Federation health snapshot",
            body = FederationHealthResponse),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn federation_health(
    State(state): State<FederationState>,
    principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApiResponse<FederationHealthResponse>>, AppError> {
    principal.require_admin()?;

    // Snapshot the in-memory resolver's source_ids. Slots that
    // exist but are not yet hydrated report `None` — distinguishing
    // cold from warm without triggering lazy initialisation.
    let resolver_ids: Option<std::collections::HashSet<String>> =
        match state.federation_resolvers.get(&ws.workspace_id) {
            Some(slot) => match slot.get() {
                Some(lock) => Some(
                    lock.read()
                        .await
                        .descriptions()
                        .into_iter()
                        .map(|(id, _kind)| id.to_string())
                        .collect(),
                ),
                None => None,
            },
            None => None,
        };
    let resolver_hydrated = resolver_ids.is_some();
    let resolver_ids = resolver_ids.unwrap_or_default();
    let resolver_count = resolver_ids.len();

    let store_rows = state
        .store
        .list_data_sources()
        .await
        .map_err(AppError::from)?;
    let store_count = store_rows.len();
    let store_ids: std::collections::HashSet<String> =
        store_rows.into_iter().map(|row| row.source_id).collect();

    let mut orphans_in_resolver: Vec<String> =
        resolver_ids.difference(&store_ids).cloned().collect();
    let mut missing_from_resolver: Vec<String> =
        store_ids.difference(&resolver_ids).cloned().collect();
    orphans_in_resolver.sort();
    missing_from_resolver.sort();

    let in_sync =
        resolver_hydrated && orphans_in_resolver.is_empty() && missing_from_resolver.is_empty();

    Ok(ApiResponse::of(FederationHealthResponse {
        workspace_id: ws.workspace_id,
        resolver_hydrated,
        resolver_count,
        store_count,
        in_sync,
        orphans_in_resolver,
        missing_from_resolver,
    }))
}

// ---------------------------------------------------------------------------
// Selection-aware introspection routes
//
// `GET  /api/admin/federation/adapters/{source_id}/tables`
// `POST /api/admin/federation/adapters/{source_id}/analyze`
// `GET  /api/admin/federation/adapters/{source_id}/analysis`
//
// Together these expose the incremental ingest workflow: the UI lists
// tables cheaply, the user picks a subset (or "everything"), the
// analyse endpoint runs the selection through the kernel and stamps
// the result onto the source row, and a separate fetch endpoint
// surfaces the cached snapshot + per-table fingerprint map (so the UI
// can compute drift without re-running an analysis).
//
// All three live behind `require_admin` and the per-source build path
// uses the same `RegisterAdapterKind::from_stored` round-trip the
// detail / hydrate handlers use — one decode rule, every route.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterTableSummary {
    pub name: String,
    pub estimated_row_count: Option<u64>,
    pub column_count: u32,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterTableListResponse {
    pub source_id: String,
    pub source_type: String,
    pub tables: Vec<AdapterTableSummary>,
}

#[utoipa::path(
    get,
    path = "/api/admin/federation/adapters/{source_id}/tables",
    params(("source_id" = String, Path, description = "SourceId of the adapter")),
    responses(
        (status = 200, description = "Cheap table listing for selection UIs",
            body = AdapterTableListResponse),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "No adapter registered for that source_id",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn list_adapter_tables(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(source_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterTableListResponse>>, AppError> {
    principal.require_admin()?;
    let row = state
        .store
        .find_data_source_by_source_id(&source_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::not_found(&format!(
                "federation adapter for source_id '{source_id}'"
            ))
        })?;

    let kind = RegisterAdapterKind::from_stored(&row.kind, &row.config).map_err(|e| {
        AppError::internal(format!(
            "federation adapter for source_id '{source_id}' has a stored config \
             this server cannot decode: {e:?}"
        ))
    })?;
    let adapter = kind.build_adapter(state.secret_resolver.as_ref()).await?;

    let summaries = adapter
        .list_tables_with_summary()
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(AdapterTableListResponse {
        source_id: row.source_id,
        source_type: row.kind,
        tables: summaries
            .into_iter()
            .map(|s| AdapterTableSummary {
                name: s.name,
                estimated_row_count: s.estimated_row_count,
                column_count: s.column_count,
                last_modified: s.last_modified,
            })
            .collect(),
    }))
}

/// Body of `POST /api/admin/federation/adapters/{source_id}/analyze`.
///
/// `selection` carries the user's intent through a single tagged
/// enum — `all` for a full sweep, `subset` for a standalone pick,
/// `extend` to grow the source's stored baseline. The wire shape
/// reads `{"selection": {"kind": "extend", "tables": [...]}}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AnalyzeAdapterRequest {
    pub selection: AnalyzeSelection,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AnalyzeAdapterResponse {
    pub source_id: String,
    pub source_type: String,
    pub mode: &'static str,
    pub tables_analyzed: usize,
    pub warnings: usize,
    pub last_analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[utoipa::path(
    post,
    path = "/api/admin/federation/adapters/{source_id}/analyze",
    params(("source_id" = String, Path, description = "SourceId to analyse")),
    request_body = AnalyzeAdapterRequest,
    responses(
        (status = 200, description = "Analysis complete + cached", body = AnalyzeAdapterResponse),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "No adapter registered for that source_id",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn analyze_adapter(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(source_id): Path<String>,
    Json(req): Json<AnalyzeAdapterRequest>,
) -> Result<Json<ApiResponse<AnalyzeAdapterResponse>>, AppError> {
    principal.require_admin()?;
    req.selection.validate().map_err(AppError::from)?;

    let row = state
        .store
        .find_data_source_by_source_id(&source_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::not_found(&format!(
                "federation adapter for source_id '{source_id}'"
            ))
        })?;

    let kind = RegisterAdapterKind::from_stored(&row.kind, &row.config).map_err(|e| {
        AppError::internal(format!(
            "federation adapter for source_id '{source_id}' has a stored config \
             this server cannot decode: {e:?}"
        ))
    })?;
    let adapter = kind.build_adapter(state.secret_resolver.as_ref()).await?;
    let kernel = ox_source::IntrospectionKernel::new(adapter.clone());

    // `Extend` and `Reduce` need the stored baseline; `All` /
    // `Subset` ignore it. The kernel's `analyze` single-entry
    // routing handles dispatch; we only need to surface the mode
    // tag for the audit log here.
    let baseline = if matches!(
        req.selection,
        AnalyzeSelection::Extend { .. } | AnalyzeSelection::Reduce { .. }
    ) {
        let snapshot = row
            .last_analysis_snapshot
            .clone()
            .ok_or_else(AppError::analysis_baseline_required)?;
        let parsed: ox_source::AnalysisResult =
            serde_json::from_value(snapshot).map_err(|e| {
                AppError::internal(format!(
                    "stored analysis snapshot is not a valid AnalysisResult: {e}"
                ))
            })?;
        Some(parsed)
    } else {
        None
    };
    let mode = match &req.selection {
        AnalyzeSelection::All => "all",
        AnalyzeSelection::Subset { .. } => "subset",
        AnalyzeSelection::Extend { .. } => "extension",
        AnalyzeSelection::Reduce { .. } => "reduction",
        AnalyzeSelection::Staged { .. } => "staged",
    };
    let analysis = kernel
        .analyze(req.selection, baseline.as_ref())
        .await
        .map_err(AppError::from)?;

    // Stamp the result + per-table fingerprints back onto the source
    // row. Fingerprints compute via the kernel's default impl —
    // adapters that override it with a backend-native fingerprint
    // serve here transparently.
    let mut fingerprints = serde_json::Map::new();
    for table in &analysis.schema.tables {
        let fp = adapter
            .schema_fingerprint(&table.name)
            .await
            .map_err(AppError::from)?;
        fingerprints.insert(
            table.name.clone(),
            serde_json::to_value(&fp).map_err(|e| AppError::internal(format!(
                "fingerprint for table '{}' failed to serialise: {e}",
                table.name
            )))?,
        );
    }
    let snapshot_value = serde_json::to_value(analysis.as_ref()).map_err(|e| {
        AppError::internal(format!("analysis result failed to serialise: {e}"))
    })?;
    let updated = state
        .store
        .update_data_source_analysis(
            &source_id,
            &snapshot_value,
            &serde_json::Value::Object(fingerprints),
        )
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(AnalyzeAdapterResponse {
        source_id: updated.source_id,
        source_type: updated.kind,
        mode,
        tables_analyzed: analysis.schema.tables.len(),
        warnings: analysis.warnings.len(),
        last_analyzed_at: updated.last_analyzed_at,
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterAnalysisDriftEntry {
    pub table: String,
    pub stored_hash: Option<String>,
    pub live_hash: String,
    pub kind: &'static str,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdapterAnalysisResponse {
    pub source_id: String,
    pub source_type: String,
    pub last_analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Cached `ox_source::AnalysisResult` shape verbatim — the UI can
    /// render schema + profile + warnings without a second round-trip.
    pub snapshot: Option<serde_json::Value>,
    /// Per-table drift between the stored fingerprint map and the
    /// adapter's live fingerprint. Empty when there is nothing to
    /// flag.
    pub drift: Vec<AdapterAnalysisDriftEntry>,
}

#[utoipa::path(
    get,
    path = "/api/admin/federation/adapters/{source_id}/analysis",
    params(("source_id" = String, Path, description = "SourceId to inspect")),
    responses(
        (status = 200, description = "Cached analysis snapshot + live drift",
            body = AdapterAnalysisResponse),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "No adapter registered for that source_id",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Admin",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn get_adapter_analysis(
    State(state): State<FederationState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(source_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterAnalysisResponse>>, AppError> {
    principal.require_admin()?;

    let row = state
        .store
        .find_data_source_by_source_id(&source_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::not_found(&format!(
                "federation adapter for source_id '{source_id}'"
            ))
        })?;

    let stored_fingerprints: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_value(row.schema_fingerprints.clone()).unwrap_or_default();

    // Compute live drift only when we have a cached snapshot to
    // compare against. A source that has never been analysed
    // returns an empty drift list — there's nothing to drift from
    // yet.
    let mut drift = Vec::new();
    if row.last_analysis_snapshot.is_some() {
        let kind = RegisterAdapterKind::from_stored(&row.kind, &row.config).map_err(|e| {
            AppError::internal(format!(
                "federation adapter for source_id '{source_id}' has a stored config \
                 this server cannot decode: {e:?}"
            ))
        })?;
        let adapter = kind.build_adapter(state.secret_resolver.as_ref()).await?;
        for table in adapter.list_tables().await.map_err(AppError::from)? {
            let live = adapter
                .schema_fingerprint(&table)
                .await
                .map_err(AppError::from)?;
            match stored_fingerprints.get(&table) {
                Some(stored) => {
                    let stored_hash = stored
                        .get("hash")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    if stored_hash.as_deref() != Some(live.hash.as_str()) {
                        drift.push(AdapterAnalysisDriftEntry {
                            table,
                            stored_hash,
                            live_hash: live.hash,
                            kind: "changed",
                        });
                    }
                }
                None => {
                    drift.push(AdapterAnalysisDriftEntry {
                        table,
                        stored_hash: None,
                        live_hash: live.hash,
                        kind: "added",
                    });
                }
            }
        }
        // Tables the snapshot knows about but the live source no
        // longer advertises — surface as `removed` drift entries.
        let live_names: std::collections::HashSet<String> = adapter
            .list_tables()
            .await
            .map_err(AppError::from)?
            .into_iter()
            .collect();
        for stored_name in stored_fingerprints.keys() {
            if !live_names.contains(stored_name) {
                let stored_hash = stored_fingerprints
                    .get(stored_name)
                    .and_then(|v| v.get("hash"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                drift.push(AdapterAnalysisDriftEntry {
                    table: stored_name.clone(),
                    stored_hash,
                    // Live hash is omitted when the table is gone —
                    // emit empty rather than introducing a nullable
                    // shape just for the dropped case.
                    live_hash: String::new(),
                    kind: "removed",
                });
            }
        }
    }

    Ok(ApiResponse::of(AdapterAnalysisResponse {
        source_id: row.source_id,
        source_type: row.kind,
        last_analyzed_at: row.last_analyzed_at,
        snapshot: row.last_analysis_snapshot,
        drift,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn csv_kind_round_trips_through_stored_config() {
        let original = RegisterAdapterKind::Csv {
            credential: Credential::Inline {
                value: "id,name\n1,a\n".into(),
            },
        };
        let stored = original.to_stored_config();
        assert_eq!(
            stored,
            json!({"credential": {"kind": "inline", "value": "id,name\n1,a\n"}})
        );
        let back = RegisterAdapterKind::from_stored("csv", &stored).unwrap();
        match back {
            RegisterAdapterKind::Csv { credential } => assert_eq!(
                credential,
                Credential::Inline {
                    value: "id,name\n1,a\n".into()
                }
            ),
            _ => panic!("expected Csv variant"),
        }
    }

    #[test]
    fn json_kind_secret_ref_round_trips() {
        let original = RegisterAdapterKind::Json {
            credential: Credential::SecretRef {
                value: "env:JSON_PAYLOAD".into(),
            },
        };
        let stored = original.to_stored_config();
        assert_eq!(
            stored,
            json!({"credential": {"kind": "secret_ref", "value": "env:JSON_PAYLOAD"}})
        );
        let back = RegisterAdapterKind::from_stored("json", &stored).unwrap();
        assert!(matches!(
            back,
            RegisterAdapterKind::Json {
                credential: Credential::SecretRef { ref value }
            } if &**value == "env:JSON_PAYLOAD"
        ));
    }

    #[test]
    fn postgres_kind_round_trips_with_optional_schema() {
        let with_schema = RegisterAdapterKind::Postgres {
            credential: Credential::SecretRef {
                value: "env:PG_URL".into(),
            },
            schema_name: Some("reporting".into()),
        };
        let stored = with_schema.to_stored_config();
        assert_eq!(
            stored,
            json!({
                "credential": {"kind": "secret_ref", "value": "env:PG_URL"},
                "schema_name": "reporting",
            })
        );
        let back = RegisterAdapterKind::from_stored("postgres", &stored).unwrap();
        match back {
            RegisterAdapterKind::Postgres {
                credential,
                schema_name,
            } => {
                assert_eq!(
                    credential,
                    Credential::SecretRef {
                        value: "env:PG_URL".into()
                    }
                );
                assert_eq!(schema_name, Some("reporting".into()));
            }
            _ => panic!("expected Postgres variant"),
        }

        // Omitted schema also round-trips.
        let no_schema = RegisterAdapterKind::Postgres {
            credential: Credential::Inline {
                value: "postgres://u:p@host/db".into(),
            },
            schema_name: None,
        };
        let stored = no_schema.to_stored_config();
        assert_eq!(
            stored,
            json!({
                "credential": {"kind": "inline", "value": "postgres://u:p@host/db"},
            })
        );
        let back = RegisterAdapterKind::from_stored("postgres", &stored).unwrap();
        match back {
            RegisterAdapterKind::Postgres { schema_name, .. } => {
                assert_eq!(schema_name, None);
            }
            _ => panic!("expected Postgres variant"),
        }
    }

    #[test]
    fn mysql_kind_requires_schema_at_type_level() {
        let original = RegisterAdapterKind::Mysql {
            credential: Credential::Inline {
                value: "mysql://u:p@host/db".into(),
            },
            schema_name: "sales".into(),
        };
        let stored = original.to_stored_config();
        assert_eq!(
            stored,
            json!({
                "credential": {"kind": "inline", "value": "mysql://u:p@host/db"},
                "schema_name": "sales",
            })
        );

        // Missing schema_name on a stored mysql row is a 400 —
        // schema_name is load-bearing because MySQL has no default
        // database.
        let err = RegisterAdapterKind::from_stored(
            "mysql",
            &json!({
                "credential": {"kind": "inline", "value": "mysql://u:p@host/db"},
            }),
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("schema_name") && msg.contains("mysql"),
            "{msg}"
        );
    }

    #[test]
    fn from_stored_rejects_unknown_kind() {
        let err = RegisterAdapterKind::from_stored("duckdb", &json!({"credential": {"kind": "inline", "value": "x"}}))
            .unwrap_err();
        let msg = format!("{err:?}");
        // Typed wire: ApiErrorCode::InvalidEnumValue with params
        // {field: "kind", value: "duckdb", allowed: "csv, json, ..."}.
        // The Debug impl prints the params map verbatim.
        assert!(msg.contains("InvalidEnumValue") && msg.contains("duckdb"), "{msg}");
    }

    #[test]
    fn from_stored_rejects_missing_credential() {
        let err = RegisterAdapterKind::from_stored("csv", &json!({})).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("invalid stored config"), "{msg}");
    }

    #[test]
    fn register_adapter_request_wire_shape_round_trips() {
        // End-to-end: the wire payload the frontend sends must
        // deserialize cleanly and round-trip back to the same bytes.
        let body = json!({
            "source_id": "csv-sales",
            "kind": "csv",
            "credential": {"kind": "inline", "value": "id,x\n1,2\n"},
        });
        let req: RegisterAdapterRequest = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(req.source_id, "csv-sales");
        match req.kind {
            RegisterAdapterKind::Csv { credential } => assert_eq!(
                credential,
                Credential::Inline {
                    value: "id,x\n1,2\n".into()
                }
            ),
            _ => panic!("expected Csv"),
        }
    }

    #[test]
    fn credential_source_hides_inline_value() {
        let inline = Credential::Inline {
            value: "very-secret".into(),
        };
        let view: CredentialSource = (&inline).into();
        let v = serde_json::to_value(&view).unwrap();
        // The `value` field must not appear anywhere in the serialised
        // form of an Inline credential view — otherwise GET would
        // surface the raw secret.
        let s = v.to_string();
        assert!(!s.contains("very-secret"), "inline value leaked: {s}");
        assert_eq!(v, json!({"kind": "inline"}));

        // A secret-ref IS echoed — it's a reference, not a secret.
        let ref_cred = Credential::SecretRef {
            value: "env:DB_URL".into(),
        };
        let view: CredentialSource = (&ref_cred).into();
        let v = serde_json::to_value(&view).unwrap();
        assert_eq!(v, json!({"kind": "secret_ref", "value": "env:DB_URL"}));
    }

    /// End-to-end wire-shape pin for `AdapterDetail`. The GET
    /// response must mirror `RegisterAdapterRequest` field-for-field,
    /// except inline credentials are redacted. If a future refactor
    /// accidentally decouples the two shapes, the admin UI form
    /// would drift from the detail view — this test freezes the
    /// parity contract.
    #[test]
    fn adapter_detail_wire_shape_mirrors_register_request() {
        // Csv + inline → redacted to {kind: "inline"} with no value.
        let csv = RegisterAdapterKind::Csv {
            credential: Credential::Inline {
                value: "id,x\n1,2\n".into(),
            },
        };
        let detail = AdapterDetail {
            source_id: "csv-sales".into(),
            kind: (&csv).into(),
        };
        let wire = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            wire,
            json!({
                "source_id": "csv-sales",
                "kind": "csv",
                "credential": {"kind": "inline"},
            }),
            "inline value must NOT appear in the detail wire form"
        );

        // Postgres + secret_ref + schema_name → echo both.
        let pg = RegisterAdapterKind::Postgres {
            credential: Credential::SecretRef {
                value: "env:PG_URL".into(),
            },
            schema_name: Some("reporting".into()),
        };
        let detail = AdapterDetail {
            source_id: "pg-main".into(),
            kind: (&pg).into(),
        };
        let wire = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            wire,
            json!({
                "source_id": "pg-main",
                "kind": "postgres",
                "credential": {"kind": "secret_ref", "value": "env:PG_URL"},
                "schema_name": "reporting",
            })
        );

        // Postgres without schema_name → schema_name absent (not null).
        let pg_no_schema = RegisterAdapterKind::Postgres {
            credential: Credential::Inline {
                value: "postgres://u:p@host/db".into(),
            },
            schema_name: None,
        };
        let detail = AdapterDetail {
            source_id: "pg-local".into(),
            kind: (&pg_no_schema).into(),
        };
        let wire = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            wire,
            json!({
                "source_id": "pg-local",
                "kind": "postgres",
                "credential": {"kind": "inline"},
            }),
            "omitted schema_name must be absent, not null"
        );

        // Mysql with required schema_name.
        let mysql = RegisterAdapterKind::Mysql {
            credential: Credential::SecretRef {
                value: "env:MYSQL_URL".into(),
            },
            schema_name: "sales".into(),
        };
        let detail = AdapterDetail {
            source_id: "mysql-sales".into(),
            kind: (&mysql).into(),
        };
        let wire = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            wire,
            json!({
                "source_id": "mysql-sales",
                "kind": "mysql",
                "credential": {"kind": "secret_ref", "value": "env:MYSQL_URL"},
                "schema_name": "sales",
            })
        );

        // Bigquery — no schema_name field at all.
        let bq = RegisterAdapterKind::Bigquery {
            credential: Credential::Inline {
                value: "bigquery://proj/ds".into(),
            },
        };
        let detail = AdapterDetail {
            source_id: "bq-main".into(),
            kind: (&bq).into(),
        };
        let wire = serde_json::to_value(&detail).unwrap();
        assert_eq!(
            wire,
            json!({
                "source_id": "bq-main",
                "kind": "bigquery",
                "credential": {"kind": "inline"},
            })
        );
    }
}
