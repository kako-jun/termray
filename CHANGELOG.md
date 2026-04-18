# Changelog

All notable changes to termray are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-18

### Added
- `Camera::set_pose`, `Camera::set_position`, `Camera::set_yaw` — explicit
  pose setters for physics-driven updates (#4).
- `Camera::forward`, `Camera::right` — unit direction vectors for strafe /
  velocity math (#4).
- `examples/free_camera` — physics-style demo with Euler integration,
  friction, and strafe controls (#4).

## [0.1.0] - 2026-04-18

- Initial port from `nobiscuit-engine`: wall DDA, perspective floor and
  ceiling, sprites with depth testing, trait-based wall / floor / sprite
  texturing.
