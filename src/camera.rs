use crate::map::TileMap;
use crate::math::Vec2f;
use crate::ray::{cast_ray, RayHit};

pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub fov: f64,
}

impl Camera {
    pub fn new(x: f64, y: f64, angle: f64, fov: f64) -> Self {
        Self { x, y, angle, fov }
    }

    /// Replace position and yaw in one call.
    ///
    /// Intended for physics-driven camera updates (e.g. `rapier3d`) where both
    /// pose components change every frame. `yaw` is in radians, same convention
    /// as [`Camera::angle`].
    pub fn set_pose(&mut self, x: f64, y: f64, yaw: f64) {
        self.x = x;
        self.y = y;
        self.angle = yaw;
    }

    /// Replace position only, leaving yaw untouched.
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    /// Replace yaw only. `yaw` is in radians.
    pub fn set_yaw(&mut self, yaw: f64) {
        self.angle = yaw;
    }

    /// Unit forward vector in world space (`cos(yaw), sin(yaw)`).
    ///
    /// Useful for integrating velocity along the camera's view direction.
    pub fn forward(&self) -> Vec2f {
        Vec2f::new(self.angle.cos(), self.angle.sin())
    }

    /// Unit right-hand strafe vector — forward rotated by +90°.
    ///
    /// Useful for side-step / strafe controls in physics-style demos.
    pub fn right(&self) -> Vec2f {
        Vec2f::new(-self.angle.sin(), self.angle.cos())
    }

    /// Cast one ray per screen column.
    ///
    /// `ray::cast_ray` already returns perpendicular distance (fisheye-free via DDA),
    /// so no additional correction is applied here.
    pub fn cast_all_rays(
        &self,
        map: &dyn TileMap,
        num_rays: usize,
        max_depth: f64,
    ) -> Vec<Option<RayHit>> {
        let half_fov = self.fov / 2.0;
        let origin = Vec2f::new(self.x, self.y);

        (0..num_rays)
            .map(|i| {
                let ray_angle = self.angle - half_fov + self.fov * (i as f64) / (num_rays as f64);
                cast_ray(map, origin, ray_angle, max_depth)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {} ≈ {}", a, b);
    }

    #[test]
    fn set_pose_replaces_all_three_components() {
        let mut cam = Camera::new(0.0, 0.0, 0.0, 1.0);
        cam.set_pose(3.5, -2.25, 1.25);
        approx(cam.x, 3.5);
        approx(cam.y, -2.25);
        approx(cam.angle, 1.25);
    }

    #[test]
    fn set_position_leaves_yaw_untouched() {
        let mut cam = Camera::new(0.0, 0.0, 0.7, 1.0);
        cam.set_position(1.0, 2.0);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.angle, 0.7);
    }

    #[test]
    fn set_yaw_leaves_position_untouched() {
        let mut cam = Camera::new(4.0, 5.0, 0.0, 1.0);
        cam.set_yaw(-0.3);
        approx(cam.x, 4.0);
        approx(cam.y, 5.0);
        approx(cam.angle, -0.3);
    }

    #[test]
    fn forward_matches_unit_circle() {
        let cam0 = Camera::new(0.0, 0.0, 0.0, 1.0);
        let f0 = cam0.forward();
        approx(f0.x, 1.0);
        approx(f0.y, 0.0);

        let cam90 = Camera::new(0.0, 0.0, FRAC_PI_2, 1.0);
        let f90 = cam90.forward();
        approx(f90.x, 0.0);
        approx(f90.y, 1.0);
    }

    #[test]
    fn right_is_forward_rotated_ninety_degrees() {
        // Sample a handful of angles; right = forward rotated +90°, so
        // dot(forward, right) == 0 and the 2D cross product == +1.
        for &a in &[0.0_f64, 0.3, 1.1, -0.7, 2.8] {
            let cam = Camera::new(0.0, 0.0, a, 1.0);
            let f = cam.forward();
            let r = cam.right();
            approx(f.x * r.x + f.y * r.y, 0.0);
            approx(f.x * r.y - f.y * r.x, 1.0);
        }
    }
}
