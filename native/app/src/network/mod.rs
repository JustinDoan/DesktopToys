//! Native host/replica seam for Shared Desktop Rooms.
//!
//! This module intentionally has no socket dependency: the local bridge and
//! relay supply wire frames, while the overlay owns authoritative capture and
//! guest replica application. Keeping that boundary explicit lets localhost
//! testing exercise the same messages as the eventual Railway connection.

pub mod bridge;

use std::collections::{HashMap, HashSet};

use core_types::{AppColor, CollisionShape, ObjectState, ObjectVisualKind, RectF, Vector2};
use network_protocol::identity::{QuaternionV1, Vec2V1, Vec3V1};
use network_protocol::{
    CoordinateOriginV1, EntityDefinitionV1, EntityDynamicsV1, FeaturePolicyV1, SceneKeyframeV1,
    SharedSurfaceV1, TransformBatchV1,
};

const LOCAL_HOST_PEER_ID: &str = "local-host";
const TRANSFORM_HEARTBEAT_BATCHES: u64 = 40;
const REPLICA_INTERPOLATION_SECONDS: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomMode {
    Offline,
    Host,
    Guest,
}

#[derive(Debug)]
pub struct RoomRuntime {
    mode: RoomMode,
    authority_epoch: u64,
    next_keyframe: u64,
    network_ids: HashMap<u64, String>,
    policy: FeaturePolicyV1,
    surface_bounds: Option<RectF>,
    last_sent_transforms: HashMap<u64, TransformFingerprint>,
    transform_batch_sequence: u64,
}

#[derive(Clone, Copy, Debug)]
struct TransformFingerprint {
    position: Vector2,
    velocity: Vector2,
    depth_z: f32,
    depth_velocity: f32,
    rotation_x: f64,
    rotation_y: f64,
    rotation_z: f64,
    scale: f32,
    opacity: f32,
    visible: bool,
    sleeping: bool,
}

impl Default for RoomRuntime {
    fn default() -> Self {
        Self {
            mode: RoomMode::Offline,
            authority_epoch: 0,
            next_keyframe: 1,
            network_ids: HashMap::new(),
            policy: FeaturePolicyV1::default(),
            surface_bounds: None,
            last_sent_transforms: HashMap::new(),
            transform_batch_sequence: 0,
        }
    }
}

impl RoomRuntime {
    pub fn mode(&self) -> RoomMode {
        self.mode
    }

    pub fn enter_host(&mut self, allow_interaction: bool, surface_bounds: Option<RectF>) {
        self.mode = RoomMode::Host;
        self.authority_epoch = self.authority_epoch.saturating_add(1).max(1);
        self.policy.allow_interaction = allow_interaction;
        self.surface_bounds = surface_bounds;
        self.last_sent_transforms.clear();
        self.transform_batch_sequence = 0;
    }

    pub fn enter_guest(&mut self) {
        self.mode = RoomMode::Guest;
        self.surface_bounds = None;
        self.last_sent_transforms.clear();
        self.policy = FeaturePolicyV1::default();
    }

    pub fn leave(&mut self) {
        self.mode = RoomMode::Offline;
        self.network_ids.clear();
        self.surface_bounds = None;
        self.last_sent_transforms.clear();
    }

    pub fn set_allow_interaction(&mut self, enabled: bool) {
        self.policy.allow_interaction = enabled;
    }

    pub fn allows_interaction(&self) -> bool {
        self.policy.allow_interaction
    }

    pub fn local_id_for_network(&self, network_id: &str) -> Option<u64> {
        self.network_ids
            .iter()
            .find_map(|(local_id, candidate)| (candidate == network_id).then_some(*local_id))
    }

    pub fn host_scene_position(&self, surface_position: Vector2) -> Vector2 {
        self.surface_bounds.map_or(surface_position, |surface| {
            Vector2::new(
                surface.x + surface_position.x,
                surface.y + surface_position.y,
            )
        })
    }

