use std::time::Instant;

use core_types::{
    AppColor, AppConfig, CollisionShape, ObjectState, ObjectVisualKind, PhysicsBody, RectF,
    ScreenShardGeometry, Vector2,
};
use physics_core::PhysicsWorld;
use rand::RngExt;

const CUBE_PALETTE: [AppColor; 5] = [
    AppColor::from_rgb(127, 202, 255),
    AppColor::from_rgb(255, 143, 163),
    AppColor::from_rgb(255, 192, 104),
    AppColor::from_rgb(145, 224, 154),
    AppColor::from_rgb(183, 153, 255),
];

const DEFAULT_OBJECT_SIZE: f32 = 132.0;
const STARTUP_CUBE_SIZE: f32 = DEFAULT_OBJECT_SIZE * 0.2;
const SHATTER_WEDGE_COUNT: usize = 30;
const SHATTER_RING_FACTORS: [f32; 5] = [0.0, 0.24, 0.48, 0.72, 1.08];
const SHATTER_MIN_AREA: f32 = 260.0;
const SHARD_THICKNESS: f32 = 18.0;

const SPAWN_CATALOG: [SpawnSpec; 22] = [
    SpawnSpec::new(ObjectVisualKind::Cube, None),
    SpawnSpec::new(
        ObjectVisualKind::Ball,
        Some(AppColor::from_rgb(90, 205, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::SoftBall,
        Some(AppColor::from_rgb(106, 236, 188)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::GlassMarble,
        Some(AppColor::from_rgb(220, 246, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::PlasmaOrb,
        Some(AppColor::from_rgb(160, 88, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::PortalOrb,
        Some(AppColor::from_rgb(80, 180, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::SoapBubble,
        Some(AppColor::from_rgb(245, 255, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::ForcefieldOrb,
        Some(AppColor::from_rgb(78, 240, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::RaymarchCube,
        Some(AppColor::from_rgb(130, 92, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::RobotBuddy,
        Some(AppColor::from_rgb(150, 220, 245)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Snail,
        Some(AppColor::from_rgb(166, 214, 124)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Fan,
        Some(AppColor::from_rgb(105, 230, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::QuadDrone,
        Some(AppColor::from_rgb(248, 250, 252)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Pyramid,
        Some(AppColor::from_rgb(255, 176, 92)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Barrel,
        Some(AppColor::from_rgb(126, 226, 168)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Ring,
        Some(AppColor::from_rgb(255, 118, 210)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Star,
        Some(AppColor::from_rgb(255, 224, 92)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Crystal,
        Some(AppColor::from_rgb(108, 241, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Satellite,
        Some(AppColor::from_rgb(88, 160, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::DvdLogo,
        Some(AppColor::from_rgb(244, 78, 255)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Dice,
        Some(AppColor::from_rgb(245, 245, 240)),
    ),
    SpawnSpec::new(
        ObjectVisualKind::Crystal,
        Some(AppColor::from_rgb(255, 112, 214)),
    ),
];

#[derive(Clone, Copy, Debug)]
struct SpawnSpec {
    visual_kind: ObjectVisualKind,
    color: Option<AppColor>,
}

impl SpawnSpec {
    const fn new(visual_kind: ObjectVisualKind, color: Option<AppColor>) -> Self {
        Self { visual_kind, color }
    }
}

#[derive(Debug)]
pub struct FrameClock {
    started_at: Instant,
    last_tick: Instant,
    pub delta_time_seconds: f32,
    pub elapsed_seconds: f64,
}

impl Default for FrameClock {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_tick: now,
            delta_time_seconds: 0.0,
            elapsed_seconds: 0.0,
        }
    }
}

impl FrameClock {
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.delta_time_seconds = (now - self.last_tick).as_secs_f32().min(1.0 / 20.0);
        self.elapsed_seconds = (now - self.started_at).as_secs_f64();
        self.last_tick = now;
    }
}

#[derive(Debug)]
pub struct MouseTracker {
    samples: Vec<MouseSample>,
    next_index: usize,
    count: usize,
}

#[derive(Clone, Copy, Debug)]
struct MouseSample {
    position: Vector2,
    time_seconds: f64,
}

impl MouseTracker {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(2);
        Self {
            samples: vec![
                MouseSample {
                    position: Vector2::ZERO,
                    time_seconds: 0.0,
                };
                capacity
            ],
            next_index: 0,
            count: 0,
        }
    }

    pub fn add_sample(&mut self, position: Vector2, timestamp_seconds: f64) {
        self.samples[self.next_index] = MouseSample {
            position,
            time_seconds: timestamp_seconds,
        };
        self.next_index = (self.next_index + 1) % self.samples.len();
        self.count = self.count.saturating_add(1).min(self.samples.len());
    }

    pub fn clear(&mut self) {
        self.next_index = 0;
        self.count = 0;
    }

    pub fn estimate_velocity(
        &self,
        lookback_seconds: f64,
        sensitivity: f32,
        max_speed: f32,
    ) -> Vector2 {
        if self.count < 2 {
            return Vector2::ZERO;
        }

        let latest_index = (self.next_index + self.samples.len() - 1) % self.samples.len();
        let latest = self.samples[latest_index];
        let target_time = latest.time_seconds - lookback_seconds;
        let mut oldest = latest;

        for i in 1..self.count {
            let idx = (latest_index + self.samples.len() - i) % self.samples.len();
            let sample = self.samples[idx];
            oldest = sample;
            if sample.time_seconds <= target_time {
                break;
            }
        }

        let elapsed = latest.time_seconds - oldest.time_seconds;
        if elapsed < 0.0001 {
            return Vector2::ZERO;
        }

        let mut velocity = (latest.position - oldest.position) / elapsed as f32;
        velocity *= sensitivity;

        let speed_sq = velocity.length_squared();
        let max_sq = max_speed * max_speed;
        if speed_sq > max_sq {
            let scale = max_speed / speed_sq.sqrt();
            velocity *= scale;
        }

        velocity
    }
}

#[derive(Debug, Default)]
pub struct HitTester;

impl HitTester {
    pub fn hit_test_topmost<'a>(
        &self,
        objects: &'a [ObjectState],
        point: Vector2,
    ) -> Option<&'a ObjectState> {
        let mut best: Option<&ObjectState> = None;
        let mut best_z = i32::MIN;

        for candidate in objects {
            if !candidate.is_visible {
                continue;
            }
            // Objects pushed back in 3D space render offset by perspective, so
            // screen-space hit testing no longer lines up with them.
            if candidate.depth_z < -1.0 {
                continue;
            }

            if !contains_point(&candidate.body, point) {
                continue;
            }

            if candidate.z_index >= best_z {
                best_z = candidate.z_index;
                best = Some(candidate);
            }
        }

        best
    }

    pub fn is_point_over_any_object(&self, objects: &[ObjectState], point: Vector2) -> bool {
        objects.iter().any(|object| {
            object.is_visible && object.depth_z >= -1.0 && contains_point(&object.body, point)
        })
    }
}

fn contains_point(body: &PhysicsBody, point: Vector2) -> bool {
    if body.shape == CollisionShape::Circle {
        let radius = body.width.min(body.height) * 0.5 * body.collision_scale;
        let center_x = body.position.x + (body.width * 0.5);
        let center_y = body.position.y + (body.height * 0.5);
        let delta_x = point.x - center_x;
        let delta_y = point.y - center_y;
        return (delta_x * delta_x) + (delta_y * delta_y) <= radius * radius;
    }

    if body.shape == CollisionShape::Diamond {
        let half_width = body.width * 0.34 * body.collision_scale;
        let half_height = body.height * 0.5 * body.collision_scale;
        let center_x = body.position.x + (body.width * 0.5);
        let center_y = body.position.y + (body.height * 0.5);
        let normalized_x = (point.x - center_x).abs() / half_width.max(1.0);
        let normalized_y = (point.y - center_y).abs() / half_height.max(1.0);
        return normalized_x + normalized_y <= 1.0;
    }

    RectF::new(body.position.x, body.position.y, body.width, body.height).contains(point)
}

#[derive(Debug)]
pub struct DragController {
    mouse_tracker: MouseTracker,
    dragged_id: Option<u64>,
    cursor_offset: Vector2,
    last_drag_update_seconds: f64,
}

impl DragController {
    pub fn new(sample_capacity: usize) -> Self {
        Self {
            mouse_tracker: MouseTracker::new(sample_capacity),
            dragged_id: None,
            cursor_offset: Vector2::ZERO,
            last_drag_update_seconds: 0.0,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragged_id.is_some()
    }

    pub fn dragged_id(&self) -> Option<u64> {
        self.dragged_id
    }

    pub fn cancel_drag(&mut self, objects: &mut [ObjectState]) {
        if let Some(dragged_id) = self.dragged_id {
            if let Some(object) = objects.iter_mut().find(|object| object.id == dragged_id) {
                object.body.is_dragging = false;
                object.is_dragging = false;
            }
        }
        self.dragged_id = None;
        self.mouse_tracker.clear();
    }

    pub fn begin_drag(
        &mut self,
        objects: &mut [ObjectState],
        cursor: Vector2,
        now_seconds: f64,
        hit_tester: &HitTester,
    ) -> Option<u64> {
        if self.dragged_id.is_some() {
            return self.dragged_id;
        }

        let hit_id = hit_tester.hit_test_topmost(objects, cursor)?.id;
        let object = objects.iter_mut().find(|object| object.id == hit_id)?;
        self.dragged_id = Some(hit_id);
        object.is_dragging = true;
        object.body.is_dragging = true;
        object.body.is_sleeping = false;
        object.body.sleep_timer_seconds = 0.0;
        object.body.velocity = Vector2::ZERO;
        self.cursor_offset = cursor - object.body.position;
        self.last_drag_update_seconds = now_seconds;

        self.mouse_tracker.clear();
        self.mouse_tracker.add_sample(cursor, now_seconds);
        Some(hit_id)
    }

    pub fn update_drag(&mut self, objects: &mut [ObjectState], cursor: Vector2, now_seconds: f64) {
        let Some(dragged_id) = self.dragged_id else {
            return;
        };

        self.mouse_tracker.add_sample(cursor, now_seconds);
        if let Some(object) = objects.iter_mut().find(|object| object.id == dragged_id) {
            let previous_position = object.body.position;
            let next_position = cursor - self.cursor_offset;
            let elapsed = (now_seconds - self.last_drag_update_seconds).max(1.0 / 240.0) as f32;
            object.body.velocity = (next_position - previous_position) / elapsed;
            object.body.position = next_position;
            object.body.is_sleeping = false;
            object.body.sleep_timer_seconds = 0.0;
            self.last_drag_update_seconds = now_seconds;
        }
    }

    pub fn end_drag(
        &mut self,
        objects: &mut [ObjectState],
        now_seconds: f64,
        throw_sensitivity: f32,
        max_throw_speed: f32,
    ) -> Vector2 {
        let Some(dragged_id) = self.dragged_id else {
            return Vector2::ZERO;
        };

        let Some(object) = objects.iter_mut().find(|object| object.id == dragged_id) else {
            self.dragged_id = None;
            return Vector2::ZERO;
        };

        self.mouse_tracker
            .add_sample(object.body.position + self.cursor_offset, now_seconds);
        let throw_velocity =
            self.mouse_tracker
                .estimate_velocity(0.085, throw_sensitivity, max_throw_speed);

        object.body.is_dragging = false;
        object.body.is_sleeping = false;
        object.body.sleep_timer_seconds = 0.0;
        object.is_dragging = false;
        object.body.velocity = throw_velocity;
        self.dragged_id = None;
        self.mouse_tracker.clear();

        throw_velocity
    }
}

#[derive(Debug)]
pub struct SceneController {
    config: AppConfig,
    physics_world: PhysicsWorld,
    next_cube_color_index: usize,
    next_spawn_catalog_index: usize,
    next_id: u64,
}

impl SceneController {
    pub fn new(config: AppConfig) -> Self {
        Self {
            physics_world: PhysicsWorld::new(Vector2::new(0.0, config.gravity_y)),
            config,
            next_cube_color_index: 0,
            next_spawn_catalog_index: 0,
            next_id: 1,
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    pub fn objects(&self) -> &[ObjectState] {
        self.physics_world.objects()
    }

    pub fn objects_mut(&mut self) -> &mut [ObjectState] {
        self.physics_world.objects_mut()
    }

    pub fn add_object_velocity(&mut self, id: u64, delta: Vector2) {
        self.physics_world.add_velocity(id, delta);
    }

    pub fn teleport_object(&mut self, id: u64, position: Vector2, velocity: Vector2) -> bool {
        self.physics_world.teleport_object(id, position, velocity)
    }

    pub fn initialize(&mut self, _bounds: RectF) {
        self.next_cube_color_index = 0;
        self.next_spawn_catalog_index = 0;
    }

    pub fn set_gravity(&mut self, gravity_y: f32) {
        self.physics_world.set_gravity(Vector2::new(0.0, gravity_y));
    }

    pub fn step(&mut self, dt: f32, bounds: RectF) {
        self.physics_world.step(
            dt,
            bounds,
            self.config.sleep_threshold,
            self.config.floor_snap_threshold,
        );
    }

    pub fn spawn_next_object(&mut self, position: Vector2) -> u64 {
        let spec = SPAWN_CATALOG[self.next_spawn_catalog_index % SPAWN_CATALOG.len()];
        self.next_spawn_catalog_index += 1;
        self.spawn_object(position, spec.color, spec.visual_kind)
    }

    pub fn spawn_random_crystal(&mut self, position: Vector2) -> u64 {
        self.spawn_object(
            position,
            Some(random_crystal_color()),
            ObjectVisualKind::Crystal,
        )
    }

    pub fn spawn_random_dvd_logo(&mut self, position: Vector2) -> u64 {
        self.spawn_object(
            position,
            Some(random_logo_color()),
            ObjectVisualKind::DvdLogo,
        )
    }

    pub fn spawn_small_cube_batch(&mut self, center: Vector2, count: usize) -> Option<u64> {
        if count == 0 {
            return None;
        }

        let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
        let spacing = STARTUP_CUBE_SIZE * 1.35;
        let rows = count.div_ceil(columns);
        let total_width = (columns as f32 - 1.0) * spacing + STARTUP_CUBE_SIZE;
        let total_height = (rows as f32 - 1.0) * spacing + STARTUP_CUBE_SIZE;
        let start_x = center.x - (total_width * 0.5);
        let start_y = center.y - (total_height * 0.5);
        let mut last_id = None;

        for index in 0..count {
            let column = index % columns;
            let row = index / columns;
            let stagger = if row % 2 == 0 { 0.0 } else { spacing * 0.5 };
            let position = Vector2::new(
                start_x + (column as f32 * spacing) + stagger,
                start_y + (row as f32 * spacing),
            );
            last_id = Some(self.spawn_object_with_size(
                position,
                None,
                ObjectVisualKind::Cube,
                STARTUP_CUBE_SIZE,
            ));
        }

        last_id
    }

    pub fn spawn_screen_shatter(&mut self, bounds: RectF, impact: Vector2) -> usize {
        self.physics_world.clear();
        let impact = Vector2::new(
            impact.x.clamp(bounds.left(), bounds.right()),
            impact.y.clamp(bounds.top(), bounds.bottom()),
        );
        let mut rng = rand::rng();
        let shard_specs = generate_shatter_specs(bounds, impact, &mut rng);
        let shard_count = shard_specs.len();
        for spec in shard_specs {
            self.spawn_screen_shard(spec, bounds, impact, &mut rng);
        }
        shard_count
    }

    pub fn spawn_object(
        &mut self,
        position: Vector2,
        color: Option<AppColor>,
        visual_kind: ObjectVisualKind,
    ) -> u64 {
        self.spawn_object_with_size(position, color, visual_kind, DEFAULT_OBJECT_SIZE)
    }

    pub fn spawn_custom_object(
        &mut self,
        position: Vector2,
        size: Vector2,
        color: AppColor,
        visual_kind: ObjectVisualKind,
        shape: CollisionShape,
    ) -> u64 {
        let id = self.spawn_object_with_size(
            position,
            Some(color),
            visual_kind,
            size.x.min(size.y).max(1.0),
        );
        if let Some(object) = self
            .physics_world
            .objects_mut()
            .iter_mut()
            .find(|object| object.id == id)
        {
            object.body.width = size.x.max(1.0);
            object.body.height = size.y.max(1.0);
            object.body.shape = shape;
            object.body.collision_scale = 1.0;
        }
        id
    }

    pub fn spawn_text_object(&mut self, position: Vector2, text: String) -> u64 {
        self.spawn_text_object_with_scale(position, text, 1.0)
    }

    pub fn spawn_text_object_with_scale(
        &mut self,
        position: Vector2,
        text: String,
        scale: f32,
    ) -> u64 {
        let text = text.trim().chars().take(120).collect::<String>();
        let scale = scale.clamp(0.5, 3.0);
        let width = ((text.chars().count().max(1) as f32) * 18.0).clamp(96.0, 720.0) * scale;
        let id = self.spawn_custom_object(
            position,
            Vector2::new(width, 72.0 * scale),
            AppColor::from_rgb(248, 248, 252),
            ObjectVisualKind::Text,
            CollisionShape::Box,
        );
        if let Some(object) = self.objects_mut().iter_mut().find(|object| object.id == id) {
            object.custom_text = Some(if text.is_empty() {
                "Text".to_string()
            } else {
                text
            });
            object.body.restitution = 0.42;
            object.body.collision_scale = 0.9;
            object.rotation_x = -0.16;
            object.rotation_y = 0.22;
        }
        id
    }

    fn spawn_screen_shard(
        &mut self,
        spec: ShatterShardSpec,
        bounds: RectF,
        impact: Vector2,
        rng: &mut impl RngExt,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let center = Vector2::new(
            spec.min.x + spec.size.x * 0.5,
            spec.min.y + spec.size.y * 0.5,
        );
        let from_impact = center - impact;
        let distance = from_impact.length_squared().sqrt().max(1.0);
        let direction = from_impact / distance;
        let max_distance = max_corner_distance(bounds, impact).max(1.0);
        let near_impact = (1.0 - distance / max_distance).clamp(0.0, 1.0);
        let blast_speed =
            rng.random_range(160.0..430.0) + near_impact * rng.random_range(480.0..920.0);
        let lift = rng.random_range(80.0..310.0) + near_impact * 180.0;
        let area_mass = (spec.area / 18_000.0).clamp(0.45, 3.5);

        let mut state = ObjectState {
            id,
            z_index: self.physics_world.objects().len() as i32 + 1,
            base_color: AppColor::from_rgb(205, 208, 212),
            visual_kind: ObjectVisualKind::ScreenShard,
            screen_shard: Some(spec.geometry),
            ..ObjectState::default()
        };
        state.body.position = spec.min;
        state.body.width = spec.size.x.max(4.0);
        state.body.height = spec.size.y.max(4.0);
        state.body.velocity =
            Vector2::new(direction.x * blast_speed, direction.y * blast_speed - lift);
        state.body.mass = area_mass;
        state.body.restitution = 0.34;
        state.body.friction = 0.82;
        state.body.linear_damping = 0.986;
        state.body.gravity_scale = 1.0;
        state.body.shape = CollisionShape::Box;
        state.body.collision_scale = 0.88;
        state.depth_unlocked = true;
        state.depth_z = rng.random_range(0.0..18.0) + near_impact * 26.0;
        state.depth_velocity =
            rng.random_range(90.0..240.0) + near_impact * rng.random_range(280.0..740.0);
        state.rotation_x = rng.random_range(-10.0..10.0) as f64;
        state.rotation_y = rng.random_range(-10.0..10.0) as f64;
        state.rotation_z = rng.random_range(-4.0..4.0) as f64;
        state.angular_velocity_x = rng.random_range(-340.0..340.0) as f64;
        state.angular_velocity_y = rng.random_range(-360.0..360.0) as f64;
        state.angular_velocity_z = rng.random_range(-420.0..420.0) as f64;

        self.physics_world.add(state);
        id
    }

    pub fn clear_objects(&mut self) {
        self.physics_world.clear();
    }

    pub fn add_static_box_collider(
        &mut self,
        center: (f32, f32, f32),
        half_extents: (f32, f32, f32),
        friction: f32,
        restitution: f32,
    ) {
        self.physics_world
            .add_static_box_collider(center, half_extents, friction, restitution);
    }

    pub fn add_static_sphere_collider(
        &mut self,
        center: (f32, f32, f32),
        radius: f32,
        friction: f32,
        restitution: f32,
    ) {
        self.physics_world
            .add_static_sphere_collider(center, radius, friction, restitution);
    }

    pub fn clear_static_colliders(&mut self) {
        self.physics_world.clear_static_colliders();
    }

    pub fn remove_object(&mut self, id: u64) -> Option<ObjectState> {
        self.physics_world.remove_object(id)
    }

    fn spawn_object_with_size(
        &mut self,
        position: Vector2,
        color: Option<AppColor>,
        visual_kind: ObjectVisualKind,
        base_size: f32,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let mut state = ObjectState {
            id,
            z_index: self.physics_world.objects().len() as i32 + 1,
            base_color: color.unwrap_or_else(|| self.next_cube_color()),
            visual_kind,
            ..ObjectState::default()
        };

        if visual_kind == ObjectVisualKind::DvdLogo {
            state.body.width = base_size * 1.7;
            state.body.height = base_size * 0.78;
        } else if visual_kind == ObjectVisualKind::RobotBuddy {
            state.body.width = base_size * 0.68;
            state.body.height = base_size * 0.52;
        } else if visual_kind == ObjectVisualKind::Snail {
            state.body.width = base_size * 0.94;
            state.body.height = base_size * 0.48;
        } else if visual_kind == ObjectVisualKind::Fan {
            state.body.width = base_size * 1.10;
            state.body.height = base_size * 0.72;
        } else if visual_kind == ObjectVisualKind::QuadDrone {
            state.body.width = base_size * 1.22;
            state.body.height = base_size * 0.72;
        } else {
            state.body.width = base_size;
            state.body.height = base_size;
        }
        state.body.position = position;
        state.body.mass = 1.0;
        state.body.restitution = if visual_kind == ObjectVisualKind::DvdLogo {
            1.0
        } else if visual_kind == ObjectVisualKind::SoftBall {
            0.52
        } else {
            self.config.restitution
        };
        state.body.linear_damping = if visual_kind == ObjectVisualKind::DvdLogo {
            1.0
        } else if matches!(
            visual_kind,
            ObjectVisualKind::FoxBuddy | ObjectVisualKind::RobotBuddy
        ) {
            0.982
        } else if matches!(
            visual_kind,
            ObjectVisualKind::Snail | ObjectVisualKind::QuadDrone
        ) {
            1.0
        } else if visual_kind == ObjectVisualKind::Fan {
            0.992
        } else {
            self.config.linear_damping
        };
        state.body.gravity_scale = if matches!(
            visual_kind,
            ObjectVisualKind::Satellite
                | ObjectVisualKind::DvdLogo
                | ObjectVisualKind::Snail
                | ObjectVisualKind::Fan
                | ObjectVisualKind::QuadDrone
        ) {
            0.0
        } else {
            1.0
        };
        state.body.shape = match visual_kind {
            ObjectVisualKind::Ball
            | ObjectVisualKind::SoftBall
            | ObjectVisualKind::GlassMarble
            | ObjectVisualKind::PlasmaOrb
            | ObjectVisualKind::PortalOrb
            | ObjectVisualKind::SoapBubble
            | ObjectVisualKind::ForcefieldOrb
            | ObjectVisualKind::Ring
            | ObjectVisualKind::GameTarget
            | ObjectVisualKind::Snail
            | ObjectVisualKind::QuadDrone
            | ObjectVisualKind::Basketball => CollisionShape::Circle,
            ObjectVisualKind::Crystal | ObjectVisualKind::BitCrystal | ObjectVisualKind::Star => {
                CollisionShape::Diamond
            }
            _ => CollisionShape::Box,
        };
        state.body.collision_scale = 1.0;
        if matches!(
            visual_kind,
            ObjectVisualKind::FoxBuddy | ObjectVisualKind::RobotBuddy
        ) {
            state.body.mass = 1.45;
            state.body.friction = 1.05;
            state.body.restitution = 0.28;
        }
        if visual_kind == ObjectVisualKind::SoftBall {
            state.body.mass = 0.85;
            state.body.friction = 0.92;
            state.body.linear_damping = 0.986;
            state.body.collision_scale = 0.94;
        }
        if matches!(
            visual_kind,
            ObjectVisualKind::GlassMarble
                | ObjectVisualKind::PlasmaOrb
                | ObjectVisualKind::PortalOrb
                | ObjectVisualKind::SoapBubble
                | ObjectVisualKind::ForcefieldOrb
        ) {
            state.body.mass = 1.35;
            state.body.friction = 0.82;
            state.body.restitution = 0.42;
            state.body.linear_damping = 0.991;
            state.body.collision_scale = 0.98;
        }
        if visual_kind == ObjectVisualKind::RobotBuddy {
            state.body.mass = 1.65;
            state.body.friction = 1.35;
            state.body.restitution = 0.08;
            state.body.linear_damping = 0.975;
            state.body.collision_scale = 0.9;
            state.body.lock_rotation = true;
        }
        if visual_kind == ObjectVisualKind::Snail {
            state.body.mass = 2.4;
            state.body.friction = 1.8;
            state.body.restitution = 0.02;
            state.body.collision_scale = 0.92;
            state.body.lock_rotation = true;
        }
        if visual_kind == ObjectVisualKind::Fan {
            state.body.mass = 4.0;
            state.body.friction = 1.2;
            state.body.restitution = 0.12;
            state.body.collision_scale = 0.96;
        }
        if visual_kind == ObjectVisualKind::QuadDrone {
            state.body.mass = 1.1;
            state.body.friction = 0.5;
            state.body.restitution = 0.18;
            state.body.collision_scale = 0.88;
            state.body.lock_rotation = true;
        }
        if visual_kind == ObjectVisualKind::DvdLogo {
            state.body.velocity = Vector2::new(420.0, 260.0);
        }
        self.physics_world.add(state);
        id
    }

    pub fn spawn_imported_model(
        &mut self,
        position: Vector2,
        source_path: String,
        scale_multiplier: f32,
        tint: AppColor,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let mut state = ObjectState {
            id,
            z_index: self.physics_world.objects().len() as i32 + 1,
            base_color: tint,
            visual_kind: ObjectVisualKind::ImportedModel,
            model_source_path: Some(source_path),
            model_scale_multiplier: scale_multiplier,
            ..ObjectState::default()
        };

        let scaled_size = DEFAULT_OBJECT_SIZE * scale_multiplier;
        state.body.width = scaled_size;
        state.body.height = scaled_size;
        state.body.position = position;
        state.body.mass = 1.0;
        state.body.restitution = self.config.restitution;
        state.body.linear_damping = self.config.linear_damping;
        state.body.gravity_scale = 1.0;
        state.body.shape = CollisionShape::Box;
        state.body.collision_scale = 1.0;
        self.physics_world.add(state);
        id
    }

    pub fn reset(&mut self, _bounds: RectF) {
        self.physics_world.clear();
        self.next_cube_color_index = 0;
        self.next_spawn_catalog_index = 0;
    }

    pub fn apply_runtime_physics_config(&mut self) {
        for object in self.physics_world.objects_mut() {
            object.body.restitution = self.config.restitution;
            object.body.linear_damping = self.config.linear_damping;
        }
    }

    fn next_cube_color(&mut self) -> AppColor {
        let color = CUBE_PALETTE[self.next_cube_color_index % CUBE_PALETTE.len()];
        self.next_cube_color_index += 1;
        color
    }
}

#[derive(Debug)]
struct ShatterShardSpec {
    min: Vector2,
    size: Vector2,
    area: f32,
    geometry: ScreenShardGeometry,
}

fn generate_shatter_specs(
    bounds: RectF,
    impact: Vector2,
    rng: &mut impl RngExt,
) -> Vec<ShatterShardSpec> {
    let max_radius = max_corner_distance(bounds, impact) * 1.12;
    let impact_radius = (bounds.width.min(bounds.height) * 0.045).clamp(42.0, 92.0);
    let usable_radius = (max_radius - impact_radius).max(1.0);
    let angle_step = std::f32::consts::TAU / SHATTER_WEDGE_COUNT as f32;
    let start_angle = rng.random_range(0.0..angle_step);
    let mut angles = Vec::with_capacity(SHATTER_WEDGE_COUNT + 1);
    for index in 0..=SHATTER_WEDGE_COUNT {
        if index == SHATTER_WEDGE_COUNT {
            angles.push(angles[0] + std::f32::consts::TAU);
        } else {
            let jitter = rng.random_range(-angle_step * 0.24..angle_step * 0.24);
            angles.push(start_angle + angle_step * index as f32 + jitter);
        }
    }
    angles.sort_by(|left, right| left.total_cmp(right));
    let first = angles[0];
    angles[SHATTER_WEDGE_COUNT] = first + std::f32::consts::TAU;

    let mut radii_by_angle: Vec<Vec<f32>> = Vec::with_capacity(SHATTER_WEDGE_COUNT + 1);
    for angle_index in 0..=SHATTER_WEDGE_COUNT {
        if angle_index == SHATTER_WEDGE_COUNT {
            radii_by_angle.push(radii_by_angle[0].clone());
            continue;
        }

        let mut radii = Vec::with_capacity(SHATTER_RING_FACTORS.len());
        for (ring_index, factor) in SHATTER_RING_FACTORS.iter().enumerate() {
            let base = impact_radius + usable_radius * factor;
            let jitter = if ring_index == 0 {
                rng.random_range(0.82..1.18)
            } else if ring_index == SHATTER_RING_FACTORS.len() - 1 {
                rng.random_range(1.02..1.16)
            } else {
                rng.random_range(0.90..1.12)
            };
            radii.push(base * jitter);
        }
        radii_by_angle.push(radii);
    }

    let mut specs = Vec::with_capacity(SHATTER_WEDGE_COUNT * (SHATTER_RING_FACTORS.len() - 1));
    for wedge_index in 0..SHATTER_WEDGE_COUNT {
        let angle0 = angles[wedge_index];
        let angle1 = angles[wedge_index + 1];
        for ring_index in 0..SHATTER_RING_FACTORS.len() - 1 {
            let polygon = vec![
                point_from_polar(impact, angle0, radii_by_angle[wedge_index][ring_index]),
                point_from_polar(impact, angle1, radii_by_angle[wedge_index + 1][ring_index]),
                point_from_polar(
                    impact,
                    angle1,
                    radii_by_angle[wedge_index + 1][ring_index + 1],
                ),
                point_from_polar(impact, angle0, radii_by_angle[wedge_index][ring_index + 1]),
            ];
            let clipped = dedupe_polygon(clip_polygon_to_rect(polygon, bounds));
            if let Some(spec) = build_shatter_spec(clipped, bounds) {
                specs.push(spec);
            }
        }
    }
    specs
}

fn build_shatter_spec(points: Vec<Vector2>, bounds: RectF) -> Option<ShatterShardSpec> {
    if points.len() < 3 {
        return None;
    }

    let area = polygon_area(&points);
    if area < SHATTER_MIN_AREA {
        return None;
    }

    let mut min = Vector2::new(f32::MAX, f32::MAX);
    let mut max = Vector2::new(f32::MIN, f32::MIN);
    for point in &points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let size = Vector2::new((max.x - min.x).max(1.0), (max.y - min.y).max(1.0));
    if size.x < 4.0 || size.y < 4.0 {
        return None;
    }

    let center = Vector2::new(min.x + size.x * 0.5, min.y + size.y * 0.5);
    let local_points = points.iter().map(|point| *point - center).collect();
    let texture_uvs = points
        .iter()
        .map(|point| {
            Vector2::new(
                ((point.x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0),
                ((point.y - bounds.y) / bounds.height.max(1.0)).clamp(0.0, 1.0),
            )
        })
        .collect();

    Some(ShatterShardSpec {
        min,
        size,
        area,
        geometry: ScreenShardGeometry {
            local_points,
            texture_uvs,
            thickness: SHARD_THICKNESS,
        },
    })
}

fn clip_polygon_to_rect(points: Vec<Vector2>, bounds: RectF) -> Vec<Vector2> {
    let points = clip_polygon_edge(
        points,
        |point| point.x >= bounds.left(),
        |a, b| intersect_x(a, b, bounds.left()),
    );
    let points = clip_polygon_edge(
        points,
        |point| point.x <= bounds.right(),
        |a, b| intersect_x(a, b, bounds.right()),
    );
    let points = clip_polygon_edge(
        points,
        |point| point.y >= bounds.top(),
        |a, b| intersect_y(a, b, bounds.top()),
    );
    clip_polygon_edge(
        points,
        |point| point.y <= bounds.bottom(),
        |a, b| intersect_y(a, b, bounds.bottom()),
    )
}

fn clip_polygon_edge(
    points: Vec<Vector2>,
    inside: impl Fn(Vector2) -> bool,
    intersect: impl Fn(Vector2, Vector2) -> Vector2,
) -> Vec<Vector2> {
    if points.is_empty() {
        return points;
    }

    let mut output = Vec::with_capacity(points.len() + 2);
    let mut previous = *points.last().expect("non-empty polygon has a last point");
    let mut previous_inside = inside(previous);
    for current in points {
        let current_inside = inside(current);
        if current_inside {
            if !previous_inside {
                output.push(intersect(previous, current));
            }
            output.push(current);
        } else if previous_inside {
            output.push(intersect(previous, current));
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn intersect_x(a: Vector2, b: Vector2, x: f32) -> Vector2 {
    let dx = b.x - a.x;
    if dx.abs() <= f32::EPSILON {
        return Vector2::new(x, a.y);
    }
    let t = ((x - a.x) / dx).clamp(0.0, 1.0);
    Vector2::new(x, a.y + (b.y - a.y) * t)
}

fn intersect_y(a: Vector2, b: Vector2, y: f32) -> Vector2 {
    let dy = b.y - a.y;
    if dy.abs() <= f32::EPSILON {
        return Vector2::new(a.x, y);
    }
    let t = ((y - a.y) / dy).clamp(0.0, 1.0);
    Vector2::new(a.x + (b.x - a.x) * t, y)
}

fn dedupe_polygon(points: Vec<Vector2>) -> Vec<Vector2> {
    let mut deduped: Vec<Vector2> = Vec::with_capacity(points.len());
    for point in points {
        if deduped
            .last()
            .map(|last| (*last - point).length_squared() > 0.25)
            .unwrap_or(true)
        {
            deduped.push(point);
        }
    }
    if deduped.len() > 2
        && (deduped[0]
            - *deduped
                .last()
                .expect("deduped polygon should have a last point"))
        .length_squared()
            <= 0.25
    {
        deduped.pop();
    }
    deduped
}

fn polygon_area(points: &[Vector2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        sum += points[index].x * points[next].y - points[next].x * points[index].y;
    }
    sum.abs() * 0.5
}

fn point_from_polar(origin: Vector2, angle: f32, radius: f32) -> Vector2 {
    let (sin, cos) = angle.sin_cos();
    Vector2::new(origin.x + cos * radius, origin.y + sin * radius)
}

fn max_corner_distance(bounds: RectF, point: Vector2) -> f32 {
    [
        Vector2::new(bounds.left(), bounds.top()),
        Vector2::new(bounds.right(), bounds.top()),
        Vector2::new(bounds.right(), bounds.bottom()),
        Vector2::new(bounds.left(), bounds.bottom()),
    ]
    .into_iter()
    .map(|corner| (corner - point).length_squared().sqrt())
    .fold(0.0, f32::max)
}

fn random_crystal_color() -> AppColor {
    let mut rng = rand::rng();
    let hue = rng.random_range(0.0..360.0);
    color_from_hsv(hue, 0.55, 1.0)
}

fn random_logo_color() -> AppColor {
    let mut rng = rand::rng();
    let hue = rng.random_range(0.0..360.0);
    color_from_hsv(hue, 0.82, 1.0)
}

fn color_from_hsv(hue: f64, saturation: f64, value: f64) -> AppColor {
    let hue = ((hue % 360.0) + 360.0) % 360.0;
    let chroma = value * saturation;
    let segment = hue / 60.0;
    let x = chroma * (1.0 - ((segment % 2.0) - 1.0).abs());

    let (red, green, blue) = if segment < 1.0 {
        (chroma, x, 0.0)
    } else if segment < 2.0 {
        (x, chroma, 0.0)
    } else if segment < 3.0 {
        (0.0, chroma, x)
    } else if segment < 4.0 {
        (0.0, x, chroma)
    } else if segment < 5.0 {
        (x, 0.0, chroma)
    } else {
        (chroma, 0.0, x)
    };

    let matched = value - chroma;
    AppColor::from_rgb(
        to_byte(red + matched),
        to_byte(green + matched),
        to_byte(blue + matched),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_objects_keep_their_content_and_do_not_expire() {
        let mut scene = SceneController::new(AppConfig::default());
        let bounds = RectF::new(0.0, 0.0, 1920.0, 1080.0);
        let id = scene.spawn_text_object(Vector2::new(320.0, 120.0), "  Hello forever  ".into());

        for _ in 0..36_000 {
            scene.step(1.0 / 60.0, bounds);
        }

        let text = scene
            .objects()
            .iter()
            .find(|object| object.id == id)
            .expect("ordinary scene updates must not expire text objects");
        assert_eq!(text.visual_kind, ObjectVisualKind::Text);
        assert_eq!(text.custom_text.as_deref(), Some("Hello forever"));
    }

    #[test]
    fn text_objects_use_a_safe_default_and_limit_custom_input() {
        let mut scene = SceneController::new(AppConfig::default());
        let empty_id = scene.spawn_text_object(Vector2::ZERO, "   ".into());
        let long_id = scene.spawn_text_object(Vector2::ZERO, "x".repeat(200));

        let empty = scene
            .objects()
            .iter()
            .find(|object| object.id == empty_id)
            .unwrap();
        let long = scene
            .objects()
            .iter()
            .find(|object| object.id == long_id)
            .unwrap();
        assert_eq!(empty.custom_text.as_deref(), Some("Text"));
        assert_eq!(long.custom_text.as_deref().unwrap().chars().count(), 120);
    }

    #[test]
    fn text_size_scales_visuals_and_physics_bounds_together() {
        let mut scene = SceneController::new(AppConfig::default());
        let medium_id = scene.spawn_text_object_with_scale(Vector2::ZERO, "Size".into(), 1.0);
        let xlarge_id = scene.spawn_text_object_with_scale(Vector2::ZERO, "Size".into(), 2.0);
        let medium = scene
            .objects()
            .iter()
            .find(|object| object.id == medium_id)
            .unwrap();
        let xlarge = scene
            .objects()
            .iter()
            .find(|object| object.id == xlarge_id)
            .unwrap();

        assert_eq!(xlarge.body.width, medium.body.width * 2.0);
        assert_eq!(xlarge.body.height, medium.body.height * 2.0);
    }
}

fn to_byte(value: f64) -> u8 {
    (value.mul_add(255.0, 0.0).round().clamp(0.0, 255.0)) as u8
}
