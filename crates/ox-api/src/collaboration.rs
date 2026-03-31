// ---------------------------------------------------------------------------
// Collaboration — real-time presence and cursor sharing
// ---------------------------------------------------------------------------
// WebSocket-based collaboration for multi-user ontology editing.
//
// Architecture:
//   - Each project has a "room" (channel) identified by project_id
//   - Users join/leave rooms via WebSocket
//   - Presence (who is online) broadcasts to all room members
//   - Cursor positions broadcast to all room members (except sender)
//   - Entity locks prevent concurrent edits on the same node/edge
//
// No external dependencies (Redis, etc.) — in-process state.
// Sufficient for single-instance deployment. For multi-instance,
// replace RoomState with a Redis-backed implementation.
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

// ---------------------------------------------------------------------------
// Protocol messages (client ↔ server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollabMessage {
    /// Client → Server: Join a project room
    Join { project_id: String },
    /// Client → Server: Leave a project room
    Leave { project_id: String },
    /// Client → Server: Update cursor position on canvas
    CursorMove {
        project_id: String,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    },
    /// Client → Server: Request exclusive lock on an entity
    LockAcquire {
        project_id: String,
        entity_id: String,
    },
    /// Client → Server: Release a lock
    LockRelease {
        project_id: String,
        entity_id: String,
    },

    // --- Server → Client messages ---
    /// Server → Client: Current room presence
    Presence {
        project_id: String,
        users: Vec<PresenceInfo>,
    },
    /// Server → Client: Another user's cursor moved
    RemoteCursor {
        project_id: String,
        user_id: String,
        user_name: String,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    },
    /// Server → Client: Lock granted
    LockGranted {
        project_id: String,
        entity_id: String,
    },
    /// Server → Client: Lock denied (held by another user)
    LockDenied {
        project_id: String,
        entity_id: String,
        held_by: String,
    },
    /// Server → Client: Lock released (by any user)
    LockReleased {
        project_id: String,
        entity_id: String,
    },
    /// Server → Client: User joined the room
    UserJoined {
        project_id: String,
        user: PresenceInfo,
    },
    /// Server → Client: User left the room
    UserLeft { project_id: String, user_id: String },
    /// Server → Client: Error message
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub user_id: String,
    pub user_name: String,
    pub joined_at: DateTime<Utc>,
    pub cursor: Option<CursorPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub x: f64,
    pub y: f64,
    pub selected_element: Option<String>,
}

// ---------------------------------------------------------------------------
// Room state — per-project collaboration context
// ---------------------------------------------------------------------------

#[allow(dead_code)] // Fields read via presence_list(); compiler can't trace through Room methods
struct RoomMember {
    user_id: String,
    user_name: String,
    joined_at: DateTime<Utc>,
    cursor: Option<CursorPosition>,
}

#[allow(dead_code)] // Fields read in try_lock/release_lock; awaiting WebSocket route integration
struct EntityLock {
    held_by: String,
    acquired_at: DateTime<Utc>,
}

#[allow(dead_code)] // Fields used by CollaborationHub methods; awaiting WebSocket route integration
struct Room {
    members: HashMap<String, RoomMember>, // user_id → member
    locks: HashMap<String, EntityLock>,   // entity_id → lock
    broadcast: broadcast::Sender<CollabMessage>,
}

