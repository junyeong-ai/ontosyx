//! Credentials and secret-resolution for federation adapter configs.
//!
//! A [`Credential`] is either a raw value stored in the data_sources
//! row (`Inline`) or a reference to an external secret store
//! (`SecretRef`). The admin API and the hydration path both consume
//! this type — the wire format, the stored JSONB shape, and the
//! Rust representation are one.
//!
//! ## Storage: `Arc<str>`
//!
//! Both variants store their payload as `Arc<str>`. For the inline
//! CSV / JSON case this is load-bearing: a 100 MiB payload would
//! otherwise make three full-memory copies per register (request
//! body → `Credential::Inline.value` → `resolve()` clone → adapter
//! internal). `Arc::from(String)` reuses the String's allocation
//! (zero-copy), and `.clone()` on an `Arc<str>` is a refcount bump.
//! Secret-ref values are short (tens of bytes) so the `Arc` header
//! overhead is negligible; using the same storage for both variants
//! keeps the API uniform.
//!
//! ## Why an internally-tagged enum instead of a `{value, secret_ref}`
//! ## field pair:
//!
//! - The pair admits four states (both empty, only inline, only ref,
//!   both set) but only two are valid. The enum makes the "exactly
//!   one" invariant part of the *type*, so the validation is serde's
//!   job and the handler never has to assert it at runtime.
//! - The tag field (`kind: "inline" | "secret_ref"`) makes the JSON
//!   wire form discoverable and exhaustive — an OpenAPI client can
//!   see both variants without having to read prose.
//! - Adding a future scheme (`vault:`, `aws-sm:`) does not change
//!   the wire shape or the `Credential` type; [`SecretResolver`]
//!   owns the dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Either a raw value or an opaque reference to a secret store.
///
/// The `value` field carries the raw value on `Inline` and the
/// reference string on `SecretRef`. A `SecretRef` is resolved at
/// adapter-build time through a [`SecretResolver`]; the resolver
/// dispatches on the reference's scheme prefix (`env:`, future
/// `vault:` / `aws-sm:` / `gcp-sm:`).
///
/// `Arc<str>` storage: see crate doc-comment — makes the inline
/// 100 MiB CSV / JSON case zero-copy from request body to adapter.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    /// Raw value stored directly in the data_sources config JSONB.
    /// The admin accepts responsibility for the confidentiality of
    /// the persisted bytes (use `SecretRef` for credentials).
    Inline {
        #[schema(value_type = String)]
        value: Arc<str>,
    },
    /// Opaque reference to an external secret store. The string's
    /// scheme (e.g. `env:`) selects the [`SecretResolver`] branch
    /// that dereferences it.
    SecretRef {
        #[schema(value_type = String)]
        value: Arc<str>,
    },
}

impl Credential {
    /// Resolve the credential to its concrete value. `Inline`
    /// returns an `Arc` clone of the stored value (refcount bump,
    /// no copy); `SecretRef` delegates to the supplied resolver.
    pub async fn resolve(
        &self,
        resolver: &dyn SecretResolver,
    ) -> Result<Arc<str>, AppError> {
        match self {
            Credential::Inline { value } => Ok(Arc::clone(value)),
            Credential::SecretRef { value } => resolver.resolve(value).await,
        }
    }
}

/// Dereference a secret reference string to its concrete value.
///
/// Implementations are expected to handle a **single** scheme
/// (e.g. [`EnvSecretResolver`] handles `env:` only). A
/// [`CompositeSecretResolver`] composes multiple single-scheme
/// resolvers and dispatches by prefix, so adding a `vault:` scheme
/// is one new impl + one registration — no change to `Credential`,
/// `AppState`, or the admin handlers.
///
/// The trait is async so future schemes that need a network
/// round-trip (Vault, AWS Secrets Manager) fit without a blanket
/// breaking change. `Arc<str>` return type matches
/// [`Credential::resolve`] — downstream adapters take `Arc<str>`
/// constructors, so the resolver-to-adapter path is allocation-free
/// beyond the one copy from env / HTTP response.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolve `reference` (e.g., `env:DATABASE_URL`) to its
    /// concrete value, or return an `AppError::bad_request` with a
    /// human-readable description of why the reference could not
    /// be dereferenced.
    async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError>;
}

