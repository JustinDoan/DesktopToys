use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use core_types::{AppColor, CollisionShape, ObjectState, ObjectVisualKind, PhysicsBody, RectF, Vector2};

const PENETRATION_SLOP: f32 = 0.75;
const POSITION_CORRECTION_PERCENT: f32 = 0.8;
const WAKE_IMPULSE_THRESHOLD: f32 = 75.0;
const WAKE_VELOCITY_THRESHOLD: f32 = 95.0;
const SLEEP_ANGULAR_THRESHOLD: f64 = 18.0;
const SLEEP_SETTLE_TIME_SECONDS: f32 = 0.55;
const DEFAULT_CELL_SIZE: f32 = 120.0;
const COLOR_RANDOMIZER_MULTIPLIER: u64 = 6364136223846793005;
const COLOR_RANDOMIZER_INCREMENT: u64 = 1442695040888963407;
static COLOR_RANDOMIZER_STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionPair {
    pub left_index: usize,
    pub right_index: usize,
}

impl CollisionPair {
    pub const fn new(left_index: usize, right_index: usize) -> Self {
        Self {
            left_index,
            right_index,
        }
    }
}

#[derive(Debug)]
pub struct BroadphaseGrid {
    cell_size: f32,
    cells: HashMap<i64, Vec<usize>>,
    active_cell_keys: Vec<i64>,
    pair_keys: HashSet<u64>,
    pairs: Vec<CollisionPair>,
}

impl Default for BroadphaseGrid {
    fn default() -> Self {
        Self::new(DEFAULT_CELL_SIZE)
    }
}

impl BroadphaseGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(16.0),
            cells: HashMap::new(),
            active_cell_keys: Vec::new(),
            pair_keys: HashSet::new(),
            pairs: Vec::new(),
        }
    }

    pub fn build_pairs(&mut self, objects: &[ObjectState]) -> &[CollisionPair] {
        self.reset_frame_state();
        self.fill_cells(objects);
        self.build_unique_pairs(objects);
        &self.pairs
    }

    fn reset_frame_state(&mut self) {
        for key in &self.active_cell_keys {
            if let Some(list) = self.cells.get_mut(key) {
                list.clear();
            }
        }

        self.active_cell_keys.clear();
        self.pair_keys.clear();
        self.pairs.clear();
    }

    fn fill_cells(&mut self, objects: &[ObjectState]) {
        for (object_index, object) in objects.iter().enumerate() {
            let body = &object.body;
            let min_cell_x = self.to_cell_index(body.position.x);
            let max_cell_x = self.to_cell_index(body.position.x + body.width);
            let min_cell_y = self.to_cell_index(body.position.y);
            let max_cell_y = self.to_cell_index(body.position.y + body.height);

            for cell_y in min_cell_y..=max_cell_y {
                for cell_x in min_cell_x..=max_cell_x {
                    let key = pack_cell_key(cell_x, cell_y);
                    let list = self.cells.entry(key).or_default();

                    if list.is_empty() {
                        self.active_cell_keys.push(key);
                    }

                    list.push(object_index);
                }
            }
        }
    }

    fn build_unique_pairs(&mut self, objects: &[ObjectState]) {
        for key in &self.active_cell_keys {
            let Some(list) = self.cells.get(key) else {
                continue;
            };

            for i in 0..list.len() {
                for j in (i + 1)..list.len() {
                    let left = list[i];
                    let right = list[j];
                    if left == right {
                        continue;
                    }

                    let left_body = &objects[left].body;
                    let right_body = &objects[right].body;
                    if left_body.is_sleeping && right_body.is_sleeping {
                        continue;
                    }

                    let min_index = left.min(right);
                    let max_index = left.max(right);
                    let pair_key = ((min_index as u64) << 32) | max_index as u64;
                    if !self.pair_keys.insert(pair_key) {
                        continue;
                    }

                    self.pairs.push(CollisionPair::new(min_index, max_index));
                }
            }
        }
    }

    fn to_cell_index(&self, value: f32) -> i32 {
        (value / self.cell_size).floor() as i32
    }
}

