use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Schema information extracted from a data source (RDBMS, document DB, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct SourceSchema {
    /// Data source type (e.g., "postgresql", "mysql", "mongodb")
    pub source_type: String,
    /// Tables/collections discovered
    pub tables: Vec<SourceTableDef>,
    /// Foreign key relationships (critical for graph edge inference)
    pub foreign_keys: Vec<ForeignKeyDef>,
}

impl SourceSchema {
    /// Stable SHA-256 hash of the schema's structural shape — table /
    /// column / FK identity. Independent of declaration order: tables
    /// are sorted by name, columns by name, FKs by their canonical
    /// tuple before hashing, so two introspection runs against the
    /// same physical source yield the same hash even if the adapter
    /// returned the rows in a different order.
    ///
    /// The hash explicitly does **not** cover sample values or row
    /// counts — those vary across re-runs without being a schema
    /// change. This is what `SourceMappingArtifact.schema_snapshot_hash`
    /// pivots on: same hash ⇒ design action can replay the previous
    /// artifact instead of re-prompting the LLM.
    pub fn canonical_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.source_type.as_bytes());
        hasher.update(b"\x1f");

        let mut tables: Vec<&SourceTableDef> = self.tables.iter().collect();
        tables.sort_by(|a, b| a.name.cmp(&b.name));
        for table in tables {
            hasher.update(table.name.as_bytes());
            hasher.update(b"\x1e");

            let mut cols: Vec<&SourceColumnDef> = table.columns.iter().collect();
            cols.sort_by(|a, b| a.name.cmp(&b.name));
            for col in cols {
                hasher.update(col.name.as_bytes());
                hasher.update(b"\x1d");
                hasher.update(col.data_type.as_bytes());
                hasher.update(b"\x1d");
                hasher.update([col.nullable as u8]);
                hasher.update(b"\x1d");
            }
            hasher.update(b"\x1e");

            let mut pks: Vec<&String> = table.primary_key.iter().collect();
            pks.sort();
            for pk in pks {
                hasher.update(pk.as_bytes());
                hasher.update(b"\x1d");
            }
            hasher.update(b"\x1f");
        }

        let mut fks: Vec<&ForeignKeyDef> = self.foreign_keys.iter().collect();
        fks.sort_by(|a, b| {
            a.from_table
                .cmp(&b.from_table)
                .then_with(|| a.from_column.cmp(&b.from_column))
                .then_with(|| a.to_table.cmp(&b.to_table))
                .then_with(|| a.to_column.cmp(&b.to_column))
        });
        for fk in fks {
            hasher.update(fk.from_table.as_bytes());
            hasher.update(b"\x1d");
            hasher.update(fk.from_column.as_bytes());
            hasher.update(b"\x1d");
            hasher.update(fk.to_table.as_bytes());
            hasher.update(b"\x1d");
            hasher.update(fk.to_column.as_bytes());
            hasher.update(b"\x1d");
            hasher.update([fk.inferred as u8]);
            hasher.update(b"\x1e");
        }

        format!("{:x}", hasher.finalize())
    }
}

/// Lightweight metadata for one table — meant for **selection UIs**
/// where the user picks which subset of a source to introspect. Adapters
/// produce this from cheap backend statistics so listing 1000 tables
/// does not pay the per-table profiling cost.
///
/// Every field except `name` is `Option` because backends differ in
/// what they expose without a full scan. `None` means "the backend has
/// no cheap path to this answer", not "the answer is zero".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableSummary {
    /// Stable identifier the adapter accepts in `describe_table` etc.
    pub name: String,
    /// Approximate row count from backend statistics (e.g.
    /// `pg_stat_user_tables.n_live_tup`, MySQL InnoDB stats, MongoDB
    /// `estimatedDocumentCount`). `None` when no cheap stats path
    /// exists.
    pub estimated_row_count: Option<u64>,
    /// Number of columns reported by the catalog. Adapters can serve
    /// this from a `SELECT COUNT(*) FROM information_schema.columns`
    /// without describing every table.
    pub column_count: u32,
    /// Last-modified timestamp when the backend exposes one (e.g.
    /// `pg_stat_user_tables.last_autoanalyze`, BigQuery
    /// `last_modified_time`). `None` for backends that don't track it.
    pub last_modified: Option<DateTime<Utc>>,
}

