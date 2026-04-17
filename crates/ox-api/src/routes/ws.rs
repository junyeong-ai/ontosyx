use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket, close_code},
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use tracing::warn;

use crate::collaboration::CollabMessage;
use crate::error::AppError;
use crate::middleware::{AuthClaims, validate_jwt};
use crate::state::AppState;

/// Maximum time the client has to send the auth frame after the WS upgrade.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Hard upper bound on the auth frame size. A real auth payload is well
/// under 4 KiB; anything larger is almost certainly an attacker trying to
/// pin memory or stall serde_json. Reject before parsing.
const AUTH_FRAME_MAX_BYTES: usize = 4 * 1024;

/// WebSocket collaboration endpoint.
///
/// Authentication: first-message protocol. The client must send
/// `{"type":"auth","token":"<jwt>"}` within 5 seconds of opening the
/// connection. The server closes the socket on timeout, parse failure,
/// or invalid token.
///
/// JWT is no longer accepted in the URL — that pattern leaks the token to
/// browser history, server access logs, and proxy caches.
pub(crate) async fn collab_ws(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state)))
}

#[derive(serde::Deserialize)]
struct AuthFrame {
    #[serde(rename = "type")]
    msg_type: String,
    token: String,
}

async fn authenticate_socket(
    state: &AppState,
    socket: &mut WebSocket,
) -> Result<AuthClaims, &'static str> {
    let secret = state
        .auth_config
        .jwt_secret
        .as_ref()
        .ok_or("JWT not configured")?;

    let first = match tokio::time::timeout(AUTH_TIMEOUT, socket.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => text,
        Ok(Some(Ok(Message::Close(_)))) => return Err("client closed before auth"),
        Ok(Some(Ok(_))) => return Err("first message must be text auth frame"),
        Ok(Some(Err(_))) => return Err("transport error"),
        Ok(None) => return Err("client disconnected before auth"),
        Err(_) => return Err("auth timeout"),
    };

    // Cap the size *before* feeding to serde_json so a 1 GiB junk
    // payload cannot pin memory in the parser. JWTs are <2 KiB; the
    // frame envelope is a handful of extra bytes.
    if first.len() > AUTH_FRAME_MAX_BYTES {
        return Err("auth frame too large");
    }

    let frame: AuthFrame =
        serde_json::from_str(&first).map_err(|_| "auth frame must be valid JSON")?;
    if frame.msg_type != "auth" {
        return Err("first message type must be \"auth\"");
    }

    validate_jwt(&frame.token, secret).map_err(|_| "invalid token")
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let claims = match authenticate_socket(&state, &mut socket).await {
        Ok(c) => c,
        Err(reason) => {
            // Mis-configuration is operationally fatal — emit `error!`
            // so it surfaces in alerting; ordinary auth failures stay
            // at `warn!` so they don't drown out signal.
            if reason == "JWT not configured" {
                tracing::error!("WebSocket auth rejected: JWT secret not configured");
            } else {
                warn!(reason, "WebSocket auth failed; closing");
            }
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: close_code::POLICY,
                    reason: reason.into(),
                })))
                .await;
            return;
        }
    };
    let user_id = claims.sub.clone();
    let user_name = claims.name.clone().unwrap_or_else(|| claims.email.clone());

    serve_collab(socket, state, user_id, user_name).await;
}

async fn serve_collab(socket: WebSocket, state: AppState, user_id: String, user_name: String) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    // Track which rooms this user has joined for cleanup on disconnect
    let mut joined_rooms: Vec<String> = Vec::new();

    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let Ok(collab_msg) = serde_json::from_str::<CollabMessage>(&text) else {
                    continue;
                };
                match collab_msg {
                    CollabMessage::Join { project_id } => {
                        let rx = state
                            .collaboration
                            .join(&project_id, &user_id, &user_name)
                            .await;
                        joined_rooms.push(project_id);

                        // Spawn a task to forward broadcast messages to this client
                        let sender_for_fwd = Arc::clone(&ws_sender);
                        tokio::spawn(forward_broadcast(rx, sender_for_fwd));
                    }
                    CollabMessage::Leave { ref project_id } => {
                        state.collaboration.leave(project_id, &user_id).await;
                        joined_rooms.retain(|r| r != project_id);
                    }
                    CollabMessage::CursorMove {
                        project_id,
                        x,
                        y,
                        selected_element,
                    } => {
                        state
                            .collaboration
                            .update_cursor(
                                &project_id,
                                &user_id,
                                &user_name,
                                x,
                                y,
                                selected_element,
                            )
                            .await;
                    }
                    CollabMessage::LockAcquire {
                        project_id,
                        entity_id,
                    } => {
                        let _ = state
                            .collaboration
                            .try_lock(&project_id, &user_id, &entity_id)
                            .await;
                    }
                    CollabMessage::LockRelease {
                        project_id,
                        entity_id,
                    } => {
                        let _ = state
                            .collaboration
                            .release_lock(&project_id, &user_id, &entity_id)
                            .await;
                    }
                    _ => {} // Server-to-client messages, ignore
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup: leave all joined rooms
    for room in &joined_rooms {
        state.collaboration.leave(room, &user_id).await;
    }
}

/// Forward broadcast messages from a collaboration room to the WebSocket sender.
async fn forward_broadcast(
    mut rx: tokio::sync::broadcast::Receiver<CollabMessage>,
    sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
) {
    while let Ok(msg) = rx.recv().await {
        if let Ok(json) = serde_json::to_string(&msg) {
            let mut guard = sender.lock().await;
            if guard.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    }
}