    pub fn shared_entity_ids(&self, objects: &[ObjectState]) -> Vec<u64> {
        let mut ids: Vec<_> = objects
            .iter()
            .filter(|object| is_supported(object.visual_kind))
            .filter(|object| {
                self.surface_bounds
                    .map_or(true, |surface| object_intersects_surface(object, surface))
            })
            .map(|object| object.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn capture_keyframe(
        &mut self,
        objects: &[ObjectState],
        bounds: RectF,
        host_tick: u64,
    ) -> SceneKeyframeV1 {
        let surface = if self.mode == RoomMode::Host {
            self.surface_bounds.unwrap_or(bounds)
        } else {
            bounds
        };
        let definitions = objects
            .iter()
            .filter(|object| object_intersects_surface(object, surface))
            .filter_map(|object| self.definition(object))
            .collect();
        let dynamics = objects
            .iter()
            .filter(|object| object_intersects_surface(object, surface))
            .filter_map(|object| self.dynamics(object, host_tick, surface.x, surface.y))
            .collect();
        let keyframe_id = format!("local-{}", self.next_keyframe);
        self.next_keyframe = self.next_keyframe.saturating_add(1);
        SceneKeyframeV1 {
            keyframe_id,
            host_tick,
            authority_epoch: self.authority_epoch,
            surface: SharedSurfaceV1 {
                surface_id: "host-primary-monitor".to_string(),
                host_monitor_id: "primary".to_string(),
                width_px: surface.width.max(1.0).round() as u32,
                height_px: surface.height.max(1.0).round() as u32,
                scale_factor: 1.0,
                coordinate_origin: CoordinateOriginV1::TopLeft,
                bounds_revision: 1,
            },
            feature_policy: self.policy.clone(),
            entity_definitions: definitions,
            entity_dynamics: dynamics,
            active_feature_state: serde_json::Value::Null,
            portal_definitions: Vec::new(),
            asset_manifest: Vec::new(),
            last_reliable_event_sequence: host_tick,
        }
    }

    pub fn capture_transform_batch(
        &mut self,
        objects: &[ObjectState],
        host_tick: u64,
    ) -> TransformBatchV1 {
        let surface = self.surface_bounds;
        self.transform_batch_sequence = self.transform_batch_sequence.saturating_add(1);
        let full_refresh = self.transform_batch_sequence % TRANSFORM_HEARTBEAT_BATCHES == 0;
        let origin = surface
            .map(|bounds| (bounds.x, bounds.y))
            .unwrap_or((0.0, 0.0));
        let mut present = HashSet::new();
        let mut transforms = Vec::new();
        for object in objects {
            if !is_supported(object.visual_kind)
                || !surface.map_or(true, |bounds| object_intersects_surface(object, bounds))
            {
                continue;
            }
            present.insert(object.id);
            let fingerprint = TransformFingerprint::from_object(object);
            let changed = full_refresh
                || self
                    .last_sent_transforms
                    .get(&object.id)
                    .is_none_or(|previous| previous.materially_differs(fingerprint));
            if changed {
                self.last_sent_transforms.insert(object.id, fingerprint);
                if let Some(dynamic) = self.dynamics(object, host_tick, origin.0, origin.1) {
                    transforms.push(dynamic);
                }
            }
        }
        self.last_sent_transforms
            .retain(|local_id, _| present.contains(local_id));
        TransformBatchV1 {
            host_tick,
            transforms,
        }
    }

    fn network_id(&mut self, local_id: u64) -> String {
        self.network_ids
            .entry(local_id)
            .or_insert_with(|| format!("{}:{local_id}", self.authority_epoch))
            .clone()
    }

    fn definition(&mut self, object: &ObjectState) -> Option<EntityDefinitionV1> {
        if !is_supported(object.visual_kind) {
            return None;
        }
        Some(EntityDefinitionV1 {
            network_entity_id: self.network_id(object.id),
            entity_revision: 1,
            owner_peer_id: LOCAL_HOST_PEER_ID.to_string(),
            authority_peer_id: LOCAL_HOST_PEER_ID.to_string(),
            visual_kind_tag: visual_kind_tag(object.visual_kind).to_string(),
            width: object.body.width,
            height: object.body.height,
            collision_shape_tag: collision_shape_tag(object.body.shape).to_string(),
            collision_scale: object.body.collision_scale,
            color_argb: color_argb(object.base_color),
            mass: object.body.mass,
            friction: object.body.friction,
            restitution: object.body.restitution,
            linear_damping: object.body.linear_damping,
            angular_damping: 0.0,
            visual_metadata: if object.visual_kind == ObjectVisualKind::Text {
                serde_json::json!({ "text": object.custom_text.as_deref().unwrap_or("Text") })
            } else {
                serde_json::Value::Null
            },
            asset_ref: None,
        })
    }

    fn dynamics(
        &mut self,
        object: &ObjectState,
        host_tick: u64,
        origin_x: f32,
        origin_y: f32,
    ) -> Option<EntityDynamicsV1> {
        if !is_supported(object.visual_kind) {
            return None;
        }
        Some(EntityDynamicsV1 {
            network_entity_id: self.network_id(object.id),
            host_tick,
            position: Vec2V1 {
                x: object.body.position.x - origin_x,
                y: object.body.position.y - origin_y,
            },
            velocity: vec2(object.body.velocity),
            depth_z: object.depth_z,
            depth_velocity: object.depth_velocity,
            orientation_quaternion: quaternion_from_euler(
                object.rotation_x,
                object.rotation_y,
                object.rotation_z,
            ),
            angular_velocity: Vec3V1 {
                x: object.angular_velocity_x as f32,
                y: object.angular_velocity_y as f32,
                z: object.angular_velocity_z as f32,
            },
            scale: object.visual_scale,
            opacity: object.visual_opacity,
            visible: object.is_visible,
            sleeping: object.body.is_sleeping,
            interaction_lease_peer_id: None,
        })
    }
}

impl TransformFingerprint {
    fn from_object(object: &ObjectState) -> Self {
        Self {
            position: object.body.position,
            velocity: object.body.velocity,
            depth_z: object.depth_z,
            depth_velocity: object.depth_velocity,
            rotation_x: object.rotation_x,
            rotation_y: object.rotation_y,
            rotation_z: object.rotation_z,
            scale: object.visual_scale,
            opacity: object.visual_opacity,
            visible: object.is_visible,
            sleeping: object.body.is_sleeping,
        }
    }

    fn materially_differs(self, other: Self) -> bool {
        (self.position - other.position).length_squared() > 0.25 * 0.25
            || (self.velocity - other.velocity).length_squared() > 1.0
            || (self.depth_z - other.depth_z).abs() > 0.05
            || (self.depth_velocity - other.depth_velocity).abs() > 0.05
            || (self.rotation_x - other.rotation_x).abs() > 0.001
            || (self.rotation_y - other.rotation_y).abs() > 0.001
            || (self.rotation_z - other.rotation_z).abs() > 0.001
            || (self.scale - other.scale).abs() > 0.001
            || (self.opacity - other.opacity).abs() > 0.002
            || self.visible != other.visible
            || self.sleeping != other.sleeping
    }
}

/// Guest-side renderer state. It deliberately stores only host-authoritative
/// snapshots and applies them into a paused local scene; it never calls step.
#[derive(Debug, Default)]
pub struct ReplicaWorld {
    network_to_local: HashMap<String, u64>,
    definitions: HashMap<String, EntityDefinitionV1>,
    last_host_tick: u64,
    target_bounds: Option<RectF>,
    host_surface: Option<SharedSurfaceV1>,
    motion_targets: HashMap<String, ReplicaMotionTarget>,
}

#[derive(Clone, Copy, Debug)]
struct ReplicaMotionTarget {
    start_position: Vector2,
    end_position: Vector2,
    start_velocity: Vector2,
    end_velocity: Vector2,
    elapsed_seconds: f32,
}

impl ReplicaWorld {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn last_host_tick(&self) -> u64 {
        self.last_host_tick
    }
    pub fn set_target_bounds(&mut self, bounds: Option<RectF>) {
        self.target_bounds = bounds;
    }
    pub fn network_id_for_local(&self, local_id: u64) -> Option<&str> {
        self.network_to_local
            .iter()
            .find_map(|(network_id, candidate)| {
                (*candidate == local_id).then_some(network_id.as_str())
            })
    }
    pub fn local_id_for_network(&self, network_id: &str) -> Option<u64> {
        self.network_to_local.get(network_id).copied()
    }

    pub fn host_surface_position(&self, local_position: Vector2) -> Vector2 {
        let Some((scale, offset_x, offset_y)) = self.viewport_transform() else {
            return local_position;
        };
        Vector2::new(
            (local_position.x - offset_x) / scale,
            (local_position.y - offset_y) / scale,
        )
    }

    pub fn host_surface_velocity(&self, local_velocity: Vector2) -> Vector2 {
        let Some((scale, _, _)) = self.viewport_transform() else {
            return local_velocity;
        };
        Vector2::new(local_velocity.x / scale, local_velocity.y / scale)
    }

    pub fn predict_local_motion(&mut self, network_id: &str, position: Vector2, velocity: Vector2) {
        self.motion_targets.insert(
            network_id.to_string(),
            ReplicaMotionTarget {
                start_position: position,
                end_position: position,
                start_velocity: velocity,
                end_velocity: velocity,
                elapsed_seconds: 0.0,
            },
        );
    }

    pub fn apply_keyframe(
        &mut self,
        keyframe: SceneKeyframeV1,
        scene: &mut scene_logic::SceneController,
    ) {
        scene.clear_objects();
        let target_bounds = self.target_bounds;
        self.clear();
        self.target_bounds = target_bounds;
        self.host_surface = Some(keyframe.surface.clone());
        self.last_host_tick = keyframe.host_tick;
        for definition in keyframe.entity_definitions {
            self.definitions
                .insert(definition.network_entity_id.clone(), definition);
        }
        self.apply_dynamics(keyframe.entity_dynamics, scene, true);
    }

    pub fn apply_transform_batch(
        &mut self,
        batch: TransformBatchV1,
        scene: &mut scene_logic::SceneController,
    ) {
        if batch.host_tick < self.last_host_tick {
            return;
        }
        self.last_host_tick = batch.host_tick;
        self.apply_dynamics(batch.transforms, scene, false);
    }

    pub fn advance_interpolation(
        &mut self,
        delta_seconds: f32,
        scene: &mut scene_logic::SceneController,
    ) {
        for (network_id, target) in &mut self.motion_targets {
            target.elapsed_seconds =
                (target.elapsed_seconds + delta_seconds.clamp(0.0, 0.05)).min(0.125);
            let Some(local_id) = self.network_to_local.get(network_id).copied() else {
                continue;
            };
            let Some(object) = scene
                .objects_mut()
                .iter_mut()
                .find(|object| object.id == local_id)
            else {
                continue;
            };
            if object.body.is_dragging {
                continue;
            }
            let t = (target.elapsed_seconds / REPLICA_INTERPOLATION_SECONDS).min(1.0);
            object.body.position = if t < 1.0 {
                hermite_position(
                    target.start_position,
                    target.end_position,
                    target.start_velocity,
                    target.end_velocity,
                    t,
                    REPLICA_INTERPOLATION_SECONDS,
                )
            } else {
                let extrapolation =
                    (target.elapsed_seconds - REPLICA_INTERPOLATION_SECONDS).min(0.075);
                Vector2::new(
                    target.end_position.x + target.end_velocity.x * extrapolation,
                    target.end_position.y + target.end_velocity.y * extrapolation,
                )
            };
            object.body.velocity = target.end_velocity;
        }
    }

    fn apply_dynamics(
        &mut self,
        dynamics: Vec<EntityDynamicsV1>,
        scene: &mut scene_logic::SceneController,
        snap: bool,
    ) {
        for dynamic in dynamics {
            let Some(definition) = self.definitions.get(&dynamic.network_entity_id).cloned() else {
                continue;
            };
            let (mapped_position, mapped_velocity, viewport_scale) = self.map_dynamics(&dynamic);
            let local_id = *self
                .network_to_local
                .entry(dynamic.network_entity_id.clone())
                .or_insert_with(|| {
                    scene.spawn_custom_object(
                        Vector2::ZERO,
                        Vector2::new(
                            definition.width * viewport_scale,
                            definition.height * viewport_scale,
                        ),
                        color_from_argb(definition.color_argb),
                        visual_kind_from_tag(&definition.visual_kind_tag),
                        collision_shape_from_tag(&definition.collision_shape_tag),
                    )
                });
            let mut interpolation_start = mapped_position;
            let mut interpolation_velocity = mapped_velocity;
            if let Some(object) = scene
                .objects_mut()
                .iter_mut()
                .find(|object| object.id == local_id)
            {
                interpolation_start = object.body.position;
                interpolation_velocity = object.body.velocity;
                if snap {
                    object.body.position = mapped_position;
                    interpolation_start = mapped_position;
                }
                object.body.velocity = mapped_velocity;
                object.body.is_sleeping = dynamic.sleeping;
                object.depth_z = dynamic.depth_z;
                object.depth_velocity = dynamic.depth_velocity;
                object.visual_scale = dynamic.scale;
                object.visual_opacity = dynamic.opacity;
                object.is_visible = dynamic.visible;
                object.rotation_z = euler_z_from_quaternion(dynamic.orientation_quaternion);
                object.custom_text = definition
                    .visual_metadata
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string);
            }
            self.motion_targets.insert(
                dynamic.network_entity_id,
                ReplicaMotionTarget {
                    start_position: interpolation_start,
                    end_position: mapped_position,
                    start_velocity: interpolation_velocity,
                    end_velocity: mapped_velocity,
                    elapsed_seconds: 0.0,
                },
            );
        }
        let present: HashSet<_> = self.network_to_local.keys().cloned().collect();
        self.network_to_local.retain(|id, local_id| {
            let keep = present.contains(id) && self.definitions.contains_key(id);
            if !keep {
                let _ = scene.remove_object(*local_id);
            }
            keep
        });
    }

    fn map_dynamics(&self, dynamic: &EntityDynamicsV1) -> (Vector2, Vector2, f32) {
        let Some((scale, offset_x, offset_y)) = self.viewport_transform() else {
            return (
                Vector2::new(dynamic.position.x, dynamic.position.y),
                Vector2::new(dynamic.velocity.x, dynamic.velocity.y),
                1.0,
            );
        };
        (
            Vector2::new(
                offset_x + dynamic.position.x * scale,
                offset_y + dynamic.position.y * scale,
            ),
            Vector2::new(dynamic.velocity.x * scale, dynamic.velocity.y * scale),
            scale,
        )
    }

    fn viewport_transform(&self) -> Option<(f32, f32, f32)> {
        let (target, surface) = (self.target_bounds?, self.host_surface.as_ref()?);
        let host_width = surface.width_px.max(1) as f32;
        let host_height = surface.height_px.max(1) as f32;
        let scale = (target.width / host_width)
            .min(target.height / host_height)
            .max(0.01);
        Some((
            scale,
            target.x + (target.width - host_width * scale) * 0.5,
            target.y + (target.height - host_height * scale) * 0.5,
        ))
    }
}

fn hermite_position(
    start: Vector2,
    end: Vector2,
    start_velocity: Vector2,
    end_velocity: Vector2,
    t: f32,
    duration: f32,
) -> Vector2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    Vector2::new(
        h00 * start.x
            + h10 * start_velocity.x * duration
            + h01 * end.x
            + h11 * end_velocity.x * duration,
        h00 * start.y
            + h10 * start_velocity.y * duration
            + h01 * end.y
            + h11 * end_velocity.y * duration,
    )
}

