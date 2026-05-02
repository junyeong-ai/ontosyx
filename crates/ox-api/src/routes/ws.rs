//! WebSocket collaboration endpoint.
//!
//! ## Auth flow
//!
//! 1. Client opens `GET /ws/collab` with the standard upgrade headers.
//! 2. Within `AUTH_TIMEOUT` the client sends one
//!    `ClientMessage::Authenticate { token, workspace_id }` frame.
//! 3. The server validates the JWT, confirms the principal can read
//!    the claimed workspace (RLS), reserves a session slot, and
//!    replies with `ServerMessage::Authenticated`.
//! 4. The remainder of the connection runs inside `WORKSPACE_ID` +
//!    `GRAPH_WORKSPACE_ID` task-locals so every store / graph call
//!    rejects cross-workspace identifiers automatically.
//!
//! JWTs never travel in the URL — that pattern leaks to access logs
//! and proxy caches. Every error path closes the socket with
//! `Message::Close` after a structured `ServerMessage::Error` so
//! clients can render i18n copy keyed on the `code`.

// Close-frame sends and broadcast publishes are fire-and-forget by
// design: the socket is on the way down on a close, and broadcast
// errors only mean "no more receivers" which is a valid state for
// presence channels.
#![allow(clippy::let_underscore_must_use)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use tokio::sync::{Mutex, broadcast};
use tracing::warn;
use uuid::Uuid;

use crate::collaboration::{ClientMessage, ErrorCode, ServerMessage, SessionHandle};
use crate::error::AppError;
use crate::middleware::{AuthClaims, check_jwt_revocation, validate_jwt};
use crate::state::AppState;

/// Maximum time the client has to send the auth frame after upgrade.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Hard ceiling on the auth frame size. Real auth payloads are well
/// under 4 KiB; anything larger is almost certainly an attacker
/// trying to pin memory or stall the JSON parser.
const AUTH_FRAME_MAX_BYTES: usize = 4 * 1024;

/// How often the connection re-checks JWT revocation /
/// `token_version`. The HTTP middleware does this on every request;
/// long-lived WS connections need a periodic equivalent so a
/// `/auth/logout` or admin revoke takes effect within the window
/// rather than waiting for the JWT's natural expiry.
const SESSION_RECHECK_INTERVAL: Duration = Duration::from_secs(60);

/// `Join` runs `get_design_project` to confirm the project belongs
/// to the bound workspace. Past authorisation results stay in this
/// per-connection cache for the TTL below — leave/re-join cycles
/// don't hammer the store. RLS still gates every other query, so a
/// cached `true` is safe even if the user's role changes mid-window.
const PROJECT_CACHE_TTL: Duration = Duration::from_secs(30);

type WsSender = Arc<Mutex<SplitSink<WebSocket, Message>>>;

pub(crate) async fn collab_ws(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state)))
}

// ---------------------------------------------------------------------------
// Auth handshake
// ---------------------------------------------------------------------------

/// Outcome of the first-frame auth exchange.
struct AuthOutcome {
    user_id: String,
    user_name: String,
    workspace_id: Uuid,
    /// Retained for periodic revocation / `token_version` rechecks
    /// over the lifetime of the connection.
    claims: AuthClaims,
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let outcome = match authenticate(&state, &mut socket).await {
        Ok(out) => out,
        Err(()) => return, // close already sent
    };

    // Reserve the per-user session slot. Drop frees it on every
    // exit path including panic.
    let session = match state.collaboration.open_session(&outcome.user_id).await {
        Some(handle) => handle,
        None => {
            let _ = send_one(&mut socket, &server_error(ErrorCode::TooManyConnections)).await;
            close_socket(&mut socket, close_code::POLICY, "too many connections").await;
            return;
        }
    };

    if send_one(
        &mut socket,
        &ServerMessage::Authenticated {
            user_id: outcome.user_id.clone(),
            user_name: outcome.user_name.clone(),
        },
    )
    .await
    .is_err()
    {
        drop(session);
        return;
    }

    let workspace_id = outcome.workspace_id;
    ox_store::WORKSPACE_ID
        .scope(
            workspace_id,
            ox_runtime::GRAPH_WORKSPACE_ID.scope(
                workspace_id,
                serve_collab(state, socket, outcome, session),
            ),
        )
        .await;
}

