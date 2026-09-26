use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::BackendError;
use crate::backend::{self, FramebufferSize, PlayerState};
use eframe::egui::Key;
use eframe::glow::HasContext as _;
use eframe::{egui, glow};
use egui::{Color32, Label, WidgetText};
use tracing::error;

const SECOND: f64 = 1.0;
const MINUTE: f64 = SECOND * 60.0;
const HOUR: f64 = MINUTE * 60.0;

macro_rules! info_prop {
    ($self:expr, String => $prop:ident, $display:literal) => {{
        let str = $self
            .backend
            .$prop()
            .ok()
            .unwrap_or_else(|| "Unknown".into());
        info_prop!($display, str)
    }};
    ($self:expr, OptionalString => $prop:ident, $display:literal) => {{
        let str = $self
            .backend
            .$prop()
            .ok()
            .flatten()
            .unwrap_or_else(|| "Unknown".into());
        info_prop!($display, str)
    }};
    ($self:expr, Dimensions => $prop:ident, $display:literal) => {{
        let str = $self
            .backend
            .dimensions()
            .ok()
            .flatten()
            .map(|(w, h)| Rc::from(format!("{w}x{h}")))
            .unwrap_or_else(|| "Unknown".into());
        info_prop!($display, str)
    }};
    ($self:expr, Number => $prop:ident, $display:literal) => {{
        let str = Rc::from(
            $self
                .backend
                .$prop()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
        );

        info_prop!($display, str)
    }};
    ($display:expr, $str:expr) => {{ [Rc::from($display), $str] }};
}

macro_rules! info_props {
    ($self:expr; $($type:tt => $prop:ident, $display:literal);+ $(;)?) => {{
        [
            $( info_prop!($self, $type => $prop, $display) ),+
        ]
    }};
}

pub trait ControlsIconProvider {
    fn play(&self) -> WidgetText {
        "▶".into()
    }
    fn pause(&self) -> WidgetText {
        "⏸".into()
    }
    fn skip_backward(&self) -> WidgetText {
        "<<".into()
    }
    fn skip_forward(&self) -> WidgetText {
        ">>".into()
    }
    fn info(&self) -> WidgetText {
        "i".into()
    }
    fn muted_volume(&self) -> WidgetText {
        "🔇".into()
    }
    fn low_volume(&self) -> WidgetText {
        "🔈".into()
    }
    fn medium_volume(&self) -> WidgetText {
        "🔉".into()
    }
    fn high_volume(&self) -> WidgetText {
        "🔊".into()
    }
    fn fullscreen(&self) -> WidgetText {
        "⛶".into()
    }
    fn fullscreen_exit(&self) -> WidgetText {
        "🗗".into()
    }
}

/// The default [`ControlsIconProvider`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DefaultControlsIconProvider;
impl ControlsIconProvider for DefaultControlsIconProvider {}

#[derive(Debug, thiserror::Error)]
#[error("{cause}: {error}")]
pub struct Error {
    pub error: BackendError,
    pub cause: ErrorCause,
}

impl Error {
    #[inline]
    pub fn new(error: impl Into<BackendError>, cause: ErrorCause) -> Self {
        Self {
            error: error.into(),
            cause,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ErrorCause {
    #[error("Failed to get current video framebuffer")]
    GetFramebuffer,
    #[error("Failed to seek to the start of the video")]
    SeekStart,
    #[error("Failed to start playback")]
    Play,
    #[error("Failed to toggle playback status")]
    TogglePlayback,
    #[error("Failed to seek backward {0} seconds")]
    SeekBack(f64),
    #[error("Failed to seek forward {0} seconds")]
    SeekForward(f64),
    #[error("Failed to toggle mute")]
    ToggleMute,
    #[error("Failed to increase volume by {0}%")]
    IncreaseVolume(f64),
    #[error("Failed to decrease volume by {0}%")]
    DecreaseVolume(f64),
    #[error("Failed to set volume to {0}%")]
    SetVolume(f64),
    #[error("Failed to seek to {0} seconds")]
    Seek(f64),
}

/// Constrain [`SharkPlayer`] dimensions. To maintain the aspect ratio, only the
/// width or height can be specified.
///
/// See [`SharkPlayer::sizing`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sizing {
    /// Set the desired width. To maintain aspect ratio, the height is
    /// calculated automatically.
    Width(f32),
    /// Set the desired height. To maintain aspect ratio, the width is
    /// calculated automatically.
    Height(f32),
}

type Binding = &'static [egui::Key];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keybinds {
    pub toggle_pause: Binding,
    pub toggle_fullscreen: Binding,
    pub toggle_info: Binding,
    pub toggle_mute: Binding,
    pub skip_forward: Binding,
    pub skip_backward: Binding,
    pub volume_up: Binding,
    pub volume_down: Binding,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            toggle_pause: &[Key::Space, Key::K],
            toggle_info: &[Key::I],
            toggle_mute: &[Key::M],
            toggle_fullscreen: &[Key::F],
            skip_forward: &[Key::ArrowRight, Key::L],
            skip_backward: &[Key::ArrowLeft, Key::J],
            volume_up: &[Key::ArrowUp],
            volume_down: &[Key::ArrowDown],
        }
    }
}

