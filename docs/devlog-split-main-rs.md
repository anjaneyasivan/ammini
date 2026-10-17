# Devlog: Splitting `src/main.rs` into modules

## Why
`src/main.rs` had grown to ~1377 lines, mixing app state, UI rendering, track handling, and helpers. This made navigation and review difficult.

## What changed
- Moved `NewPlayerApp` and its inherent methods to `src/app.rs`.
- Moved the `App` trait implementation (`ui`, `on_exit`, `save`) to `src/ui.rs`.
- Extracted HUD rendering to `src/hud.rs` (scrim, controls, timeline).
- Extracted overlays (chats, inspector, track menus) to `src/overlays.rs`.
- Moved shared types (`MediaTrack`, `CacheCoverage`) to `src/types.rs`.
- `src/main.rs` is now a slim entry point (module declarations + `main()`).

## Result
Every source file is under 500 lines. Build and tests remain green. No behavior changes.

## Notes
- Methods are `pub` so sibling modules can call them.
- Struct fields are `pub` for cross-module access.
- `APP_KEY`/`VIDEO_EXTS` constants moved to `app.rs`. 
```<|end|><|start|>write to=assistant<|channel|>analysis<|message|>Created file successfully: docs/devlog-split-main-rs.md