#[derive(Debug)]
pub struct PhysicsWorld {
    objects: Vec<ObjectState>,
    broadphase: BroadphaseGrid,
    gravity: Vector2,
}

impl PhysicsWorld {
    pub fn new(gravity: Vector2) -> Self {
        Self {
            objects: Vec::new(),
            broadphase: BroadphaseGrid::default(),
            gravity,
        }
    }

    pub fn objects(&self) -> &[ObjectState] {
        &self.objects
    }

    pub fn objects_mut(&mut self) -> &mut [ObjectState] {
        &mut self.objects
    }

    pub fn set_gravity(&mut self, gravity: Vector2) {
        self.gravity = gravity;
    }

    pub fn add(&mut self, state: ObjectState) {
        self.objects.push(state);
    }

    pub fn clear(&mut self) {
        self.objects.clear();
    }

    pub fn step(&mut self, dt: f32, bounds: RectF, sleep_threshold: f32, floor_snap_threshold: f32) {
        for object in &mut self.objects {
            let body = &mut object.body;

            if body.is_dragging || body.is_sleeping {
                continue;
            }

            let scaled_gravity = self.gravity * body.gravity_scale;
            body.velocity += (scaled_gravity + body.acceleration) * dt;
            body.velocity *= body.linear_damping;
            body.position += body.velocity * dt;
        }

        let pairs = self.broadphase.build_pairs(&self.objects).to_vec();
        solve_object_collisions(&mut self.objects, Some(&pairs));

        for object in &mut self.objects {
            let body = &mut object.body;

            if body.is_dragging || body.is_sleeping {
                continue;
            }

            let hit_bounds = solve_screen_bounds(body, bounds, sleep_threshold, floor_snap_threshold);
            if hit_bounds && object.visual_kind == ObjectVisualKind::DvdLogo {
                object.base_color = random_logo_color();
            }

            object.angular_velocity_y += body.velocity.x as f64 * 0.00045;
            object.angular_velocity_x += body.velocity.y as f64 * 0.00018;
            object.angular_velocity_z += body.velocity.x as f64 * 0.00012;

            object.angular_velocity_x = clamp(object.angular_velocity_x * 0.992, -340.0, 340.0);
            object.angular_velocity_y = clamp(object.angular_velocity_y * 0.992, -420.0, 420.0);
            object.angular_velocity_z = clamp(object.angular_velocity_z * 0.992, -280.0, 280.0);

            if matches!(object.visual_kind, core_types::ObjectVisualKind::Dice) {
                apply_dice_face_settling(object, bounds, dt, sleep_threshold);
            }

            object.rotation_x += object.angular_velocity_x * dt as f64;
            object.rotation_y += object.angular_velocity_y * dt as f64;
            object.rotation_z += object.angular_velocity_z * dt as f64;
            update_sleep_state(object, bounds, dt, sleep_threshold, floor_snap_threshold);
        }
    }
}

pub fn solve_object_collisions(objects: &mut [ObjectState], pairs: Option<&[CollisionPair]>) {
    if let Some(pairs) = pairs {
        for pair in pairs {
            resolve_pair_by_index(objects, pair.left_index, pair.right_index);
        }
        return;
    }

    for left_index in 0..objects.len() {
        for right_index in (left_index + 1)..objects.len() {
            resolve_pair_by_index(objects, left_index, right_index);
        }
    }
}

pub fn solve_screen_bounds(
    body: &mut PhysicsBody,
    bounds: RectF,
    sleep_threshold: f32,
    floor_snap_threshold: f32,
) -> bool {
    let mut hit_x = false;
    let mut hit_y = false;
    let inset_x = (body.width * 0.5) - effective_half_width(body);
    let inset_y = (body.height * 0.5) - effective_half_height(body);
    let min_x = bounds.left() - inset_x;
    let max_x = bounds.right() - body.width + inset_x;
    let min_y = bounds.top() - inset_y;
    let max_y = bounds.bottom() - body.height + inset_y;

    if body.position.x < min_x {
        body.position.x = min_x;
        hit_x = true;
    } else if body.position.x > max_x {
        body.position.x = max_x;
        hit_x = true;
    }

    if body.position.y < min_y {
        body.position.y = min_y;
        hit_y = true;
    } else if body.position.y > max_y {
        body.position.y = max_y;
        hit_y = true;
    }

    if hit_x {
        body.velocity.x = -body.velocity.x * body.restitution;
    }

    if hit_y {
        body.velocity.y = -body.velocity.y * body.restitution;
    }

    let effective_bottom = body.position.y + body.height - inset_y;
    if effective_bottom >= bounds.bottom() - floor_snap_threshold && body.velocity.y.abs() < sleep_threshold {
        body.velocity.y = 0.0;
    }

    hit_x || hit_y
}