#[derive(Clone, Default)]
struct UiState {
    hover_start: Option<(Instant, egui::Pos2)>,
    show_info: bool,
    show_volume_slider: bool,
    dragged_time: Option<f64>,
    dragged_volume: Option<f64>,
    fullscreen: bool,
    requested_initial_focus: bool,
    /// Last valid playback position; fallback while mpv reports `time-pos`
    /// unavailable (e.g. mid-seek), so the label never flashes a bogus `00:00`.
    last_time_pos: f64,
}

/// The video player widget. This is what actually get's created in the `ui`
/// function.
#[must_use]
pub struct SharkPlayer<'a, P: ControlsIconProvider = DefaultControlsIconProvider> {
    backend: &'a mut PlayerState,
    icons: P,
    sizing: Option<Sizing>,
    bar_height: f32,
    default_info_width: f32,
    skip_seconds: f64,
    volume_step: f64,
    background_color: Color32,
    controls_timeout: Duration,
    animations: bool,
    keybinds: Keybinds,
    on_error: Rc<dyn Fn(Error)>,
    // Ammini patch #3: invoked after an arrow/skip-button seek, so the app can
    // measure seek latency. `true` = forward, second arg = the widget-side
    // predicted target position after the skip.
    seek_callback: Option<Rc<dyn Fn(bool, f64)>>,
    // Ammini patch #4: cached byte-range coverage (video time in seconds) shaded
    // lighter than the rail, plus the fill color (theme-derived by the app).
    cache_spans: Vec<(f64, f64)>,
    cache_color: Color32,
}

impl<'a> SharkPlayer<'a> {
    /// The id of the focusable player surface the widget creates internally.
    ///
    /// Keyboard focus lands on this id (initial focus, click-to-focus, focus
    /// requested by fullscreen toggling), not on the response returned by
    /// `Widget::ui` — hosts that need to know whether the video surface is
    /// focused should check this id.
    #[must_use]
    pub fn player_focus_id(ui: &egui::Ui) -> egui::Id {
        ui.id().with("sharkplayer_player")
    }

    /// Create a new [`SharkPlayer`]. To use a custom icons provider, see
    /// [`SharkPlayer::new_with_icons`].
    #[inline]
    pub fn new(backend: &'a mut PlayerState) -> Self {
        SharkPlayer::new_with_icons(backend, DefaultControlsIconProvider)
    }
}

impl<'a, P: ControlsIconProvider> SharkPlayer<'a, P> {
    /// Create a new [`SharkPlayer`] using the provied [`ControlsIconProvider`].
    #[inline]
    pub fn new_with_icons(backend: &'a mut PlayerState, icons: P) -> Self {
        Self {
            backend,
            icons,
            sizing: None,
            bar_height: 40.0,
            default_info_width: 369.,
            skip_seconds: 10.0,
            volume_step: 5.0,
            background_color: Color32::from_black_alpha(169),
            controls_timeout: Duration::from_secs(3),
            animations: true,
            keybinds: Keybinds::default(),
            on_error: Rc::new(|e| {
                error!("{e}");
            }),
            seek_callback: None,
            cache_spans: Vec::new(),
            cache_color: Color32::TRANSPARENT,
        }
    }

    pub fn keybindings(mut self, keybindings: Keybinds) -> Self {
        self.keybinds = keybindings;
        self
    }

    /// Whether to use animations.
    pub fn animations(mut self, show: bool) -> Self {
        self.animations = show;
        self
    }

    /// Set how long to wait for pointer movement before hiding the controls.
    pub fn controls_timeout(mut self, timeout: Duration) -> Self {
        self.controls_timeout = timeout;
        self
    }

    /// Constrain player size.
    #[inline]
    pub fn sizing(mut self, sizing: Sizing) -> Self {
        self.sizing = Some(sizing);
        self
    }

    /// Set the control bar height.
    #[inline]
    pub fn bar_height(mut self, height: f32) -> Self {
        self.bar_height = height;
        self
    }

    /// Set how many seconds the skip buttons seek by.
    #[inline]
    pub fn skip_seconds(mut self, seconds: f64) -> Self {
        self.skip_seconds = seconds.abs();
        self
    }

