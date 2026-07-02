use std::time::Instant;

use core_types::{AppColor, AppConfig, CollisionShape, ObjectState, ObjectVisualKind, PhysicsBody, RectF, Vector2};
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
const STARTUP_CUBE_COUNT: usize = 100;
const STARTUP_CUBE_COLUMNS: usize = 10;
const STARTUP_CUBE_SIZE: f32 = DEFAULT_OBJECT_SIZE * 0.2;

const SPAWN_CATALOG: [SpawnSpec; 11] = [
    SpawnSpec::new(ObjectVisualKind::Cube, None),
    SpawnSpec::new(ObjectVisualKind::Ball, Some(AppColor::from_rgb(90, 205, 255))),
    SpawnSpec::new(ObjectVisualKind::Pyramid, Some(AppColor::from_rgb(255, 176, 92))),
    SpawnSpec::new(ObjectVisualKind::Barrel, Some(AppColor::from_rgb(126, 226, 168))),
    SpawnSpec::new(ObjectVisualKind::Ring, Some(AppColor::from_rgb(255, 118, 210))),
    SpawnSpec::new(ObjectVisualKind::Star, Some(AppColor::from_rgb(255, 224, 92))),
    SpawnSpec::new(ObjectVisualKind::Crystal, Some(AppColor::from_rgb(108, 241, 255))),
    SpawnSpec::new(ObjectVisualKind::Satellite, Some(AppColor::from_rgb(88, 160, 255))),
    SpawnSpec::new(ObjectVisualKind::DvdLogo, Some(AppColor::from_rgb(244, 78, 255))),
    SpawnSpec::new(ObjectVisualKind::Dice, Some(AppColor::from_rgb(245, 245, 240))),
    SpawnSpec::new(ObjectVisualKind::Crystal, Some(AppColor::from_rgb(255, 112, 214))),
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

    pub fn estimate_velocity(&self, lookback_seconds: f64, sensitivity: f32, max_speed: f32) -> Vector2 {
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
    pub fn hit_test_topmost<'a>(&self, objects: &'a [ObjectState], point: Vector2) -> Option<&'a ObjectState> {
        let mut best: Option<&ObjectState> = None;
        let mut best_z = i32::MIN;

        for candidate in objects {
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
        objects.iter().any(|object| contains_point(&object.body, point))
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
        let throw_velocity = self
            .mouse_tracker
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

    pub fn initialize(&mut self, bounds: RectF) {
        self.next_cube_color_index = 0;
        self.next_spawn_catalog_index = 0;
        self.spawn_initial_objects(bounds);
    }

    pub fn set_gravity(&mut self, gravity_y: f32) {
        self.physics_world.set_gravity(Vector2::new(0.0, gravity_y));
    }

    pub fn step(&mut self, dt: f32, bounds: RectF) {
        self.physics_world
            .step(dt, bounds, self.config.sleep_threshold, self.config.floor_snap_threshold);
    }

    pub fn spawn_next_object(&mut self, position: Vector2) -> u64 {
        let spec = SPAWN_CATALOG[self.next_spawn_catalog_index % SPAWN_CATALOG.len()];
        self.next_spawn_catalog_index += 1;
        self.spawn_object(position, spec.color, spec.visual_kind)
    }

    pub fn spawn_random_crystal(&mut self, position: Vector2) -> u64 {
        self.spawn_object(position, Some(random_crystal_color()), ObjectVisualKind::Crystal)
    }

    pub fn spawn_random_dvd_logo(&mut self, position: Vector2) -> u64 {
        self.spawn_object(position, Some(random_logo_color()), ObjectVisualKind::DvdLogo)
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
            last_id = Some(self.spawn_object_with_size(position, None, ObjectVisualKind::Cube, STARTUP_CUBE_SIZE));
        }

        last_id
    }

    pub fn spawn_object(&mut self, position: Vector2, color: Option<AppColor>, visual_kind: ObjectVisualKind) -> u64 {
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
        let id = self.spawn_object_with_size(position, Some(color), visual_kind, size.x.min(size.y).max(1.0));
        if let Some(object) = self.physics_world.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.width = size.x.max(1.0);
            object.body.height = size.y.max(1.0);
            object.body.shape = shape;
            object.body.collision_scale = 1.0;
        }
        id
    }

    pub fn clear_objects(&mut self) {
        self.physics_world.clear();
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
        } else {
            state.body.width = base_size;
            state.body.height = base_size;
        }
        state.body.position = position;
        state.body.mass = 1.0;
        state.body.restitution = if visual_kind == ObjectVisualKind::DvdLogo {
            1.0
        } else {
            self.config.restitution
        };
        state.body.linear_damping = if visual_kind == ObjectVisualKind::DvdLogo {
            1.0
        } else {
            self.config.linear_damping
        };
        state.body.gravity_scale = if matches!(visual_kind, ObjectVisualKind::Satellite | ObjectVisualKind::DvdLogo) {
            0.0
        } else {
            1.0
        };
        state.body.shape = match visual_kind {
            ObjectVisualKind::Ball | ObjectVisualKind::Ring => CollisionShape::Circle,
            ObjectVisualKind::Crystal | ObjectVisualKind::Star => CollisionShape::Diamond,
            _ => CollisionShape::Box,
        };
        state.body.collision_scale = 1.0;
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

    pub fn reset(&mut self, bounds: RectF) {
        self.physics_world.clear();
        self.next_cube_color_index = 0;
        self.next_spawn_catalog_index = 0;
        self.spawn_initial_objects(bounds);
    }

    pub fn apply_runtime_physics_config(&mut self) {
        for object in self.physics_world.objects_mut() {
            object.body.restitution = self.config.restitution;
            object.body.linear_damping = self.config.linear_damping;
        }
    }

    fn spawn_initial_objects(&mut self, bounds: RectF) {
        let spacing = STARTUP_CUBE_SIZE * 1.35;
        let total_width = (STARTUP_CUBE_COLUMNS as f32 - 1.0) * spacing + STARTUP_CUBE_SIZE;
        let start_x = ((bounds.width - total_width) * 0.5).max(12.0);
        let start_y = (bounds.height * 0.10).max(12.0);

        for index in 0..STARTUP_CUBE_COUNT {
            let column = index % STARTUP_CUBE_COLUMNS;
            let row = index / STARTUP_CUBE_COLUMNS;
            let stagger = if row % 2 == 0 { 0.0 } else { spacing * 0.5 };
            let position = Vector2::new(
                start_x + (column as f32 * spacing) + stagger,
                start_y + (row as f32 * spacing),
            );
            self.spawn_object_with_size(position, None, ObjectVisualKind::Cube, STARTUP_CUBE_SIZE);
        }
    }

    fn next_cube_color(&mut self) -> AppColor {
        let color = CUBE_PALETTE[self.next_cube_color_index % CUBE_PALETTE.len()];
        self.next_cube_color_index += 1;
        color
    }
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
    AppColor::from_rgb(to_byte(red + matched), to_byte(green + matched), to_byte(blue + matched))
}

fn to_byte(value: f64) -> u8 {
    (value.mul_add(255.0, 0.0).round().clamp(0.0, 255.0)) as u8
}