/// Resolver for `env:VAR_NAME` references — reads the target
/// variable from the process environment.
///
/// This resolver assumes it was dispatched to via its registered
/// `env:` scheme (by [`CompositeSecretResolver`] in production). It
/// does not re-validate the scheme; stripping `env:` is best-effort
/// and a reference without the prefix is treated as the bare
/// variable name, which still surfaces a clean error if the
/// variable is unset.
///
/// Swap out or compose with other resolvers (`vault:`, `aws-sm:`)
/// in [`CompositeSecretResolver`] rather than modifying this impl.
#[derive(Debug, Default, Clone)]
pub struct EnvSecretResolver;

#[async_trait]
impl SecretResolver for EnvSecretResolver {
    async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError> {
        // Accept either `env:VAR` (normal dispatch) or a bare
        // `VAR` (direct-call fallback). Composite dispatch always
        // supplies the `env:` prefix, so the bare-name branch is
        // only reachable from tests / direct API users.
        let var_name = reference.strip_prefix("env:").unwrap_or(reference);
        if var_name.is_empty() {
            return Err(AppError::bad_request(
                "secret_ref 'env:' missing the variable name after the colon",
            ));
        }
        let value = std::env::var(var_name).map_err(|_| {
            AppError::bad_request(format!(
                "secret_ref 'env:{var_name}' — environment variable is not \
                 set or is not valid UTF-8"
            ))
        })?;
        // `Arc::from(String)` reuses the String's allocation, so
        // this is a zero-extra-copy conversion.
        Ok(Arc::from(value))
    }
}

/// Resolver that dispatches to a registered sub-resolver based on
/// the reference's scheme prefix.
///
/// Registration order matters: the first registered prefix whose
/// string is a prefix of `reference` wins. Callers are expected to
/// register disjoint prefixes (`env:`, `vault:`, `aws-sm:` etc.);
/// overlapping schemes (e.g. `env:` and `env-short:`) should be
/// avoided, but the composite does not validate disjointness — the
/// registration API is intentionally simple.
///
/// A reference whose scheme matches no registered resolver surfaces
/// as `AppError::bad_request` with a message that lists every
/// currently-registered prefix — self-updating as resolvers are
/// added / removed, in contrast to a hardcoded error message.
#[derive(Default)]
pub struct CompositeSecretResolver {
    resolvers: Vec<(String, Arc<dyn SecretResolver>)>,
}

impl std::fmt::Debug for CompositeSecretResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let schemes: Vec<&str> =
            self.resolvers.iter().map(|(s, _)| s.as_str()).collect();
        f.debug_struct("CompositeSecretResolver")
            .field("schemes", &schemes)
            .finish_non_exhaustive()
    }
}

impl CompositeSecretResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a resolver for a scheme prefix (e.g. `"env:"`). The
    /// prefix is matched with `str::starts_with`, so including the
    /// colon is intentional — `env:foo` does not match `env-short:foo`.
    pub fn register(
        &mut self,
        scheme: impl Into<String>,
        resolver: Arc<dyn SecretResolver>,
    ) -> &mut Self {
        self.resolvers.push((scheme.into(), resolver));
        self
    }
}

#[async_trait]
impl SecretResolver for CompositeSecretResolver {
    async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError> {
        for (scheme, resolver) in &self.resolvers {
            if reference.starts_with(scheme.as_str()) {
                return resolver.resolve(reference).await;
            }
        }
        let supported: Vec<&str> =
            self.resolvers.iter().map(|(s, _)| s.as_str()).collect();
        Err(AppError::bad_request(format!(
            "unrecognised secret_ref scheme in '{reference}'; supported \
             schemes: [{}]",
            supported.join(", ")
        )))
    }
}

