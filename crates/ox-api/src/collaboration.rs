// `tokio::sync::broadcast::Sender::send` returns `Err(SendError)`
// only when there are zero active receivers — a legitimate "nobody
// listening" state for a presence channel, not a failure to surface.
// Every collaboration broadcast is fire-and-forget by the same
// design.
#![allow(clippy::let_underscore_must_use)]

//! Realtime collaboration — presence, cursor sharing, entity locks.
//!
//! Each project has a "room" identified by `ontology_draft_id`. WebSocket
//! clients join rooms, broadcast cursor positions and lock state
//! changes, and receive a presence snapshot on entry.
//!
//! All state is in-process (RwLock + tokio broadcast). Single-instance
//! by design — for multi-instance deployment swap [`CollaborationHub`]
//! for a Redis-backed implementation that fans out via pub/sub.
//!
//! ## Wire protocol
//!
//! Two split enums — [`ClientMessage`] (client → server) and
//! [`ServerMessage`] (server → client). Splitting the directions at
//! the type level prevents either side from accidentally producing
//! the other side's frames. Both serialize as
//! `{"type": "<variant>", ...payload}` (snake_case, internally tagged).
//!
//! Auth flow: the first frame after WS open MUST be
//! `ClientMessage::Authenticate { token, workspace_id }`. The server
//! validates the JWT, confirms the principal has membership in the
//! claimed workspace (RLS-backed), reserves a session slot
//! (`max_sessions_per_user`), and replies with
//! `ServerMessage::Authenticated`. Workspace context is bound for
//! the rest of the connection — every subsequent `Join` runs
//! through RLS and rejects cross-workspace `ontology_draft_id`s.
//!
//! ## Self-echo
//!
//! `ServerMessage::RemoteCursor` and `LockGranted` are broadcast to
//! every member, including the originator. Clients filter by their
//! own `user_id` to avoid rendering the loopback. The simpler
//! "broadcast to everyone" rule keeps the hub free of per-subscriber
//! filtering and scales the same regardless of room size.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ox_ontology::command::OntologyCommand;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, broadcast};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Wire protocol
// ---------------------------------------------------------------------------

/// Messages the client may send. The first frame after WS open MUST
/// be [`ClientMessage::Authenticate`]; anything else is rejected with
/// [`ErrorCode::AuthRequired`] and the socket closed.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Bind this connection to a JWT principal + workspace.
    Authenticate {
        token: String,
        workspace_id: Uuid,
    },
    /// Join a project room. The server responds with
    /// [`ServerMessage::Presence`] (unicast snapshot) and broadcasts
    /// [`ServerMessage::UserJoined`] to existing members.
    Join {
        ontology_draft_id: Uuid,
    },
    /// Leave a room — releases every lock the caller holds in it.
    Leave {
        ontology_draft_id: Uuid,
    },
    /// Update cursor position. Throttled per
    /// `collaboration.cursor_throttle_ms`; events inside the window
    /// are silently dropped.
    MoveCursor {
        ontology_draft_id: Uuid,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    },
    /// Request exclusive lock on an entity. Idempotent for the
    /// caller — repeated acquires by the same user refresh the TTL.
    AcquireLock {
        ontology_draft_id: Uuid,
        entity_id: String,
    },
    /// Release a lock the caller holds. No-op for foreign locks.
    ReleaseLock {
        ontology_draft_id: Uuid,
        entity_id: String,
    },
}