    #[inline]
    pub fn background_color(mut self, color: Color32) -> Self {
        self.background_color = color;
        self
    }

    #[inline]
    pub fn default_info_width(mut self, width: f32) -> Self {
        self.default_info_width = width;
        self
    }

    #[inline]
    pub fn error_callback(mut self, f: Rc<dyn Fn(Error)>) -> Self {
        self.on_error = f;
        self
    }

    /// Install a callback fired after an arrow/skip-button seek. Receives the
    /// seek direction (`true` = forward) and the widget-side predicted target
    /// position in seconds.
    #[inline]
    pub fn seek_callback(mut self, f: Rc<dyn Fn(bool, f64)>) -> Self {
        self.seek_callback = Some(f);
        self
    }

    /// Shade the given time spans (video seconds) as cached on the seekbar, using
    /// `color` — the app derives a lighter variant of the rail background so both
    /// light and dark themes stay correct.
    #[inline]
    pub fn cache_overlay(mut self, spans: Vec<(f64, f64)>, color: Color32) -> Self {
        self.cache_spans = spans;
        self.cache_color = color;
        self
    }

    /// `MM:SS` with zero-padded whole seconds, e.g. `04:32`. The digit count is fixed
    /// (5 chars for any time under an hour), so the label layout doesn't change as the
    /// seek position advances.
    fn format_time_mm_ss(time: f64) -> String {
        let t = time.max(0.0) as u64;
        format!("{:02}:{:02}", t / 60, t % 60)
    }

