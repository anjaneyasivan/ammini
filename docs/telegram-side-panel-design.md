# Telegram Side Panel — Design Reference

Source: `/Users/anjaneya/Downloads/ammini_telegram-side-panel.tsx` (chat-thread listing UI only).

Scope: chat list sidebar (header, search, filters, thread rows, footer). Download progress bars, chips under the last message, and the detailed media view are intentionally excluded.

## 1. Layout

- Sidebar width: `w-80` (320px) / `md:w-96` (384px).
- Root: `flex flex-col h-full`, bg `#16161a`, right border `#282830`, `z-20`, `flex-shrink-0`.
- Header: `p-3 bg-[#121215] border-b border-[#282830] space-y-3`.
- Threads list: `flex-1 overflow-y-auto divide-y divide-[#1a1a22]`.
- Footer: `p-2.5 bg-[#121215] border-t border-[#282830]`.

## 2. Color tokens

| Token | Hex | Usage |
|---|---|---|
| sidebar bg | `#16161a` | aside background |
| header/footer bg | `#121215` | header & footer |
| canvas | `#0e0e11` | search input bg, status badge bg |
| raised | `#18181d`, `#1c1c22`, `#212127` | hover rows, inputs, badges |
| border | `#282830`, `#30303b`, `#3b3b48` | borders |
| divider | `#1a1a22` | row dividers |
| text primary | `#f4f4f5` | titles |
| text secondary | `#e4e4e7`, `#d4d4d8` | chat names |
| text muted | `#a1a1aa`, `#8e8e99` | snippets |
| text dim | `#71717a`, `#52525b` | captions, timestamps |
| accent sky | sky-400 (`#38bdf8`) | verified, bot, unread badge bg |
| accent amber | amber-400/80 | pinned icon |
| accent emerald | emerald-400 | online dot |
| unread badge text | black on sky-500 |

## 3. Typography

- UI labels: `text-xs` (12px) semibold for chat name; `text-[11px]` (11px) for snippet.
- Metadata: `text-[10px]` / `text-[9px]` font-mono.
- Timestamps: `text-[10px] font-mono text-[#52525b]`.

## 4. Spacing & padding (Tailwind → px)

Tailwind spacing scale (1 unit = 4px). Values used:

| Tailwind | px | Where |
|---|---|---|
| `p-3` | 12 | header padding |
| `p-2.5` | 10 | footer padding |
| `p-2` | 8 | profile bar padding |
| `space-y-3` | 12 | header vertical gap |
| `space-x-3` | 12 | row horizontal gap (avatar ↔ content) |
| `space-x-2.5` | 10 | profile row gap |
| `space-x-1.5` | 6 | name/icon gap |
| `space-x-1` | 4 | badge gap |
| `mb-0.5` | 2 | name → snippet |
| `mb-1` | 4 | snippet bottom |
| `mt-1.5` | 6 | footer row top |
| `py-2` | 8 | search input vertical |
| `pl-8` | 32 | search input left (icon room) |
| `pr-12` | 48 | search input right (kbd room) |
| `px-2.5 py-1` | 10/4 | filter pill |
| `px-1.5 py-0.2` | 6/0.8 | format badge |
| `px-1.5 py-0.5` | 6/2 | TDLib badge, proxy badge |
| `p-3` (row) | 12 | chat row padding |
| `gap-1.5` | 6 | format badge gap |

## 5. Component specs

### Header
- App title row: icon box `w-6 h-6 rounded bg-[#212127] border border-[#30303b]`, title `font-semibold text-xs tracking-tight`, TDLib badge `text-[10px] font-mono bg-[#1c1c22] px-1.5 py-0.5 rounded border border-[#282830]`.
- Connection badge: `bg-[#0e0e11] border border-[#282830] px-2 py-0.5 rounded text-[10px] font-mono`, dot `w-1.5 h-1.5 rounded-full bg-emerald-400`.

### User profile bar
- Container: `bg-[#18181d] border border-[#282830] rounded p-2`.
- Avatar: `w-7 h-7 rounded-full {color} border border-[#3b3b48]`, text `text-xs font-semibold`.
- Online dot: `w-2 h-2 rounded-full bg-emerald-400 border border-[#16161a] absolute -bottom-0.5 -right-0.5`.
- Name row: `text-xs font-medium text-[#e4e4e7]` + verified `CheckCircle2 w-3 h-3 text-sky-400`.
- Username: `text-[10px] font-mono text-[#71717a]`.
- Proxy badge: `text-[10px] font-mono bg-[#212127] text-[#a1a1aa] px-1.5 py-0.5 rounded border border-[#282830]`.

