//! Minimal 3D math for the case stage.
//!
//! The case sequence runs in its own right-handed space (X right, Y up,
//! Z toward the viewer) and is projected to screen pixels on the CPU, so it
//! needs a vector type independent of the overlay's screen-space conventions.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const fn splat(value: f32) -> Self {
        Self::new(value, value, value)
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length <= 1e-6 {
            Self::new(0.0, 0.0, 1.0)
        } else {
            self * (1.0 / length)
        }
    }

    pub fn lerp(self, other: Self, amount: f32) -> Self {
        self + (other - self) * amount
    }

    pub fn rotate_x(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(
            self.x,
            self.y * cos - self.z * sin,
            self.y * sin + self.z * cos,
        )
    }

    pub fn rotate_y(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(
            self.x * cos + self.z * sin,
            self.y,
            -self.x * sin + self.z * cos,
        )
    }

    pub fn rotate_z(self, radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(
            self.x * cos - self.y * sin,
            self.x * sin + self.y * cos,
            self.z,
        )
    }
}

impl Add for V3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for V3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for V3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for V3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Neg for V3 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::V3;

    #[test]
    fn cross_follows_the_right_hand_rule() {
        let result = V3::new(1.0, 0.0, 0.0).cross(V3::new(0.0, 1.0, 0.0));
        assert_eq!(result, V3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn normalizing_zero_gives_a_usable_axis() {
        assert_eq!(V3::ZERO.normalized(), V3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn quarter_turns_land_on_axes() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let rotated = V3::new(1.0, 0.0, 0.0).rotate_z(quarter);
        assert!((rotated.x).abs() < 1e-6);
        assert!((rotated.y - 1.0).abs() < 1e-6);
    }
}