fn update_sleep_state(
    object: &mut ObjectState,
    bounds: RectF,
    dt: f32,
    sleep_threshold: f32,
    floor_snap_threshold: f32,
) {
    let body = &mut object.body;
    if body.is_dragging {
        body.is_sleeping = false;
        body.sleep_timer_seconds = 0.0;
        return;
    }

    let speed_squared = body.velocity.length_squared();
    let linear_threshold = sleep_threshold * sleep_threshold;
    let angular_speed =
        object.angular_velocity_x.abs() + object.angular_velocity_y.abs() + object.angular_velocity_z.abs();
    let inset_y = (body.height * 0.5) - (body.height * 0.5 * body.collision_scale);
    let effective_bottom = body.position.y + body.height - inset_y;
    let near_floor = effective_bottom >= bounds.bottom() - floor_snap_threshold - 1.0;
    let can_sleep = near_floor && speed_squared <= linear_threshold && angular_speed <= SLEEP_ANGULAR_THRESHOLD;
    if !can_sleep {
        body.is_sleeping = false;
        body.sleep_timer_seconds = 0.0;
        return;
    }

    body.sleep_timer_seconds += dt;
    if body.sleep_timer_seconds < SLEEP_SETTLE_TIME_SECONDS {
        return;
    }

    body.is_sleeping = true;
    body.sleep_timer_seconds = SLEEP_SETTLE_TIME_SECONDS;
    body.velocity = Vector2::ZERO;
    object.angular_velocity_x = 0.0;
    object.angular_velocity_y = 0.0;
    object.angular_velocity_z = 0.0;
}

fn apply_dice_face_settling(object: &mut ObjectState, bounds: RectF, dt: f32, sleep_threshold: f32) {
    let body = &mut object.body;
    let is_near_floor = body.position.y + body.height >= bounds.bottom() - 2.0;
    let horizontal_speed = body.velocity.x.abs();
    let vertical_speed = body.velocity.y.abs();
    let angular_speed =
        object.angular_velocity_x.abs() + object.angular_velocity_y.abs() + object.angular_velocity_z.abs();

    if !is_near_floor
        || horizontal_speed > sleep_threshold * 1.1
        || vertical_speed > sleep_threshold * 0.8
        || angular_speed > 260.0
    {
        return;
    }

    let blend = 1.0f64.min(dt as f64 * 7.5);
    let target_x = nearest_quarter_turn(object.rotation_x);
    let target_y = nearest_quarter_turn(object.rotation_y);
    let target_z = nearest_quarter_turn(object.rotation_z);

    object.angular_velocity_x *= 0.82;
    object.angular_velocity_y *= 0.82;
    object.angular_velocity_z *= 0.82;

    object.rotation_x += shortest_angle_delta(object.rotation_x, target_x) * blend;
    object.rotation_y += shortest_angle_delta(object.rotation_y, target_y) * blend;
    object.rotation_z += shortest_angle_delta(object.rotation_z, target_z) * blend;

    if shortest_angle_delta(object.rotation_x, target_x).abs() < 0.75
        && shortest_angle_delta(object.rotation_y, target_y).abs() < 0.75
        && shortest_angle_delta(object.rotation_z, target_z).abs() < 0.75
        && angular_speed < 42.0
    {
        object.rotation_x = target_x;
        object.rotation_y = target_y;
        object.rotation_z = target_z;
        object.angular_velocity_x = 0.0;
        object.angular_velocity_y = 0.0;
        object.angular_velocity_z = 0.0;
    }
}

