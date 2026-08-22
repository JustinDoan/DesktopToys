use serde::{Deserialize, Serialize};

/// Public, opaque identifiers. They deliberately remain strings so a relay can
/// issue/change their format without coupling this crate to a UUID library.
pub type RoomId = String;
pub type PeerId = String;
pub type SurfaceId = String;
pub type NetworkEntityId = String;
pub type AuthorityEpoch = u64;
pub type EntityRevision = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vec2V1 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vec3V1 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuaternionV1 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
