//! Prints the utoipa-generated OpenAPI document to stdout as pretty JSON.
//!
//! Usage:
//!   cargo run --quiet --bin dump_openapi -p ox-api > web/openapi.json
//!
//! Wired into `scripts/gen-openapi-types.sh`, which also regenerates the
//! frontend TypeScript types from the dumped spec.

use ox_api::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() -> anyhow::Result<()> {
    let json = ApiDoc::openapi().to_pretty_json()?;
    println!("{json}");
    Ok(())
}
