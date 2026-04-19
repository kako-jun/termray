use crate::map::TileMap;
use crate::math::Vec2f;
use crate::ray::{RayHit, cast_ray};

/// First-person camera pose plus projection parameters.
///
/// # Pitch (vertical look)
///
/// `pitch` is a **horizon offset** (Doom / Heretic style), not a full 3D
/// rotation. All termray renderers treat it by shifting the vertical center
/// of the projection:
///
/// ```text
/// focal_px = (fb_width  / 2) / tan(fov / 2)
/// center_y = fb_height / 2 + tan(pitch) * focal_px
/// ```
///
/// `center_y` replaces the naive `fb_height / 2` horizon used by walls,
/// floors, sprites, and labels, so pitch affects all of them uniformly
/// without per-row 3D math. The approximation is visually natural for
/// moderate angles (|pitch| ≲ 45°) and is well-established in retro 2.5D
/// engines.
pub struct Camera {
    pub x: f64,
    pub y: f64,
    /// Eye height in world units (world space, same scale as
    /// [`crate::CornerHeights::floor`] / [`crate::CornerHeights::ceil`]).
    ///
    /// The default value `0.5` matches the "standing between floor=0 and
    /// ceiling=1" assumption used throughout termray's examples.
    pub z: f64,
    /// Vertical look offset (radians), positive = looking up.
    ///
    /// Interpreted as a horizon shift rather than a true 3D rotation —
    /// see the type-level docs for the shift formula. Typical range is
    /// `(-FRAC_PI_2, FRAC_PI_2)`; at exactly `±FRAC_PI_2` `tan(pitch)`
    /// diverges so callers should clamp before reaching the endpoints.
    /// Defaults to `0.0` (horizontal view).
    pub pitch: f64,
    /// Yaw in radians (rotation around the vertical axis).
    pub angle: f64,
    /// Horizontal field of view in radians.
    pub fov: f64,
}

impl Camera {
    /// Construct a camera at the given 2D pose with the default eye
    /// height (`z = 0.5`) and `pitch = 0.0` (horizontal).
    pub fn new(x: f64, y: f64, angle: f64, fov: f64) -> Self {
        Self {
            x,
            y,
            z: 0.5,
            pitch: 0.0,
            angle,
            fov,
        }
    }

    /// Construct a camera with an explicit eye height, still with
    /// `pitch = 0.0`.
    pub fn with_z(x: f64, y: f64, z: f64, angle: f64, fov: f64) -> Self {
        Self {
            x,
            y,
            z,
            pitch: 0.0,
            angle,
            fov,
        }
    }

    /// Replace position and yaw in one call.
    ///
    /// Intended for physics-driven camera updates (e.g. `rapier3d`) where both
    /// pose components change every frame. `yaw` is in radians, same convention
    /// as [`Camera::angle`]. Leaves [`Camera::z`] and [`Camera::pitch`]
    /// untouched.
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

    /// Replace eye height only.
    pub fn set_z(&mut self, z: f64) {
        self.z = z;
    }

    /// Replace pitch only. `pitch` is in radians, positive = up.
    ///
    /// See the [`Camera`] type docs for how pitch is interpreted as a
    /// vertical horizon offset. Callers should keep `|pitch|` strictly less
    /// than `FRAC_PI_2` to avoid the `tan(pitch)` singularity at the poles.
    pub fn set_pitch(&mut self, pitch: f64) {
        self.pitch = pitch;
    }

    /// Unit forward vector in world space (`cos(yaw), sin(yaw)`).
    ///
    /// Useful for integrating velocity along the camera's view direction.
    /// Pitch does not affect this vector — movement in termray is still
    /// strictly 2D on the floor plane.
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

/// Compute the focal length in pixels for a framebuffer of the given width
/// at the camera's horizontal field of view.
///
/// This is the pixel-space distance from the image plane to the focal point
/// implied by the raycaster's perspective. It is used to convert vertical
/// offsets (world-space heights, `pitch`) into screen y coordinates.
///
/// Formula: `focal_px = (fb_width / 2) / tan(fov / 2)`.
#[inline]
pub(crate) fn focal_px(fb_width: usize, fov: f64) -> f64 {
    (fb_width as f64 / 2.0) / (fov / 2.0).tan()
}

/// Vertical center of the projection after applying the camera's pitch.
///
/// `pitch = 0` returns `fb_height / 2`. Positive pitch (looking up) shifts
/// the horizon downward on the screen, matching the Doom / Heretic horizon-
/// shift convention used by all termray renderers.
///
/// Formula: `center_y = fb_height / 2 + tan(pitch) * focal_px(fb_width, fov)`.
///
/// # Horizontal vs vertical FOV note
///
/// `focal_px = (fb_width / 2) / tan(fov / 2)` is derived from the **horizontal**
/// FOV, while the floor / sprite / label projections use `focal_y =
/// fb_height / 2` — i.e. they implicitly assume a **vertical** FOV of ≈ 90°.
/// The two are independent, so pitch's pixel-shift (`tan(pitch) * focal_px`)
/// scales with `fb_width`, not `fb_height`. At `fb_width / fb_height = 1`
/// and `fov = 90°` they agree exactly; for other ratios there is a mild
/// anisotropy in how pitch tilts the world relative to how height differences
/// project. This is the same pseudo-pitch approximation Doom / Heretic used;
/// v0.3 keeps it because the result looks natural for moderate angles and
/// aspect ratios, and because making it strictly isotropic would require
/// re-deriving every renderer around a single focal length.
#[inline]
pub(crate) fn projection_center_y(fb_width: usize, fb_height: usize, cam: &Camera) -> f64 {
    fb_height as f64 / 2.0 + cam.pitch.tan() * focal_px(fb_width, cam.fov)
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
    fn new_uses_default_eye_height_and_zero_pitch() {
        let cam = Camera::new(1.0, 2.0, 0.3, 1.0);
        approx(cam.z, 0.5);
        approx(cam.pitch, 0.0);
    }

    #[test]
    fn with_z_sets_eye_height_exactly() {
        let cam = Camera::with_z(1.0, 2.0, 0.75, 0.3, 1.0);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.z, 0.75);
        approx(cam.angle, 0.3);
        approx(cam.pitch, 0.0);
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
    fn set_pitch_replaces_only_pitch() {
        let mut cam = Camera::new(1.0, 2.0, 0.3, 1.0);
        cam.set_pitch(0.2);
        approx(cam.x, 1.0);
        approx(cam.y, 2.0);
        approx(cam.angle, 0.3);
        approx(cam.pitch, 0.2);
    }

    #[test]
    fn set_pose_leaves_eye_height_and_pitch_untouched() {
        let mut cam = Camera::with_z(0.0, 0.0, 0.9, 0.0, 1.0);
        cam.set_pitch(-0.25);
        cam.set_pose(1.0, 2.0, 0.5);
        approx(cam.z, 0.9);
        approx(cam.pitch, -0.25);
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

    #[test]
    fn projection_center_y_zero_pitch_is_fb_half() {
        let cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());
        approx(projection_center_y(80, 40, &cam), 20.0);
    }

    #[test]
    fn projection_center_y_positive_pitch_shifts_horizon_down() {
        let mut cam = Camera::new(0.0, 0.0, 0.0, 70f64.to_radians());
        cam.set_pitch(0.2);
        let c = projection_center_y(80, 40, &cam);
        assert!(
            c > 20.0,
            "positive pitch must shift center_y downward, got {c}"
        );
    }
}
