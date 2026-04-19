# Changelog

All notable changes to termray are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-04-19

Breaking release: the height / pitch / slope pipeline was unified behind
`CornerHeights` + `HitFace` + a single `render_walls` / `render_floor_ceiling`
pair. Downstream callers must migrate their `HeightMap` implementations and
their wall / floor / sprite / label call sites.

### Added
- `CornerHeights` struct (floor + ceiling per cell, `[NW, NE, SW, SE]`
  order) with bilinear `sample_floor` / `sample_ceil` helpers, plus
  `CORNER_NW` / `CORNER_NE` / `CORNER_SW` / `CORNER_SE` index constants (#8).
- `HitFace` enum (`West`, `East`, `North`, `South`) and `RayHit::face`
  field — exposes which of the cell's four faces a ray crossed so the
  wall renderer can pick the correct pair of corner heights for
  `wall_x` interpolation (#8).
- `Camera::pitch` and `Camera::set_pitch` — vertical horizon shift
  (Doom / Heretic style) of `tan(pitch) * focal_px` applied uniformly
  across walls, floors, sprites, and labels (#8).
- `SpriteRenderResult::screen_y_feet` — pre-projected ground-contact row
  computed by `project_sprites` from the bilinear-sampled floor under
  the sprite and the camera pitch. Makes sprites stand on sloped floors
  instead of always anchoring to `fb_h / 2`. Stored as `f64` and
  quantized to `i32` at the `render_sprites` pixel-write boundary so
  sub-pixel pitch / slope motion doesn't alias into row-granularity
  jitter (#8).
- `SpriteRenderResult::screen_height` is now `f64` (was `i32` in earlier
  #8 rounds). `project_sprites` emits `fb_height / distance` without
  early quantization; `render_sprites` multiplies by the per-type
  `height_scale` and casts to `i32` at the pixel-write boundary. Matches
  the `screen_y_feet` / `screen_y_baseline` late-quantization pipeline
  so distance changes of under one pixel don't step the pattern row
  count (#8).
- `examples/slope` — continuous-slope demo with a smooth hill and a
  bowl valley, plus `r` / `f` pitch controls (#8).
- `tests/invariants.rs` — integration tests covering the five v0.3.0
  behavioral contracts: tile-flat == `FlatHeightMap`, pitch horizon
  shift, bilinear interpolation, adjacent-cell continuity, sprite
  slope-anchoring (#8).
- `Label`, `ProjectedLabel`, `GlyphRenderer` trait, `Font8x8` default impl,
  `project_labels`, `render_labels` — world-anchored text labels with
  word-wrap and two-tier occlusion: glyph-level for text (skipped wholesale
  if any of its columns is behind a wall) and per-column for the optional
  background rectangle (#5, carried over from the 0.2.x cycle).
- `examples/labeled_sprites` — friendly-filer-style demo of sprites with
  file-name labels that are occluded by walls (#5).

### Changed (breaking)
- `HeightMap` — the `floor_height` / `ceiling_height` methods are gone.
  `cell_heights(x, y) -> CornerHeights` is now the single entry point and
  the default implementation returns `CornerHeights::flat(0.0, 1.0)` so a
  bare `impl HeightMap for MyType {}` still gives the pre-v0.3 flat world.
  Applications that return distinct corners per neighbour must uphold the
  continuity contract (`here.NE == east.NW`, etc.) to avoid seams.
- `render_walls` now takes `heights: &dyn HeightMap` and `camera: &Camera`
  and interpolates the wall's top / bottom between the two cell corners
  bounding the hit face. The separate `render_walls_with_heights` function
  from 0.2.x is removed; call `render_walls` with `FlatHeightMap` to get
  the old tile-flat behavior.
- `render_floor_ceiling` now takes `heights: &dyn HeightMap`, `camera:
  &Camera`, and `max_depth`. The renderer walks the grid per-column with
  DDA, linearizes the bilinear floor along each cell segment, and inverts
  `y(d)` per pixel to recover world `(wx, wy)` — so slope features
  (hills, ramps, bowls) actually show up on the ground plane.
- `project_sprites` and `project_labels` now take `camera: &Camera` and
  `heights: &dyn HeightMap` plus the framebuffer's `screen_width` and
  `screen_height`. Both bilinear-sample the floor under each anchor, and
  both participate in the pitch horizon shift via the shared
  `projection_center_y`. `ProjectedLabel::world_height` is replaced by
  `ProjectedLabel::screen_y_baseline` (pre-projected, pitch-aware,
  stored as `f64` and quantized to `i32` inside `render_labels`).
- `Camera::set_pose_z` is removed. Use `set_pose` followed by `set_z`.

### Removed
- `tests/back_compat.rs` and the `render_walls_with_heights` function it
  guarded. The equivalent guarantee (`FlatHeightMap` reproduces legacy
  output) is enforced by `tests/invariants.rs::tile_flat_reproduces_flat_heightmap`.

### Notes
- Glyphs render at the font's native pixel size (no distance scaling) to
  preserve readability. Applications wanting distance-dependent sizing can
  wrap their own `GlyphRenderer`.
- `Font8x8` covers `basic_latin` (0x20..=0x7E). CJK / non-Latin support is
  expected to come from application-provided `GlyphRenderer` implementations.
- MSRV raised to Rust 1.85.0 (edition 2024).

## [0.2.0] - 2026-04-18

### Added
- `Camera::set_pose`, `Camera::set_position`, `Camera::set_yaw` — explicit
  pose setters for physics-driven updates (#4).
- `Camera::forward`, `Camera::right` — unit direction vectors for strafe /
  velocity math (#4).
- `examples/free_camera` — physics-style demo with Euler integration,
  friction, and strafe controls (#4).
- `HeightMap` trait and `FlatHeightMap` zero-sized default — per-tile
  floor / ceiling heights (#3 Phase 1).
- `Camera::z`, `Camera::with_z`, `Camera::set_z`, `Camera::set_pose_z` —
  eye-height state for heightmap-aware rendering (#3 Phase 1). Existing
  `Camera::new` keeps its signature and initializes `z = 0.5`.
- `render_walls_with_heights` — new wall renderer that consults a
  `HeightMap` and the camera's `z`. The original `render_walls` is
  unchanged and equivalent to calling the new renderer with
  `FlatHeightMap` + `Camera::z == 0.5` (covered by `tests/back_compat.rs`).
- `examples/terrain` — stepped-height demo where the camera rises and
  falls with the tile it stands on (#3 Phase 1).

### Notes
- Phase 1 handles stepped heights only. `render_floor_ceiling` still paints
  a flat horizontal plane; true corner-interpolated slopes and
  `Camera::pitch` are tracked in #8 for `v0.3.0`.

## [0.1.0] - 2026-04-18

- Initial port from `nobiscuit-engine`: wall DDA, perspective floor and
  ceiling, sprites with depth testing, trait-based wall / floor / sprite
  texturing.