    /// `HH:MM:SS` with zero-padded whole seconds, e.g. `01:04:32` (fixed 8 chars).
    fn format_time_hh_mm_ss(time: f64) -> String {
        let t = time.max(0.0) as u64;
        format!("{:02}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
    }

    fn player_size(&self, ui: &mut egui::Ui, aspect_ratio: f32) -> egui::Vec2 {
        match self.sizing {
            Some(Sizing::Width(w)) => egui::vec2(w, w / aspect_ratio),
            Some(Sizing::Height(h)) => egui::vec2(h * aspect_ratio, h),
            None => {
                let available_size = ui.available_size();
                let (max_w, max_h) = (available_size.x, available_size.y);

                let (width, height) = if max_w / aspect_ratio <= max_h {
                    (max_w, max_w / aspect_ratio)
                } else {
                    (max_h * aspect_ratio, max_h)
                };
                egui::vec2(width, height)
            }
        }
    }

    #[expect(clippy::cast_possible_truncation)]
    fn player_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let ppp = ui.ctx().pixels_per_point();
        let size = FramebufferSize {
            width: (rect.width() * ppp).floor() as i32,
            height: (rect.height() * ppp).floor() as i32,
        };

        let rendered_frame = match self.backend.render_frame(ui.ctx(), size) {
            Ok(res) => res,
            Err(e) => {
                (self.on_error)(Error::new(e, ErrorCause::GetFramebuffer));
                return;
            }
        };

        match rendered_frame {
            backend::RenderedFrame::GlFramebuffer(res) => {
                let fb = res.framebuffer();
                let fb_size = res.framebuffer_size();

                let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
                    let gl = painter.gl();
                    paint(gl, rect, fb, fb_size, &info);
                });

                ui.painter().add(egui::PaintCallback {
                    rect,
                    callback: Arc::new(cb),
                });
            }
            #[cfg(feature = "wgpu")]
            backend::RenderedFrame::EguiTexture(texture_id) => {
                // UV coordinates map the entire texture to our widget rect
                // let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0,
                // 1.0));
                let uv = egui::Rect::from_min_max(
                    egui::pos2(0.0, 1.0), // Flipped Y
                    egui::pos2(1.0, 0.0),
                );
                ui.painter()
                    .image(texture_id, rect, uv, egui::Color32::WHITE);
            }
        }
    }

    fn info_window(&self, ui: &mut egui::Ui, rect: egui::Rect, player_id: egui::Id) {
        let row_fn = |ui: &mut egui::Ui, key: &str, val: &str| {
            ui.add(Label::new(key).selectable(false));
            ui.label(val);
            ui.end_row();
        };

        egui::Window::new("Info")
            .id(player_id.with("Info"))
            .frame(egui::Frame::window(ui.style()).fill(self.background_color))
            .default_width(self.default_info_width)
            .default_pos(egui::pos2(rect.max.x - self.default_info_width, rect.min.y))
            .title_bar(false)
            .collapsible(false)
            .drag_area(egui::WindowDrag::Anywhere)
            .vscroll(true)
            .show(ui, |ui| {
                // Take up remaining space so there's no gap on the right.
                ui.set_width(ui.available_width());

                let i_love_cum = info_props!(
                    self;
                    OptionalString => media_title, "Title";
                    OptionalString => filename, "File Name";
                    Dimensions => dimensions, "Dimensions";
                    OptionalString => file_format, "Format";
                    OptionalString => video_format, "Video Format";
                    OptionalString => video_codec, "Video Codec";
                    Number => container_fps, "FPS";
                    OptionalString => colormatrix, "Color Matrix";
                    OptionalString => hwdec_current, "Hardware Decoding";
                    Number => frame_drop_count, "Frames Dropped";
                );

                let font = egui::TextStyle::Body.resolve(ui.style());
                // Measure the rendered width of the key column
                let key_col_width = i_love_cum
                    .iter()
                    .map(|[key, _]| {
                        ui.painter()
                            .layout_no_wrap(key.to_string(), font.clone(), Color32::WHITE)
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max);
                let value_wrap_width =
                    ui.available_width() - key_col_width - ui.spacing().item_spacing.x;

                egui::Grid::new(ui.id().with("info-grid"))
                    .max_col_width(value_wrap_width)
                    .show(ui, |ui| {
                        for [key, value] in i_love_cum {
                            row_fn(ui, key.as_ref(), value.as_ref());
                        }
                    });
            });
    }

    fn action_toggle_payback(&self, ui: &mut egui::Ui, current_time: f64, duration: f64) {
        /// The threshold, in seconds, by which to determine whether the video
        /// has finished playing.
        const FINISHED_THRESHOLD: f64 = 0.2;

        // restart playback upon request when completed
        if (current_time - duration).abs() < FINISHED_THRESHOLD {
            if let Err(e) = self.backend.seek_to(0.0) {
                (self.on_error)(Error::new(e, ErrorCause::SeekStart));
            }
            if let Err(e) = self.backend.play() {
                (self.on_error)(Error::new(e, ErrorCause::Play));
            }
        } else if let Err(e) = self.backend.toggle_pause() {
            (self.on_error)(Error::new(e, ErrorCause::TogglePlayback));
        }
        ui.request_repaint();
    }

    fn action_seek_back(&self, current_time: &mut f64) {
        *current_time = (*current_time - self.skip_seconds).max(0.);
        if let Err(e) = self.backend.seek_relative(-self.skip_seconds) {
            (self.on_error)(Error::new(e, ErrorCause::SeekBack(self.skip_seconds)));
        }
        if let Some(cb) = &self.seek_callback {
            cb(false, *current_time);
        }
    }

    fn action_seek_forward(&self, current_time: &mut f64) {
        *current_time = (*current_time + self.skip_seconds).max(0.);
        if let Err(e) = self.backend.seek_relative(self.skip_seconds) {
            (self.on_error)(Error::new(e, ErrorCause::SeekForward(self.skip_seconds)));
        }
        if let Some(cb) = &self.seek_callback {
            cb(true, *current_time);
        }
    }

    fn action_toggle_mute(&self, ui: &egui::Ui) {
        if let Err(e) = self.backend.toggle_mute() {
            (self.on_error)(Error::new(e, ErrorCause::ToggleMute));
        }
        ui.request_repaint();
    }

    fn playback_toggle_button(&self, ui: &mut egui::Ui, current_time: f64, duration: f64) {
        let is_paused = self.backend.paused().unwrap_or(true);

        if ui
            .button(if is_paused {
                self.icons.play()
            } else {
                self.icons.pause()
            })
            .clicked()
        {
            self.action_toggle_payback(ui, current_time, duration);
        }
    }

    fn handle_keybinds(
        &self,
        ui: &mut egui::Ui,
        player_response: &egui::Response,
        ui_state: &mut UiState,
        current_time: &mut f64,
        duration: f64,
        player_id: egui::Id,
    ) {
        if !player_response.has_focus() {
            return;
        }

        // The player surface consumes the arrow keys (seek + volume), so stop
        // egui's default focus navigation from moving focus away on the first
        // press — same pattern as egui's own Slider.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                player_response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default() // Tab / Esc keep their default focus behavior
                },
            );
        });

        // The host owns Command shortcuts (Cmd/Ctrl+←/→ = previous/next track). Ignore
        // the widget's own transport keys while Command is held so a Cmd+arrow doesn't
        // also seek the current video.
        if ui.input(|i| i.modifiers.command) {
            return;
        }

        if ui.input(|i| {
            self.keybinds
                .toggle_pause
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            self.action_toggle_payback(ui, *current_time, duration);
        }

        if ui.input(|i| {
            self.keybinds
                .skip_forward
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            self.action_seek_forward(current_time);
        }
        if ui.input(|i| {
            self.keybinds
                .skip_backward
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            self.action_seek_back(current_time);
        }
        if ui.input(|i| {
            self.keybinds
                .toggle_mute
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            self.action_toggle_mute(ui);
        }

        if ui.input(|i| {
            self.keybinds
                .volume_up
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            let vol = self.backend.volume().unwrap_or(100.0) + self.volume_step;
            if let Err(e) = self.backend.set_volume(vol) {
                (self.on_error)(Error::new(e, ErrorCause::IncreaseVolume(self.volume_step)));
            }
        }
        if ui.input(|i| {
            self.keybinds
                .volume_down
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            let vol = self.backend.volume().unwrap_or(100.0) - self.volume_step;
            if let Err(e) = self.backend.set_volume(vol) {
                (self.on_error)(Error::new(e, ErrorCause::DecreaseVolume(self.volume_step)));
            }
        }

        if ui.input(|i| {
            self.keybinds
                .toggle_info
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            ui_state.show_info ^= true;
        }

        if ui.input(|i| {
            self.keybinds
                .toggle_fullscreen
                .iter()
                .any(|&key| i.key_pressed(key))
        }) {
            Self::toggle_fullscreen(ui, ui_state, player_id);
        }
    }

    fn skip_buttons(&self, ui: &mut egui::Ui, current_time: &mut f64) {
        if ui.button(self.icons.skip_backward()).clicked() {
            self.action_seek_back(current_time);
        }
        if ui.button(self.icons.skip_forward()).clicked() {
            self.action_seek_forward(current_time);
        }
    }

    fn volume_control(&self, ui: &mut egui::Ui, ui_state: &mut UiState) {
        let is_muted = self.backend.muted().unwrap_or(false);

        let mut current_volume = ui_state
            .dragged_volume
            .unwrap_or_else(|| self.backend.volume().unwrap_or(100.0));

        let volume_icon = if is_muted {
            self.icons.muted_volume()
        } else if (0.0..=33.3).contains(&current_volume) {
            self.icons.low_volume()
        } else if (33.3..=66.6).contains(&current_volume) {
            self.icons.medium_volume()
        } else {
            self.icons.high_volume()
        };

        let volume_button = ui.button(volume_icon);
        if volume_button.clicked() {
            self.action_toggle_mute(ui);
        }
        if volume_button.hovered() || ui_state.show_volume_slider {
            ui_state.show_volume_slider = true;
            let slider = ui.add(
                egui::Slider::new(&mut current_volume, 0.0..=100.0)
                    .integer()
                    .show_value(true)
                    .trailing_fill(true),
            );

            let interaction_rect = volume_button
                .rect
                .union(slider.rect)
                .expand2(egui::vec2(0.0, 10.0));

            if !ui.rect_contains_pointer(interaction_rect) && !slider.dragged() {
                ui_state.show_volume_slider = false;
            }

            if slider.dragged() {
                ui_state.dragged_volume = Some(current_volume);
            }

            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                ui_state.dragged_volume = None;
                if let Err(e) = self.backend.set_volume(current_volume) {
                    (self.on_error)(Error::new(e, ErrorCause::SetVolume(current_volume)));
                }
            }
        }
    }

    /// Render a time string in a cell sized to the widest layout its format can
    /// produce (`00:00` / `00:00:00`). The label fonts (Inter) have proportional
    /// digits, so without this the adjacent seekbar would shift a few points every
    /// time a digit changes.
    fn fixed_time_label(ui: &mut egui::Ui, text: String, template: &str) {
        let font_id = egui::TextStyle::Body.resolve(ui.style());
        let width = ui.fonts_mut(|f| {
            template
                .chars()
                .map(|c| f.glyph_width(&font_id, c))
                .sum::<f32>()
        });
        let height = ui.text_style_height(&egui::TextStyle::Body);
        ui.add_sized(
            egui::vec2(width, height),
            egui::Label::new(text).halign(egui::Align::RIGHT),
        );
    }

    fn time_label(ui: &mut egui::Ui, current_time: f64, duration: f64) {
        // Current time label
        if duration >= HOUR {
            Self::fixed_time_label(ui, Self::format_time_hh_mm_ss(current_time), "00:00:00");
        } else {
            Self::fixed_time_label(ui, Self::format_time_mm_ss(current_time), "00:00");
        }
    }

    fn duration_label(ui: &mut egui::Ui, duration: f64) {
        if duration >= HOUR {
            Self::fixed_time_label(ui, Self::format_time_hh_mm_ss(duration), "00:00:00");
        } else {
            Self::fixed_time_label(ui, Self::format_time_mm_ss(duration), "00:00");
        }
    }

    fn info_button(&self, ui: &mut egui::Ui, ui_state: &mut UiState) {
        if ui.button(self.icons.info()).clicked() {
            ui_state.show_info ^= true;
        }
    }

    fn seekbar(
        &self,
        ui: &mut egui::Ui,
        ui_state: &mut UiState,
        current_time: &mut f64,
        duration: f64,
    ) {
        ui.spacing_mut().slider_width = ui.available_width();

        // Ammini patch #4: the seekbar is painted manually instead of via egui's
        // Slider so cached ranges can be shaded between the rail and the played
        // fill. Geometry mirrors egui's slider (rail centered in an interact-sized
        // allocation, handle radius = height/2.5, position shrunk by the handle),
        // and the drag/click seek + hover tooltip behave exactly as the slider did.
        let thickness = ui
            .text_style_height(&egui::TextStyle::Body)
            .max(ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.spacing().slider_width, thickness),
            egui::Sense::click_and_drag(),
        );

        if !ui.is_rect_visible(rect) {
            return;
        }

        let visuals = ui.style().interact(&response);
        let widget_visuals = &ui.visuals().widgets;
        let spacing = &ui.style().spacing;
        let rail_radius = (spacing.slider_rail_height / 2.0).max(0.0);
        let rail_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.center().y - rail_radius),
            egui::pos2(rect.right(), rect.center().y + rail_radius),
        );
        let corner_radius = widget_visuals.inactive.corner_radius;
        let painter = ui.painter();
        let handle_radius = rect.height() / 2.5;
        let position_range = rect.x_range().shrink(handle_radius);

        // Track background.
        painter.rect_filled(rail_rect, corner_radius, widget_visuals.inactive.bg_fill);

        if duration > 0.0 {
            let to_x = |seconds: f64| {
                position_range.min
                    + (seconds / duration).clamp(0.0, 1.0) as f32 * position_range.span()
            };

            // Ammini patch #4: cached ranges, a lighter shade than the rail, painted
            // under the played fill so the full cached extent reads even behind the
            // playhead.
            for &(start, end) in &self.cache_spans {
                let x0 = to_x(start);
                let x1 = to_x(end);
                if x1 > x0 {
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(x0, rail_rect.top()),
                            egui::pos2(x1, rail_rect.bottom()),
                        ),
                        corner_radius,
                        self.cache_color,
                    );
                }
            }

            // Played portion, clamped to the handle (mirrors the slider's
            // `trailing_fill`).
            let center = egui::pos2(to_x(*current_time), rail_rect.center().y);
            let mut trailing_rail_rect = rail_rect;
            trailing_rail_rect.max.x = center.x + f32::from(corner_radius.nw);
            painter.rect_filled(
                trailing_rail_rect,
                corner_radius,
                ui.visuals().selection.bg_fill,
            );

            // Handle.
            let radius = handle_radius + visuals.expansion;
            painter.circle_filled(center, radius, visuals.bg_fill);
            painter.circle_stroke(center, radius, visuals.fg_stroke);

            // Drag/click to seek. Mirrors the slider this replaces: while the
            // pointer is down the widget previews the target live (persisting it
            // across frames only while actually dragging), a press without movement
            // seeks immediately (click-to-seek), and a drag seeks on release. Every
            // gesture clears the preview, so the seekbar can never be left stuck on
            // a stale time — a stuck `dragged_time` would freeze the seekbar and
            // time label (arrows still seek the video, but the bar wouldn't move).
            if let Some(ppos) = response.interact_pointer_pos() {
                let t = ((ppos.x - position_range.min) / position_range.span()).clamp(0.0, 1.0);
                let target = f64::from(t) * duration;
                let changed = (*current_time - target).abs() > 1e-9;
                *current_time = target;
                if response.dragged() {
                    ui_state.dragged_time = Some(target);
                } else if changed {
                    // Press without movement: click-to-seek on press, like the slider.
                    if let Err(e) = self.backend.seek_to(target) {
                        (self.on_error)(Error::new(e, ErrorCause::Seek(target)));
                    }
                }
            }
            if response.drag_stopped() {
                if let Err(e) = self.backend.seek_to(*current_time) {
                    (self.on_error)(Error::new(e, ErrorCause::Seek(*current_time)));
                }
                ui_state.dragged_time = None;
            } else if response.clicked() {
                // Release of a plain click — the press already sought. Just make sure
                // the preview is cleared (e.g. a click that landed exactly on the
                // playhead never sought, so nothing else would clear it).
                ui_state.dragged_time = None;
            }

            // Display a tooltip informing the user where a click would seek to.
            if let Some(ppos) = ui.ctx().pointer_latest_pos()
                && (rect.contains(ppos) || response.dragged())
            {
                let prev_time =
                    f64::from(((ppos.x - rect.min.x) / rect.width()).clamp(0., 1.)) * duration;
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    ui.id(),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            if duration >= HOUR {
                                ui.label(Self::format_time_hh_mm_ss(prev_time));
                            } else {
                                ui.label(Self::format_time_mm_ss(prev_time));
                            }
                        })
                });
            }
        }
    }

    fn toggle_fullscreen(ui: &egui::Ui, ui_state: &mut UiState, player_id: egui::Id) {
        // Derive from the real window state, not a local flag: the window can also
        // change fullscreen outside this widget (host app shortcuts, the OS), and
        // `ui_state.fullscreen` is re-synced from the viewport every frame.
        let enter = !ui.ctx().input(|i| i.viewport().fullscreen).unwrap_or(false);
        ui_state.fullscreen = enter;
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(enter));
        ui.ctx().memory_mut(|mem| mem.request_focus(player_id));
    }

    fn fullscreen_button(&self, ui: &mut egui::Ui, ui_state: &mut UiState, player_id: egui::Id) {
        let icon = if ui_state.fullscreen {
            self.icons.fullscreen_exit()
        } else {
            self.icons.fullscreen()
        };
        if ui.button(icon).clicked() {
            Self::toggle_fullscreen(ui, ui_state, player_id);
        }
    }

    fn controls_ui(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        ui_state: &mut UiState,
        mut current_time: f64,
        duration: f64,
        player_id: egui::Id,
    ) {
        let controls_rect = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - self.bar_height),
            rect.max,
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls_rect), |ui| {
            egui::Frame::menu(ui.style())
                .corner_radius(0.)
                .fill(self.background_color)
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        self.playback_toggle_button(ui, current_time, duration);
                        self.skip_buttons(ui, &mut current_time);
                        self.volume_control(ui, ui_state);
                        Self::time_label(ui, current_time, duration);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.fullscreen_button(ui, ui_state, player_id);
                            self.info_button(ui, ui_state);
                            Self::duration_label(ui, duration);
                            self.seekbar(ui, ui_state, &mut current_time, duration);
                        });
                    });
                });
        });
    }

    fn show_in_rect(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        ui_state: &mut UiState,
        player_id: egui::Id,
    ) {
        let player_response = ui.interact(
            rect,
            player_id,
            egui::Sense::FOCUSABLE | egui::Sense::click(),
        );
        if player_response.clicked() {
            player_response.request_focus();
        }

        self.player_ui(ui, rect);

        let live_time = self.backend.time_pos().ok().flatten();
        let live_duration = self.backend.duration().ok().flatten();
        // A new file is loading while its duration isn't known yet (mpv exposes it
        // once the file parses); forget the previous file's position so the label
        // doesn't fall back to a stale value while the new file's time-pos is
        // unavailable.
        if live_duration.is_none() && ui_state.dragged_time.is_none() {
            ui_state.last_time_pos = 0.0;
        }
        if let Some(position) = live_time {
            ui_state.last_time_pos = position;
        }
        let mut current_time = ui_state
            .dragged_time
            .unwrap_or(live_time.unwrap_or(ui_state.last_time_pos));
        let duration = live_duration.unwrap_or(0.0);
        self.handle_keybinds(
            ui,
            &player_response,
            ui_state,
            &mut current_time,
            duration,
            player_id,
        );
        // A skip button/arrow previews the target for this frame; keep it inside the
        // media so the played fill (and the cache shade it uncovers) can't shoot past
        // the ends of the bar.
        if duration > 0.0 {
            current_time = current_time.clamp(0.0, duration);
        }
        // Determine whether to show controls
        {
            let is_hovered = ui.rect_contains_pointer(rect);
            let is_paused = self.backend.paused().unwrap_or(true);

            if is_hovered {
                if ui_state.hover_start.is_none_or(|(_, prev_pos)| {
                    ui.pointer_hover_pos()
                        .is_none_or(|current_pos| current_pos != prev_pos)
                }) {
                    ui_state.hover_start = ui.pointer_hover_pos().map(|pos| (Instant::now(), pos));
                }
            } else {
                ui_state.hover_start = None;
            }

            // Hide the controls if the pointer is stationary for a period of time.
            let show_controls = is_paused
                || ui_state
                    .hover_start
                    .is_some_and(|(start, _)| start.elapsed() < self.controls_timeout);

            // Hide the cursor when the controls are hidden for an unobtstructed view.
            if is_hovered && !show_controls {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            }

            // Animate fade-in/fade-out
            let opacity = if self.animations {
                ui.ctx()
                    .animate_bool_responsive(ui.id().with("controls_fade"), show_controls)
            } else {
                1.0
            };
            if opacity > 0.0 {
                ui.scope(|ui| {
                    ui.multiply_opacity(opacity);

                    self.controls_ui(ui, rect, ui_state, current_time, duration, player_id);
                });
            }
        }

        if ui_state.show_info {
            self.info_window(ui, rect, player_id);
        }
    }
}

