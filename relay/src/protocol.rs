use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    pub protocol_version: u16,
    pub message_type: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomPayload {
    pub nickname: String,
    #[serde(default)]
    pub host_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomPayload {
    #[serde(default)]
    pub room_id: Option<String>,
    pub invite_token: String,
    pub nickname: String,
    #[serde(default)]
    pub client_instance_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEnvelope {
    pub protocol_version: u16,
    pub message_type: String,
    pub room_id: String,
    pub peer_id: String,
    pub authority_epoch: u64,
    pub sequence: u64,
    pub host_tick: u64,
    pub sent_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_peer_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

impl RelayEnvelope {
    pub fn server(
        message_type: impl Into<String>,
        room_id: impl Into<String>,
        peer_id: impl Into<String>,
        authority_epoch: u64,
        payload: Value,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message_type: message_type.into(),
            room_id: room_id.into(),
            peer_id: peer_id.into(),
            authority_epoch,
            sequence: 0,
            host_tick: 0,
            sent_at_unix_ms: unix_time_ms(),
            target_peer_id: None,
            payload,
        }
    }

    pub fn protocol_error(
        room_id: &str,
        peer_id: &str,
        authority_epoch: u64,
        code: &str,
        message: &str,
    ) -> Self {
        Self::server(
            "room.protocolError",
            room_id,
            peer_id,
            authority_epoch,
            json!({ "code": code, "message": message }),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerRole {
    Host,
    Guest,
}

impl PeerRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Guest => "guest",
        }
    }
}

pub fn validate_message_direction(role: PeerRole, message_type: &str) -> bool {
    if message_type == "room.ping" || message_type == "room.pong" {
        return true;
    }

    match role {
        PeerRole::Host => {
            message_type.starts_with("scene.")
                || message_type == "room.policyChanged"
                || message_type == "room.permissionChanged"
                || message_type == "room.ended"
                || message_type.starts_with("interaction.grab")
                || message_type.starts_with("interaction.intent")
                || message_type == "interaction.leaseRevoked"
        }
        PeerRole::Guest => {
            message_type == "scene.keyframeRequest"
                || message_type == "interaction.beginGrab"
                || message_type == "interaction.moveGrab"
                || message_type == "interaction.endGrab"
                || message_type == "interaction.spawnIntent"
                || message_type == "interaction.toolIntent"
        }
    }
}

fn unix_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_matrix_rejects_guest_scene_mutation() {
        assert!(validate_message_direction(
            PeerRole::Host,
            "scene.transformBatch"
        ));
        assert!(validate_message_direction(
            PeerRole::Guest,
            "interaction.beginGrab"
        ));
        assert!(validate_message_direction(
            PeerRole::Guest,
            "scene.keyframeRequest"
        ));
        assert!(!validate_message_direction(
            PeerRole::Guest,
            "scene.transformBatch"
        ));
        assert!(!validate_message_direction(
            PeerRole::Host,
            "interaction.beginGrab"
        ));
    }
}