/// Stable hash of a table's column list — used to detect schema drift
/// between two introspection runs without re-describing every table.
///
/// The hash is over `(column_name, data_type, nullable, position)`
/// tuples plus the primary-key column list. Adapters that want a
/// backend-native fingerprint (e.g., a `SHOW TABLE STATUS` checksum)
/// override [`crate::source_schema::SchemaFingerprint`] computation in
/// the adapter's primitive; the default kernel path derives one from
/// `describe_table`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaFingerprint {
    /// Hex-encoded hash. Algorithm intentionally unspecified at the
    /// type level — adapters and the default kernel both use SHA-256
    /// today, but the value is opaque to consumers.
    pub hash: String,
    /// When the fingerprint was computed. Used to age cached
    /// fingerprints and to attribute drift to a window.
    pub computed_at: DateTime<Utc>,
}

impl SchemaFingerprint {
    /// Compute the canonical fingerprint of a [`SourceTableDef`].
    /// Adapters that have no backend-native fingerprint path use
    /// this — it walks the `describe_table` shape directly so the
    /// hash matches across re-runs that produce identical metadata.
    pub fn from_table(table: &SourceTableDef) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(table.name.as_bytes());
        hasher.update(b"\x1f");
        for (idx, col) in table.columns.iter().enumerate() {
            hasher.update((idx as u32).to_be_bytes());
            hasher.update(col.name.as_bytes());
            hasher.update(b"\x1e");
            hasher.update(col.data_type.as_bytes());
            hasher.update(b"\x1e");
            hasher.update([col.nullable as u8]);
        }
        hasher.update(b"\x1f");
        for pk in &table.primary_key {
            hasher.update(pk.as_bytes());
            hasher.update(b"\x1e");
        }
        Self {
            hash: format!("{:x}", hasher.finalize()),
            computed_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct SourceTableDef {
    pub name: String,
    pub columns: Vec<SourceColumnDef>,
    pub primary_key: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct SourceColumnDef {
    pub name: String,
    /// Original DB type (e.g., "varchar", "int4", "jsonb", "timestamp")
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema, utoipa::ToSchema)]
pub struct ForeignKeyDef {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    /// True if this relationship was inferred from document structure (e.g., JSON nesting)
    /// rather than declared in the source schema (e.g., DB foreign key constraint).
    #[serde(default, skip_serializing_if = "is_false")]
    pub inferred: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// Statistics collected from actual data in the source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct SourceProfile {
    pub table_profiles: Vec<TableProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct TableProfile {
    pub table_name: String,
    pub row_count: u64,
    pub column_stats: Vec<ColumnStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, utoipa::ToSchema)]
pub struct ColumnStats {
    pub column_name: String,
    pub null_count: u64,
    pub distinct_count: u64,
    /// Up to 30 distinct values. Empty when distinct count is too
    /// high *or* when the column is flagged PII-suspect — raw values
    /// are dropped at collection time so they never enter the
    /// `SourceProfile` payload that downstream consumers (admin UI,
    /// LLM context, audit log) eventually surface.
    pub sample_values: Vec<String>,
    /// Smallest observed value in the column. Suppressed when
    /// `pii_redacted` is set so the bounds don't disclose
    /// real-world ranges (date of birth, salary, etc.).
    pub min_value: Option<String>,
    /// Largest observed value. Same redaction policy as `min_value`.
    pub max_value: Option<String>,
    /// `Some(kind)` when a heuristic flagged the column name as
    /// likely PII at collection time and the analyzer dropped raw
    /// values to keep them out of `SourceProfile`. The user later
    /// confirms or overrides through the admin UI's PII suggestion
    /// flow; the confirmed `PiiKind` lives on `PropertyDef::pii_kind`,
    /// independent of this collection-time hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pii_redacted: Option<PiiSuspectKind>,
}

/// Heuristic PII pattern detected by name at sample-collection time.
///
/// Mirrors the high-confidence subset of `ox_ontology::PiiKind` so
/// the FE can render a "Redacted: <kind>" badge without round-
/// tripping the user-confirmed annotation. Open extension via
/// [`PiiSuspectKind::Other`] for catalogues that want to flag a
/// custom pattern without adding a first-class variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PiiSuspectKind {
    Email,
    Phone,
    Name,
    Address,
    NationalId,
    PaymentCard,
    Password,
    Token,
    Other(String),
}

/// Heuristic PII detection by column name. Matches the high-frequency
/// patterns warehouse columns use (`email`, `phone_number`,
/// `customer_email_address`, `password_hash`, `ssn`, `credit_card`,
/// etc.) so sample collection can drop raw values before they ever
/// reach `SourceProfile`. Returns `None` for columns that don't match
/// any heuristic — the analyzer keeps raw samples in that case so
/// downstream value-set inference / clustering still works.
///
/// The match is intentionally conservative: false positives are
/// cheap (FE shows a "Redacted" badge the user can override),
/// false negatives are expensive (raw PII leaks into the analysis
/// surface). When in doubt, redact.
pub fn classify_pii_suspect_by_name(column_name: &str) -> Option<PiiSuspectKind> {
    let n = column_name.to_ascii_lowercase();

    // Auth secrets — always redact, no false-positive cost. Even
    // `api_key_id` columns dereference into key material, so the
    // FK-suffix demote below does not apply here.
    if n.contains("password") || n.contains("passwd") || n == "pwd" {
        return Some(PiiSuspectKind::Password);
    }
    if n.contains("token") || n.contains("secret") || n.contains("api_key") {
        return Some(PiiSuspectKind::Token);
    }

    // Substance PII below — columns named `*_template_id` /
    // `*_config_id` / `*_lookup_id` / `*_enum_id` / `*_kind_id` /
    // `*_type_id` / `*_category_id` are FKs into configuration /
    // lookup rows, not the sensitive substance itself.
    // `email_template_id` is the id of an email-template row, not an
    // email address; demoting here keeps the redaction badge off
    // identifier columns that would otherwise lose all sample
    // visibility for the wrong reason.
    if is_lookup_table_fk(&n) {
        return None;
    }

    // Identity fields. Substring on both spellings — `customer_e_mail_backup`
    // must classify as Email even though "email" is split across separators.
    if n.contains("email") || n.contains("e_mail") {
        return Some(PiiSuspectKind::Email);
    }
    if n.contains("phone")
        || n.contains("mobile")
        || n.contains("tel_no")
        || n.contains("telephone")
    {
        return Some(PiiSuspectKind::Phone);
    }
    if n.contains("ssn")
        || n.contains("national_id")
        || n.contains("rrn") // KR resident registration number
        || n.contains("passport")
        || n.contains("drivers_license")
    {
        return Some(PiiSuspectKind::NationalId);
    }

    // Financial.
    if n.contains("credit_card")
        || n.contains("card_number")
        || n.contains("card_no")
        || n.contains("iban")
        || n.contains("bank_account")
    {
        return Some(PiiSuspectKind::PaymentCard);
    }

    // Addressing — match conservative patterns so we don't redact
    // every column that happens to contain "addr" (`mac_address`,
    // `ip_address` carry a different sensitivity profile).
    if n == "address"
        || n.ends_with("_address")
        || n.contains("street")
        || n.contains("postal_code")
        || n == "zip"
        || n.contains("zipcode")
    {
        // Skip MAC / IP addresses — those are technical identifiers,
        // not personal addresses.
        if n.contains("mac_") || n.contains("ip_") {
            return None;
        }
        return Some(PiiSuspectKind::Address);
    }

    // Personal name fields. Match strictly so `username` /
    // `display_name` (already-public handles) don't redact.
    if n == "first_name"
        || n == "last_name"
        || n == "middle_name"
        || n == "given_name"
        || n == "family_name"
        || n == "full_name"
        || n == "real_name"
        || n == "patient_name"
        || n == "customer_name"
    {
        return Some(PiiSuspectKind::Name);
    }

    None
}

/// Match the lookup-table FK naming patterns the substance-PII
/// heuristics demote. Kept as a pure helper so the test fixture can
/// exercise each suffix variant directly.
fn is_lookup_table_fk(name: &str) -> bool {
    const SUFFIXES: &[&str] = &[
        "_template_id",
        "_config_id",
        "_lookup_id",
        "_enum_id",
        "_kind_id",
        "_type_id",
        "_category_id",
    ];
    SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: &str, nullable: bool) -> SourceColumnDef {
        SourceColumnDef {
            name: name.to_string(),
            data_type: ty.to_string(),
            nullable,
        }
    }

