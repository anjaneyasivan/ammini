# Ammini UI Redesign Reference

Source: `/Users/anjaneya/Downloads/ammini_redesign.tsx` (React/Tailwind prototype). Goal: port this design into the `newplayer` egui app.

## 1. Overall layout

- **Window shell**: full-screen, `bg #0e0e11`, text `#f4f4f5`, `font-sans`, `select-none`, antialiased.
- **Top utility toolbar** (40px, `#16161a`, bottom border `#26262e`): macOS traffic-light dots, app title + version, local server stream badge (copyable proxy URL `127.0.0.1:54941`), quick actions (Open File, Open URL), panel toggle buttons (nav rail / chats / inspector).
- **Main workspace** (flex-1, `overflow-hidden`): 4-column horizontal layout:
  1. **Primary nav rail** (56px, `#111114`) — vertical icon buttons with hover tooltips, top (Library, Telegram, Audio, Subtitles, Recent) and bottom (Settings, user avatar). Collapsible.
  2. **Chats & files sidebar** (288px, `#121215`) — search/filter, channel list with format tags, cache progress, status footer. Collapsible.
  3. **Main media viewport** (flex-1, `#08080a`) — poster frame, play overlay, subtitle overlay, HUD toolbar (timeline scrubber + controls). Auto-hiding controls on hover.
  4. **Right inspector** (288px, `#121215`) — media spec table, Telegram source details, copy proxy link. Collapsible.

## 2. Color palette (hex)

| Token | Hex | Usage |
|---|---|---|
| window bg | `#0e0e11` | root background |
| toolbar bg | `#16161a` | top header |
| rail bg | `#111114` | nav rail |
| sidebar/inspector bg | `#121215` | chats + inspector |
| viewport bg | `#08080a` / `#050507` | video canvas |
| card/raised | `#18181d`, `#1f1f26`, `#22222b` | buttons, inputs |
| border subtle | `#22222a`, `#24242c`, `#26262e`, `#2b2b36`, `#333342` | borders |
| text primary | `#f4f4f5` | headings |
| text secondary | `#e4e4e7`, `#d4d4d8` | labels |
| text muted | `#a1a1aa`, `#8e8e99` | secondary |
| text dim | `#71717a`, `#52525b` | captions |
| accent (gold) | gold-300/400 | highlights |
| accent (lagoon) | `#3fa9ad` | Telegram accent |
| status green | emerald-500 | connected/cached |
| subtitle amber | amber-100 | subtitle text |

## 3. Typography

- **UI (sans)**: Inter / system-ui stack (`-apple-system`, `BlinkMacSystemFont`, `"Segoe UI"`, `Roboto`, `sans-serif`). Used for UI text, chat titles, nav items, buttons, labels.
- **Technical (mono)**: JetBrains Mono / Fira Code / monospace (`ui-monospace`, `SFMono-Regular`, `Menlo`, `Monaco`, `Consolas`, `"Liberation Mono"`, `monospace`). Used for stream URLs, filenames, specs (codec, bitrate, resolution), timestamps, format tags.
- Sizes: 9–11px for tags/metadata, 12–13px for body, 14px+ headings.

## 4. Key components

- **Nav rail button**: 40×40 rounded-lg, icon 16px, tooltip popover on hover (left-anchored, mono 10px), active state `bg #22222b border #333342`.
- **Chat row**: border-l-2 indicator (selected = `border #f4f4f5` + `bg #1c1c22`), channel name, unread badge, filename (mono), format tags (quality/container), size, cache progress bar (1px).
- **Filter tabs**: All/Movies/Series, pill style.
- **Media viewport**: poster image with opacity toggle (85% playing / 40% paused+brightness-75), center play button overlay (64px circle) when paused, subtitle overlay (bottom, amber-100 on `#0e0e11/90`).
- **HUD toolbar**: gradient from bottom, timeline scrubber (1px→2px on hover, played fill `#f4f4f5`, buffered `#363644`), controls: play/pause, ±10s skip, volume slider, time counter (mono), audio/subs/speed popovers, PiP, fullscreen. Auto-hide (opacity 0 → hover/focus-within).
- **Inspector**: spec table (label mono dim / value mono medium), Telegram source card, copy proxy link button.
- **Popovers**: bottom-anchored, `#16161a` bg, `#2b2b36` border, shadow-2xl, 1.5 padding, mono headers.

## 5. Data model (media source)

Fields: id, channel, filename, title, subtitleName, type (Movies/Series), quality, codec, container, size, bitrate, fps, audioCodec, resolution, unread, status (Streaming/Cached/Downloading), progress (0–100), poster URL, duration (s), currentTime (s), subtitles[], audioTracks[], caption, streamUrl.

## 6. Interactions

- Space = play/pause (ignore when input focused).
- Click timeline to seek.
- Click chat row to select media + auto-play.
- Collapsible sidebars via toggle buttons.
- Copy stream URL → "Copied Link" for 2s.
- Filter/search on filename/channel/title.

## 7. Porting notes for egui (`newplayer`)

- Use `egui::Panel` for the 4 columns (left nav, chats, right inspector) and `Panel::top` for toolbar.
- Map color hexes to `egui::Color32::from_hex` or constants.
- Fonts: egui default is proportional; register Inter as primary and a mono face (JetBrains Mono) for technical text. Use `egui::RichText::family(FontFamily::Monospace)`.
- Icons: lucide → `egui_material_icons` glyphs or async Iconify loader (already implemented) for brand icons like Telegram.
- Auto-hide HUD: track hover state; set opacity/paint accordingly. egui doesn't have CSS opacity easily; use `ui.set_enabled`/alpha via `ui.painter()` with alpha, or hide the panel.
- Timeline scrubber: custom painted rect (already have seekbar in sharkplayer; reuse).
- Popovers: `egui::Area`/`Window` or `egui::ComboBox`/manual floating frame.
- Collapsible panels: conditionally show based on bool state; animate width manually if desired.

## 8. Font install snippet (egui)

```rust
// Primary: Inter (proportional), Mono: JetBrains Mono
// Register via ctx.set_fonts(...) in App creation, after egui_material_icons::initialize.
```