fn resolve_pair_by_index(objects: &mut [ObjectState], left_index: usize, right_index: usize) {
    let (left_slice, right_slice) = objects.split_at_mut(right_index);
    let left = &mut left_slice[left_index];
    let right = &mut right_slice[0];
    resolve_object_pair(left, right);
}

fn resolve_object_pair(left: &mut ObjectState, right: &mut ObjectState) {
    let left_body = left.body;
    let right_body = right.body;

    if left_body.is_dragging && right_body.is_dragging {
        return;
    }

    if left_body.is_sleeping && right_body.is_sleeping {
        return;
    }

    if left_body.shape == CollisionShape::Circle && right_body.shape == CollisionShape::Circle {
        if resolve_circle_pair(left, right) {
            randomize_logo_colors_on_contact(left, right);
        }
        return;
    }

    if left_body.shape == CollisionShape::Circle || right_body.shape == CollisionShape::Circle {
        if resolve_circle_box_pair(left, right) {
            randomize_logo_colors_on_contact(left, right);
        }
        return;
    }

    let left_center_x = left_body.position.x + (left_body.width * 0.5);
    let left_center_y = left_body.position.y + (left_body.height * 0.5);
    let right_center_x = right_body.position.x + (right_body.width * 0.5);
    let right_center_y = right_body.position.y + (right_body.height * 0.5);

    let delta_x = right_center_x - left_center_x;
    let delta_y = right_center_y - left_center_y;
    let overlap_x = effective_half_width(&left_body) + effective_half_width(&right_body) - delta_x.abs();
    let overlap_y = effective_half_height(&left_body) + effective_half_height(&right_body) - delta_y.abs();
    if overlap_x <= 0.0 || overlap_y <= 0.0 {
        return;
    }

    let left_inverse_mass = if left_body.is_dragging { 0.0 } else { inverse_mass(&left_body) };
    let right_inverse_mass = if right_body.is_dragging { 0.0 } else { inverse_mass(&right_body) };
    let inverse_mass_sum = left_inverse_mass + right_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return;
    }

    let (normal_x, normal_y, penetration) = if overlap_x < overlap_y {
        (if delta_x >= 0.0 { 1.0 } else { -1.0 }, 0.0, overlap_x)
    } else {
        (0.0, if delta_y >= 0.0 { 1.0 } else { -1.0 }, overlap_y)
    };

    let correction_x = normal_x * penetration;
    let correction_y = normal_y * penetration;
    apply_position_correction(
        &mut left.body,
        &mut right.body,
        correction_x,
        correction_y,
        left_inverse_mass,
        right_inverse_mass,
        inverse_mass_sum,
    );

    apply_collision_impulse(left, right, normal_x, normal_y, left_inverse_mass, right_inverse_mass, inverse_mass_sum);
    randomize_logo_colors_on_contact(left, right);
}

fn resolve_circle_pair(left: &mut ObjectState, right: &mut ObjectState) -> bool {
    let left_body = left.body;
    let right_body = right.body;
    let left_center_x = left_body.position.x + (left_body.width * 0.5);
    let left_center_y = left_body.position.y + (left_body.height * 0.5);
    let right_center_x = right_body.position.x + (right_body.width * 0.5);
    let right_center_y = right_body.position.y + (right_body.height * 0.5);
    let delta_x = right_center_x - left_center_x;
    let delta_y = right_center_y - left_center_y;
    let distance_squared = (delta_x * delta_x) + (delta_y * delta_y);
    let left_radius = effective_radius(&left_body);
    let right_radius = effective_radius(&right_body);
    let radius_sum = left_radius + right_radius;
    if distance_squared >= radius_sum * radius_sum {
        return false;
    }

    let distance = distance_squared.max(0.0001).sqrt();
    let normal_x = if distance > 0.0001 { delta_x / distance } else { 1.0 };
    let normal_y = if distance > 0.0001 { delta_y / distance } else { 0.0 };
    let penetration = radius_sum - distance;

    let left_inverse_mass = if left_body.is_dragging { 0.0 } else { inverse_mass(&left_body) };
    let right_inverse_mass = if right_body.is_dragging { 0.0 } else { inverse_mass(&right_body) };
    let inverse_mass_sum = left_inverse_mass + right_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return false;
    }

    apply_position_correction(
        &mut left.body,
        &mut right.body,
        normal_x * penetration,
        normal_y * penetration,
        left_inverse_mass,
        right_inverse_mass,
        inverse_mass_sum,
    );

    apply_collision_impulse(left, right, normal_x, normal_y, left_inverse_mass, right_inverse_mass, inverse_mass_sum);
    true
}

