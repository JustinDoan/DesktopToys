use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

use core_types::{AppColor, CollisionShape, ObjectState, ObjectVisualKind, PhysicsBody, RectF, Vector2};

const PENETRATION_SLOP: f32 = 0.75;
const POSITION_CORRECTION_PERCENT: f32 = 0.8;
const WAKE_IMPULSE_THRESHOLD: f32 = 75.0;
const WAKE_VELOCITY_THRESHOLD: f32 = 95.0;
#[allow(dead_code)]
const SLEEP_ANGULAR_THRESHOLD: f64 = 18.0;
const SLEEP_SETTLE_TIME_SECONDS: f32 = 0.55;
const DEFAULT_CELL_SIZE: f32 = 120.0;
const BOX3D_PIXELS_PER_METER: f32 = 100.0;
const BOX3D_SUB_STEPS: i32 = 4;
const COLOR_RANDOMIZER_MULTIPLIER: u64 = 6364136223846793005;
const COLOR_RANDOMIZER_INCREMENT: u64 = 1442695040888963407;
static COLOR_RANDOMIZER_STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);

#[repr(C)]
#[derive(Clone, Copy)]
struct SopBox3dBodyDef {
    id: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    velocity_x: f32,
    velocity_y: f32,
    mass: f32,
    restitution: f32,
    friction: f32,
    linear_damping: f32,
    gravity_scale: f32,
    motor_enabled: bool,
    motor_velocity_x: f32,
    lock_rotation: bool,
    collision_scale: f32,
    shape: i32,
    is_dragging: bool,
    z: f32,
    velocity_z: f32,
    depth_unlocked: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SopBox3dSnapshot {
    id: u64,
    x: f32,
    y: f32,
    velocity_x: f32,
    velocity_y: f32,
    rotation_x: f32,
    rotation_y: f32,
    rotation_z: f32,
    rotation_w: f32,
    is_awake: bool,
    z: f32,
    velocity_z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BodySyncState {
    width: f32,
    height: f32,
    mass: f32,
    restitution: f32,
    friction: f32,
    linear_damping: f32,
    gravity_scale: f32,
    motor_enabled: bool,
    motor_velocity_x: f32,
    lock_rotation: bool,
    collision_scale: f32,
    shape: i32,
    is_dragging: bool,
    depth_unlocked: bool,
}

impl BodySyncState {
    fn static_fields_changed(self, next: Self) -> bool {
        self.width != next.width
            || self.height != next.height
            || self.mass != next.mass
            || self.restitution != next.restitution
            || self.friction != next.friction
            || self.linear_damping != next.linear_damping
            || self.gravity_scale != next.gravity_scale
            || self.motor_enabled != next.motor_enabled
            || self.motor_velocity_x != next.motor_velocity_x
            || self.lock_rotation != next.lock_rotation
            || self.collision_scale != next.collision_scale
            || self.shape != next.shape
            || self.depth_unlocked != next.depth_unlocked
    }
}

unsafe extern "C" {
    fn sop_box3d_create(
        gravity_y: f32,
        bounds_width: f32,
        bounds_height: f32,
        pixels_per_meter: f32,
    ) -> *mut c_void;
    fn sop_box3d_destroy(world: *mut c_void);
    fn sop_box3d_reset(world: *mut c_void, gravity_y: f32, bounds_width: f32, bounds_height: f32);
    fn sop_box3d_set_gravity(world: *mut c_void, gravity_y: f32);
    fn sop_box3d_sync_body(world: *mut c_void, def: *const SopBox3dBodyDef);
    fn sop_box3d_add_velocity(world: *mut c_void, id: u64, delta_x: f32, delta_y: f32, delta_z: f32) -> bool;
    fn sop_box3d_remove_body(world: *mut c_void, id: u64);
    fn sop_box3d_add_static_box(
        world: *mut c_void,
        center_x: f32,
        center_y: f32,
        center_z: f32,
        half_width: f32,
        half_height: f32,
        half_depth: f32,
        friction: f32,
        restitution: f32,
    );
    fn sop_box3d_add_static_sphere(
        world: *mut c_void,
        center_x: f32,
        center_y: f32,
        center_z: f32,
        radius: f32,
        friction: f32,
        restitution: f32,
    );
    fn sop_box3d_step(world: *mut c_void, time_step: f32, sub_step_count: i32);
    fn sop_box3d_snapshot_count(world: *const c_void) -> i32;
    fn sop_box3d_get_snapshots(
        world: *mut c_void,
        snapshots: *mut SopBox3dSnapshot,
        capacity: i32,
    ) -> i32;
}

#[derive(Debug)]
struct Box3dBackend {
    raw: NonNull<c_void>,
    synced_bodies: HashMap<u64, BodySyncState>,
    snapshot_buffer: Vec<SopBox3dSnapshot>,
    gravity_y: f32,
}

impl Box3dBackend {
    fn new(gravity_y: f32, bounds: RectF) -> Self {
        let raw = unsafe {
            sop_box3d_create(
                gravity_y,
                bounds.width.max(1.0),
                bounds.height.max(1.0),
                BOX3D_PIXELS_PER_METER,
            )
        };
        let raw = NonNull::new(raw).expect("Box3D backend allocation failed");
        Self {
            raw,
            synced_bodies: HashMap::new(),
            snapshot_buffer: Vec::new(),
            gravity_y,
        }
    }

    fn reset(&mut self, gravity_y: f32, bounds: RectF) {
        unsafe {
            sop_box3d_reset(
                self.raw.as_ptr(),
                gravity_y,
                bounds.width.max(1.0),
                bounds.height.max(1.0),
            )
        };
        self.synced_bodies.clear();
        self.snapshot_buffer.clear();
        self.gravity_y = gravity_y;
    }

    fn set_gravity(&mut self, gravity_y: f32) {
        if self.gravity_y == gravity_y {
            return;
        }
        unsafe { sop_box3d_set_gravity(self.raw.as_ptr(), gravity_y) };
        self.gravity_y = gravity_y;
    }

    fn sync_body_if_needed(&mut self, object: &ObjectState) {
        let body = &object.body;
        if !body.collidable {
            self.synced_bodies.remove(&object.id);
            return;
        }
        let shape = match body.shape {
            CollisionShape::Box => 0,
            CollisionShape::Circle => 1,
            CollisionShape::Diamond => 2,
        };
        let is_dragging = body.is_dragging || object.is_dragging;
        let next_state = BodySyncState {
            width: body.width.max(1.0),
            height: body.height.max(1.0),
            mass: body.mass,
            restitution: body.restitution,
            friction: body.friction,
            linear_damping: body.linear_damping,
            gravity_scale: body.gravity_scale,
            motor_enabled: body.motor_enabled,
            motor_velocity_x: body.motor_velocity_x,
            lock_rotation: body.lock_rotation,
            collision_scale: body.collision_scale,
            shape,
            is_dragging,
            depth_unlocked: object.depth_unlocked,
        };
        let should_sync = match self.synced_bodies.get(&object.id) {
            None => true,
            Some(previous) => {
                is_dragging || previous.is_dragging || next_state.motor_enabled || previous.static_fields_changed(next_state)
            },
        };
        if !should_sync {
            return;
        }

        let def = SopBox3dBodyDef {
            id: object.id,
            x: body.position.x,
            y: body.position.y,
            width: next_state.width,
            height: next_state.height,
            velocity_x: body.velocity.x,
            velocity_y: body.velocity.y,
            mass: body.mass,
            restitution: body.restitution,
            friction: body.friction,
            linear_damping: body.linear_damping,
            gravity_scale: body.gravity_scale,
            motor_enabled: body.motor_enabled,
            motor_velocity_x: body.motor_velocity_x,
            lock_rotation: body.lock_rotation,
            collision_scale: body.collision_scale,
            shape,
            is_dragging,
            z: object.depth_z,
            velocity_z: object.depth_velocity,
            depth_unlocked: object.depth_unlocked,
        };
        unsafe { sop_box3d_sync_body(self.raw.as_ptr(), &def) };
        self.synced_bodies.insert(object.id, next_state);
    }

    fn add_static_box(&mut self, center: (f32, f32, f32), half_extents: (f32, f32, f32), friction: f32, restitution: f32) {
        unsafe {
            sop_box3d_add_static_box(
                self.raw.as_ptr(),
                center.0,
                center.1,
                center.2,
                half_extents.0,
                half_extents.1,
                half_extents.2,
                friction,
                restitution,
            )
        };
    }

    fn add_static_sphere(&mut self, center: (f32, f32, f32), radius: f32, friction: f32, restitution: f32) {
        unsafe {
            sop_box3d_add_static_sphere(self.raw.as_ptr(), center.0, center.1, center.2, radius, friction, restitution)
        };
    }

    fn step(&mut self, dt: f32) {
        unsafe { sop_box3d_step(self.raw.as_ptr(), dt, BOX3D_SUB_STEPS) };
    }

    fn add_velocity(&mut self, id: u64, delta: Vector2, delta_z: f32) -> bool {
        unsafe { sop_box3d_add_velocity(self.raw.as_ptr(), id, delta.x, delta.y, delta_z) }
    }

    fn remove_body(&mut self, id: u64) {
        unsafe { sop_box3d_remove_body(self.raw.as_ptr(), id) };
        self.synced_bodies.remove(&id);
    }

    fn snapshots(&mut self) -> &[SopBox3dSnapshot] {
        let count = unsafe { sop_box3d_snapshot_count(self.raw.as_ptr()) }.max(0);
        if count == 0 {
            self.snapshot_buffer.clear();
            return &self.snapshot_buffer;
        }

        let empty_snapshot = SopBox3dSnapshot {
                id: 0,
                x: 0.0,
                y: 0.0,
                velocity_x: 0.0,
                velocity_y: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                rotation_w: 1.0,
                is_awake: false,
                z: 0.0,
                velocity_z: 0.0,
        };
        self.snapshot_buffer.resize(count as usize, empty_snapshot);
        let filled =
            unsafe { sop_box3d_get_snapshots(self.raw.as_ptr(), self.snapshot_buffer.as_mut_ptr(), count) }
                .max(0) as usize;
        self.snapshot_buffer.truncate(filled);

        &self.snapshot_buffer
    }
}

impl Drop for Box3dBackend {
    fn drop(&mut self) {
        unsafe { sop_box3d_destroy(self.raw.as_ptr()) };
    }
}

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
            if !body.collidable {
                continue;
            }
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

#[derive(Clone, Copy, Debug)]
enum StaticColliderShape {
    Box { half_extents: (f32, f32, f32) },
    Sphere { radius: f32 },
}

#[derive(Clone, Copy, Debug)]
struct StaticCollider {
    center: (f32, f32, f32),
    shape: StaticColliderShape,
    friction: f32,
    restitution: f32,
}

#[derive(Debug)]
pub struct PhysicsWorld {
    objects: Vec<ObjectState>,
    gravity: Vector2,
    box3d: Option<Box3dBackend>,
    box3d_bounds: Option<(u32, u32)>,
    object_indices: HashMap<u64, usize>,
    static_colliders: Vec<StaticCollider>,
    static_colliders_synced: bool,
    pending_velocity_deltas: Vec<(u64, Vector2, f32)>,
}

impl PhysicsWorld {
    pub fn new(gravity: Vector2) -> Self {
        Self {
            objects: Vec::new(),
            gravity,
            box3d: None,
            box3d_bounds: None,
            object_indices: HashMap::new(),
            static_colliders: Vec::new(),
            static_colliders_synced: true,
            pending_velocity_deltas: Vec::new(),
        }
    }

    /// Registers a static (immovable) box collider, e.g. a backboard. It is
    /// re-created automatically whenever the physics backend resets.
    pub fn add_static_box_collider(
        &mut self,
        center: (f32, f32, f32),
        half_extents: (f32, f32, f32),
        friction: f32,
        restitution: f32,
    ) {
        self.static_colliders.push(StaticCollider {
            center,
            shape: StaticColliderShape::Box { half_extents },
            friction,
            restitution,
        });
        self.static_colliders_synced = false;
    }

    /// Registers a static sphere collider, e.g. one segment of a hoop rim.
    pub fn add_static_sphere_collider(&mut self, center: (f32, f32, f32), radius: f32, friction: f32, restitution: f32) {
        self.static_colliders.push(StaticCollider {
            center,
            shape: StaticColliderShape::Sphere { radius },
            friction,
            restitution,
        });
        self.static_colliders_synced = false;
    }

    pub fn clear_static_colliders(&mut self) {
        if self.static_colliders.is_empty() {
            return;
        }
        self.static_colliders.clear();
        // Static bodies have no handles we can remove individually, so rebuild
        // the backend world; dynamic bodies re-sync from Rust-side state.
        if let (Some(box3d), Some((width, height))) = (&mut self.box3d, self.box3d_bounds) {
            let bounds = RectF::new(0.0, 0.0, width as f32, height as f32);
            box3d.reset(self.gravity.y, bounds);
        }
        self.static_colliders_synced = true;
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

    pub fn add_velocity(&mut self, id: u64, delta: Vector2) {
        if delta.length_squared() <= f32::EPSILON {
            return;
        }
        if let Some(object) = self.objects.iter_mut().find(|object| object.id == id) {
            object.body.velocity += delta;
            object.body.is_sleeping = false;
            object.body.sleep_timer_seconds = 0.0;
        }
        self.pending_velocity_deltas.push((id, delta, 0.0));
    }

    pub fn teleport_object(&mut self, id: u64, position: Vector2, velocity: Vector2) -> bool {
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.body.position = position;
        object.body.velocity = velocity;
        object.body.is_sleeping = false;
        object.body.sleep_timer_seconds = 0.0;
        if let Some(box3d) = &mut self.box3d {
            box3d.remove_body(id);
        }
        self.pending_velocity_deltas
            .retain(|(pending_id, _, _)| *pending_id != id);
        true
    }

    pub fn remove_object(&mut self, id: u64) -> Option<ObjectState> {
        let index = self.objects.iter().position(|object| object.id == id)?;
        let removed = self.objects.swap_remove(index);
        if let Some(box3d) = &mut self.box3d {
            box3d.remove_body(id);
        }
        self.pending_velocity_deltas
            .retain(|(pending_id, _, _)| *pending_id != id);
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.pending_velocity_deltas.clear();
        if let (Some(box3d), Some((width, height))) = (&mut self.box3d, self.box3d_bounds) {
            let bounds = RectF::new(0.0, 0.0, width as f32, height as f32);
            box3d.reset(self.gravity.y, bounds);
            self.static_colliders_synced = self.static_colliders.is_empty();
        }
    }

    pub fn step(&mut self, dt: f32, bounds: RectF, sleep_threshold: f32, floor_snap_threshold: f32) {
        let _ = (sleep_threshold, floor_snap_threshold);
        self.ensure_box3d(bounds);

        let Some(box3d) = &mut self.box3d else {
            return;
        };

        box3d.set_gravity(self.gravity.y);
        if !self.static_colliders_synced {
            for collider in &self.static_colliders {
                match collider.shape {
                    StaticColliderShape::Box { half_extents } => {
                        box3d.add_static_box(collider.center, half_extents, collider.friction, collider.restitution);
                    },
                    StaticColliderShape::Sphere { radius } => {
                        box3d.add_static_sphere(collider.center, radius, collider.friction, collider.restitution);
                    },
                }
            }
            self.static_colliders_synced = true;
        }
        for object in &self.objects {
            if !object.body.collidable {
                continue;
            }
            box3d.sync_body_if_needed(object);
        }

        for (id, delta, delta_z) in self.pending_velocity_deltas.drain(..) {
            box3d.add_velocity(id, delta, delta_z);
        }

        box3d.step(dt);
        let snapshots = box3d.snapshots();
        self.object_indices.clear();
        self.object_indices
            .extend(self.objects.iter().enumerate().map(|(index, object)| (object.id, index)));
        for snapshot in snapshots {
            let Some(&object_index) = self.object_indices.get(&snapshot.id) else {
                continue;
            };
            let object = &mut self.objects[object_index];

            if object.body.is_dragging || object.is_dragging {
                object.body.is_sleeping = false;
                object.body.sleep_timer_seconds = 0.0;
                continue;
            }

            object.body.position = Vector2::new(snapshot.x, snapshot.y);
            object.body.velocity = Vector2::new(snapshot.velocity_x, snapshot.velocity_y);
            if object.depth_unlocked {
                object.depth_z = snapshot.z;
                object.depth_velocity = snapshot.velocity_z;
            }
            object.body.is_sleeping = !snapshot.is_awake;
            object.body.sleep_timer_seconds = if snapshot.is_awake {
                0.0
            } else {
                SLEEP_SETTLE_TIME_SECONDS
            };

            let (rotation_x, rotation_y, rotation_z) = quat_to_euler_degrees(
                snapshot.rotation_x as f64,
                snapshot.rotation_y as f64,
                snapshot.rotation_z as f64,
                snapshot.rotation_w as f64,
            );
            object.rotation_x = rotation_x;
            object.rotation_y = rotation_y;
            object.rotation_z = rotation_z;
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
        }

    }

    fn ensure_box3d(&mut self, bounds: RectF) {
        let key = (bounds.width.max(1.0).round() as u32, bounds.height.max(1.0).round() as u32);
        if self.box3d.is_none() {
            self.box3d = Some(Box3dBackend::new(self.gravity.y, bounds));
            self.box3d_bounds = Some(key);
            return;
        }

        if self.box3d_bounds == Some(key) {
            return;
        }

        if let Some(box3d) = &mut self.box3d {
            box3d.reset(self.gravity.y, bounds);
            self.static_colliders_synced = self.static_colliders.is_empty();
        }
        self.box3d_bounds = Some(key);
    }
}

fn quat_to_euler_degrees(x: f64, y: f64, z: f64, w: f64) -> (f64, f64, f64) {
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let roll = sinr_cosp.atan2(cosr_cosp);

    let sinp = 2.0 * (w * y - z * x);
    let pitch = if sinp.abs() >= 1.0 {
        sinp.signum() * std::f64::consts::FRAC_PI_2
    } else {
        sinp.asin()
    };

    let siny_cosp = 2.0 * (w * z + x * y);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    let yaw = siny_cosp.atan2(cosy_cosp);

    (roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees())
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

#[allow(dead_code)]
fn nearest_quarter_turn(angle: f64) -> f64 {
    (angle / 90.0).round() * 90.0
}

#[allow(dead_code)]
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
