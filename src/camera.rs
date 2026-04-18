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