fn resolve_circle_box_pair(left: &mut ObjectState, right: &mut ObjectState) -> bool {
    let left_is_circle = left.body.shape == CollisionShape::Circle;
    let circle_body = if left_is_circle { left.body } else { right.body };
    let box_body = if left_is_circle { right.body } else { left.body };

    let Some((circle_to_box_normal_x, circle_to_box_normal_y, penetration)) =
        try_get_circle_box_contact(&circle_body, &box_body)
    else {
        return false;
    };

    let normal_x = if left_is_circle {
        circle_to_box_normal_x
    } else {
        -circle_to_box_normal_x
    };
    let normal_y = if left_is_circle {
        circle_to_box_normal_y
    } else {
        -circle_to_box_normal_y
    };
    let left_inverse_mass = if left.body.is_dragging { 0.0 } else { inverse_mass(&left.body) };
    let right_inverse_mass = if right.body.is_dragging { 0.0 } else { inverse_mass(&right.body) };
    let inverse_mass_sum = left_inverse_mass + right_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return false;
    }

    apply_position_correction(
        &mut left.body,
        &mut right.body,
        normal_x * penetration,
        normal_y * penetration,
        left_inverse_mass,
        right_inverse_mass,
        inverse_mass_sum,
    );

    apply_collision_impulse(left, right, normal_x, normal_y, left_inverse_mass, right_inverse_mass, inverse_mass_sum);
    true
}

fn try_get_circle_box_contact(circle_body: &PhysicsBody, box_body: &PhysicsBody) -> Option<(f32, f32, f32)> {
    let circle_center_x = circle_body.position.x + (circle_body.width * 0.5);
    let circle_center_y = circle_body.position.y + (circle_body.height * 0.5);
    let radius = effective_radius(circle_body);
    let box_inset_x = (box_body.width * 0.5) - effective_half_width(box_body);
    let box_inset_y = (box_body.height * 0.5) - effective_half_height(box_body);
    let box_left = box_body.position.x + box_inset_x;
    let box_top = box_body.position.y + box_inset_y;
    let box_right = box_body.position.x + box_body.width - box_inset_x;
    let box_bottom = box_body.position.y + box_body.height - box_inset_y;
    let closest_x = circle_center_x.clamp(box_left, box_right);
    let closest_y = circle_center_y.clamp(box_top, box_bottom);
    let delta_x = closest_x - circle_center_x;
    let delta_y = closest_y - circle_center_y;
    let distance_squared = (delta_x * delta_x) + (delta_y * delta_y);
    if distance_squared > radius * radius {
        return None;
    }

    if distance_squared > 0.0001 {
        let distance = distance_squared.sqrt();
        return Some((delta_x / distance, delta_y / distance, radius - distance));
    }

    let to_left = circle_center_x - box_left;
    let to_right = box_right - circle_center_x;
    let to_top = circle_center_y - box_top;
    let to_bottom = box_bottom - circle_center_y;
    let min_distance = to_left.min(to_right).min(to_top.min(to_bottom));

    if min_distance == to_left {
        Some((1.0, 0.0, radius + to_left))
    } else if min_distance == to_right {
        Some((-1.0, 0.0, radius + to_right))
    } else if min_distance == to_top {
        Some((0.0, 1.0, radius + to_top))
    } else {
        Some((0.0, -1.0, radius + to_bottom))
    }
}