#[allow(dead_code)]
impl Room {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            members: HashMap::new(),
            locks: HashMap::new(),
            broadcast: tx,
        }
    }

    fn presence_list(&self) -> Vec<PresenceInfo> {
        self.members
            .values()
            .map(|m| PresenceInfo {
                user_id: m.user_id.clone(),
                user_name: m.user_name.clone(),
                joined_at: m.joined_at,
                cursor: m.cursor.clone(),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// CollaborationHub — manages all rooms
// ---------------------------------------------------------------------------

pub struct CollaborationHub {
    #[allow(dead_code)] // Used by all Hub methods; awaiting WebSocket route integration
    rooms: RwLock<HashMap<String, Room>>, // project_id → room
}

#[allow(dead_code)] // All methods awaiting WebSocket route integration
impl CollaborationHub {
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
        }
    }

    /// Join a project room. Returns a broadcast receiver for room events.
    pub async fn join(
        &self,
        project_id: &str,
        user_id: &str,
        user_name: &str,
    ) -> broadcast::Receiver<CollabMessage> {
        let mut rooms = self.rooms.write().await;
        let room = rooms
            .entry(project_id.to_string())
            .or_insert_with(Room::new);

        let member = RoomMember {
            user_id: user_id.to_string(),
            user_name: user_name.to_string(),
            joined_at: Utc::now(),
            cursor: None,
        };

        room.members.insert(user_id.to_string(), member);

        // Broadcast join to other members
        let _ = room.broadcast.send(CollabMessage::UserJoined {
            project_id: project_id.to_string(),
            user: PresenceInfo {
                user_id: user_id.to_string(),
                user_name: user_name.to_string(),
                joined_at: Utc::now(),
                cursor: None,
            },
        });

        room.broadcast.subscribe()
    }

    /// Leave a project room. Releases any held locks.
    pub async fn leave(&self, project_id: &str, user_id: &str) {
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get_mut(project_id) {
            room.members.remove(user_id);

            // Release all locks held by this user
            let released: Vec<String> = room
                .locks
                .iter()
                .filter(|(_, lock)| lock.held_by == user_id)
                .map(|(id, _)| id.clone())
                .collect();

            for entity_id in &released {
                room.locks.remove(entity_id);
                let _ = room.broadcast.send(CollabMessage::LockReleased {
                    project_id: project_id.to_string(),
                    entity_id: entity_id.clone(),
                });
            }

            let _ = room.broadcast.send(CollabMessage::UserLeft {
                project_id: project_id.to_string(),
                user_id: user_id.to_string(),
            });

            // Clean up empty rooms
            if room.members.is_empty() {
                rooms.remove(project_id);
            }
        }
    }

    /// Update cursor position for a user in a room.
    pub async fn update_cursor(
        &self,
        project_id: &str,
        user_id: &str,
        user_name: &str,
        x: f64,
        y: f64,
        selected_element: Option<String>,
    ) {
        let rooms = self.rooms.read().await;
        if let Some(room) = rooms.get(project_id) {
            let _ = room.broadcast.send(CollabMessage::RemoteCursor {
                project_id: project_id.to_string(),
                user_id: user_id.to_string(),
                user_name: user_name.to_string(),
                x,
                y,
                selected_element,
            });
        }
        // Update stored cursor position
        drop(rooms);
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get_mut(project_id)
            && let Some(member) = room.members.get_mut(user_id)
        {
            member.cursor = Some(CursorPosition {
                x,
                y,
                selected_element: None,
            });
        }
    }

    /// Try to acquire an exclusive lock on an entity.
    pub async fn try_lock(
        &self,
        project_id: &str,
        user_id: &str,
        entity_id: &str,
    ) -> Result<(), String> {
        let mut rooms = self.rooms.write().await;
        let room = rooms.get_mut(project_id).ok_or("Room not found")?;

        if let Some(existing) = room.locks.get(entity_id) {
            if existing.held_by != user_id {
                let _ = room.broadcast.send(CollabMessage::LockDenied {
                    project_id: project_id.to_string(),
                    entity_id: entity_id.to_string(),
                    held_by: existing.held_by.clone(),
                });
                return Err(format!("Lock held by {}", existing.held_by));
            }
            // Already held by this user — idempotent
            return Ok(());
        }

        room.locks.insert(
            entity_id.to_string(),
            EntityLock {
                held_by: user_id.to_string(),
                acquired_at: Utc::now(),
            },
        );

        let _ = room.broadcast.send(CollabMessage::LockGranted {
            project_id: project_id.to_string(),
            entity_id: entity_id.to_string(),
        });

        Ok(())
    }

    /// Release a lock on an entity.
    pub async fn release_lock(
        &self,
        project_id: &str,
        user_id: &str,
        entity_id: &str,
    ) -> Result<(), String> {
        let mut rooms = self.rooms.write().await;
        let room = rooms.get_mut(project_id).ok_or("Room not found")?;

        if let Some(lock) = room.locks.get(entity_id)
            && lock.held_by != user_id
        {
            return Err("Lock held by another user".to_string());
        }

        room.locks.remove(entity_id);
        let _ = room.broadcast.send(CollabMessage::LockReleased {
            project_id: project_id.to_string(),
            entity_id: entity_id.to_string(),
        });

        Ok(())
    }

    /// Get current presence for a room.
    pub async fn get_presence(&self, project_id: &str) -> Vec<PresenceInfo> {
        let rooms = self.rooms.read().await;
        rooms
            .get(project_id)
            .map(|r| r.presence_list())
            .unwrap_or_default()
    }

    /// Count active rooms (for monitoring).
    pub async fn active_room_count(&self) -> usize {
        self.rooms.read().await.len()
    }
}

impl Default for CollaborationHub {
    fn default() -> Self {
        Self::new()
    }
}
