use crate::{
    identity::{EntityRevision, NetworkEntityId, PeerId, QuaternionV1, SurfaceId, Vec2V1, Vec3V1},
    room::FeaturePolicyV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSurfaceV1 {
    pub surface_id: SurfaceId,
    pub host_monitor_id: String,
    pub width_px: u32,
    pub height_px: u32,
    pub scale_factor: f32,
    pub coordinate_origin: CoordinateOriginV1,
    pub bounds_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateOriginV1 {
    TopLeft,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDefinitionV1 {
    pub network_entity_id: NetworkEntityId,
    pub entity_revision: EntityRevision,
    pub owner_peer_id: PeerId,
    pub authority_peer_id: PeerId,
    pub visual_kind_tag: String,
    pub width: f32,
    pub height: f32,
    pub collision_shape_tag: String,
    pub collision_scale: f32,
    pub color_argb: u32,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    #[serde(default)]
    pub visual_metadata: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDynamicsV1 {
    pub network_entity_id: NetworkEntityId,
    pub host_tick: u64,
    pub position: Vec2V1,
    pub velocity: Vec2V1,
    pub depth_z: f32,
    pub depth_velocity: f32,
    pub orientation_quaternion: QuaternionV1,
    pub angular_velocity: Vec3V1,
    pub scale: f32,
    pub opacity: f32,
    pub visible: bool,
    pub sleeping: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_lease_peer_id: Option<PeerId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneKeyframeV1 {
    pub keyframe_id: String,
    pub host_tick: u64,
    pub authority_epoch: u64,
    pub surface: SharedSurfaceV1,
    pub feature_policy: FeaturePolicyV1,
    pub entity_definitions: Vec<EntityDefinitionV1>,
    pub entity_dynamics: Vec<EntityDynamicsV1>,
    #[serde(default)]
    pub active_feature_state: Value,
    #[serde(default)]
    pub portal_definitions: Vec<Value>,
    #[serde(default)]
    pub asset_manifest: Vec<AssetManifestEntryV1>,
    pub last_reliable_event_sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformBatchV1 {
    pub host_tick: u64,
    pub transforms: Vec<EntityDynamicsV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDespawnV1 {
    pub network_entity_id: NetworkEntityId,
    pub entity_revision: EntityRevision,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDefinitionPatchV1 {
    pub network_entity_id: NetworkEntityId,
    pub entity_revision: EntityRevision,
    pub patch: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyframeRequestV1 {
    pub reason: String,
    pub last_host_tick: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetManifestEntryV1 {
    pub asset_ref: String,
    pub content_type: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectEventV1 {
    pub event_id: String,
    pub effect_kind: String,
    pub host_tick: u64,
    #[serde(default)]
    pub data: Value,
}