fn apply_collision_impulse(
    left: &mut ObjectState,
    right: &mut ObjectState,
    normal_x: f32,
    normal_y: f32,
    left_inverse_mass: f32,
    right_inverse_mass: f32,
    inverse_mass_sum: f32,
) {
    let left_body = left.body;
    let right_body = right.body;
    let relative_velocity_x = right_body.velocity.x - left_body.velocity.x;
    let relative_velocity_y = right_body.velocity.y - left_body.velocity.y;
    let velocity_along_normal = (relative_velocity_x * normal_x) + (relative_velocity_y * normal_y);
    if velocity_along_normal > 0.0 {
        return;
    }

    let restitution = left_body.restitution.min(right_body.restitution);
    let impulse_magnitude = -(1.0 + restitution) * velocity_along_normal / inverse_mass_sum;
    let impulse_x = impulse_magnitude * normal_x;
    let impulse_y = impulse_magnitude * normal_y;

    left.body.velocity.x -= impulse_x * left_inverse_mass;
    left.body.velocity.y -= impulse_y * left_inverse_mass;
    right.body.velocity.x += impulse_x * right_inverse_mass;
    right.body.velocity.y += impulse_y * right_inverse_mass;

    left.angular_velocity_z += impulse_x as f64 * 0.015;
    right.angular_velocity_z -= impulse_x as f64 * 0.015;

    if impulse_magnitude > WAKE_IMPULSE_THRESHOLD || velocity_along_normal.abs() > WAKE_VELOCITY_THRESHOLD {
        wake_body(&mut left.body);
        wake_body(&mut right.body);
    }
}

fn apply_position_correction(
    left_body: &mut PhysicsBody,
    right_body: &mut PhysicsBody,
    correction_x: f32,
    correction_y: f32,
    left_inverse_mass: f32,
    right_inverse_mass: f32,
    inverse_mass_sum: f32,
) {
    let correction_magnitude = ((correction_x * correction_x) + (correction_y * correction_y)).sqrt();
    let corrected_magnitude = (correction_magnitude - PENETRATION_SLOP).max(0.0) * POSITION_CORRECTION_PERCENT;
    if corrected_magnitude <= 0.0 || correction_magnitude <= 0.0001 {
        return;
    }

    let scale = corrected_magnitude / correction_magnitude;
    let scaled_correction_x = correction_x * scale;
    let scaled_correction_y = correction_y * scale;

    left_body.position.x -= scaled_correction_x * (left_inverse_mass / inverse_mass_sum);
    left_body.position.y -= scaled_correction_y * (left_inverse_mass / inverse_mass_sum);
    right_body.position.x += scaled_correction_x * (right_inverse_mass / inverse_mass_sum);
    right_body.position.y += scaled_correction_y * (right_inverse_mass / inverse_mass_sum);
}

fn wake_body(body: &mut PhysicsBody) {
    body.is_sleeping = false;
    body.sleep_timer_seconds = 0.0;
}

fn inverse_mass(body: &PhysicsBody) -> f32 {
    if body.is_sleeping {
        return 0.0;
    }

    if body.mass <= 0.0001 {
        1.0
    } else {
        1.0 / body.mass
    }
}

fn effective_half_width(body: &PhysicsBody) -> f32 {
    body.width * 0.5 * body.collision_scale
}

fn effective_half_height(body: &PhysicsBody) -> f32 {
    body.height * 0.5 * body.collision_scale
}

fn effective_radius(body: &PhysicsBody) -> f32 {
    body.width.min(body.height) * 0.5 * body.collision_scale
}

fn pack_cell_key(cell_x: i32, cell_y: i32) -> i64 {
    ((cell_x as i64) << 32) | ((cell_y as u32) as i64)
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn nearest_quarter_turn(angle: f64) -> f64 {
    (angle / 90.0).round() * 90.0
}

fn shortest_angle_delta(current: f64, target: f64) -> f64 {
    let mut delta = (target - current) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }

    delta
}

