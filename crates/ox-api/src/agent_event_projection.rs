//! Project entelix's runtime [`AgentEvent<ReActState>`] onto the
//! canonical SSE wire shape [`AgentEventPayload`].
//!
//! The agent run emits a stream of `AgentEvent`s through a fan-out of
//! sinks; the per-request `ChannelSink` forwards each event to the SSE
//! generator, which calls [`agent_event_to_payload`] to convert into
//! the wire shape the FE consumes.
//!
//! ## Wildcard policy
//!
//! `entelix::AgentEvent<S>` is `#[non_exhaustive]`. Variants we do
//! not currently project (HITL approval book-ends, future entelix
//! additions) return `None` so the SSE wire stays explicit about
//! what it advertises. The `tracing::debug!` line under each
//! unprojected arm surfaces drift in dev / staging the moment a new
//! variant fires, so an FE consumer expecting a lifecycle event
//! doesn't silently miss it.

use entelix::{AgentEvent, ErrorClass, ErrorEnvelope, ReActState};

use ox_brain::classify_wire_code;
use ox_store::{AgentEventPayload, LlmFailureEnvelope, ToolFailureEnvelope};

use crate::error::{ApiErrorClass, llm_error_code_for};

/// Project an entelix [`AgentEvent<ReActState>`] onto the persisted
/// SSE payload. Returns `None` for variants without a wire-level
/// representation (HITL approval book-ends, future entelix variants).
pub(crate) fn agent_event_to_payload(event: &AgentEvent<ReActState>) -> Option<AgentEventPayload> {
    Some(match event {
        AgentEvent::Started { run_id, agent, .. } => AgentEventPayload::Started {
            run_id: run_id.clone(),
            agent: agent.clone(),
        },
        AgentEvent::ToolStart {
            run_id,
            tool_use_id,
            tool,
            input,
            ..
        } => AgentEventPayload::ToolStart {
            run_id: run_id.clone(),
            tool_use_id: tool_use_id.clone(),
            tool: tool.clone(),
            input: input.clone(),
        },
        AgentEvent::ToolComplete {
            run_id,
            tool_use_id,
            tool,
            output,
            duration_ms,
            ..
        } => AgentEventPayload::ToolComplete {
            run_id: run_id.clone(),
            tool_use_id: tool_use_id.clone(),
            tool: tool.clone(),
            // entelix carries the tool result as `serde_json::Value`
            // (object / array / string / primitive). The wire shape
            // standardises on a single `String`: `Value::String`
            // round-trips its inner content (no extra `"…"` wrap),
            // every other variant serialises through `to_string()`
            // so the FE consumer reads one type regardless of which
            // tool fired.
            output: match output {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            duration_ms: i64::try_from(*duration_ms).unwrap_or(i64::MAX),
        },
        AgentEvent::ToolError {
            run_id,
            tool_use_id,
            tool,
            error,
            error_for_llm,
            duration_ms,
            envelope,
            ..
        } => AgentEventPayload::ToolError {
            run_id: run_id.clone(),
            tool_use_id: tool_use_id.clone(),
            tool: tool.clone(),
            error: error.clone(),
            error_for_llm: error_for_llm.as_inner().clone(),
            duration_ms: i64::try_from(*duration_ms).unwrap_or(i64::MAX),
            envelope: project_tool_envelope(envelope),
        },
        AgentEvent::Complete {
            run_id,
            state,
            usage,
            ..
        } => AgentEventPayload::Complete {
            run_id: run_id.clone(),
            text: state.last_assistant_text().unwrap_or_default(),
            steps: state.steps as u32,
            input_tokens: usage
                .as_ref()
                .map(|u| i64::try_from(u.input_tokens).unwrap_or(i64::MAX)),
            output_tokens: usage
                .as_ref()
                .map(|u| i64::try_from(u.output_tokens).unwrap_or(i64::MAX)),
        },
        AgentEvent::Failed {
            run_id,
            error,
            envelope,
            ..
        } => {
            let projected = project_llm_envelope(envelope);
            // `warn` carries the typed bucket (operators search by
            // code); `debug` carries the Display rendering. Provider
            // errors occasionally echo user prompt fragments through
            // their response body, so gating the prose at `debug`
            // keeps production logs PII-safe by default.
            tracing::warn!(
                run_id = %run_id,
                code = %projected.code,
                wire_code = envelope.wire_code,
                provider_status = envelope.provider_status,
                "agent failed — typed envelope emitted"
            );
            tracing::debug!(run_id = %run_id, detail = %error, "agent failure detail");
            AgentEventPayload::Failed {
                run_id: run_id.clone(),
                envelope: projected,
                params: serde_json::json!({}),
            }
        }
        AgentEvent::ToolCallApproved {
            run_id,
            tool_use_id,
            tool,
            ..
        } => {
            tracing::debug!(
                %run_id, %tool_use_id, %tool,
                "tool-call approved — no SSE projection (HITL not wired)"
            );
            return None;
        }
        AgentEvent::ToolCallDenied {
            run_id,
            tool_use_id,
            tool,
            reason,
            ..
        } => {
            tracing::debug!(
                %run_id, %tool_use_id, %tool, %reason,
                "tool-call denied — no SSE projection (HITL not wired)"
            );
            return None;
        }
        // `AgentEvent` is `#[non_exhaustive]`; a future entelix minor
        // adding a new variant lands here. The debug line surfaces
        // drift in dev so a projection arm can be added before the
        // FE silently misses lifecycle events.
        _other => {
            tracing::debug!(
                "agent event variant not projected to SSE wire — extend agent_event_to_payload"
            );
            return None;
        }
    })
}

/// Project entelix's [`ErrorEnvelope`] onto the LLM failure wire
/// shape. Routes the wire bucket through `LlmErrorCode` →
/// `ApiErrorCode::Llm*` so the agent SSE surface keys off the same
/// `errors.llm_<code>` template set every synchronous HTTP LLM error
/// already uses.
pub(crate) fn project_llm_envelope(envelope: &ErrorEnvelope) -> LlmFailureEnvelope {
    let api_code = llm_error_code_for(classify_wire_code(envelope.wire_code));
    LlmFailureEnvelope {
        code: api_code.as_str().to_string(),
        class: match api_code.class() {
            ApiErrorClass::ClientError => "client_error",
            ApiErrorClass::ServerError => "server_error",
        }
        .to_string(),
        retry_after_secs: envelope.retry_after_secs,
        provider_status: envelope.provider_status,
    }
}

/// Project entelix's [`ErrorEnvelope`] onto the tool failure wire
/// shape. Mirrors `wire_code` and `wire_class` verbatim — tool
/// dispatch errors (parse / source-adapter / SaaS-proxy) live in a
/// namespace distinct from LLM provider errors, so FE i18n keys off
/// `errors.tool_<wire_code>` instead of forcing every tool error
/// through the LLM bucket set.
pub(crate) fn project_tool_envelope(envelope: &ErrorEnvelope) -> ToolFailureEnvelope {
    ToolFailureEnvelope {
        wire_code: envelope.wire_code.to_string(),
        class: match envelope.wire_class {
            ErrorClass::Client => "client_error",
            ErrorClass::Server => "server_error",
            _ => "server_error",
        }
        .to_string(),
        retry_after_secs: envelope.retry_after_secs,
        provider_status: envelope.provider_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use entelix::{Error, LlmRenderable, TenantId};
    use serde_json::json;

    fn tenant() -> TenantId {
        TenantId::new("t-test")
    }

    // ---------------------------------------------------------------------
    // Started / ToolStart / ToolComplete — pass-through wire shape
    // ---------------------------------------------------------------------

    #[test]
    fn started_event_projects_run_id_and_agent() {
        let event: AgentEvent<ReActState> = AgentEvent::Started {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            parent_run_id: None,
            agent: "ontosyx".into(),
        };
        let payload = agent_event_to_payload(&event).expect("Started has a wire projection");
        match payload {
            AgentEventPayload::Started { run_id, agent } => {
                assert_eq!(run_id, "run-1");
                assert_eq!(agent, "ontosyx");
            }
            other => panic!("expected Started, got {other:?}"),
        }
    }

    #[test]
    fn tool_start_event_preserves_input_value_verbatim() {
        let event: AgentEvent<ReActState> = AgentEvent::ToolStart {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "query_graph".into(),
            tool_version: None,
            input: json!({"query": "match (n) return n"}),
        };
        let payload = agent_event_to_payload(&event).expect("ToolStart has a wire projection");
        match payload {
            AgentEventPayload::ToolStart {
                run_id,
                tool_use_id,
                tool,
                input,
            } => {
                assert_eq!(run_id, "run-1");
                assert_eq!(tool_use_id, "tu-1");
                assert_eq!(tool, "query_graph");
                assert_eq!(input, json!({"query": "match (n) return n"}));
            }
            other => panic!("expected ToolStart, got {other:?}"),
        }
    }

    #[test]
    fn tool_complete_string_output_unwraps_without_extra_quotes() {
        // Wire-shape contract: a tool returning `Value::String` must
        // unwrap to its inner string, not the JSON-encoded form. This
        // is what `query_graph` relies on so the FE doesn't end up
        // rendering `"hello"` with literal quotes.
        let event: AgentEvent<ReActState> = AgentEvent::ToolComplete {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "query_graph".into(),
            tool_version: None,
            duration_ms: 42,
            output: json!("hello"),
        };
        let payload = agent_event_to_payload(&event).expect("ToolComplete has a wire projection");
        match payload {
            AgentEventPayload::ToolComplete {
                output,
                duration_ms,
                ..
            } => {
                assert_eq!(output, "hello");
                assert_eq!(duration_ms, 42);
            }
            other => panic!("expected ToolComplete, got {other:?}"),
        }
    }

    #[test]
    fn tool_complete_object_output_serialises_through_to_string() {
        let event: AgentEvent<ReActState> = AgentEvent::ToolComplete {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "visualize".into(),
            tool_version: None,
            duration_ms: 7,
            output: json!({"chart_type": "bar", "title": "demo"}),
        };
        let payload = agent_event_to_payload(&event).expect("ToolComplete has a wire projection");
        match payload {
            AgentEventPayload::ToolComplete { output, .. } => {
                // The wire stays a single `String` regardless of the
                // tool's JSON shape. The FE consumer parses it back.
                let parsed: serde_json::Value =
                    serde_json::from_str(&output).expect("output round-trips as JSON");
                assert_eq!(parsed, json!({"chart_type": "bar", "title": "demo"}));
            }
            other => panic!("expected ToolComplete, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // ToolError / Failed — envelope projection
    // ---------------------------------------------------------------------

    #[test]
    fn tool_error_event_mirrors_wire_code_and_class() {
        // entelix `provider_http(503, …)` produces wire bucket
        // `upstream_unavailable` with `Server` class. Tool failure
        // envelope mirrors both literally (no LLM-namespace
        // projection — `errors.tool_upstream_unavailable` is the FE
        // i18n key).
        let source = Error::provider_http(503, "vendor down");
        let envelope = source.envelope();
        let llm_facing = source.for_llm();
        let event: AgentEvent<ReActState> = AgentEvent::ToolError {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "query_graph".into(),
            tool_version: None,
            error: "operator-facing detail".into(),
            error_for_llm: llm_facing,
            envelope,
            duration_ms: 12,
        };
        let payload = agent_event_to_payload(&event).expect("ToolError has a wire projection");
        match payload {
            AgentEventPayload::ToolError {
                tool,
                envelope: projected,
                duration_ms,
                ..
            } => {
                assert_eq!(tool, "query_graph");
                assert_eq!(projected.wire_code, "upstream_unavailable");
                assert_eq!(projected.class, "server_error");
                assert_eq!(projected.provider_status, Some(503));
                assert_eq!(duration_ms, 12);
            }
            other => panic!("expected ToolError, got {other:?}"),
        }
    }

    #[test]
    fn failed_event_classifies_wire_code_through_llm_namespace() {
        // entelix 503 → wire_code `upstream_error` → LlmErrorCode
        // `ProviderUnavailable` → ApiErrorCode `llm_provider_unavailable`.
        // The Failed envelope renders the LLM-namespace code so FE
        // i18n keys off `errors.llm_provider_unavailable` rather than
        // the entelix bucket name.
        let envelope = Error::provider_http(503, "vendor down").envelope();
        let event: AgentEvent<ReActState> = AgentEvent::Failed {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            error: "vendor down".into(),
            envelope,
        };
        let payload = agent_event_to_payload(&event).expect("Failed has a wire projection");
        match payload {
            AgentEventPayload::Failed {
                envelope: projected,
                params,
                ..
            } => {
                assert_eq!(projected.code, "llm_provider_unavailable");
                assert_eq!(projected.class, "server_error");
                assert_eq!(projected.provider_status, Some(503));
                assert_eq!(params, json!({}));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn failed_event_with_rate_limit_carries_retry_after_hint() {
        // LLM rate-limit failure: code → `llm_rate_limited`, class →
        // `server_error` (ontosyx classifies every Llm* code as
        // server-class because the failure is an infrastructure
        // condition not caused by the API caller's request shape).
        // The `retry_after_secs` hint must pass through so the FE
        // rate-limit handler keys off vendor guidance.
        let source =
            Error::provider_http(429, "rate").with_retry_after(std::time::Duration::from_secs(11));
        let envelope = source.envelope();
        let event: AgentEvent<ReActState> = AgentEvent::Failed {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            error: "rate".into(),
            envelope,
        };
        let payload = agent_event_to_payload(&event).expect("Failed has a wire projection");
        match payload {
            AgentEventPayload::Failed {
                envelope: projected,
                ..
            } => {
                assert_eq!(projected.code, "llm_rate_limited");
                assert_eq!(projected.class, "server_error");
                assert_eq!(projected.retry_after_secs, Some(11));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // Approval book-ends + non-exhaustive wildcard → None
    // ---------------------------------------------------------------------

    #[test]
    fn tool_call_approved_has_no_sse_projection() {
        let event: AgentEvent<ReActState> = AgentEvent::ToolCallApproved {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "edit_ontology".into(),
        };
        assert!(agent_event_to_payload(&event).is_none());
    }

    #[test]
    fn tool_call_denied_has_no_sse_projection() {
        let event: AgentEvent<ReActState> = AgentEvent::ToolCallDenied {
            run_id: "run-1".into(),
            tenant_id: tenant(),
            tool_use_id: "tu-1".into(),
            tool: "edit_ontology".into(),
            reason: "operator declined".into(),
        };
        assert!(agent_event_to_payload(&event).is_none());
    }

    // ---------------------------------------------------------------------
    // Envelope helpers — pure, exercised independently
    // ---------------------------------------------------------------------

    #[test]
    fn project_llm_envelope_maps_5xx_to_provider_unavailable() {
        let envelope = Error::provider_http(503, "x").envelope();
        let projected = project_llm_envelope(&envelope);
        assert_eq!(projected.code, "llm_provider_unavailable");
        assert_eq!(projected.class, "server_error");
        assert_eq!(projected.provider_status, Some(503));
    }

    #[test]
    fn project_llm_envelope_maps_401_to_auth_failed() {
        // Every `Llm*` code is server-class — the caller never set
        // the provider credential, so even a 401 at the vendor is
        // surfaced as a 5xx-class failure from the API caller's
        // perspective. The `Llm` namespace itself is the i18n
        // marker that distinguishes platform-side credential
        // failures from caller-supplied input errors.
        let envelope = Error::provider_http(401, "bad bearer").envelope();
        let projected = project_llm_envelope(&envelope);
        assert_eq!(projected.code, "llm_auth_failed");
        assert_eq!(projected.class, "server_error");
    }

    #[test]
    fn project_tool_envelope_uses_entelix_class_verbatim() {
        let envelope = Error::invalid_request("empty messages").envelope();
        let projected = project_tool_envelope(&envelope);
        assert_eq!(projected.wire_code, "invalid_request");
        assert_eq!(projected.class, "client_error");
    }

    #[test]
    fn project_tool_envelope_carries_retry_after_when_present() {
        let envelope = Error::provider_http(429, "rate")
            .with_retry_after(std::time::Duration::from_secs(7))
            .envelope();
        let projected = project_tool_envelope(&envelope);
        assert_eq!(projected.wire_code, "rate_limited");
        assert_eq!(projected.retry_after_secs, Some(7));
    }
}