/// Messages the server may send. The client filters
/// `RemoteCursor.user_id == self` and `LockGranted` echoes for
/// locks it requested itself.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Auth ACK — sent once after a successful
    /// [`ClientMessage::Authenticate`].
    Authenticated {
        user_id: String,
        user_name: String,
    },
    /// Atomic snapshot of the room — unicast to a freshly joined
    /// client so it can render presence and current locks without
    /// waiting for the next broadcast frame. Existing members
    /// receive [`ServerMessage::UserJoined`] instead.
    Presence {
        ontology_draft_id: Uuid,
        users: Vec<PresenceInfo>,
        locks: Vec<LockSnapshot>,
    },
    /// Another member's cursor moved. Broadcast to all members
    /// including the originator (clients filter own user_id).
    RemoteCursor {
        ontology_draft_id: Uuid,
        user_id: String,
        user_name: String,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    },
    /// Lock granted. `held_by` is the user who now owns the lock —
    /// other members render the entity as locked-by-them, the
    /// holder renders its own affordances. `expires_at` is the
    /// TTL deadline; clients should renew or release before then.
    LockGranted {
        ontology_draft_id: Uuid,
        entity_id: String,
        held_by: String,
        expires_at: DateTime<Utc>,
    },
    /// Lock denied — `held_by` is the current owner. Unicast to the
    /// requester only; other members don't see denials.
    LockDenied {
        ontology_draft_id: Uuid,
        entity_id: String,
        held_by: String,
    },
    /// Lock released — by the holder, by leave, or by TTL expiry.
    LockReleased {
        ontology_draft_id: Uuid,
        entity_id: String,
    },
    /// Member joined the room.
    UserJoined {
        ontology_draft_id: Uuid,
        user: PresenceInfo,
    },
    /// Member left the room.
    UserLeft {
        ontology_draft_id: Uuid,
        user_id: String,
    },
    /// Another member committed ontology commands to the project.
    /// Broadcast after a successful `apply_ontology_commands` save so
    /// every collaborator's command-stack baseline can advance,
    /// surfacing the merge surface (`MergeBanner` →
    /// `CommandStackDiffDialog`) with a fully-rendered remote-ops
    /// inventory instead of the opaque "remote arrived" fallback.
    ///
    /// `base_revision` is the revision the commit was authored
    /// against; `new_revision` is the revision after the commit
    /// landed; `commands` is the exact ordered op list the author
    /// applied (oldest first).
    EntityUpdated {
        ontology_draft_id: Uuid,
        author_user_id: String,
        author_user_name: String,
        base_revision: i32,
        new_revision: i32,
        /// `OntologyCommand` is internally tagged JSON; utoipa can't
        /// derive a static schema for the runtime variant set, so the
        /// wire shape surfaces as `Vec<Object>` in OpenAPI and the FE
        /// reads it through the typed `OntologyCommand` union.
        #[schema(value_type = Vec<Object>)]
        commands: Vec<OntologyCommand>,
    },
    /// Structured error. The frontend renders the message via i18n
    /// keyed on `code`; `params` interpolate into the localised
    /// string. The backend never produces user-facing prose
    /// directly — see `crates/ox-api/CLAUDE.md`
    /// "Language-neutral wire shape".
    Error {
        code: ErrorCode,
        params: HashMap<String, String>,
    },
}