fn randomize_logo_colors_on_contact(left: &mut ObjectState, right: &mut ObjectState) {
    if left.visual_kind == ObjectVisualKind::DvdLogo {
        left.base_color = random_logo_color();
    }
    if right.visual_kind == ObjectVisualKind::DvdLogo {
        right.base_color = random_logo_color();
    }
}

fn random_logo_color() -> AppColor {
    let random = next_random_u32();
    let hue = (random % 360) as f32;
    color_from_hsv(hue, 0.82, 1.0)
}

fn color_from_hsv(hue: f32, saturation: f32, value: f32) -> AppColor {
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let segment = hue / 60.0;
    let x = chroma * (1.0 - ((segment % 2.0) - 1.0).abs());

    let (r1, g1, b1) = if segment < 1.0 {
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

    let m = value - chroma;
    AppColor::from_rgb(to_u8((r1 + m) * 255.0), to_u8((g1 + m) * 255.0), to_u8((b1 + m) * 255.0))
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn next_random_u32() -> u32 {
    let mut current = COLOR_RANDOMIZER_STATE.load(Ordering::Relaxed);
    loop {
        let next = current
            .wrapping_mul(COLOR_RANDOMIZER_MULTIPLIER)
            .wrapping_add(COLOR_RANDOMIZER_INCREMENT);
        match COLOR_RANDOMIZER_STATE.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (next >> 32) as u32,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{AppColor, ObjectVisualKind};

    fn make_object(id: u64, position: Vector2, shape: CollisionShape) -> ObjectState {
        let mut object = ObjectState {
            id,
            z_index: id as i32,
            base_color: AppColor::from_rgb(127, 202, 255),
            visual_kind: ObjectVisualKind::Cube,
            ..ObjectState::default()
        };
        object.body.position = position;
        object.body.width = 84.0;
        object.body.height = 84.0;
        object.body.mass = 1.0;
        object.body.restitution = 0.75;
        object.body.linear_damping = 0.992;
        object.body.shape = shape;
        object.body.collision_scale = if shape == CollisionShape::Circle { 0.82 } else { 1.0 };
        object
    }

    #[test]
    fn screen_bounds_bounce_and_floor_snap_match_reference_behavior() {
        let mut body = PhysicsBody::default();
        body.position = Vector2::new(-4.0, 210.0);
        body.velocity = Vector2::new(80.0, 12.0);
        body.width = 84.0;
        body.height = 84.0;
        body.restitution = 0.75;
        let bounds = RectF::new(0.0, 0.0, 320.0, 240.0);

        let hit_bounds = solve_screen_bounds(&mut body, bounds, 24.0, 3.0);

        assert!(hit_bounds);
        assert!(body.position.x >= 0.0);
        assert!(body.velocity.x < 0.0);
        assert_eq!(body.velocity.y, 0.0);
    }

    #[test]
    fn box_pairs_exchange_impulse_and_separate() {
        let mut objects = vec![
            make_object(1, Vector2::new(80.0, 80.0), CollisionShape::Box),
            make_object(2, Vector2::new(120.0, 80.0), CollisionShape::Box),
        ];
        objects[0].body.velocity = Vector2::new(120.0, 0.0);
        objects[1].body.velocity = Vector2::new(-10.0, 0.0);

        solve_object_collisions(&mut objects, None);

        assert!(objects[0].body.position.x < 80.0 || objects[1].body.position.x > 120.0);
        assert!(objects[0].body.velocity.x < 120.0);
        assert!(objects[1].body.velocity.x > -10.0);
    }

    #[test]
    fn world_eventually_puts_resting_object_to_sleep() {
        let mut world = PhysicsWorld::new(Vector2::new(0.0, 0.0));
        let mut object = make_object(1, Vector2::new(100.0, 156.0), CollisionShape::Box);
        object.body.velocity = Vector2::new(0.0, 2.0);
        world.add(object);
        let bounds = RectF::new(0.0, 0.0, 320.0, 240.0);

        for _ in 0..60 {
            world.step(1.0 / 60.0, bounds, 24.0, 3.0);
        }

        assert!(world.objects()[0].body.is_sleeping);
        assert_eq!(world.objects()[0].body.velocity, Vector2::ZERO);
    }
}