    #[test]
    fn pii_classifier_flags_email_columns() {
        for n in [
            "email",
            "email_address",
            "customer_email",
            "billing_email_address",
        ] {
            assert_eq!(
                classify_pii_suspect_by_name(n),
                Some(PiiSuspectKind::Email),
                "{n} should classify as Email"
            );
        }
    }

    #[test]
    fn pii_classifier_flags_password_token_unconditionally() {
        assert_eq!(
            classify_pii_suspect_by_name("user_password"),
            Some(PiiSuspectKind::Password)
        );
        assert_eq!(
            classify_pii_suspect_by_name("password_hash"),
            Some(PiiSuspectKind::Password)
        );
        assert_eq!(
            classify_pii_suspect_by_name("api_key"),
            Some(PiiSuspectKind::Token)
        );
        assert_eq!(
            classify_pii_suspect_by_name("refresh_token"),
            Some(PiiSuspectKind::Token)
        );
    }

    #[test]
    fn pii_classifier_skips_technical_addresses() {
        assert_eq!(classify_pii_suspect_by_name("ip_address"), None);
        assert_eq!(classify_pii_suspect_by_name("mac_address"), None);
    }

    #[test]
    fn pii_classifier_strict_match_for_personal_names() {
        // Strict match — `username` / `display_name` must NOT
        // trigger the heuristic (already-public handles).
        assert_eq!(classify_pii_suspect_by_name("username"), None);
        assert_eq!(classify_pii_suspect_by_name("display_name"), None);
        assert_eq!(
            classify_pii_suspect_by_name("first_name"),
            Some(PiiSuspectKind::Name)
        );
        assert_eq!(
            classify_pii_suspect_by_name("full_name"),
            Some(PiiSuspectKind::Name)
        );
    }

