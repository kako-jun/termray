use std::f64::consts::TAU;
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy)]
pub struct Vec2f {
    pub x: f64,
    pub y: f64,
}

impl Vec2f {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len == 0.0 {
            self
        } else {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        }
    }
}

impl Add for Vec2f {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vec2f {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f64> for Vec2f {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

/// Normalize angle to `0..TAU`.
pub fn normalize_angle(a: f64) -> f64 {
    let mut a = a % TAU;
    if a < 0.0 {
        a += TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn vec2f_ops() {
        let a = Vec2f::new(3.0, 4.0);
        assert!((a.length() - 5.0).abs() < 1e-9);
        let n = a.normalized();
        assert!((n.length() - 1.0).abs() < 1e-9);
        let b = Vec2f::new(1.0, 2.0);
        assert_eq!((a + b).x, 4.0);
        assert_eq!((a - b).y, 2.0);
        assert_eq!((b * 3.0).x, 3.0);
    }

    #[test]
    fn normalize_angle_wraps() {
        assert!((normalize_angle(-PI) - PI).abs() < 1e-9);
        assert!((normalize_angle(3.0 * TAU) - 0.0).abs() < 1e-9);
    }
}