async fn authenticate(state: &AppState, socket: &mut WebSocket) -> Result<AuthOutcome, ()> {
    let secret = match state.auth_config.jwt_secret.as_ref() {
        Some(s) => s,
        None => {
            tracing::error!("WebSocket auth rejected: JWT secret not configured");
            let _ = send_one(socket, &server_error(ErrorCode::AuthUnavailable)).await;
            close_socket(socket, close_code::POLICY, "auth unavailable").await;
            return Err(());
        }
    };

    let first = match tokio::time::timeout(AUTH_TIMEOUT, socket.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => text,
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return Err(()),
        Ok(Some(Ok(_))) => {
            warn!("WebSocket auth: first frame was not text");
            let _ = send_one(socket, &server_error(ErrorCode::AuthRequired)).await;
            close_socket(socket, close_code::POLICY, "auth required").await;
            return Err(());
        }
        Ok(Some(Err(_))) => return Err(()),
        Err(_) => {
            let _ = send_one(socket, &server_error(ErrorCode::AuthTimeout)).await;
            close_socket(socket, close_code::POLICY, "auth timeout").await;
            return Err(());
        }
    };

    if first.len() > AUTH_FRAME_MAX_BYTES {
        let _ = send_one(socket, &server_error(ErrorCode::MalformedFrame)).await;
        close_socket(socket, close_code::POLICY, "auth frame too large").await;
        return Err(());
    }

    let Ok(parsed) = serde_json::from_str::<ClientMessage>(&first) else {
        let _ = send_one(socket, &server_error(ErrorCode::MalformedFrame)).await;
        close_socket(socket, close_code::POLICY, "malformed auth frame").await;
        return Err(());
    };

    let ClientMessage::Authenticate { token, workspace_id } = parsed else {
        let _ = send_one(socket, &server_error(ErrorCode::AuthRequired)).await;
        close_socket(socket, close_code::POLICY, "auth required").await;
        return Err(());
    };

    let claims: AuthClaims = match validate_jwt(&token, secret) {
        Ok(c) => c,
        Err(_) => {
            let _ = send_one(socket, &server_error(ErrorCode::AuthInvalid)).await;
            close_socket(socket, close_code::POLICY, "invalid token").await;
            return Err(());
        }
    };

    // Per-jti revocation + bulk `token_version` invalidation. Same
    // surface the HTTP middleware uses on every request — a token
    // rejected there must be rejected here too.
    if check_jwt_revocation(state, &claims).await.is_err() {
        let _ = send_one(socket, &server_error(ErrorCode::SessionRevoked)).await;
        close_socket(socket, close_code::POLICY, "session revoked").await;
        return Err(());
    }

    // Workspace membership is enforced by binding `WORKSPACE_ID` to
    // the claimed workspace and asking the store for its row. RLS
    // returns None when the principal isn't a member, so we don't
    // duplicate the membership predicate here.
    let store = Arc::clone(&state.store);
    let membership = ox_store::WORKSPACE_ID
        .scope(workspace_id, async move { store.get_workspace(workspace_id).await })
        .await;
    match membership {
        Ok(Some(_)) => {}
        _ => {
            let _ = send_one(socket, &server_error(ErrorCode::UnauthorizedWorkspace)).await;
            close_socket(socket, close_code::POLICY, "workspace forbidden").await;
            return Err(());
        }
    }

    let user_name = claims.name.clone().unwrap_or_else(|| claims.email.clone());
    Ok(AuthOutcome {
        user_id: claims.sub.clone(),
        user_name,
        workspace_id,
        claims,
    })
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn serve_collab(
    state: AppState,
    socket: WebSocket,
    auth: AuthOutcome,
    session: SessionHandle,
) {
    let (sender, mut receiver) = socket.split();
    let sender: WsSender = Arc::new(Mutex::new(sender));
    let AuthOutcome {
        user_id,
        user_name,
        claims,
        ..
    } = auth;

    // Per-connection state: rooms joined for disconnect cleanup,
    // project authorisation cache to avoid hammering the store on
    // every Join.
    let mut joined_rooms: HashSet<Uuid> = HashSet::new();
    let mut project_cache: HashMap<Uuid, Instant> = HashMap::new();

    let mut session_check = tokio::time::interval(SESSION_RECHECK_INTERVAL);
    session_check.tick().await; // skip the immediate first tick

    loop {
        tokio::select! {
            biased;
            _ = session_check.tick() => {
                if check_jwt_revocation(&state, &claims).await.is_err() {
                    let _ = send_via(&sender, &server_error(ErrorCode::SessionRevoked)).await;
                    close_via(&sender, close_code::POLICY, "session revoked").await;
                    break;
                }
            }
            frame = receiver.next() => {
                let Some(Ok(msg)) = frame else { break };
                let text = match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => break,
                    _ => continue,
                };

                let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) else {
                    let _ = send_via(&sender, &server_error(ErrorCode::MalformedFrame)).await;
                    continue;
                };

                if !handle_client_message(
                    &state,
                    &sender,
                    &user_id,
                    &user_name,
                    client_msg,
                    &mut joined_rooms,
                    &mut project_cache,
                )
                .await
                {
                    // Reserved for terminal cases (currently none —
                    // every variant continues the loop).
                    break;
                }
            }
        }
    }

    // Cleanup: leave every joined room.
    for room in &joined_rooms {
        state.collaboration.leave(*room, &user_id).await;
    }
    drop(session);
}

