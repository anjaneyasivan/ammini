# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/anjaneyasivan/ammini/compare/v0.1.0...v0.1.1) - 2026-09-26

### Added

- media-card video bubbles and Telegram resume positions

### Fixed

- keep arrow-key seek working after the first press
- single window fullscreen — F toggles it, Esc leaves it

### Other

- add Windows support research and plan
- ignore ZCode artifacts and the eframe sandbox

## [0.1.0](https://github.com/anjaneyasivan/ammini/releases/tag/v0.1.0) - 2026-09-24

### Added

- cache-coverage seekbar shading, snappier keyframe seeks, loading spinner
- save session on quit, resume recent picks, keep display awake while playing
- enrich telegram.video_played with media metadata; drop byte counter
- track recent/audio-track seeks and identify the Sentry user
- add Sentry telemetry via OTLP events and native metrics
- rebrand to Ammini, add app icon, and package a macOS .app/.dmg
- add Recent Telegram files section to the Recent menu
- add audio stream switcher dropdown to the top bar
- prefetch upcoming blocks during Telegram video streaming
- sign out revokes the session and wipes local data
- reuse the Telegram block cache across sessions and GC it
- persist playlist, volume and per-file resume positions
- serve Telegram videos through a disk-backed block cache
- add msg_id to VideoReady message and log telegram event details
- move load more messages button to the top and add state machine tests
- bundle DejaVu Sans font to improve symbol and character fallback support
- add emoji and unicode font support via bundled NotoEmoji and system script fallbacks
- add HEVC video detection and implement on-demand range streaming for Telegram proxy
- play Telegram videos through a local axum proxy with disk cache
- implement Telegram integration module for authentication, messaging, and media handling
- initialize min-mpv desktop video player with playlist and file management support

### Fixed

- show play button for Telegram video attachments (.mkv) lacking video attributes

### Other

- skip cargo-semver-checks for the binary app
- wire up release-plz releases and a macOS DMG workflow
- set the macOS bundle floor from the build machine's OS
- bake .env credentials into the binary at compile time
- rebuild Telegram panel UI and enlarge icon glyphs
- document the macOS-style UI pass (Inter font, style.rs, bubbles)
- iMessage-style message bubbles with sender grouping
- macOS-style chat list cards and action buttons
- macOS-style global theme, Inter font, system light/dark
- rewrite README, add .env.example and CI workflow
- cargo fmt and clippy --all-targets clean
- abstract Telegram downloads behind a VideoSource trait
- agent
- stream Telegram videos directly from network, drop disk cache
- migrate application state to a state machine pattern and add URL proxying support
- add README.md with project overview, requirements, build instructions, and keyboard shortcuts