/// Stable identifier for collaboration error conditions. The FE
/// catalogue maps each variant to a localised message.
#[derive(Debug, Clone, Copy, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// First frame wasn't `Authenticate`, or auth never completed.
    AuthRequired,
    /// Auth frame arrived but the JWT is invalid / expired.
    AuthInvalid,
    /// JWT subsystem isn't configured on the server.
    AuthUnavailable,
    /// No frame arrived inside the auth timeout window.
    AuthTimeout,
    /// JSON parse failed on a frame.
    MalformedFrame,
    /// `workspace_id` claimed at auth doesn't match the principal's
    /// memberships.
    UnauthorizedWorkspace,
    /// `ontology_draft_id` doesn't belong to the bound workspace.
    UnauthorizedOntologyDraft,
    /// Per-user concurrent connection cap reached.
    TooManyConnections,
    /// Broadcast channel lagged — the receiver couldn't keep up.
    /// Clients should re-join the room to resync presence.
    BroadcastLagged,
    /// Lock / cursor / leave op arrived for a room the caller
    /// hasn't `Join`ed. Indicates a client bug.
    NotJoined,
    /// JWT was revoked (per-jti) or `token_version` advanced
    /// (bulk invalidation) since the connection authenticated.
    /// The client must reconnect with a fresh token.
    SessionRevoked,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PresenceInfo {
    pub user_id: String,
    pub user_name: String,
    pub joined_at: DateTime<Utc>,
    pub cursor: Option<CursorPosition>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CursorPosition {
    pub x: f64,
    pub y: f64,
    pub selected_element: Option<String>,
}

/// One element of the lock snapshot a freshly joined client
/// receives. Mirrors [`ServerMessage::LockGranted`] minus the
/// `ontology_draft_id` (the surrounding `Presence` frame already carries
/// it).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct LockSnapshot {
    pub entity_id: String,
    pub held_by: String,
    pub expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct RoomMember {
    user_name: String,
    joined_at: DateTime<Utc>,
    cursor: Option<CursorPosition>,
    /// Last accepted cursor event (for throttling). `None` until
    /// the first `MoveCursor`.
    last_cursor_at: Option<Instant>,
    /// Wall-clock timestamp of the most recent client frame from
    /// this member — refreshed by every `MoveCursor` / `AcquireLock`
    /// / `ReleaseLock`. Idle members past `idle_timeout` are reaped
    /// by [`CollaborationHub::reap_idle_members`] so a hung tab
    /// doesn't leave ghost presence in the room.
    last_activity_at: DateTime<Utc>,
}

struct LockEntry {
    held_by: String,
    expires_at: DateTime<Utc>,
}

struct Room {
    members: HashMap<String, RoomMember>, // user_id → member
    locks: HashMap<String, LockEntry>, // entity_id → lock
    broadcast: broadcast::Sender<ServerMessage>,
}

impl Room {
    fn new(buffer: usize) -> Self {
        let (tx, _) = broadcast::channel(buffer);
        Self {
            members: HashMap::new(),
            locks: HashMap::new(),
            broadcast: tx,
        }
    }

    fn presence_snapshot(&self) -> Vec<PresenceInfo> {
        self.members
            .iter()
            .map(|(user_id, m)| PresenceInfo {
                user_id: user_id.clone(),
                user_name: m.user_name.clone(),
                joined_at: m.joined_at,
                cursor: m.cursor.clone(),
            })
            .collect()
    }

    fn lock_snapshot(&self) -> Vec<LockSnapshot> {
        self.locks
            .iter()
            .map(|(entity_id, lock)| LockSnapshot {
                entity_id: entity_id.clone(),
                held_by: lock.held_by.clone(),
                expires_at: lock.expires_at,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Hub
// ---------------------------------------------------------------------------

/// Tuning knobs surfaced through `OxConfig.collaboration`.
#[derive(Debug, Clone)]
pub struct HubLimits {
    pub broadcast_buffer: usize,
    pub lock_ttl: Duration,
    pub max_sessions_per_user: usize,
    pub cursor_throttle: Duration,
    /// Members with no client frames for this long are reaped on
    /// the next [`CollaborationHub::reap_idle_members`] pass —
    /// covers the case where the browser tab dies without sending
    /// a `Close` frame (process kill, crashed renderer, NAT
    /// reset). The reap publishes `UserLeft` so other members see
    /// presence converge.
    pub idle_timeout: Duration,
}

/// Outcome of [`CollaborationHub::join`] — the broadcast receiver
/// the WS handler subscribes to plus an atomic snapshot of the
/// room (presence + active locks) it forwards directly to the
/// joining socket.
pub struct JoinOutcome {
    pub receiver: broadcast::Receiver<ServerMessage>,
    pub users: Vec<PresenceInfo>,
    pub locks: Vec<LockSnapshot>,
}

/// Hub-wide gauges surfaced through the Prometheus `/metrics`
/// endpoint. Per-room cardinality is intentionally omitted —
/// `ontology_draft_id` would explode the label space and cripple the
/// scrape. Operators wanting per-room visibility can pull it from
/// the structured tracing logs.
#[derive(Debug, Clone, Copy)]
pub struct HubStats {
    pub active_rooms: usize,
    pub active_sessions: usize,
}

/// RAII guard returned by [`open_session`]. The session slot is
/// freed when the handle drops — even on panic / error paths in
/// the WS handler.
pub struct SessionHandle {
    hub: Arc<dyn CollaborationHub>,
    user_id: String,
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let hub = Arc::clone(&self.hub);
        let user_id = std::mem::take(&mut self.user_id);
        // `release_session` only mutates the hub's in-memory
        // session-count map; it never touches the store, so it's
        // independent of the WORKSPACE_ID / SYSTEM_BYPASS
        // task-locals that `spawn_scoped` exists to preserve.
        // Plain `tokio::spawn` is correct here.
        #[allow(clippy::disallowed_methods)]
        tokio::spawn(async move { hub.release_session(&user_id).await });
    }
}

/// Realtime-collaboration backend abstraction. The default
/// in-process implementation lives in
/// [`InProcessCollaborationHub`] and covers single-instance
/// deployments; a future Redis / NATS-backed implementation can
/// drop in without touching the WS handler or the workbench
/// layout. `AppState` holds an `Arc<dyn CollaborationHub>`, so
/// every consumer reads the same surface.
#[async_trait]
pub trait CollaborationHub: Send + Sync {
    /// Reserve a session slot for `user_id`. Returns `false` when
    /// the per-user concurrent cap is reached. Pair every
    /// successful reservation with [`Self::release_session`] —
    /// the [`SessionHandle`] returned by [`open_session`]
    /// automates that.
    async fn try_reserve_session(&self, user_id: &str) -> bool;

    /// Free a session slot reserved through
    /// [`Self::try_reserve_session`]. No-op when no slot is held.
    async fn release_session(&self, user_id: &str);

    /// Join a project room. Broadcasts `UserJoined` to existing
    /// members; returns a fresh receiver plus a unicast snapshot
    /// (presence + active locks) the WS handler forwards to the
    /// joining socket.
    async fn join(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        user_name: &str,
    ) -> JoinOutcome;

    /// Leave a project room. Releases every lock held by `user_id`
    /// and broadcasts `UserLeft` plus per-lock `LockReleased`.
    async fn leave(&self, ontology_draft_id: Uuid, user_id: &str);

    /// Broadcast a cursor move. Throttled per `cursor_throttle`;
    /// calls inside the throttle window or for unknown users /
    /// rooms are silently dropped — cursor data is lossy by
    /// design. The hub reads `user_name` from the joined
    /// `RoomMember`, so callers don't pass it on every frame.
    async fn move_cursor(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    );

    /// Try to acquire an exclusive lock. See
    /// [`InProcessCollaborationHub::acquire_lock`] for the result
    /// semantics — every implementation honours the same
    /// `LockGranted` / `LockDenied` / `Error{NotJoined}`
    /// triple, broadcasting new grants and unicasting
    /// idempotent refreshes.
    async fn acquire_lock(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        entity_id: &str,
    ) -> ServerMessage;

    /// Release a lock owned by `user_id`. `None` on success
    /// (`LockReleased` was broadcast or the lock didn't exist);
    /// `Some(Error { NotJoined })` when the caller hasn't joined.
    async fn release_lock(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        entity_id: &str,
    ) -> Option<ServerMessage>;

    /// Sweep idle members past the configured timeout and `leave`
    /// them. Returns the count reaped.
    async fn reap_idle_members(&self) -> usize;

    /// Snapshot of hub-wide gauges.
    async fn stats(&self) -> HubStats;

    /// Broadcast an `EntityUpdated` frame after the author's
    /// `apply_ontology_commands` save lands. Fire-and-forget; rooms
    /// without subscribers silently drop. Called from the HTTP
    /// handler so the realtime channel and the persistence layer
    /// stay decoupled — the WS protocol is the only realtime
    /// surface, the HTTP response remains the authoritative path.
    async fn broadcast_entity_updated(
        &self,
        ontology_draft_id: Uuid,
        author_user_id: &str,
        author_user_name: &str,
        base_revision: i32,
        new_revision: i32,
        commands: Vec<OntologyCommand>,
    );
}

/// Reserve a session slot and wrap it in a [`SessionHandle`] that
/// frees the slot on drop. Free function rather than a trait
/// method because it has to capture an owned `Arc<dyn ...>` for
/// the guard's `Drop` impl — trait methods can't return guards
/// that outlive the reference they were called through.
pub async fn open_session(
    hub: Arc<dyn CollaborationHub>,
    user_id: &str,
) -> Option<SessionHandle> {
    if !hub.try_reserve_session(user_id).await {
        return None;
    }
    Some(SessionHandle {
        hub,
        user_id: user_id.to_string(),
    })
}

/// In-process implementation of [`CollaborationHub`]. Single-
/// instance by design; for multi-instance deployment swap this
/// for a pub/sub-backed implementation that fans broadcast frames
/// across the cluster.
pub struct InProcessCollaborationHub {
    rooms: RwLock<HashMap<Uuid, Arc<Mutex<Room>>>>,
    sessions: RwLock<HashMap<String, usize>>, // user_id → active session count
    limits: HubLimits,
}

impl InProcessCollaborationHub {
    pub fn new(limits: HubLimits) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            limits,
        }
    }

    /// Borrow (or create) the room arc. Two-phase locking keeps
    /// the common case (room exists) on the read path.
    async fn room(&self, ontology_draft_id: Uuid) -> Arc<Mutex<Room>> {
        if let Some(room) = self.rooms.read().await.get(&ontology_draft_id) {
            return Arc::clone(room);
        }
        let mut rooms = self.rooms.write().await;
        Arc::clone(
            rooms
                .entry(ontology_draft_id)
                .or_insert_with(|| Arc::new(Mutex::new(Room::new(self.limits.broadcast_buffer)))),
        )
    }
}

#[async_trait]
impl CollaborationHub for InProcessCollaborationHub {
    async fn try_reserve_session(&self, user_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        let count = sessions.entry(user_id.to_string()).or_insert(0);
        if *count >= self.limits.max_sessions_per_user {
            return false;
        }
        *count += 1;
        true
    }

    async fn release_session(&self, user_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(count) = sessions.get_mut(user_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                sessions.remove(user_id);
            }
        }
    }

    async fn join(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        user_name: &str,
    ) -> JoinOutcome {
        let room_arc = self.room(ontology_draft_id).await;
        let mut room = room_arc.lock().await;

        let now = Utc::now();
        room.members.insert(
            user_id.to_string(),
            RoomMember {
                user_name: user_name.to_string(),
                joined_at: now,
                cursor: None,
                last_cursor_at: None,
                last_activity_at: now,
            },
        );

        let users = room.presence_snapshot();
        let locks = room.lock_snapshot();
        // Subscribe before broadcasting so this connection receives
        // every frame from its own join onward — including the
        // `UserJoined` it just emitted. Drainage is the FE's
        // responsibility (presence is also delivered via
        // `JoinOutcome`, so the duplicate is harmless).
        let receiver = room.broadcast.subscribe();
        let _ = room.broadcast.send(ServerMessage::UserJoined {
            ontology_draft_id,
            user: PresenceInfo {
                user_id: user_id.to_string(),
                user_name: user_name.to_string(),
                joined_at: now,
                cursor: None,
            },
        });

        JoinOutcome {
            receiver,
            users,
            locks,
        }
    }

    /// Leave a room. Releases every lock held by `user_id` and
    /// broadcasts `UserLeft` plus per-lock `LockReleased`.
    async fn leave(&self, ontology_draft_id: Uuid, user_id: &str) {
        let room_arc = match self.rooms.read().await.get(&ontology_draft_id) {
            Some(arc) => Arc::clone(arc),
            None => return,
        };
        let mut room = room_arc.lock().await;
        room.members.remove(user_id);

        let released: Vec<String> = room
            .locks
            .iter()
            .filter(|(_, lock)| lock.held_by == user_id)
            .map(|(id, _)| id.clone())
            .collect();
        for entity_id in &released {
            room.locks.remove(entity_id);
            let _ = room.broadcast.send(ServerMessage::LockReleased {
                ontology_draft_id,
                entity_id: entity_id.clone(),
            });
        }
        let _ = room.broadcast.send(ServerMessage::UserLeft {
            ontology_draft_id,
            user_id: user_id.to_string(),
        });

        let empty = room.members.is_empty();
        drop(room);
        // Drop our outstanding Arc before the strong-count check
        // below so it sees only the registry's reference.
        drop(room_arc);

        if empty {
            let mut rooms = self.rooms.write().await;
            // Re-check under the write lock: another task may have
            // re-joined the room between our `drop(room)` and here,
            // and `Arc::strong_count` lets us detect that without
            // double-locking the inner mutex.
            if let Some(arc) = rooms.get(&ontology_draft_id)
                && Arc::strong_count(arc) == 1
            {
                rooms.remove(&ontology_draft_id);
            }
        }
    }

    /// Move a cursor. Throttled per `cursor_throttle`. Calls
    /// inside the throttle window or for unknown users / rooms
    /// are silently dropped — cursor data is lossy by design. The
    /// hub reads `user_name` from the joined member, so callers
    /// only pass `user_id`.
    async fn move_cursor(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    ) {
        let room_arc = match self.rooms.read().await.get(&ontology_draft_id) {
            Some(arc) => Arc::clone(arc),
            None => return,
        };
        let mut room = room_arc.lock().await;
        let now = Instant::now();

        let Some(member) = room.members.get_mut(user_id) else {
            return;
        };
        if let Some(prev) = member.last_cursor_at
            && now.duration_since(prev) < self.limits.cursor_throttle
        {
            return;
        }
        member.last_cursor_at = Some(now);
        member.last_activity_at = Utc::now();
        member.cursor = Some(CursorPosition {
            x,
            y,
            selected_element: selected_element.clone(),
        });
        // Capture the stored display name for the broadcast — the
        // wire frame still carries it so receivers don't need a
        // separate roster fetch on every cursor move.
        let user_name = member.user_name.clone();

        let _ = room.broadcast.send(ServerMessage::RemoteCursor {
            ontology_draft_id,
            user_id: user_id.to_string(),
            user_name,
            x,
            y,
            selected_element,
        });
    }

    /// Try to acquire an exclusive lock. Stale locks (past
    /// `expires_at`) are reaped before contention is decided.
    /// Returns the resulting message:
    ///
    /// * `LockGranted` — broadcast to every room member; clients
    ///   filter their own request via `entity_id`.
    /// * `LockDenied` — **not** broadcast; the WS handler unicasts
    ///   it to the requester so other members don't see denials
    ///   that aren't relevant to them.
    /// * `Error { code: NotJoined }` — the caller hasn't `Join`ed
    ///   the room. Indicates a client bug; unicast to the caller.
    async fn acquire_lock(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        entity_id: &str,
    ) -> ServerMessage {
        let room_arc = self.room(ontology_draft_id).await;
        let mut room = room_arc.lock().await;

        if let Some(member) = room.members.get_mut(user_id) {
            member.last_activity_at = Utc::now();
        } else {
            return ServerMessage::Error {
                code: ErrorCode::NotJoined,
                params: HashMap::new(),
            };
        }

        let now = Utc::now();
        let ttl = chrono::Duration::from_std(self.limits.lock_ttl).unwrap_or(chrono::Duration::seconds(300));
        let expires_at = now + ttl;

        // Evaluate existing entry first; pull the bits we need so
        // the &mut borrow below doesn't conflict.
        let existing = room
            .locks
            .get(entity_id)
            .map(|l| (l.held_by.clone(), l.expires_at));

        if let Some((held_by, exp)) = existing {
            if held_by == user_id {
                // Idempotent refresh — caller-only signal. Other
                // members already see the entity as locked-by-this-
                // user, so a TTL bump is private. The WS handler
                // unicasts the `LockGranted` to the requester.
                if let Some(lock) = room.locks.get_mut(entity_id) {
                    lock.expires_at = expires_at;
                }
                return ServerMessage::LockGranted {
                    ontology_draft_id,
                    entity_id: entity_id.to_string(),
                    held_by: user_id.to_string(),
                    expires_at,
                };
            }
            if exp > now {
                // Still held — deny without broadcast.
                return ServerMessage::LockDenied {
                    ontology_draft_id,
                    entity_id: entity_id.to_string(),
                    held_by,
                };
            }
            // TTL expired — fall through and replace.
        }

        room.locks.insert(
            entity_id.to_string(),
            LockEntry {
                held_by: user_id.to_string(),
                expires_at,
            },
        );
        let msg = ServerMessage::LockGranted {
            ontology_draft_id,
            entity_id: entity_id.to_string(),
            held_by: user_id.to_string(),
            expires_at,
        };
        let _ = room.broadcast.send(msg.clone());
        msg
    }

    /// Release a lock owned by `user_id`.
    ///
    /// Returns `None` on success (`LockReleased` was broadcast, or
    /// the entity wasn't locked / was held by someone else — both
    /// idempotent on the wire). Returns `Some(Error { NotJoined })`
    /// when the caller hasn't joined the room — a client bug the
    /// WS handler unicasts back to the requester.
    async fn release_lock(
        &self,
        ontology_draft_id: Uuid,
        user_id: &str,
        entity_id: &str,
    ) -> Option<ServerMessage> {
        let room_arc = self.rooms.read().await.get(&ontology_draft_id).cloned()?;
        let mut room = room_arc.lock().await;

        if let Some(member) = room.members.get_mut(user_id) {
            member.last_activity_at = Utc::now();
        } else {
            return Some(ServerMessage::Error {
                code: ErrorCode::NotJoined,
                params: HashMap::new(),
            });
        }

        let owns = matches!(room.locks.get(entity_id), Some(lock) if lock.held_by == user_id);
        if !owns {
            return None;
        }
        room.locks.remove(entity_id);
        let _ = room.broadcast.send(ServerMessage::LockReleased {
            ontology_draft_id,
            entity_id: entity_id.to_string(),
        });
        None
    }

    /// Snapshot of hub-wide gauges. Called per `/metrics` scrape;
    /// cheap because both reads are O(1) on the registry maps.
    async fn stats(&self) -> HubStats {
        let active_rooms = self.rooms.read().await.len();
        let active_sessions: usize = self.sessions.read().await.values().sum();
        HubStats {
            active_rooms,
            active_sessions,
        }
    }

    /// Sweep every room for members whose last frame is older than
    /// `idle_timeout` and `leave` them. Called from a background
    /// timer; covers the dead-tab case where the browser never
    /// sent a `Close` frame so the WS handler's cleanup loop
    /// never ran.
    ///
    /// Returns the number of members reaped — useful for tracing
    /// / metrics.
    async fn reap_idle_members(&self) -> usize {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(self.limits.idle_timeout)
                .unwrap_or(chrono::Duration::seconds(300));

        // Snapshot the (room, user) pairs to reap so we don't hold
        // the registry lock while taking per-room mutexes.
        let mut victims: Vec<(Uuid, Vec<String>)> = Vec::new();
        {
            let rooms = self.rooms.read().await;
            for (ontology_draft_id, arc) in rooms.iter() {
                let room = arc.lock().await;
                let mut idle: Vec<String> = Vec::new();
                for (user_id, member) in room.members.iter() {
                    if member.last_activity_at < cutoff {
                        idle.push(user_id.clone());
                    }
                }
                if !idle.is_empty() {
                    victims.push((*ontology_draft_id, idle));
                }
            }
        }

        let mut total = 0usize;
        for (ontology_draft_id, users) in victims {
            for user_id in users {
                self.leave(ontology_draft_id, &user_id).await;
                total += 1;
            }
        }
        total
    }

    /// Broadcast `EntityUpdated` to every subscriber of `ontology_draft_id`.
    /// Drops silently when the room hasn't been opened yet (no
    /// active subscribers) — collaboration broadcasts are
    /// fire-and-forget by design; the HTTP response remains the
    /// authoritative path for the author's own UI.
    async fn broadcast_entity_updated(
        &self,
        ontology_draft_id: Uuid,
        author_user_id: &str,
        author_user_name: &str,
        base_revision: i32,
        new_revision: i32,
        commands: Vec<OntologyCommand>,
    ) {
        let Some(room_arc) = self.rooms.read().await.get(&ontology_draft_id).cloned() else {
            return;
        };
        let room = room_arc.lock().await;
        let _ = room.broadcast.send(ServerMessage::EntityUpdated {
            ontology_draft_id,
            author_user_id: author_user_id.to_string(),
            author_user_name: author_user_name.to_string(),
            base_revision,
            new_revision,
            commands,
        });
    }
}

// `Default` is intentionally not implemented: the hub always needs
// explicit limits driven by `OxConfig.collaboration`.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_limits() -> HubLimits {
        HubLimits {
            broadcast_buffer: 32,
            lock_ttl: Duration::from_secs(300),
            max_sessions_per_user: 3,
            cursor_throttle: Duration::from_millis(50),
            idle_timeout: Duration::from_secs(300),
        }
    }

    #[tokio::test]
    async fn move_cursor_preserves_selected_element() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        let _outcome = hub.join(project, "u1", "Alice").await;
        // Drain the UserJoined frame so move_cursor's broadcast is the next one.
        let outcome = hub.join(project, "u2", "Bob").await;
        let mut rx = outcome.receiver;
        let _ = rx.recv().await; // UserJoined for whichever order

        hub.move_cursor(project, "u1", 10.0, 20.0, Some("node-42".into()))
            .await;

        // Read frames until we see the RemoteCursor for u1.
        let mut found = None;
        for _ in 0..4 {
            if let Ok(msg) = rx.try_recv() {
                if let ServerMessage::RemoteCursor {
                    user_id,
                    selected_element,
                    ..
                } = msg
                {
                    if user_id == "u1" {
                        found = selected_element;
                        break;
                    }
                }
            } else {
                tokio::task::yield_now().await;
            }
        }
        assert_eq!(found.as_deref(), Some("node-42"));
    }

    #[tokio::test]
    async fn cursor_throttle_drops_floods() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        let _ = hub.join(project, "u1", "Alice").await;

        // 10 events back-to-back; only the first survives.
        for i in 0..10 {
            hub.move_cursor(project, "u1", i as f64, 0.0, None).await;
        }
        // Exactly one cursor was stored (last_cursor_at gates the rest).
        let rooms = hub.rooms.read().await;
        let arc = rooms.get(&project).unwrap().clone();
        drop(rooms);
        let room = arc.lock().await;
        let m = room.members.get("u1").unwrap();
        assert_eq!(m.cursor.as_ref().unwrap().x, 0.0);
    }

    #[tokio::test]
    async fn acquire_lock_idempotent_for_same_user() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        let _ = hub.join(project, "u1", "Alice").await;

        let g1 = hub.acquire_lock(project, "u1", "ent-1").await;
        let g2 = hub.acquire_lock(project, "u1", "ent-1").await;
        match (g1, g2) {
            (
                ServerMessage::LockGranted {
                    held_by: h1, ..
                },
                ServerMessage::LockGranted {
                    held_by: h2, ..
                },
            ) => {
                assert_eq!(h1, "u1");
                assert_eq!(h2, "u1");
            }
            other => panic!("expected LockGranted pair, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn acquire_lock_denied_for_other_user() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        let _ = hub.join(project, "u1", "Alice").await;
        let _ = hub.join(project, "u2", "Bob").await;

        let _ = hub.acquire_lock(project, "u1", "ent-1").await;
        let denied = hub.acquire_lock(project, "u2", "ent-1").await;
        match denied {
            ServerMessage::LockDenied { held_by, .. } => assert_eq!(held_by, "u1"),
            other => panic!("expected LockDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn acquire_lock_reaps_expired_holder() {
        let limits = HubLimits {
            lock_ttl: Duration::from_millis(20),
            ..test_limits()
        };
        let hub = Arc::new(InProcessCollaborationHub::new(limits));
        let project = Uuid::new_v4();
        let _ = hub.join(project, "u1", "Alice").await;
        let _ = hub.join(project, "u2", "Bob").await;

        let _ = hub.acquire_lock(project, "u1", "ent-1").await;
        tokio::time::sleep(Duration::from_millis(40)).await;

        let granted = hub.acquire_lock(project, "u2", "ent-1").await;
        assert!(matches!(granted, ServerMessage::LockGranted { .. }));
    }

    #[tokio::test]
    async fn leave_releases_locks_and_reaps_empty_room() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        let _ = hub.join(project, "u1", "Alice").await;
        let _ = hub.acquire_lock(project, "u1", "ent-1").await;

        hub.leave(project, "u1").await;
        assert_eq!(hub.stats().await.active_rooms, 0);
    }

    #[tokio::test]
    async fn acquire_lock_rejects_unjoined_caller() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        // u1 hasn't joined — direct lock attempt should yield NotJoined.
        let result = hub.acquire_lock(project, "u1", "ent-1").await;
        match result {
            ServerMessage::Error { code, .. } => {
                assert!(matches!(code, ErrorCode::NotJoined));
            }
            other => panic!("expected NotJoined error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn release_lock_signals_unjoined_caller() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();
        // First seed a room with a different user so the registry
        // entry exists; otherwise release_lock would early-return None.
        let _ = hub.join(project, "u1", "Alice").await;
        let result = hub.release_lock(project, "u2", "ent-1").await;
        match result {
            Some(ServerMessage::Error { code, .. }) => {
                assert!(matches!(code, ErrorCode::NotJoined));
            }
            other => panic!("expected NotJoined error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_session_caps_concurrent_per_user() {
        let limits = HubLimits {
            max_sessions_per_user: 2,
            ..test_limits()
        };
        let hub: Arc<dyn CollaborationHub> = Arc::new(InProcessCollaborationHub::new(limits));

        let h1 = open_session(Arc::clone(&hub), "u1").await;
        let h2 = open_session(Arc::clone(&hub), "u1").await;
        let h3 = open_session(Arc::clone(&hub), "u1").await;
        assert!(h1.is_some());
        assert!(h2.is_some());
        assert!(h3.is_none());

        drop(h1);
        // The Drop impl spawns the close — let it run.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let h4 = open_session(Arc::clone(&hub), "u1").await;
        assert!(h4.is_some());
    }

    #[tokio::test]
    async fn join_emits_presence_snapshot() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();

        let _ = hub.join(project, "u1", "Alice").await;
        let outcome = hub.join(project, "u2", "Bob").await;

        // u2's snapshot includes both members.
        let names: Vec<String> = outcome.users.iter().map(|p| p.user_id.clone()).collect();
        assert!(names.contains(&"u1".to_string()));
        assert!(names.contains(&"u2".to_string()));
    }

    #[tokio::test]
    async fn reap_idle_members_evicts_silent_clients() {
        let limits = HubLimits {
            idle_timeout: Duration::from_millis(20),
            ..test_limits()
        };
        let hub = Arc::new(InProcessCollaborationHub::new(limits));
        let project = Uuid::new_v4();

        let _ = hub.join(project, "u1", "Alice").await;
        let _ = hub.join(project, "u2", "Bob").await;

        // u1 stays active by sending a cursor; u2 goes silent.
        tokio::time::sleep(Duration::from_millis(15)).await;
        hub.move_cursor(project, "u1", 0.0, 0.0, None).await;
        tokio::time::sleep(Duration::from_millis(15)).await;

        let reaped = hub.reap_idle_members().await;
        assert_eq!(reaped, 1, "expected u2 to be reaped");
        // u1 still in the room — fetched via snapshot from a third joiner.
        let outcome = hub.join(project, "u3", "Carol").await;
        let names: Vec<String> = outcome.users.iter().map(|p| p.user_id.clone()).collect();
        assert!(names.contains(&"u1".to_string()));
        assert!(!names.contains(&"u2".to_string()));
    }

    #[tokio::test]
    async fn join_includes_active_locks_in_snapshot() {
        let hub = Arc::new(InProcessCollaborationHub::new(test_limits()));
        let project = Uuid::new_v4();

        let _ = hub.join(project, "u1", "Alice").await;
        let _ = hub.acquire_lock(project, "u1", "ent-1").await;

        // u2 joins later — its snapshot must include u1's existing lock.
        let outcome = hub.join(project, "u2", "Bob").await;
        assert_eq!(outcome.locks.len(), 1);
        assert_eq!(outcome.locks[0].entity_id, "ent-1");
        assert_eq!(outcome.locks[0].held_by, "u1");
    }
}
