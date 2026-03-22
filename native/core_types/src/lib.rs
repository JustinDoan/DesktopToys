use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub, SubAssign};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AppColor {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl AppColor {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { a: 255, r, g, b }
    }

    pub const fn from_argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self { a, r, g, b }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length_squared(self) -> f32 {
        (self.x * self.x) + (self.y * self.y)
    }
}

impl Add for Vector2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vector2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vector2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vector2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f32> for Vector2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl MulAssign<f32> for Vector2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<f32> for Vector2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn left(self) -> f32 {
        self.x
    }

    pub fn top(self) -> f32 {
        self.y
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn contains(self, point: Vector2) -> bool {
        point.x >= self.left()
            && point.x <= self.right()
            && point.y >= self.top()
            && point.y <= self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectVisualKind {
    Cube,
    Dice,
    Crystal,
    Satellite,
    DvdLogo,
    ImportedModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionShape {
    Box,
    Circle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsBody {
    pub position: Vector2,
    pub velocity: Vector2,
    pub acceleration: Vector2,
    pub width: f32,
    pub height: f32,
    pub mass: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub gravity_scale: f32,
    pub shape: CollisionShape,
    pub collision_scale: f32,
    pub is_dragging: bool,
    pub is_sleeping: bool,
    pub sleep_timer_seconds: f32,
}

impl Default for PhysicsBody {
    fn default() -> Self {
        Self {
            position: Vector2::ZERO,
            velocity: Vector2::ZERO,
            acceleration: Vector2::ZERO,
            width: 0.0,
            height: 0.0,
            mass: 1.0,
            restitution: 0.75,
            linear_damping: 0.992,
            gravity_scale: 1.0,
            shape: CollisionShape::Box,
            collision_scale: 1.0,
            is_dragging: false,
            is_sleeping: false,
            sleep_timer_seconds: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectState {
    pub id: u64,
    pub body: PhysicsBody,
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub rotation_z: f64,
    pub angular_velocity_x: f64,
    pub angular_velocity_y: f64,
    pub angular_velocity_z: f64,
    pub is_hovered: bool,
    pub is_dragging: bool,
    pub z_index: i32,
    pub base_color: AppColor,
    pub visual_kind: ObjectVisualKind,
    pub model_source_path: Option<String>,
    pub model_scale_multiplier: f32,
}

impl Default for ObjectState {
    fn default() -> Self {
        Self {
            id: 0,
            body: PhysicsBody::default(),
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            angular_velocity_x: 0.0,
            angular_velocity_y: 0.0,
            angular_velocity_z: 0.0,
            is_hovered: false,
            is_dragging: false,
            z_index: 0,
            base_color: AppColor::from_rgb(127, 202, 255),
            visual_kind: ObjectVisualKind::Cube,
            model_source_path: None,
            model_scale_multiplier: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppConfig {
    pub gravity_y: f32,
    pub throw_sensitivity: f32,
    pub max_throw_speed: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub sleep_threshold: f32,
    pub floor_snap_threshold: f32,
    pub interaction_debounce_ms: u32,
    pub start_in_pass_through: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gravity_y: 1800.0,
            throw_sensitivity: 1.1,
            max_throw_speed: 2600.0,
            restitution: 0.75,
            linear_damping: 0.992,
            sleep_threshold: 24.0,
            floor_snap_threshold: 3.0,
            interaction_debounce_ms: 80,
            start_in_pass_through: true,
        }
    }
}