    #[test]
    fn pii_classifier_returns_none_for_neutral_columns() {
        for n in ["id", "created_at", "status", "amount", "quantity"] {
            assert_eq!(
                classify_pii_suspect_by_name(n),
                None,
                "{n} should not classify"
            );
        }
    }

    #[test]
    fn pii_classifier_demotes_lookup_table_fk_columns() {
        // The id of a configuration / template / lookup row is not
        // itself the substance the heuristic targets. Without this
        // demote, `email_template_id` redacts the integer ids of
        // email-template rows, hiding sample data the operator needs
        // to recognise the column as a relationship.
        for n in [
            "email_template_id",
            "phone_format_lookup_id",
            "address_kind_id",
            "name_category_id",
            "billing_email_config_id",
            "address_type_id",
            "delivery_address_enum_id",
        ] {
            assert_eq!(
                classify_pii_suspect_by_name(n),
                None,
                "{n} should demote — lookup-table FK"
            );
        }
    }

    #[test]
    fn pii_classifier_keeps_secret_substance_under_lookup_suffix() {
        // Secret substance survives the FK-suffix demote — the id of
        // an api-key row dereferences to key material; redacting the
        // column hides that link from the prompt while keeping the
        // operator-visible badge.
        assert_eq!(
            classify_pii_suspect_by_name("api_key_template_id"),
            Some(PiiSuspectKind::Token)
        );
        assert_eq!(
            classify_pii_suspect_by_name("password_config_id"),
            Some(PiiSuspectKind::Password)
        );
    }

    #[test]
    fn pii_classifier_flags_separator_split_email_variants() {
        // Token-boundary FN: `customer_e_mail_backup` contains the
        // standard "e_mail" spelling but no contiguous "email"
        // substring. The substring check on both spellings catches
        // the variant before the sample heuristics get to it.
        assert_eq!(
            classify_pii_suspect_by_name("customer_e_mail_backup"),
            Some(PiiSuspectKind::Email)
        );
        assert_eq!(
            classify_pii_suspect_by_name("e_mail_history"),
            Some(PiiSuspectKind::Email)
        );
        assert_eq!(
            classify_pii_suspect_by_name("user_e_mail"),
            Some(PiiSuspectKind::Email)
        );
    }

