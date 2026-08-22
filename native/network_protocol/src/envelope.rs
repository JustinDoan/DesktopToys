use crate::identity::{AuthorityEpoch, PeerId, RoomId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u16 = 1;

/// The fixed set of v1 message names. New protocol releases may add names but
/// may not silently repurpose an existing one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    #[serde(rename = "room.hostHello")]
    HostHello,
    #[serde(rename = "room.guestHello")]
    GuestHello,
    #[serde(rename = "room.resume")]
    Resume,
    #[serde(rename = "room.resumeAccepted")]
    ResumeAccepted,
    #[serde(rename = "room.resumeRejected")]
    ResumeRejected,
    #[serde(rename = "room.peerJoined")]
    PeerJoined,
    #[serde(rename = "room.peerLeft")]
    PeerLeft,
    #[serde(rename = "room.hostUnavailable")]
    HostUnavailable,
    #[serde(rename = "room.hostRecovered")]
    HostRecovered,
    #[serde(rename = "room.ended")]
    Ended,
    #[serde(rename = "room.policyChanged")]
    PolicyChanged,
    #[serde(rename = "room.permissionChanged")]
    PermissionChanged,
    #[serde(rename = "room.protocolError")]
    ProtocolError,
    #[serde(rename = "room.ping")]
    Ping,
    #[serde(rename = "room.pong")]
    Pong,
    #[serde(rename = "scene.keyframe")]
    SceneKeyframe,
    #[serde(rename = "scene.keyframeRequest")]
    SceneKeyframeRequest,
    #[serde(rename = "scene.entitySpawn")]
    EntitySpawn,
    #[serde(rename = "scene.entityDefinitionPatch")]
    EntityDefinitionPatch,
    #[serde(rename = "scene.entityDespawn")]
    EntityDespawn,
    #[serde(rename = "scene.transformBatch")]
    TransformBatch,
    #[serde(rename = "scene.featureStatePatch")]
    FeatureStatePatch,
    #[serde(rename = "scene.effectEvent")]
    EffectEvent,
    #[serde(rename = "scene.assetManifest")]
    AssetManifest,
    #[serde(rename = "interaction.beginGrab")]
    BeginGrab,
    #[serde(rename = "interaction.grabGranted")]
    GrabGranted,
    #[serde(rename = "interaction.grabRejected")]
    GrabRejected,
    #[serde(rename = "interaction.moveGrab")]
    MoveGrab,
    #[serde(rename = "interaction.endGrab")]
    EndGrab,
    #[serde(rename = "interaction.leaseRevoked")]
    LeaseRevoked,
    #[serde(rename = "interaction.spawnIntent")]
    SpawnIntent,
    #[serde(rename = "interaction.toolIntent")]
    ToolIntent,
    #[serde(rename = "interaction.intentAccepted")]
    IntentAccepted,
    #[serde(rename = "interaction.intentRejected")]
    IntentRejected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub protocol_version: u16,
    pub message_type: MessageType,
    pub room_id: RoomId,
    pub peer_id: PeerId,
    pub authority_epoch: AuthorityEpoch,
    pub sequence: u64,
    pub host_tick: u64,
    pub sent_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_peer_id: Option<PeerId>,
    pub payload: Value,
}

impl Envelope {
    pub fn new(
        message_type: MessageType,
        room_id: RoomId,
        peer_id: PeerId,
        payload: Value,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message_type,
            room_id,
            peer_id,
            authority_epoch: 0,
            sequence: 0,
            host_tick: 0,
            sent_at_unix_ms: 0,
            target_peer_id: None,
            payload,
        }
    }
}
