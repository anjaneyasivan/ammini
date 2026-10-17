# Newplayer UI style guide (Rust + egui)

Assumption: the app is Rust with egui (eframe) on a wgpu backend, with video frames uploaded as GPU textures. If you meant a different toolkit, the rules still apply; only the code sketches change. The Rust snippets are sketches and have not been compiled. Type and method names shift between egui versions (for example `Rounding` became `CornerRadius`), so check them against the version you pin.

---

## 1. What changed in the mockup

### Video position

| | Before | After |
|---|---|---|
| Picture size | Fixed height (~68% of the pane), full width | Largest box that fits the pane at the file's real aspect ratio |
| Vertical position | Pinned to the top, black band below | Centered both ways, equal bars top and bottom |
| Aspect ratio | Whatever the container gave it | 16:9 default, switchable to 2.39:1 and 4:3, ideally read from the file |
| Leftover space | Dead black area | Blurred "ambient" glow behind the picture, or the next-episode strip |
| Anchor for overlays | The pane (controls floated mid-window) | The picture box itself |

### HUD controls

- **Anchored to the picture.** The control bar sits on the bottom edge of the video box, never in the letterbox bars and never mid-window.
- **Gradient scrim.** A dark gradient rises from the bottom edge so white icons stay readable over bright frames.
- **Auto-hide.** The bar, the top-right buttons and the cursor fade out after about 3 seconds without mouse movement while playing. Paused video keeps them visible.
- **Subtitles lift.** Subtitles sit about 24px above the bottom edge when the HUD is hidden and move up to clear the HUD when it shows, with a short ease. They never touch the seek bar.
- **Seek bar on top.** It is one full-width row above the buttons. It thickens on hover, shows a time tooltip, and shows the buffered range as a lighter segment under the played fill.
- **Volume.** A mute icon with a slider that expands on hover or focus. It no longer sits loose between the skip buttons and the time.
- **Track pickers.** Audio and subtitle chips show the icon plus the current track name ("English"). Clicking opens a small menu with a checkmark on the active entry. A speed menu was added.
- **Grouping.** Left: play/pause, back 10s, forward 10s, volume, time. Right: speed, audio, subtitles, fullscreen.
- **Top-right buttons** (info, list, layers) fade with the HUD and get tooltips.
- **Shortcuts.** Space play/pause, Left/Right seek 10s, M mute, F fullscreen.

---

## 2. General style rules

### Color tokens (dark only)

| Token | Value | Use |
|---|---|---|
| `bg` | `#0b0b0e` | Window and letterbox |
| `panel` | `#131318` | Sidebar, title bar |
| `card` | `#1a1a21` | Episode cards, menus |
| `line` | `#26262f` | 1px borders and dividers |
| `fg` | `#ececf1` | Primary text |
| `muted` | `#8d8d9a` | Secondary text, idle icons |
| `accent` | `#34c8b0` | Playing state, seek fill, focus ring |
| `ok` / `warn` | `#8bd49a` / `#e5a23c` | Ready / attention states only |

Rules: one accent, used for "now playing", progress and focus only. Neutrals lean slightly cool. No pure grey. Never use `warn` or `ok` as decoration.

### Typography

- UI text: DM Sans (or any humanist sans), 14px body, 12px secondary, 15px titles. Weights 400, 500, 700.
- Times, sizes, filenames and tags: JetBrains Mono (or any mono with tabular digits), 11 to 12px. Times must not jitter as digits change.
- Keep the scale to 11 / 12 / 14 / 15 / 18. Do not add sizes.
- Subtitles: scale with the picture height (about 4 to 5% of frame height), white, with a 2px dark shadow or outline. Never a background box.

### Spacing, shape, depth

- 4px grid: 4, 8, 12, 16, 24.
- Radii: 6px chips and menu items, 8px buttons, 10px cards and menus, 12px window.
- One border color (`line`). Shadows only on floating things (menus, the video frame).
- Icon buttons are 36px square (touch-friendly), icons 18px, one filled style across the set.

### Sidebar and episode cards

- Primary line is a clean title ("VisionQuest S01E01"), parsed from the release name. Secondary line is episode name and duration.
- Raw filename goes in one truncated mono line at the bottom of the card, full text in a tooltip. Never wrap it.
- Technical facts become chips: `1080p`, `HEVC 10bit`, `WEBRip`, `English`, `ESUB`.
- Drop the promo text (channel and bot plugs) from the display entirely.
- The playing card gets an accent border and a tinted background. A thin progress bar shows watched position, and the footer says "Resume at 17:49".
- The list is collapsible from the left rail, and the player takes the full width when it is closed.

### Motion

- 120 to 200ms ease for hover, fade and subtitle lift. Nothing bouncy.
- The only animations are HUD fade, menu open and subtitle lift. Respect a reduced-motion setting by setting durations to 0.

### Accessibility

