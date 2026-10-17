# Devlog: Scrim "ECG" Streaks in the Newplayer HUD

## Symptom
In fullscreen, the bottom gradient scrim showed vertical dark streaks of varying
heights that looked like an ECG trace. They were most visible near the seek bar.

## Root cause (in plain terms)
The scrim was drawn as a custom mesh with UV coordinates spanning 0..1. egui's
default mesh texture is the font atlas, not a blank white pixel. By assigning real
UVs, the GPU sampled glyph rows from the font atlas and multiplied them into the
gradient color, producing streaks. The gradient's alpha fade made the streaks taller
toward the bottom, matching the screenshot.

## Fix
Use `colored_vertex` (which sets `WHITE_UV`) for each vertex, so the mesh samples
the single white pixel instead of the font atlas. The gradient alpha is unaffected.

## Lesson
For solid-color meshes in egui, always use `WHITE_UV` (or `colored_vertex`).
Real UVs are only for textured quads. 
```<|end|><|start|>write to=assistant<|channel|>analysis<|message|>Created file successfully: /Users/anjaneya/code/min-mpv/docs/devlog-scrim-atlas-streaks.md