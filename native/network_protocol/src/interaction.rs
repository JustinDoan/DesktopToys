use crate::identity::{NetworkEntityId, PeerId, Vec2V1};
use crate::room::ProtocolErrorCodeV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginGrabV1 {
    pub intent_id: String,
    pub network_entity_id: NetworkEntityId,
    pub pointer: Vec2V1,
    pub observed_host_tick: u64,
    pub input_sequence: u64,
    pub client_monotonic_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveGrabV1 {
    pub intent_id: String,
    pub network_entity_id: NetworkEntityId,
    pub pointer: Vec2V1,
    pub observed_host_tick: u64,
    pub input_sequence: u64,
    pub client_monotonic_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndGrabV1 {
    pub intent_id: String,
    pub network_entity_id: NetworkEntityId,
    pub input_sequence: u64,
    pub client_monotonic_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_velocity: Option<Vec2V1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrabGrantedV1 {
    pub intent_id: String,
    pub network_entity_id: NetworkEntityId,
    pub lease_id: String,
    pub lease_expires_at_host_tick: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrabRejectedV1 {
    pub intent_id: String,
    pub code: ProtocolErrorCodeV1,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseRevokedV1 {
    pub network_entity_id: NetworkEntityId,
    pub previous_lease_peer_id: PeerId,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnIntentV1 {
    pub intent_id: String,
    pub visual_kind_tag: String,
    pub pointer: Vec2V1,
    #[serde(default)]
    pub options: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolIntentV1 {
    pub intent_id: String,
    pub tool_kind: String,
    #[serde(default)]
    pub data: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentAcceptedV1 {
    pub intent_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentRejectedV1 {
    pub intent_id: String,
    pub code: ProtocolErrorCodeV1,
    pub message: String,
}