- Every icon-only button has a tooltip and an accessible label.
- Visible 2px accent focus ring on keyboard focus.
- Text contrast of at least 4.5:1 on `bg`, `panel` and `card`.
- The HUD must never hide while a menu is open or while the pointer is over it.

---

## 3. Implementing it in egui

### Fit the picture, centered

```rust
fn fit(avail: egui::Rect, aspect: f32) -> egui::Rect {
    let (w, h) = (avail.width(), avail.height());
    let size = if w / h > aspect { egui::vec2(h * aspect, h) } else { egui::vec2(w, w / aspect) };
    egui::Rect::from_center_size(avail.center(), size)
}

egui::CentralPanel::default()
    .frame(egui::Frame::none().fill(BG))
    .show(ctx, |ui| {
        let pane = ui.available_rect_before_wrap();
        let frame = fit(pane, self.aspect);          // aspect = video_w / video_h
        // ambient glow first, drawn into `pane`, then the video on top:
        ui.painter().image(self.tex_id, frame,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE);
        self.hud(ui, frame);                          // everything is positioned from `frame`
    });
```

- Set `self.aspect` from the decoder's width and height when the stream opens. If the sample aspect ratio is not 1:1, use width x SAR / height.
- Do all HUD layout from `frame`, never from the panel rect. That is what keeps controls on the picture.
- Ambient glow: render the same texture to a small offscreen target with a blur, or just draw it stretched over `pane` at low alpha with a heavy blur shader. Skip it if it costs frame time.

### HUD visibility and fade

```rust
// state: last_move: Instant, menu_open: bool, playing: bool
let moved = ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down());
if moved { self.last_move = Instant::now(); }
let hud_on = !self.playing || self.menu_open
    || self.last_move.elapsed() < Duration::from_millis(2800)
    || ui.rect_contains_pointer(hud_rect);
let a = ctx.animate_bool_with_time(egui::Id::new("hud"), hud_on, 0.18); // 0.0..1.0
if !hud_on { ctx.set_cursor_icon(egui::CursorIcon::None); }
ctx.request_repaint_after(Duration::from_millis(250)); // so the timeout fires while idle
```

Draw the scrim and widgets with their alpha multiplied by `a` (for example `ui.set_opacity(a)` or `multiply_opacity`, depending on your egui version), and skip input handling when `a == 0.0`.

### Scrim, seek bar, subtitles

- Scrim: build a `Mesh` with a vertical gradient quad (transparent at the top, `#000000d0` at the bottom) over the bottom 25% of `frame`.
- Seek bar: allocate `Sense::click_and_drag()` for a 18px tall rect. Paint track, buffered and played with `painter.rect_filled`, growing height from 4px to 7px on hover. On hover, paint a time label above the pointer. Map x to `t = (x - rect.left()) / rect.width() * duration`.
- Subtitles: compute a lift target (`hud_height + 16.0` when the HUD is on, otherwise `24.0`) and ease toward it with `ctx.animate_value_with_time`. Draw the text twice, first offset by (0, 2) in a translucent black, then in white at `frame.center_bottom() - vec2(0.0, lift)`. Wrap at 90% of frame width.
- Controls: use `ui.allocate_ui_at_rect(hud_rect, |ui| ui.horizontal(...))` with left and right groups (`ui.with_layout(Layout::right_to_left(Align::Center), ...)`).
- Fullscreen: `ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true))`.
- Keyboard: read `ctx.input(|i| i.key_pressed(egui::Key::Space))` and so on. Skip shortcuts while a text field has focus.

### Theme and fonts

```rust
let mut v = egui::Visuals::dark();
v.panel_fill = PANEL;
v.window_fill = CARD;
v.override_text_color = Some(FG);
v.selection.bg_fill = ACCENT.linear_multiply(0.35);
v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
ctx.set_visuals(v);

let mut style = (*ctx.style()).clone();
style.spacing.item_spacing = egui::vec2(8.0, 8.0);
style.spacing.button_padding = egui::vec2(10.0, 6.0);
ctx.set_style(style);
```

Load DM Sans as the proportional family and JetBrains Mono as the monospace family through `FontDefinitions`, and use the monospace family for times, sizes and tags.

### Video path

- Decode on a worker thread. Upload a frame to the GPU texture only when a new frame is ready, and call `request_repaint` from that thread. Do not repaint continuously when paused.
- Keep the displayed frame as a texture registered once, and update its contents each frame instead of recreating it.
- Fullscreen and resize should only change `frame` rect math, not re-create the texture.

---

## 4. Checklist before shipping a screen

1. Is the picture centered, at the file's real aspect ratio, with equal bars?
2. Are all HUD elements positioned from the picture rect?
3. Do controls, cursor and top buttons hide after idle while playing, and stay while paused or in a menu?
4. Do subtitles clear the HUD in both states?
5. Does every icon button have a tooltip and a focus ring?
6. Do episode cards show a clean title, chips, progress and a truncated raw filename?
7. Are only the defined tokens, sizes and radii used?