/// Process one decoded `ClientMessage`. Returns `true` to continue
/// the main loop, `false` to terminate the connection.
async fn handle_client_message(
    state: &AppState,
    sender: &WsSender,
    user_id: &str,
    user_name: &str,
    msg: ClientMessage,
    joined_rooms: &mut HashSet<Uuid>,
    project_cache: &mut HashMap<Uuid, Instant>,
) -> bool {
    match msg {
        ClientMessage::Authenticate { .. } => {
            // Re-auth mid-session is not supported.
            let _ = send_via(sender, &server_error(ErrorCode::AuthRequired)).await;
        }
        ClientMessage::Join { project_id } => {
            if !verify_project(state, project_id, project_cache).await {
                let _ = send_via(sender, &server_error(ErrorCode::UnauthorizedProject)).await;
                return true;
            }
            let outcome = state
                .collaboration
                .join(project_id, user_id, user_name)
                .await;
            joined_rooms.insert(project_id);

            // Unicast the atomic snapshot (presence + active
            // locks) to the joining socket — other members
            // already have it; only this one needs to bootstrap.
            let _ = send_via(
                sender,
                &ServerMessage::Presence {
                    project_id,
                    users: outcome.users,
                    locks: outcome.locks,
                },
            )
            .await;

            // `spawn_scoped` carries WORKSPACE_ID into the forward
            // task so any store-touching code we add later stays
            // workspace-scoped.
            let sender_for_fwd = Arc::clone(sender);
            crate::spawn_scoped::spawn_scoped(forward_broadcast(outcome.receiver, sender_for_fwd));
        }
        ClientMessage::Leave { project_id } => {
            state.collaboration.leave(project_id, user_id).await;
            joined_rooms.remove(&project_id);
        }
        ClientMessage::MoveCursor {
            project_id,
            x,
            y,
            selected_element,
        } => {
            state
                .collaboration
                .move_cursor(project_id, user_id, user_name, x, y, selected_element)
                .await;
        }
        ClientMessage::AcquireLock {
            project_id,
            entity_id,
        } => {
            let result = state
                .collaboration
                .acquire_lock(project_id, user_id, &entity_id)
                .await;
            // Always unicast to the requester. New grants are
            // also broadcast to the room (other members need to
            // know the entity is locked); idempotent refreshes
            // are private to the holder. Caller seeing the new
            // grant twice (broadcast + unicast) is benign — the
            // store reducer is idempotent.
            let _ = send_via(sender, &result).await;
        }
        ClientMessage::ReleaseLock {
            project_id,
            entity_id,
        } => {
            if let Some(err) = state
                .collaboration
                .release_lock(project_id, user_id, &entity_id)
                .await
            {
                let _ = send_via(sender, &err).await;
            }
        }
    }
    true
}

/// Authorise a `project_id` against the bound workspace. RLS
/// guarantees foreign ids resolve to `None`; the result is cached
/// for [`PROJECT_CACHE_TTL`] so Leave→Join cycles stay cheap.
async fn verify_project(
    state: &AppState,
    project_id: Uuid,
    cache: &mut HashMap<Uuid, Instant>,
) -> bool {
    let now = Instant::now();
    if cache.get(&project_id).is_some_and(|&exp| now < exp) {
        return true;
    }
    let authorised = matches!(
        state.store.get_design_project(project_id).await,
        Ok(Some(_))
    );
    if authorised {
        cache.insert(project_id, now + PROJECT_CACHE_TTL);
    } else {
        // Negative results aren't cached — operator may grant access
        // mid-session and the user shouldn't have to reconnect.
        cache.remove(&project_id);
    }
    authorised
}

// ---------------------------------------------------------------------------
// Broadcast forwarding
// ---------------------------------------------------------------------------

/// Pump server-side broadcast messages out to a single client
/// socket. On `Lagged`, send a structured `BroadcastLagged` error
/// so the FE can re-join (resync presence) instead of silently
/// dropping. Any send failure on either branch terminates the
/// task — once the socket is dead, further publishes can't reach
/// it.
async fn forward_broadcast(
    mut rx: broadcast::Receiver<ServerMessage>,
    sender: WsSender,
) {
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if send_via(&sender, &msg).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                if send_via(&sender, &server_error(ErrorCode::BroadcastLagged))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Send helpers
// ---------------------------------------------------------------------------

fn server_error(code: ErrorCode) -> ServerMessage {
    ServerMessage::Error {
        code,
        params: HashMap::new(),
    }
}

async fn send_one(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    socket.send(Message::Text(json.into())).await.map_err(|_| ())
}

async fn send_via(sender: &WsSender, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    let mut guard = sender.lock().await;
    guard.send(Message::Text(json.into())).await.map_err(|_| ())
}

async fn close_socket(socket: &mut WebSocket, code: u16, reason: &str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_string().into(),
        })))
        .await;
}

async fn close_via(sender: &WsSender, code: u16, reason: &str) {
    let mut guard = sender.lock().await;
    let _ = guard
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_string().into(),
        })))
        .await;
}