impl<P: ControlsIconProvider> egui::Widget for SharkPlayer<'_, P> {
    #[expect(clippy::cast_possible_truncation)]
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let ui_state_id = ui.id().with("sharkplayer_ui_state");
        let player_id = ui.id().with("sharkplayer_player");
        let mut ui_state = ui
            .ctx()
            .data(|d| d.get_temp::<UiState>(ui_state_id).unwrap_or_default());

        if !ui_state.requested_initial_focus {
            ui.ctx().memory_mut(|mem| mem.request_focus(player_id));
            ui_state.requested_initial_focus = true;
        }

        // Follow the real window state so the fullscreen layout exits however
        // fullscreen was left (F key, Esc in the host app, native macOS chrome).
        ui_state.fullscreen = ui.input(|i| i.viewport().fullscreen).unwrap_or(false);

        let response = if ui_state.fullscreen {
            let rect = ui.ctx().content_rect();
            egui::Area::new(ui.id().with("sharkplayer_fullscreen"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.min)
                .show(ui.ctx(), |ui| {
                    ui.set_min_size(rect.size());
                    self.show_in_rect(ui, rect, &mut ui_state, player_id);
                });

            ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover())
                .1
        } else {
            // Only take up as much space the aspect ratio requires as to remove
            // letterboxing.
            let aspect_ratio = self
                .backend
                .aspect_ratio()
                .ok()
                .flatten()
                .unwrap_or(16.0 / 9.0);

            let sz = self.player_size(ui, aspect_ratio as f32);
            let (rect, response) =
                ui.allocate_exact_size(sz, egui::Sense::FOCUSABLE | egui::Sense::click());

            self.show_in_rect(ui, rect, &mut ui_state, player_id);
            response
        };

        ui.ctx()
            .data_mut(|data| data.insert_temp(ui_state_id, ui_state));
        response
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn paint(
    gl: &glow::Context,
    rect: egui::Rect,
    fb: glow::NativeFramebuffer,
    fb_size: FramebufferSize,
    info: &egui::PaintCallbackInfo,
) {
    unsafe {
        let prev_read_fb = gl
            .get_parameter_i32(eframe::glow::READ_FRAMEBUFFER_BINDING)
            .cast_unsigned();
        gl.bind_framebuffer(eframe::glow::READ_FRAMEBUFFER, Some(fb));

        let p_per_point = info.pixels_per_point;
        let screen_h = info.screen_size_px[1] as f32;

        gl.blit_framebuffer(
            0,
            0,
            fb_size.width,
            fb_size.height,
            (rect.min.x * p_per_point) as i32,
            (screen_h - rect.max.y * p_per_point) as i32,
            (rect.max.x * p_per_point) as i32,
            (screen_h - rect.min.y * p_per_point) as i32,
            eframe::glow::COLOR_BUFFER_BIT,
            eframe::glow::LINEAR,
        );

        let prev_fb = std::num::NonZeroU32::new(prev_read_fb).map(eframe::glow::NativeFramebuffer);
        gl.bind_framebuffer(eframe::glow::READ_FRAMEBUFFER, prev_fb);
    }
}

