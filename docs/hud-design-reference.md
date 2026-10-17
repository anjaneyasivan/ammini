# HUD Design Reference (from ammini_redesign.tsx)

Source: `/Users/anjaneya/Downloads/ammini_redesign.tsx` (lines 566–777).

## Container
- Positioned `absolute inset-x-0 bottom-0`, `z-20`.
- Background: `bg-gradient-to-t from-[#0e0e11] via-[#0e0e11]/80 to-transparent`, `pt-8 pb-4 px-5`.
- Visibility: `opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity duration-200` — auto-hides, shows on hover/focus.

## Timeline scrubber
- Wrapper: `mb-3 relative group/scrub cursor-pointer`.
- Rail: `w-full h-1 bg-[#262630] rounded`, `group-hover/scrub:h-2 transition-all`.
- Buffered (behind played): `absolute left-0 top-0 bottom-0 bg-[#363644]`, width = played% + 12%.
- Played fill: `absolute left-0 top-0 bottom-0 bg-[#f4f4f5]`, width = played%.
- Click to seek: `clickPos = (clientX - rect.left) / rect.width`, `setCurrentTime(clickPos * duration)`.

## Controls bar (flex, space-between)
Left group (`space-x-3`):
- Play/Pause button: `p-1.5 rounded bg-[#22222b] hover:bg-[#2c2c38] text-[#f4f4f5] border border-[#333342]`.
- Skip ±10s: `text-[#8e8e99] hover:text-[#f4f4f5] p-1`.
- Volume: mute button + range slider (`w-16 h-1 bg-[#262632] accent-[#f4f4f5]`).
- Time counter: `text-xs font-mono text-[#a1a1aa]`, played in `text-[#f4f4f5]`.

Right group (`space-x-2`):
- Audio track popover trigger: `px-2 py-1 rounded text-xs font-mono border`, active `bg-[#282832] text-[#f4f4f5] border-[#404050]`, idle `bg-[#16161a] text-[#a1a1aa] border-[#262632] hover:text-[#d4d4d8]`. Icon `Music w-3.5 h-3.5` + "Audio".
- Subtitles popover: same style, icon `Subtitles` + "Subs".
- Speed button: `px-2 py-1 rounded text-xs font-mono bg-[#16161a] text-[#a1a1aa] border-[#262632]`, shows `{speed}x`.
- PiP button: `p-1.5 text-[#8e8e99] hover:text-[#f4f4f5]`.
- Fullscreen button: `p-1.5 text-[#8e8e99] hover:text-[#f4f4f5]`.

## Popovers (audio/subs/speed)
- Container: `absolute bottom-9 right-0 w-60/w-52/w-28 bg-[#16161a] border border-[#2b2b36] rounded shadow-2xl p-1.5 z-50`.
- Header: `text-[10px] font-mono text-[#71717a] uppercase px-2 py-1 border-b border-[#22222a] mb-1`.
- Items: `w-full text-left text-xs px-2 py-1.5 rounded`, selected `bg-[#24242e] text-[#f4f4f5] font-medium`, idle `text-[#a1a1aa] hover:bg-[#1a1a20]`. Selected shows `Check` icon.
- Speed items show `{rate}` with 2 decimals.

## Colors used (HUD)
- Rail `#262630`, buffered `#363644`, played `#f4f4f5`.
- Control bg `#22222b`/`#2c2c38`, border `#333342`.
- Muted text `#8e8e99`/`#a1a1aa`, primary `#f4f4f5`.
- Popover bg `#16161a`, border `#2b2b36`, item hover `#1a1a20`, selected `#24242e`.
- Accent slider `#f4f4f5` on `#262632`.

## Porting notes
- Subtitles are rendered by mpv (no overlay needed).
- Canvas is blank (`#050507`) when no media.
- Auto-hide via hover timer (3s) or paused; hide fully (no CSS opacity in egui).
- Popovers should be `egui::Area` anchored above their trigger buttons, so the HUD stays visible while open.
