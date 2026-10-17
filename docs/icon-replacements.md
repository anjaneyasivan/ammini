# Icon replacements: lucide → `material-symbols-light`

Discovered via the Iconify API:
- Collections: `https://api.iconify.design/collection?prefix=material-symbols-light`
- Search: `https://api.iconify.design/search?query=<name>&prefixes=material-symbols-light`

Render any icon with: `https://api.iconify.design/material-symbols-light/<name>.svg?color=%23ffffff&height=24`

## Mapping (lucide → material-symbols-light)

| lucide (redesign) | material-symbols-light | Notes |
|---|---|---|
| Play | `play-arrow` | use `play-arrow` |
| Pause | `pause` | |
| Volume2 | `volume-up` | |
| VolumeX | `volume-off` | |
| Maximize2 | `fullscreen` | |
| PictureInPicture2 | `picture-in-picture` (search: `picture in picture`) | verify exact name |
| Settings | `settings` | |
| Folder | `folder` | |
| Globe | `language` or `public` | search `globe` |
| Subtitles | `subtitles` | |
| Music | `music-note` | |
| Copy | `content-copy` | search `copy` |
| Check | `check` | |
| Search | `search` | |
| ChevronRight | `chevron-right` | |
| ChevronLeft | `chevron-left` | |
| Info | `info` | |
| SkipBack | `skip-previous` | |
| SkipForward | `skip-next` | |
| Server | `dns` or `storage` | `storage` for hard drive |
| HardDrive | `storage` | |
| X | `close` | |
| MessageSquare | `chat` | |
| Tv | `tv` | search `tv` |
| Clock | `schedule` | |
| LogOut | `logout` | search `logout` |
| Film | `movie` | |
| List | `list` | |
| Layers | `layers` | |
| Sliders | `tune` | |
| Shield | `shield` | |

## Usage in Rust (newplayer)

Use the existing `iconify_icon` helper:

```rust
iconify_icon(ui, &self.icon_cache, &ctx, "material-symbols-light", "play-arrow", "#ffffff", 24);
```

For brand icons (Telegram) keep `arcticons:telegram` via the same helper.
