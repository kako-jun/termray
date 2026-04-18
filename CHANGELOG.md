# Changelog

All notable changes to termray are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Label`, `ProjectedLabel`, `GlyphRenderer` trait, `Font8x8` default impl,
  `project_labels`, `render_labels` — world-anchored text labels with
  word-wrap and two-tier occlusion: glyph-level for text (skipped wholesale
  if any of its columns is behind a wall) and per-column for the optional
  background rectangle (#5).
- `examples/labeled_sprites` — friendly-filer-style demo of sprites with
  file-name labels that are occluded by walls.

### Notes
- Glyphs render at the font's native pixel size (no distance scaling) to
  preserve readability. Applications wanting distance-dependent sizing can
  wrap their own `GlyphRenderer`.
- `Font8x8` covers `basic_latin` (0x20..=0x7E). CJK / non-Latin support is
  expected to come from application-provided `GlyphRenderer` implementations.

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
