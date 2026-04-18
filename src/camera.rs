use crate::map::TileMap;
use crate::math::Vec2f;
use crate::ray::{RayHit, cast_ray};

pub struct Camera {
    pub x: f64,
    pub y: f64,
    /// Eye height in world units (world space, same scale as
    /// [`crate::HeightMap::floor_height`] / [`crate::HeightMap::ceiling_height`]).
    ///
    /// The default value `0.5` matches the implicit assumption of the legacy
    /// [`crate::render_walls`] renderer, which always treats the camera as
    /// being centered between floor=0 and ceiling=1. Only
    /// [`crate::render_walls_with_heights`] currently consults this field;
    /// the existing `render_walls` / `render_floor_ceiling` keep their
    /// original flat-world behavior.
    pub z: f64,
    pub angle: f64,
    pub fov: f64,
}

impl Camera {
    /// Construct a camera at the given 2D pose with the default eye
    /// height (`z = 0.5`), matching the legacy flat-world assumption.
    pub fn new(x: f64, y: f64, angle: f64, fov: f64) -> Self {
        Self {
            x,
            y,
            z: 0.5,
            angle,
            fov,
        }
    }

    /// Construct a camera with an explicit eye height.
    ///
    /// Use this when pairing the camera with a non-flat [`crate::HeightMap`]
    /// and [`crate::render_walls_with_heights`].
    pub fn with_z(x: f64, y: f64, z: f64, angle: f64, fov: f64) -> Self {
        Self {
            x,
            y,
            z,
            angle,
            fov,
        }
    }

    /// Replace position and yaw in one call.
    ///
    /// Intended for physics-driven camera updates (e.g. `rapier3d`) where both
    /// pose components change every frame. `yaw` is in radians, same convention
    /// as [`Camera::angle`]. Leaves [`Camera::z`] untouched.
    pub fn set_pose(&mut self, x: f64, y: f64, yaw: f64) {
        self.x = x;
        self.y = y;
        self.angle = yaw;
    }

    /// Replace 3D position and yaw in one call.
    ///
    /// Same as [`Camera::set_pose`], but also updates [`Camera::z`]. Use this
    /// when your physics / terrain step produces an eye-height update in the
    /// same frame (e.g. stepping up a stair).
    pub fn set_pose_z(&mut self, x: f64, y: f64, z: f64, yaw: f64) {
        self.x = x;
        self.y = y;
        self.z = z;
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

    /// Replace eye height only.
    pub fn set_z(&mut self, z: f64) {
        self.z = z;
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
    fn new_uses_default_eye_height() {
        let cam = Camera::new(1.0, 2.0, 0.3, 1.0);
        approx(cam.z, 0.5);
    }

    #[test]
    fn with_z_sets_eye_height_exactly() {
        let cam = Camera::with_z(1.0, 2.0, 0.75, 0.3, 1.0);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.z, 0.75);
        approx(cam.angle, 0.3);
    }

    #[test]
    fn set_z_replaces_only_eye_height() {
        let mut cam = Camera::new(1.0, 2.0, 0.3, 1.0);
        cam.set_z(1.25);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.z, 1.25);
        approx(cam.angle, 0.3);
    }

    #[test]
    fn set_pose_leaves_eye_height_untouched() {
        let mut cam = Camera::with_z(0.0, 0.0, 0.9, 0.0, 1.0);
        cam.set_pose(1.0, 2.0, 0.5);
        approx(cam.z, 0.9);
    }

    #[test]
    fn set_pose_z_replaces_all_four_components() {
        let mut cam = Camera::new(0.0, 0.0, 0.0, 1.0);
        cam.set_pose_z(1.0, 2.0, 0.8, -0.4);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.z, 0.8);
        approx(cam.angle, -0.4);
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