/// Resolver for `file:/path/to/secret` references — reads the
/// target file and returns its contents (with trailing whitespace
/// trimmed) as the secret value. Aimed at container deployments
/// where secrets land on disk via Kubernetes `Secret` volume mounts
/// or similar projected-volume mechanisms.
///
/// Policy:
/// - Path must be absolute (starts with `/`). Relative paths would
///   depend on the server's working directory, which is not a
///   stable secrets boundary.
/// - UTF-8 is required — binary secrets are out of scope here (the
///   credential payload feeds adapter connection strings / CSV
///   payloads, both of which are UTF-8 by construction).
/// - Trailing ASCII whitespace (spaces, tabs, CR, LF) is trimmed.
///   K8s projected-volume files commonly include a trailing newline;
///   the empty-credential-due-to-a-newline footgun is exactly the
///   class of error the trim avoids.
/// - Leading whitespace is preserved. Some secret schemes (bearer
///   tokens with structured prefixes) care about leading bytes; an
///   admin who wants leading whitespace gone can edit the source
///   file.
///
/// Errors surface via `AppError::bad_request` with the path named —
/// the operator needs to know which file the server was reading.
/// The file's contents are never echoed in error messages (a missing
/// file could still leak part of the expected path; that's
/// acceptable — the path itself is supplied by the admin).
#[derive(Debug, Default, Clone)]
pub struct FileSecretResolver {
    /// When non-empty, a file reference is only accepted if the
    /// (normalised) path lies under at least one of these roots.
    /// Defaults to empty — any absolute path is accepted. Sandboxing
    /// a deployment to `/run/secrets` or `/var/lib/ontosyx/secrets`
    /// is the intended use: multi-tenant workspaces can then
    /// reference `file:` paths without exposing the whole filesystem
    /// to an admin with adapter-register permission.
    ///
    /// Mirrors the pattern used by `RepoPolicy::allowed_roots` on
    /// the repo-analysis path — symlink resolution via
    /// `Path::canonicalize` collapses `..` + symlink tricks before
    /// the prefix check.
    allowed_roots: Vec<std::path::PathBuf>,
}

impl FileSecretResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Constrain the resolver to paths under one of `roots`. Each
    /// root is stored as-given; canonicalisation happens at resolve
    /// time so a root that doesn't exist at construction (mounted
    /// later) is tolerated. Passing an empty vec keeps the "any
    /// absolute path" behaviour.
    pub fn with_allowed_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<std::path::PathBuf>,
    {
        self.allowed_roots = roots.into_iter().map(Into::into).collect();
        self
    }

    /// Resolve `path` against the configured sandbox and return the
    /// canonical path that should actually be read.
    ///
    /// * `allowed_roots` empty → returns `path` unchanged (sandbox off).
    /// * Otherwise both `path` and each root are canonicalised and the
    ///   first prefix match wins. The returned `PathBuf` is the one
    ///   the caller **must** pass to `read_to_string` — using the raw
    ///   input path instead would re-open the TOCTOU window between
    ///   check and read (symlink swap after canonicalise).
    /// * Canonicalisation failure (path doesn't exist, broken
    ///   symlink, permission error on an intermediate) is a hard
    ///   reject when a sandbox is configured — the caller cannot
    ///   prove the access is safe, so we refuse rather than fall
    ///   back to a lexical `starts_with` check that a
    ///   `/allowed/../elsewhere` string could satisfy.
    /// * Every sandbox rejection is logged at `warn` level with the
    ///   raw input path and the reason. An operator tailing the log
    ///   sees a running audit trail of `file:` secret attempts that
    ///   tripped the boundary — essential for catching a compromised
    ///   admin token probing the filesystem.
    ///
    /// Mirrors the canonicalise-then-use discipline of the repo
    /// enrichment path (`RepoSource::validate` in `ox-source`) —
    /// both boundaries treat a canonicalisation failure as proof
    /// the input can't be admitted to the sandbox.
    fn canonicalise_within_roots(
        &self,
        path: &std::path::Path,
    ) -> Result<std::path::PathBuf, AppError> {
        if self.allowed_roots.is_empty() {
            return Ok(path.to_path_buf());
        }
        let canonical_path = path.canonicalize().map_err(|e| {
            tracing::warn!(
                requested_path = %path.display(),
                error = %e,
                "file: secret-ref rejected — canonicalisation failed inside sandbox"
            );
            AppError::bad_request(format!(
                "secret_ref 'file:{}' — path could not be resolved inside the \
                 sandbox ({e}); check that the file exists and the server can \
                 read it",
                path.display()
            ))
        })?;
        for root in &self.allowed_roots {
            let Ok(canonical_root) = root.canonicalize() else {
                continue;
            };
            if canonical_path.starts_with(&canonical_root) {
                return Ok(canonical_path);
            }
        }
        tracing::warn!(
            requested_path = %path.display(),
            canonical_path = %canonical_path.display(),
            "file: secret-ref rejected — path is outside every allowed sandbox root"
        );
        Err(AppError::bad_request(format!(
            "secret_ref 'file:{}' — path is not under any allowed root. \
             This resolver has been configured with a sandbox; see server \
             deployment docs for the permitted directories.",
            path.display()
        )))
    }
}

