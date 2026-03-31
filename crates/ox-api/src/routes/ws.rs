use std::sync::Arc;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::Mutex;

use crate::collaboration::CollabMessage;
use crate::error::AppError;
use crate::middleware::{AuthClaims, validate_jwt};
use crate::state::AppState;

/// WebSocket collaboration endpoint.
///
/// Authentication: JWT via query parameter `?token=...`.
///
/// NOTE: In production, prefer first-message authentication pattern:
///
///   1. Accept WebSocket without auth
///   2. Require first message to be `{ type: "auth", token: "..." }`
///   3. Close connection if first message is not auth within 5 seconds
///
/// This avoids JWT exposure in server access logs and proxy caches.
pub(crate) async fn collab_ws(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<WsAuthParams>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let claims = validate_ws_token(&state, &params.token)?;
    let user_id = claims.sub.clone();
    let user_name = claims.name.clone().unwrap_or_else(|| claims.email.clone());

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, user_id, user_name)))
}

#[derive(serde::Deserialize)]
pub struct WsAuthParams {
    pub token: String,
}

fn validate_ws_token(state: &AppState, token: &str) -> Result<AuthClaims, AppError> {
    let secret = state
        .auth_config
        .jwt_secret
        .as_ref()
        .ok_or_else(|| AppError::unauthorized("JWT not configured"))?;
    validate_jwt(token, secret)
}

async fn handle_ws(socket: WebSocket, state: AppState, user_id: String, user_name: String) {
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