fn object_intersects_surface(object: &ObjectState, surface: RectF) -> bool {
    let half_width = object.body.width * object.visual_scale * 0.5;
    let half_height = object.body.height * object.visual_scale * 0.5;
    object.body.position.x + half_width >= surface.left()
        && object.body.position.x - half_width <= surface.right()
        && object.body.position.y + half_height >= surface.top()
        && object.body.position.y - half_height <= surface.bottom()
}

fn is_supported(kind: ObjectVisualKind) -> bool {
    !matches!(
        kind,
        ObjectVisualKind::ImportedModel | ObjectVisualKind::ScreenShard
    )
}
fn vec2(value: Vector2) -> Vec2V1 {
    Vec2V1 {
        x: value.x,
        y: value.y,
    }
}
fn color_argb(value: AppColor) -> u32 {
    ((value.a as u32) << 24) | ((value.r as u32) << 16) | ((value.g as u32) << 8) | value.b as u32
}
fn color_from_argb(value: u32) -> AppColor {
    AppColor::from_argb(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}
fn collision_shape_tag(shape: CollisionShape) -> &'static str {
    match shape {
        CollisionShape::Box => "box",
        CollisionShape::Circle => "circle",
        CollisionShape::Diamond => "diamond",
    }
}
fn collision_shape_from_tag(tag: &str) -> CollisionShape {
    match tag {
        "circle" => CollisionShape::Circle,
        "diamond" => CollisionShape::Diamond,
        _ => CollisionShape::Box,
    }
}
fn visual_kind_tag(kind: ObjectVisualKind) -> &'static str {
    match kind {
        ObjectVisualKind::Cube => "cube",
        ObjectVisualKind::Dice => "dice",
        ObjectVisualKind::Crystal => "crystal",
        ObjectVisualKind::BitCrystal => "bit_crystal",
        ObjectVisualKind::Satellite => "satellite",
        ObjectVisualKind::DvdLogo => "dvd_logo",
        ObjectVisualKind::Ball => "ball",
        ObjectVisualKind::SoftBall => "soft_ball",
        ObjectVisualKind::GlassMarble => "glass_marble",
        ObjectVisualKind::PlasmaOrb => "plasma_orb",
        ObjectVisualKind::PortalOrb => "portal_orb",
        ObjectVisualKind::SoapBubble => "soap_bubble",
        ObjectVisualKind::ForcefieldOrb => "forcefield_orb",
        ObjectVisualKind::RaymarchCube => "raymarch_cube",
        ObjectVisualKind::Pyramid => "pyramid",
        ObjectVisualKind::Barrel => "barrel",
        ObjectVisualKind::Ring => "ring",
        ObjectVisualKind::Star => "star",
        ObjectVisualKind::GamePlank => "game_plank",
        ObjectVisualKind::GameTarget => "game_target",
        ObjectVisualKind::FoxBuddy => "fox_buddy",
        ObjectVisualKind::RobotBuddy => "robot_buddy",
        ObjectVisualKind::Snail => "snail",
        ObjectVisualKind::Fan => "fan",
        ObjectVisualKind::QuadDrone => "quad_drone",
        ObjectVisualKind::Basketball => "basketball",
        ObjectVisualKind::BasketballHoop => "basketball_hoop",
        ObjectVisualKind::Text => "text",
        ObjectVisualKind::ScreenShard => "screen_shard",
        ObjectVisualKind::ImportedModel => "imported_model",
    }
}
fn visual_kind_from_tag(tag: &str) -> ObjectVisualKind {
    match tag {
        "dice" => ObjectVisualKind::Dice,
        "crystal" => ObjectVisualKind::Crystal,
        "bit_crystal" => ObjectVisualKind::BitCrystal,
        "satellite" => ObjectVisualKind::Satellite,
        "dvd_logo" => ObjectVisualKind::DvdLogo,
        "ball" => ObjectVisualKind::Ball,
        "soft_ball" => ObjectVisualKind::SoftBall,
        "glass_marble" => ObjectVisualKind::GlassMarble,
        "plasma_orb" => ObjectVisualKind::PlasmaOrb,
        "portal_orb" => ObjectVisualKind::PortalOrb,
        "soap_bubble" => ObjectVisualKind::SoapBubble,
        "forcefield_orb" => ObjectVisualKind::ForcefieldOrb,
        "raymarch_cube" => ObjectVisualKind::RaymarchCube,
        "pyramid" => ObjectVisualKind::Pyramid,
        "barrel" => ObjectVisualKind::Barrel,
        "ring" => ObjectVisualKind::Ring,
        "star" => ObjectVisualKind::Star,
        "game_plank" => ObjectVisualKind::GamePlank,
        "game_target" => ObjectVisualKind::GameTarget,
        "fox_buddy" => ObjectVisualKind::FoxBuddy,
        "robot_buddy" => ObjectVisualKind::RobotBuddy,
        "snail" => ObjectVisualKind::Snail,
        "fan" => ObjectVisualKind::Fan,
        "quad_drone" => ObjectVisualKind::QuadDrone,
        "basketball" => ObjectVisualKind::Basketball,
        "basketball_hoop" => ObjectVisualKind::BasketballHoop,
        "text" => ObjectVisualKind::Text,
        _ => ObjectVisualKind::Cube,
    }
}
fn quaternion_from_euler(x: f64, y: f64, z: f64) -> QuaternionV1 {
    let (sx, cx) = (
        (x.to_radians() * 0.5).sin() as f32,
        (x.to_radians() * 0.5).cos() as f32,
    );
    let (sy, cy) = (
        (y.to_radians() * 0.5).sin() as f32,
        (y.to_radians() * 0.5).cos() as f32,
    );
    let (sz, cz) = (
        (z.to_radians() * 0.5).sin() as f32,
        (z.to_radians() * 0.5).cos() as f32,
    );
    QuaternionV1 {
        x: sx * cy * cz - cx * sy * sz,
        y: cx * sy * cz + sx * cy * sz,
        z: cx * cy * sz - sx * sy * cz,
        w: cx * cy * cz + sx * sy * sz,
    }
}
fn euler_z_from_quaternion(value: QuaternionV1) -> f64 {
    let sin: f32 = 2.0 * (value.w * value.z + value.x * value.y);
    let cos: f32 = 1.0 - 2.0 * (value.y * value.y + value.z * value.z);
    sin.atan2(cos).to_degrees() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_ids_are_stable_and_unsupported_assets_are_excluded() {
        let mut runtime = RoomRuntime::default();
        runtime.enter_host(false, None);
        let mut supported = ObjectState::default();
        supported.id = 42;
        let mut model = ObjectState::default();
        model.id = 43;
        model.visual_kind = ObjectVisualKind::ImportedModel;
        let frame = runtime.capture_keyframe(
            &[supported.clone(), model],
            RectF::new(0.0, 0.0, 100.0, 50.0),
            7,
        );
        assert_eq!(frame.entity_definitions.len(), 1);
        assert_eq!(frame.entity_definitions[0].network_entity_id, "1:42");
        let first_batch = runtime.capture_transform_batch(&[supported.clone()], 8);
        assert_eq!(first_batch.transforms[0].network_entity_id, "1:42");
        assert!(
            runtime
                .capture_transform_batch(&[supported.clone()], 9)
                .transforms
                .is_empty()
        );
        supported.body.position.x += 2.0;
        assert_eq!(
            runtime
                .capture_transform_batch(&[supported], 10)
                .transforms
                .len(),
            1
        );
    }
    #[test]
    fn stale_transform_batches_are_ignored() {
        let mut replica = ReplicaWorld::default();
        replica.last_host_tick = 10;
        let mut scene = scene_logic::SceneController::new(core_types::AppConfig::default());
        replica.apply_transform_batch(
            TransformBatchV1 {
                host_tick: 9,
                transforms: Vec::new(),
            },
            &mut scene,
        );
        assert_eq!(replica.last_host_tick(), 10);
    }

    #[test]
    fn text_content_round_trips_through_a_keyframe() {
        let mut runtime = RoomRuntime::default();
        runtime.enter_host(true, None);
        let mut text = ObjectState::default();
        text.id = 91;
        text.visual_kind = ObjectVisualKind::Text;
        text.custom_text = Some("Shared hello 👋".to_string());
        text.body.width = 240.0;
        text.body.height = 72.0;
        text.body.position = Vector2::new(20.0, 30.0);
        let frame = runtime.capture_keyframe(&[text], RectF::new(0.0, 0.0, 800.0, 600.0), 1);
        assert_eq!(frame.entity_definitions[0].visual_kind_tag, "text");
        assert_eq!(
            frame.entity_definitions[0].visual_metadata["text"],
            "Shared hello 👋"
        );

        let mut replica = ReplicaWorld::default();
        let mut scene = scene_logic::SceneController::new(core_types::AppConfig::default());
        replica.apply_keyframe(frame, &mut scene);
        assert_eq!(scene.objects()[0].visual_kind, ObjectVisualKind::Text);
        assert_eq!(
            scene.objects()[0].custom_text.as_deref(),
            Some("Shared hello 👋")
        );
    }

    #[test]
    fn host_surface_positions_are_relative_and_guest_maps_them_to_target_display() {
        let mut runtime = RoomRuntime::default();
        let host_surface = RectF::new(1000.0, 0.0, 200.0, 100.0);
        runtime.enter_host(false, Some(host_surface));
        let mut object = ObjectState::default();
        object.id = 7;
        object.body.position = Vector2::new(1050.0, 25.0);
        object.body.width = 64.0;
        object.body.height = 64.0;

        let frame = runtime.capture_keyframe(&[object], RectF::new(0.0, 0.0, 2000.0, 1000.0), 1);
        assert_eq!(frame.surface.width_px, 200);
        assert_eq!(frame.entity_dynamics[0].position.x, 50.0);
        assert_eq!(frame.entity_dynamics[0].position.y, 25.0);

        let mut replica = ReplicaWorld::default();
        replica.set_target_bounds(Some(RectF::new(2000.0, 100.0, 400.0, 200.0)));
        let mut scene = scene_logic::SceneController::new(core_types::AppConfig::default());
        replica.apply_keyframe(frame, &mut scene);
        let replicated = &scene.objects()[0];
        assert_eq!(replicated.body.position, Vector2::new(2100.0, 150.0));
        assert_eq!(replicated.body.width, 128.0);
        assert_eq!(replicated.body.height, 128.0);
        assert_eq!(
            replica.host_surface_position(replicated.body.position),
            Vector2::new(50.0, 25.0)
        );
        assert_eq!(
            replica.host_surface_velocity(Vector2::new(200.0, 100.0)),
            Vector2::new(100.0, 50.0)
        );
        assert_eq!(
            runtime.host_scene_position(Vector2::new(50.0, 25.0)),
            Vector2::new(1050.0, 25.0)
        );

        let mut moved = runtime.capture_transform_batch(&[], 2);
        moved
            .transforms
            .push(scene_keyframe_dynamics("1:7", 2, 150.0, 25.0));
        replica.apply_transform_batch(moved, &mut scene);
        assert_eq!(
            scene.objects()[0].body.position,
            Vector2::new(2100.0, 150.0)
        );
        replica.advance_interpolation(1.0 / 60.0, &mut scene);
        assert!(scene.objects()[0].body.position.x > 2100.0);
        assert!(scene.objects()[0].body.position.x < 2300.0);
    }

    fn scene_keyframe_dynamics(
        network_entity_id: &str,
        host_tick: u64,
        x: f32,
        y: f32,
    ) -> EntityDynamicsV1 {
        EntityDynamicsV1 {
            network_entity_id: network_entity_id.to_string(),
            host_tick,
            position: Vec2V1 { x, y },
            velocity: Vec2V1::default(),
            depth_z: 0.0,
            depth_velocity: 0.0,
            orientation_quaternion: QuaternionV1::default(),
            angular_velocity: Vec3V1::default(),
            scale: 1.0,
            opacity: 1.0,
            visible: true,
            sleeping: false,
            interaction_lease_peer_id: None,
        }
    }
}