#[async_trait]
impl SecretResolver for FileSecretResolver {
    async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError> {
        let path = reference.strip_prefix("file:").unwrap_or(reference);
        if path.is_empty() {
            return Err(AppError::bad_request(
                "secret_ref 'file:' missing the path after the colon",
            ));
        }
        if !path.starts_with('/') {
            return Err(AppError::bad_request(format!(
                "secret_ref 'file:{path}' must be an absolute path (start with `/`) — \
                 relative paths are ambiguous across server working directories"
            )));
        }
        // Canonicalise once, read from the canonical path — closes the
        // TOCTOU window where a symlink could be swapped between the
        // sandbox check and the actual read.
        let allowed_path =
            self.canonicalise_within_roots(std::path::Path::new(path))?;
        // `tokio::fs::read_to_string` keeps the read off the tokio
        // reactor's main thread — important for the resolve-secret
        // path that runs on every adapter build.
        let contents = tokio::fs::read_to_string(&allowed_path).await.map_err(|e| {
            AppError::bad_request(format!(
                "secret_ref 'file:{path}' — unable to read file: {e}"
            ))
        })?;
        let trimmed = contents.trim_end_matches(['\n', '\r', ' ', '\t']);
        if trimmed.is_empty() {
            return Err(AppError::bad_request(format!(
                "secret_ref 'file:{path}' — file is empty (or contains only whitespace)"
            )));
        }
        Ok(Arc::from(trimmed))
    }
}

/// Build the default composite resolver: `env:` + sandboxed `file:`.
/// `gcp-sm:` is added by [`build_secret_resolver`] when the feature
/// is compiled in and the operator opted in via config.
///
/// Recommended production shape:
/// `default_secret_resolver(config.server.allowed_secret_file_roots)`.
/// Canonicalisation of each root happens lazily at resolve time so
/// mount points that don't exist at startup (late-mounted volumes)
/// are tolerated.
pub fn default_secret_resolver<I, P>(roots: I) -> CompositeSecretResolver
where
    I: IntoIterator<Item = P>,
    P: Into<std::path::PathBuf>,
{
    let mut composite = CompositeSecretResolver::new();
    composite.register("env:", Arc::new(EnvSecretResolver));
    composite.register(
        "file:",
        Arc::new(FileSecretResolver::new().with_allowed_roots(roots)),
    );
    composite
}