#[cfg(test)]
mod time_format_tests {
    use super::DefaultControlsIconProvider;
    use super::SharkPlayer;

    type P = SharkPlayer<'static, DefaultControlsIconProvider>;

    #[test]
    fn mm_ss_zero_pads_minutes_and_seconds() {
        assert_eq!(P::format_time_mm_ss(0.0), "00:00");
        assert_eq!(P::format_time_mm_ss(5.0), "00:05");
        assert_eq!(P::format_time_mm_ss(65.0), "01:05");
        assert_eq!(P::format_time_mm_ss(3599.0), "59:59");
    }

    #[test]
    fn mm_ss_floors_fractional_seconds() {
        // No rounding up into the next second: 59.999 shows 59, never 60.
        assert_eq!(P::format_time_mm_ss(59.999), "00:59");
        assert_eq!(P::format_time_mm_ss(90.7), "01:30");
    }

    #[test]
    fn hh_mm_ss_pads_and_floors() {
        assert_eq!(P::format_time_hh_mm_ss(3600.0), "01:00:00");
        assert_eq!(P::format_time_hh_mm_ss(3725.5), "01:02:05");
        assert_eq!(P::format_time_hh_mm_ss(3661.0), "01:01:01");
    }

    #[test]
    fn negative_times_clamp_to_zero() {
        assert_eq!(P::format_time_mm_ss(-3.0), "00:00");
        assert_eq!(P::format_time_hh_mm_ss(-1.0), "00:00:00");
    }
}