    fn table(name: &str, columns: Vec<SourceColumnDef>, pk: Vec<&str>) -> SourceTableDef {
        SourceTableDef {
            name: name.to_string(),
            columns,
            primary_key: pk.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn schema_fingerprint_is_stable_across_recomputation() {
        let t = table(
            "users",
            vec![col("id", "uuid", false), col("email", "text", true)],
            vec!["id"],
        );
        let a = SchemaFingerprint::from_table(&t);
        let b = SchemaFingerprint::from_table(&t);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn schema_fingerprint_differs_when_column_added() {
        let base = table(
            "users",
            vec![col("id", "uuid", false)],
            vec!["id"],
        );
        let extended = table(
            "users",
            vec![col("id", "uuid", false), col("email", "text", true)],
            vec!["id"],
        );
        assert_ne!(
            SchemaFingerprint::from_table(&base).hash,
            SchemaFingerprint::from_table(&extended).hash,
        );
    }

    #[test]
    fn schema_fingerprint_differs_when_nullability_flips() {
        let strict = table("users", vec![col("email", "text", false)], vec![]);
        let lax = table("users", vec![col("email", "text", true)], vec![]);
        assert_ne!(
            SchemaFingerprint::from_table(&strict).hash,
            SchemaFingerprint::from_table(&lax).hash,
        );
    }

    #[test]
    fn schema_fingerprint_differs_when_column_order_changes() {
        let abc = table(
            "t",
            vec![col("a", "int", false), col("b", "text", false)],
            vec![],
        );
        let bca = table(
            "t",
            vec![col("b", "text", false), col("a", "int", false)],
            vec![],
        );
        assert_ne!(
            SchemaFingerprint::from_table(&abc).hash,
            SchemaFingerprint::from_table(&bca).hash,
        );
    }

    #[test]
    fn schema_fingerprint_differs_when_pk_changes() {
        let no_pk = table("t", vec![col("id", "int", false)], vec![]);
        let with_pk = table("t", vec![col("id", "int", false)], vec!["id"]);
        assert_ne!(
            SchemaFingerprint::from_table(&no_pk).hash,
            SchemaFingerprint::from_table(&with_pk).hash,
        );
    }

    fn schema(tables: Vec<SourceTableDef>, fks: Vec<ForeignKeyDef>) -> SourceSchema {
        SourceSchema {
            source_type: "postgresql".into(),
            tables,
            foreign_keys: fks,
        }
    }

    #[test]
    fn canonical_hash_is_stable_across_recomputation() {
        let s = schema(
            vec![table("users", vec![col("id", "uuid", false)], vec!["id"])],
            vec![],
        );
        assert_eq!(s.canonical_hash(), s.canonical_hash());
        assert_eq!(s.canonical_hash().len(), 64);
    }

    #[test]
    fn canonical_hash_ignores_table_declaration_order() {
        let a = schema(
            vec![
                table("orders", vec![col("id", "uuid", false)], vec!["id"]),
                table("customers", vec![col("id", "uuid", false)], vec!["id"]),
            ],
            vec![],
        );
        let b = schema(
            vec![
                table("customers", vec![col("id", "uuid", false)], vec!["id"]),
                table("orders", vec![col("id", "uuid", false)], vec!["id"]),
            ],
            vec![],
        );
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_ignores_column_declaration_order() {
        let a = schema(
            vec![table(
                "users",
                vec![col("a", "int", false), col("b", "text", false)],
                vec![],
            )],
            vec![],
        );
        let b = schema(
            vec![table(
                "users",
                vec![col("b", "text", false), col("a", "int", false)],
                vec![],
            )],
            vec![],
        );
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_changes_when_column_added() {
        let a = schema(
            vec![table("users", vec![col("id", "uuid", false)], vec!["id"])],
            vec![],
        );
        let b = schema(
            vec![table(
                "users",
                vec![col("id", "uuid", false), col("email", "text", true)],
                vec!["id"],
            )],
            vec![],
        );
        assert_ne!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_changes_when_data_type_changes() {
        let a = schema(
            vec![table("users", vec![col("id", "uuid", false)], vec!["id"])],
            vec![],
        );
        let b = schema(
            vec![table("users", vec![col("id", "bigint", false)], vec!["id"])],
            vec![],
        );
        assert_ne!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_changes_when_fk_added() {
        let bare = schema(
            vec![
                table("orders", vec![col("customer_id", "uuid", false)], vec![]),
                table("customers", vec![col("id", "uuid", false)], vec!["id"]),
            ],
            vec![],
        );
        let with_fk = schema(
            bare.tables.clone(),
            vec![ForeignKeyDef {
                from_table: "orders".into(),
                from_column: "customer_id".into(),
                to_table: "customers".into(),
                to_column: "id".into(),
                inferred: false,
            }],
        );
        assert_ne!(bare.canonical_hash(), with_fk.canonical_hash());
    }

    #[test]
    fn canonical_hash_ignores_fk_declaration_order() {
        let tables = vec![
            table("a", vec![col("id", "uuid", false)], vec!["id"]),
            table("b", vec![col("id", "uuid", false)], vec!["id"]),
            table("c", vec![col("id", "uuid", false)], vec!["id"]),
        ];
        let fk1 = ForeignKeyDef {
            from_table: "a".into(),
            from_column: "id".into(),
            to_table: "b".into(),
            to_column: "id".into(),
            inferred: false,
        };
        let fk2 = ForeignKeyDef {
            from_table: "b".into(),
            from_column: "id".into(),
            to_table: "c".into(),
            to_column: "id".into(),
            inferred: false,
        };
        let one = schema(tables.clone(), vec![fk1.clone(), fk2.clone()]);
        let two = schema(tables.clone(), vec![fk2, fk1]);
        assert_eq!(one.canonical_hash(), two.canonical_hash());
    }
}