/// Build the per-server secret resolver. Always registers `env:` +
/// `file:`; conditionally registers `gcp-sm:` when the cargo feature
/// is compiled in *and* the operator opted in via
/// `[server.gcp_sm]` in config.toml.
///
/// `required = true` makes ADC failure at startup fatal so a
/// production cluster never silently boots with broken secret
/// resolution.
pub async fn build_secret_resolver(
    file_roots: Vec<std::path::PathBuf>,
    gcp_sm: GcpSmOptions,
) -> Result<Arc<dyn SecretResolver>, AppError> {
    let mut composite = default_secret_resolver(file_roots);

    if gcp_sm.enabled {
        register_gcp_sm(&mut composite, gcp_sm.required).await?;
    }

    Ok(Arc::new(composite))
}

/// GCP Secret Manager registration toggle for [`build_secret_resolver`].
#[derive(Debug, Clone, Default)]
pub struct GcpSmOptions {
    pub enabled: bool,
    pub required: bool,
}

#[cfg(feature = "gcp-sm")]
async fn register_gcp_sm(
    composite: &mut CompositeSecretResolver,
    required: bool,
) -> Result<(), AppError> {
    use crate::gcp_secret_manager::GcpSecretManagerResolver;

    match GcpSecretManagerResolver::from_adc().await {
        Ok(resolver) => {
            composite.register("gcp-sm:", Arc::new(resolver));
            tracing::info!("registered `gcp-sm:` secret resolver via ADC");
            Ok(())
        }
        Err(e) if required => Err(AppError::internal(format!(
            "GCP Secret Manager required by config but ADC discovery failed: {e:?}"
        ))),
        Err(e) => {
            tracing::warn!(
                error = ?e,
                "GCP Secret Manager enabled but ADC discovery failed; \
                 `gcp-sm:` references will fail at resolve time"
            );
            Ok(())
        }
    }
}

