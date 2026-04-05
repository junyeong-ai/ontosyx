use ox_core::source_schema::SourceSchema;

/// Stable FNV-1a hash for fingerprint computation.
/// Unlike `DefaultHasher`, this produces deterministic results across Rust versions.
pub(super) fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Compute a stable fingerprint for a PostgreSQL source from its identity fields.
/// Extracts host, port, and dbname from the connection string so that credential
/// rotation, sslmode changes, or query parameter differences don't change identity.
///
/// Handles both URL-style (`postgres://user:pass@[::1]:5432/mydb?sslmode=require`)
/// and key=value conninfo (`host='/var/run/postgresql' dbname='my db' port=5432`).
pub(super) fn pg_fingerprint(connection_string: &str, schema: &str) -> String {
    let cs = connection_string.trim();

    let (host, port, dbname) = if cs.starts_with("postgres://") || cs.starts_with("postgresql://") {
        // url crate handles IPv6 brackets, percent-encoding, and edge cases correctly.
        match url::Url::parse(cs) {
            Ok(url) => {
                let h = url.host_str().unwrap_or("localhost").to_string();
                let p = url.port().unwrap_or(5432).to_string();
                let db = url.path().trim_start_matches('/').to_string();
                (h, p, db)
            }
            Err(_) => {
                // Unparseable URL — fall back to conninfo parser below
                parse_conninfo_identity(cs)
            }
        }
    } else {
        parse_conninfo_identity(cs)
    };

    let identity = format!("{host}:{port}/{dbname}/{schema}");
    format!("{:016x}", fnv1a(identity.as_bytes()))
}

/// Parse libpq-style key=value conninfo string.
/// Handles both quoted (`dbname='my db'`) and unquoted (`host=localhost`) values.
pub(super) fn parse_conninfo_identity(cs: &str) -> (String, String, String) {
    let mut host = "localhost".to_string();
    let mut port = "5432".to_string();
    let mut dbname = String::new();

    let mut chars = cs.chars().peekable();
    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        // Read key
        let key: String = chars.by_ref().take_while(|&c| c != '=').collect();
        let key = key.trim();
        if key.is_empty() {
            break;
        }
        // Read value (possibly quoted)
        let value = if chars.peek() == Some(&'\'') {
            chars.next(); // consume opening quote
            let mut val = String::new();
            loop {
                match chars.next() {
                    None => break,
                    Some('\'') => {
                        if chars.peek() == Some(&'\'') {
                            chars.next();
                            val.push('\'');
                        } else {
                            break;
                        }
                    }
                    Some('\\') => {
                        if let Some(next) = chars.next() {
                            val.push(next);
                        }
                    }
                    Some(ch) => val.push(ch),
                }
            }
            val
        } else {
            let mut val = String::new();
            while let Some(ch) = chars.next_if(|c| !c.is_whitespace()) {
                val.push(ch);
            }
            val
        };

        match key {
            "host" => host = value,
            "port" => port = value,
            "dbname" => dbname = value,
            _ => {}
        }
    }

    (host, port, dbname)
}

/// Compute a stable fingerprint for a MySQL source from its identity fields.
/// Extracts host, port, and database from the connection string.
///
/// Handles URL-style (`mysql://user:pass@host:3306/mydb`) connection strings.
pub(super) fn mysql_fingerprint(connection_string: &str, database: &str) -> String {
    let cs = connection_string.trim();

    let (host, port, dbname) = if cs.starts_with("mysql://") || cs.starts_with("mariadb://") {
        match url::Url::parse(cs) {
            Ok(url) => {
                let h = url.host_str().unwrap_or("localhost").to_string();
                let p = url.port().unwrap_or(3306).to_string();
                let db = url.path().trim_start_matches('/').to_string();
                (h, p, db)
            }
            Err(_) => (
                "localhost".to_string(),
                "3306".to_string(),
                database.to_string(),
            ),
        }
    } else {
        (
            "localhost".to_string(),
            "3306".to_string(),
            database.to_string(),
        )
    };

    let identity = format!("{host}:{port}/{dbname}/{database}");
    format!("{:016x}", fnv1a(identity.as_bytes()))
}

/// Compute a stable fingerprint for a MongoDB source from its identity fields.
/// Extracts host, port, and database from the connection string.
///
/// Handles `mongodb://` and `mongodb+srv://` connection strings.
pub(super) fn mongodb_fingerprint(connection_string: &str, database: &str) -> String {
    let cs = connection_string.trim();

    let (host, port) = if cs.starts_with("mongodb://") || cs.starts_with("mongodb+srv://") {
        match url::Url::parse(cs) {
            Ok(url) => {
                let h = url.host_str().unwrap_or("localhost").to_string();
                let p = url.port().unwrap_or(27017).to_string();
                (h, p)
            }
            Err(_) => ("localhost".to_string(), "27017".to_string()),
        }
    } else {
        ("localhost".to_string(), "27017".to_string())
    };

    let identity = format!("{host}:{port}/{database}");
    format!("{:016x}", fnv1a(identity.as_bytes()))
}

/// Compute a stable fingerprint for a Snowflake source from its identity fields.
/// Uses account, database, and schema to form the identity.
pub(super) fn snowflake_fingerprint(account: &str, database: &str, schema: &str) -> String {
    let identity = format!("{account}/{database}/{schema}");
    format!("{:016x}", fnv1a(identity.as_bytes()))
}

/// Compute a stable fingerprint for a BigQuery source from its identity fields.
/// Uses project_id and dataset to form the identity (credentials are excluded).
pub(super) fn bigquery_fingerprint(project_id: &str, dataset: &str) -> String {
    let identity = format!("{project_id}/{dataset}");
    format!("{:016x}", fnv1a(identity.as_bytes()))
}

/// Compute a stable fingerprint for a CSV/JSON source from its full schema structure.
/// Includes table names, column names+types, primary keys, and foreign keys so that
/// structural changes (type change, PK/FK reorganization) invalidate decisions.
pub(super) fn schema_fingerprint(schema: &SourceSchema) -> String {
    let mut parts: Vec<String> = schema
        .tables
        .iter()
        .map(|t| {
            let mut cols: Vec<String> = t
                .columns
                .iter()
                .map(|c| format!("{}:{}", c.name, c.data_type))
                .collect();
            cols.sort();
            let mut pk: Vec<&str> = t.primary_key.iter().map(|s| s.as_str()).collect();
            pk.sort();
            format!("{}[{}]({})", t.name, pk.join(","), cols.join(","))
        })
        .collect();
    parts.sort();

    let mut fks: Vec<String> = schema
        .foreign_keys
        .iter()
        .map(|fk| {
            format!(
                "{}.{}>{}.{}",
                fk.from_table, fk.from_column, fk.to_table, fk.to_column
            )
        })
        .collect();
    fks.sort();

    let identity = format!("{}|FK:{}", parts.join("|"), fks.join(","));
    format!("{:016x}", fnv1a(identity.as_bytes()))
}
