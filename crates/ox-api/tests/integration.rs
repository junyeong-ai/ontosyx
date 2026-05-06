//! Integration test entry — single binary covering every focus
//! module under `tests/integration/`.

mod integration {
    mod federation_acl_e2e;
    mod federation_admin_pipeline;
    mod federation_pipeline;
    mod openapi_schema_pinned;
}
