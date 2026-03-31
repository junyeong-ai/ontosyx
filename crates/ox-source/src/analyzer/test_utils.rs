#[cfg(test)]
use ox_core::source_schema::*;

#[cfg(test)]
pub(crate) fn make_schema(
    tables: &[(&str, &[&str])],
    fks: &[(&str, &str, &str, &str)],
) -> SourceSchema {
    SourceSchema {
        source_type: "postgresql".to_string(),
        tables: tables
            .iter()
            .map(|(name, cols)| SourceTableDef {
                name: name.to_string(),
                columns: cols
                    .iter()
                    .map(|c| SourceColumnDef {
                        name: c.to_string(),
                        data_type: "integer".to_string(),
                        nullable: false,
                    })
                    .collect(),
                primary_key: vec!["id".to_string()],
            })
            .collect(),
        foreign_keys: fks
            .iter()
            .map(|(ft, fc, tt, tc)| ForeignKeyDef {
                from_table: ft.to_string(),
                from_column: fc.to_string(),
                to_table: tt.to_string(),
                to_column: tc.to_string(),
                inferred: false,
            })
            .collect(),
    }
}