### Search input
- Container `relative`.
- Icon: `Search w-3.5 h-3.5 text-[#71717a] absolute left-3 top-2.5`.
- Input: `w-full bg-[#0e0e11] text-[#e4e4e7] text-xs pl-8 pr-12 py-2 rounded border border-[#282830] focus:border-[#3b3b48] transition-colors placeholder-[#52525b]`.
- Shortcut kbd: `absolute right-2 top-2 bg-[#1c1c22] text-[#71717a] border border-[#282830] text-[9px] font-mono px-1.5 py-0.5 rounded`.

### Filter pills
- Row: `flex items-center space-x-1 overflow-x-auto no-scrollbar pt-0.5`.
- Pill: `px-2.5 py-1 rounded text-[10px] font-medium transition-all whitespace-nowrap`.
- Active: `bg-[#212127] text-[#f4f4f5] border border-[#3b3b48]`.
- Inactive: `text-[#71717a] hover:text-[#a1a1aa] hover:bg-[#18181d]`.
- Filters: `All`, `Channels`, `Groups`, `Directs`, `Unread`, `Media`.

### Chat row
- Container: `p-3 cursor-pointer transition-all relative border-l-2`.
  - Selected: `bg-[#1f1f26] border-sky-400`.
  - Hover: `bg-[#18181d]`.
  - Default: `bg-transparent border-transparent`.
- Inner: `flex items-start space-x-3`.

#### Avatar
- `w-9 h-9 rounded-full {avatarColor} border border-[#282830] flex items-center justify-center font-bold text-xs text-[#f4f4f5] shadow-inner`.
- Bot badge: `absolute -bottom-1 -right-1 bg-[#121215] text-sky-400 p-0.5 rounded-full border border-[#282830]`, icon `Bot w-2.5 h-2.5`.

#### Content
- Wrapper: `flex-1 min-w-0`.
- Name row: `flex items-center justify-between mb-0.5`.
  - Name: `text-xs font-semibold truncate`, selected `text-[#ffffff]`, else `text-[#e4e4e7]`.
  - Icons: verified `CheckCircle2 w-3 h-3 text-sky-400`; muted `VolumeX w-3 h-3 text-[#52525b]`; pinned `Pin w-3 h-3 text-amber-400/80 rotate-45`.
  - Timestamp: `text-[10px] font-mono text-[#52525b] flex-shrink-0 ml-1`.
- Snippet (active streaming): `my-1 px-1.5 py-0.5 rounded bg-emerald-950/40 border border-emerald-800/40 flex items-center space-x-1.5`, dot `w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse`, text `text-[10px] font-mono text-emerald-300 truncate`.
- Snippet (has media): `text-[11px] text-[#8e8e99] font-mono truncate mb-1 leading-snug` (filename).
- Snippet (none): `text-[11px] text-[#71717a] truncate mb-1` (description).

#### Metadata footer
- Container: `flex items-center justify-between mt-1.5`.
- Left: `flex items-center space-x-1`.
  - Quality badge: `px-1.5 py-0.2 rounded text-[9px] font-mono bg-[#121215] text-[#a1a1aa] border border-[#282830]`.
  - Container badge: `px-1.5 py-0.2 rounded text-[9px] font-mono bg-[#121215] text-[#71717a] border border-[#282830]`.
  - Subscribers: `text-[10px] font-mono text-[#52525b]`.
- Unread badge: `px-1.5 py-0.2 rounded-full text-[10px] font-mono bg-sky-500 text-black font-bold`.

### Footer
- Container: `p-2.5 bg-[#121215] border-t border-[#282830] flex items-center justify-between text-xs text-[#71717a]`.
- Left: `HardDrive w-3.5 h-3.5 text-[#52525b]` + `font-mono text-[10px]` "Local Cache: 12.4 GB".
- Reset button: `hover:text-[#f4f4f5] transition-colors p-1`, icon `RefreshCw w-3 h-3`.

## 6. Filtering logic (chat-thread listing)

- Search: matches `name`, `username`, or any `mediaFiles[].filename`.
- Filter pills: `All`, `Channels` (type=channel), `Groups` (type=group), `Directs` (type=direct), `Unread` (unreadCount>0), `Media` (mediaFiles.length>0).

## 7. Porting notes for egui

- Chat row: use `ui.allocate_exact_size(vec2(width, 64), Sense::click())` + painted text (as current).
- Filter pills: `ui.horizontal` with `Button` per filter; active fill `#212127` border `#3b3b48`.
- Search input: `TextEdit::singleline` with hint, desired width full; icon painted/labelled left.
- Badges: `Frame` with inner_margin or `RichText::background_color`.
- Unread badge: rounded pill, sky-500 bg, black bold text.
- Avatar: painted circle with initials; bot badge as small circle.
- Status dot (streaming): painted circle with pulse (static in egui; use `ctx.request_repaint` for animation if desired).
- Footer: pinned bottom panel with cache info + reset button.
- Chat types: `bot`, `direct`, `channel`, `group`, `official`.