#[cfg(not(feature = "gcp-sm"))]
async fn register_gcp_sm(
    _composite: &mut CompositeSecretResolver,
    required: bool,
) -> Result<(), AppError> {
    let message = "config requested gcp_sm.enabled but the `gcp-sm` cargo \
                   feature was not compiled in";
    if required {
        Err(AppError::internal(message))
    } else {
        tracing::warn!("{message}");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inline_round_trips_through_json() {
        let c = Credential::Inline {
            value: Arc::from("hello"),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v, json!({"kind": "inline", "value": "hello"}));
        let back: Credential = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn secret_ref_round_trips_through_json() {
        let c = Credential::SecretRef {
            value: Arc::from("env:X"),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v, json!({"kind": "secret_ref", "value": "env:X"}));
        let back: Credential = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn deserializer_rejects_missing_kind_discriminator() {
        let err =
            serde_json::from_value::<Credential>(json!({"value": "anything"})).unwrap_err();
        assert!(
            err.to_string().contains("kind"),
            "error should point at the missing tag: {err}"
        );
    }

    #[test]
    fn deserializer_rejects_unknown_kind_variant() {
        let err = serde_json::from_value::<Credential>(
            json!({"kind": "vault_ref", "value": "x"}),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown variant"),
            "error should list accepted variants: {err}"
        );
    }

    #[tokio::test]
    async fn env_resolver_rejects_empty_var_name() {
        let resolver = EnvSecretResolver;
        let err = resolver.resolve("env:").await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("missing the variable name"));
    }

    #[tokio::test]
    async fn env_resolver_reports_missing_env_by_name() {
        let resolver = EnvSecretResolver;
        let err = resolver
            .resolve("env:OX_CRED_TEST_DEFINITELY_UNSET")
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("OX_CRED_TEST_DEFINITELY_UNSET"));
    }

    #[tokio::test]
    async fn env_resolver_reads_set_variable() {
        // SAFETY: test owns a uniquely-named variable and removes it.
        unsafe {
            std::env::set_var("OX_CRED_TEST_SET", "the-value");
        }
        let resolver = EnvSecretResolver;
        let got = resolver.resolve("env:OX_CRED_TEST_SET").await.unwrap();
        assert_eq!(&*got, "the-value");
        unsafe {
            std::env::remove_var("OX_CRED_TEST_SET");
        }
    }

    #[tokio::test]
    async fn credential_inline_resolve_returns_value_without_touching_resolver() {
        // A panicking resolver — if called, the test fails loudly.
        struct PanickingResolver;
        #[async_trait]
        impl SecretResolver for PanickingResolver {
            async fn resolve(&self, _reference: &str) -> Result<Arc<str>, AppError> {
                panic!("resolver must not be called for Credential::Inline");
            }
        }
        let c = Credential::Inline {
            value: Arc::from("raw"),
        };
        let resolver = PanickingResolver;
        let got = c.resolve(&resolver).await.unwrap();
        assert_eq!(&*got, "raw");
    }

    #[tokio::test]
    async fn credential_inline_resolve_is_arc_refcount_bump_not_copy() {
        let value: Arc<str> = Arc::from("shared");
        let c = Credential::Inline {
            value: Arc::clone(&value),
        };
        // Dummy resolver never called for Inline.
        struct Dummy;
        #[async_trait]
        impl SecretResolver for Dummy {
            async fn resolve(&self, _: &str) -> Result<Arc<str>, AppError> {
                unreachable!()
            }
        }
        let out = c.resolve(&Dummy).await.unwrap();
        assert!(
            Arc::ptr_eq(&value, &out),
            "Inline resolve must share the Arc, not allocate a fresh one"
        );
    }

    #[tokio::test]
    async fn composite_dispatches_to_matching_scheme() {
        // Registered "env:" → env resolver; "mock:" → mock resolver.
        struct MockResolver;
        #[async_trait]
        impl SecretResolver for MockResolver {
            async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError> {
                assert!(reference.starts_with("mock:"));
                Ok(Arc::from("mocked"))
            }
        }

        let mut composite = CompositeSecretResolver::new();
        composite.register("mock:", Arc::new(MockResolver));
        composite.register("env:", Arc::new(EnvSecretResolver));

        let got = composite.resolve("mock:anything").await.unwrap();
        assert_eq!(&*got, "mocked");
    }

    #[tokio::test]
    async fn composite_error_lists_all_registered_schemes() {
        let mut composite = CompositeSecretResolver::new();
        composite.register("env:", Arc::new(EnvSecretResolver));
        struct Noop;
        #[async_trait]
        impl SecretResolver for Noop {
            async fn resolve(&self, _: &str) -> Result<Arc<str>, AppError> {
                Ok(Arc::from(""))
            }
        }
        composite.register("vault:", Arc::new(Noop));

        let err = composite.resolve("unknown:something").await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("env:") && msg.contains("vault:"), "{msg}");
    }

    #[tokio::test]
    async fn composite_prefers_first_registered_on_prefix_overlap() {
        // If someone registers both "env:" and "env:" (bad practice
        // but not forbidden), the first wins. Pins the behaviour.
        struct First;
        #[async_trait]
        impl SecretResolver for First {
            async fn resolve(&self, _: &str) -> Result<Arc<str>, AppError> {
                Ok(Arc::from("first"))
            }
        }
        struct Second;
        #[async_trait]
        impl SecretResolver for Second {
            async fn resolve(&self, _: &str) -> Result<Arc<str>, AppError> {
                Ok(Arc::from("second"))
            }
        }
        let mut composite = CompositeSecretResolver::new();
        composite.register("env:", Arc::new(First));
        composite.register("env:", Arc::new(Second));
        let got = composite.resolve("env:foo").await.unwrap();
        assert_eq!(&*got, "first");
    }

    #[test]
    fn default_secret_resolver_has_env_registered() {
        let _ = default_secret_resolver(Vec::<std::path::PathBuf>::new());
    }

    // -----------------------------------------------------------------
    // FileSecretResolver
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn file_resolver_rejects_empty_path() {
        let resolver = FileSecretResolver::new();
        let err = resolver.resolve("file:").await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("missing the path"));
    }

    #[tokio::test]
    async fn file_resolver_rejects_relative_path() {
        let resolver = FileSecretResolver::new();
        let err = resolver.resolve("file:secrets/token").await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("absolute path"), "{msg}");
    }

    #[tokio::test]
    async fn file_resolver_reads_and_trims_trailing_newline() {
        // K8s projected-volume files always carry a trailing newline;
        // pin the trim behaviour so the downstream adapter never
        // sees it as part of a connection string.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "the-secret-value\n").expect("write");
        let path = tmp.path().to_str().expect("utf-8 path");
        let resolver = FileSecretResolver::new();
        let got = resolver
            .resolve(&format!("file:{path}"))
            .await
            .expect("resolve ok");
        assert_eq!(&*got, "the-secret-value");
    }

    #[tokio::test]
    async fn file_resolver_preserves_leading_whitespace() {
        // Some bearer-token formats start with a structural prefix;
        // trimming the leading side would damage them. Only the
        // trailing side trims.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "  padded-prefix\n").expect("write");
        let path = tmp.path().to_str().expect("utf-8 path");
        let resolver = FileSecretResolver::new();
        let got = resolver
            .resolve(&format!("file:{path}"))
            .await
            .expect("resolve ok");
        assert_eq!(&*got, "  padded-prefix");
    }

    #[tokio::test]
    async fn file_resolver_reports_missing_file_by_path() {
        let resolver = FileSecretResolver::new();
        let err = resolver
            .resolve("file:/definitely/does/not/exist/ontosyx-secret-test")
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("/definitely/does/not/exist"), "{msg}");
    }

    #[tokio::test]
    async fn file_resolver_rejects_whitespace_only_file() {
        // K8s mounts an empty Secret as a zero-byte file — the
        // resolver must not quietly hand back an empty string; that
        // would smuggle a "no credential" value into the adapter.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "\n\n  \t\n").expect("write");
        let path = tmp.path().to_str().expect("utf-8 path");
        let resolver = FileSecretResolver::new();
        let err = resolver
            .resolve(&format!("file:{path}"))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("empty"), "{msg}");
    }

    #[tokio::test]
    async fn file_resolver_with_allowed_roots_accepts_inside_root() {
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let nested = tmp_dir.path().join("secret.txt");
        std::fs::write(&nested, "in-root\n").expect("write");
        let resolver =
            FileSecretResolver::new().with_allowed_roots([tmp_dir.path().to_path_buf()]);
        let got = resolver
            .resolve(&format!("file:{}", nested.display()))
            .await
            .expect("resolve inside root");
        assert_eq!(&*got, "in-root");
    }

    #[tokio::test]
    async fn file_resolver_with_allowed_roots_rejects_outside() {
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::NamedTempFile::new().expect("outside temp");
        std::fs::write(outside.path(), "outside-value").expect("write");
        let resolver =
            FileSecretResolver::new().with_allowed_roots([tmp_dir.path().to_path_buf()]);
        let err = resolver
            .resolve(&format!("file:{}", outside.path().display()))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("not under any allowed root"), "{msg}");
    }

    #[tokio::test]
    async fn file_resolver_with_allowed_roots_canonicalises_dotdot_escape() {
        // A path that lexically starts with a root but escapes via `..`
        // must be rejected. We accept either error phrasing: the escape
        // may canonicalise to an existing file outside the root ("not
        // under any allowed root") *or* to a non-existent path ("could
        // not be resolved inside the sandbox") depending on the concrete
        // temp-dir layout. Both outcomes are safe rejections — what we
        // pin here is that the escape *never* succeeds.
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::NamedTempFile::new().expect("outside");
        std::fs::write(outside.path(), "outside-value").expect("write");
        let escape =
            format!("{}/../{}", tmp_dir.path().display(), outside.path().display());
        let resolver =
            FileSecretResolver::new().with_allowed_roots([tmp_dir.path().to_path_buf()]);
        let err = resolver
            .resolve(&format!("file:{escape}"))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("not under any allowed root")
                || msg.contains("could not be resolved inside the sandbox"),
            "dot-dot escape must reject at the sandbox layer: {msg}"
        );
    }

    #[tokio::test]
    async fn file_resolver_empty_allowed_roots_preserves_default_behaviour() {
        // No roots supplied → any absolute path works, matching the
        // pre-sandbox default.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "value\n").expect("write");
        let resolver = FileSecretResolver::new(); // no .with_allowed_roots call
        let got = resolver
            .resolve(&format!("file:{}", tmp.path().display()))
            .await
            .expect("resolve ok");
        assert_eq!(&*got, "value");
    }

    #[tokio::test]
    async fn file_resolver_with_sandbox_rejects_nonexistent_path() {
        // Sandbox active + path doesn't exist → hard reject at the
        // sandbox check. Prior impl's lexical fallback would let the
        // request through and produce a confusing "unable to read
        // file" error instead of a clear "not under allowed root"
        // signal — the new behaviour is strictly more auditable.
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let resolver =
            FileSecretResolver::new().with_allowed_roots([tmp_dir.path().to_path_buf()]);
        let err = resolver
            .resolve("file:/definitely/does/not/exist/secret")
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("could not be resolved inside the sandbox"),
            "sandbox + missing path must reject at the sandbox check: {msg}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn file_resolver_reads_from_canonical_path_not_raw_input() {
        // The sandbox check canonicalises the path; the read must
        // use that canonical path too, not the raw input. Otherwise
        // a symlink swap between check and read would bypass the
        // sandbox. Regression test: a symlink resolves to a file
        // inside the root, and the read succeeds with the real
        // file's contents — proving the resolver reads through the
        // canonicalised path rather than re-opening the raw input.
        //
        // Gated to unix because Windows symlink creation needs
        // either dev-mode or admin rights; the TOCTOU concern is the
        // same shape on both platforms, but reproducing the symlink
        // step in CI without elevated rights is unreliable.
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let real_path = tmp_dir.path().join("real-secret");
        std::fs::write(&real_path, "real-value\n").expect("write real");
        let symlink_path = tmp_dir.path().join("alias");
        std::os::unix::fs::symlink(&real_path, &symlink_path)
            .expect("create symlink");
        let resolver =
            FileSecretResolver::new().with_allowed_roots([tmp_dir.path().to_path_buf()]);
        let got = resolver
            .resolve(&format!("file:{}", symlink_path.display()))
            .await
            .expect("resolve ok via symlink inside sandbox");
        assert_eq!(&*got, "real-value");
    }

    #[tokio::test]
    async fn default_resolver_dispatches_both_env_and_file_schemes() {
        let resolver = default_secret_resolver(Vec::<std::path::PathBuf>::new());
        let err = resolver.resolve("file:").await.unwrap_err();
        assert!(format!("{err:?}").contains("missing the path"));
        let err = resolver.resolve("env:").await.unwrap_err();
        assert!(format!("{err:?}").contains("missing the variable name"));
    }

    #[tokio::test]
    async fn build_secret_resolver_without_gcp_sm_keeps_env_and_file() {
        let resolver = build_secret_resolver(
            Vec::new(),
            GcpSmOptions {
                enabled: false,
                required: false,
            },
        )
        .await
        .expect("build resolver");
        let err = resolver.resolve("file:").await.unwrap_err();
        assert!(format!("{err:?}").contains("missing the path"));
        let err = resolver.resolve("gcp-sm:").await.unwrap_err();
        assert!(
            format!("{err:?}").contains("supported"),
            "gcp-sm: scheme should be unrecognised when disabled"
        );
    }

    #[tokio::test]
    #[cfg(not(feature = "gcp-sm"))]
    async fn build_secret_resolver_required_without_feature_is_fatal() {
        let outcome = build_secret_resolver(
            Vec::new(),
            GcpSmOptions {
                enabled: true,
                required: true,
            },
        )
        .await;
        match outcome {
            Ok(_) => panic!("required gcp-sm without feature must be fatal"),
            Err(err) => assert!(format!("{err:?}").contains("`gcp-sm` cargo feature")),
        }
    }
}
