use crate::identity::{AuthorityEpoch, PeerId, RoomId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePolicyV1 {
    pub allow_interaction: bool,
    #[serde(default)]
    pub share_chat: bool,
    #[serde(default)]
    pub share_effects: bool,
    #[serde(default)]
    pub share_games: bool,
    #[serde(default)]
    pub share_portals: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostHelloV1 {
    pub nickname: String,
    pub host_session_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestHelloV1 {
    pub invite_token: String,
    pub nickname: String,
    pub client_instance_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeV1 {
    pub resume_token: String,
    pub last_authority_epoch: AuthorityEpoch,
    pub last_sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerV1 {
    pub peer_id: PeerId,
    pub nickname: String,
    pub role: RoomRoleV1,
    pub connected: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomRoleV1 {
    Host,
    Guest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomPolicyChangedV1 {
    pub policy: FeaturePolicyV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionChangedV1 {
    pub peer_id: PeerId,
    pub can_interact: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomEndedV1 {
    pub code: ProtocolErrorCodeV1,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolErrorV1 {
    pub code: ProtocolErrorCodeV1,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCodeV1 {
    UnsupportedProtocol,
    InvalidInvite,
    InviteRevoked,
    RoomFull,
    HostUnavailable,
    PermissionDenied,
    EntityNotFound,
    EntityBusy,
    StaleAuthorityEpoch,
    StaleEntityRevision,
    RateLimited,
    PayloadTooLarge,
    KeyframeRequired,
    UnsupportedFeature,
    InvalidPayload,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingV1 {
    pub nonce: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PongV1 {
    pub nonce: u64,
    pub room_id: RoomId,
}
