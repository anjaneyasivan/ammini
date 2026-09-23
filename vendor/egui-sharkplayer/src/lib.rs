//! A hardware-accelerated video widget for `egui`, backed by [`libmpv2`].
//!
//! This crate provides an immediate-mode video player for `eframe`
//! applications.
//!
//! # Overview
//!
//! - [`PlayerState`]: Manages the `libmpv` context, media properties, and
//!   playback state. This requires access to the graphics context and must be
//!   initialized once using the [`eframe::CreationContext`].
//!
//!    A single [`PlayerState`] maps to a single video stream. Displaying
//!   multiple videos simultaneously requires independent state instances.
//!
//! - [`SharkPlayer`]: The actual UI widget. This is what you create in your
//!   `ui` method.
//!
//! The video player works best with the `glow` backend because libmpv2's render
//! API only supports OpenGL. When using the `wgpu` feature, instead of a direct
//! VRAM copy, image data has to roundtrip through main system memory. If at all
//! possible, use `glow` and disable the `wgpu` feature.
//!
//! # Features
//!
//! - Integrated contol overlays (play/pause, volume, seekbar, info overlay,
//!   etc.)
//! - Fullscreening support
//!
//! # Quick Start
//!
//! ```no_run
//! use eframe::egui;
//! use egui_sharkplayer::{PlayerState, SharkPlayer};
//!
//! struct App {
//!     player: PlayerState,
//! }
//!
//! impl App {
//!     fn new(cc: &eframe::CreationContext<'_>) -> Self {
//!         let player = PlayerState::new(cc).expect("failed to initialize player");
//!         player.load_file("video.mp4").expect("failed to load video.mp4");
//!         Self { player }
//!     }
//! }
//!
//! impl eframe::App for App {
//!     fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
//!         // Required: see "Cleaning up".
//!         self.player.destroy_gl_resources();
//!     }
//!
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         egui::CentralPanel::default().show_inside(ui, |ui| {
//!             ui.add(SharkPlayer::new(&mut self.player));
//!         });
//!     }
//! }
//!
//! fn main() {
//!     eframe::run_native(
//!         "Sharkplayer",
//!         eframe::NativeOptions::default(),
//!         Box::new(|cc| Ok(Box::new(App::new(cc)))),
//!     )
//!     .unwrap();
//! }
//! ```
//!
//! If you want to play multiple videos at the same time, you'll need to have
//! multiple [`PlayerState`]s.
//!
//! # Player Size
//!
//! By default, the widget claims all the space available in its panel. However,
//! it's possible to contrain one of the axes using [`SharkPlayer::sizing`] to
//! pin one axis instead — the other is derived from the video's aspect ratio,
//! so the player doesn't letterbox.
//!
//! ```no_run
//! # use eframe::egui;
//! # use egui_sharkplayer::{PlayerState, SharkPlayer, Sizing};
//! # fn ui(ui: &mut egui::Ui, player: &mut PlayerState) {
//! ui.add(SharkPlayer::new(player).sizing(Sizing::Width(640.0)));
//! # }
//! ```
//!
//! # Styling Controls
//!
//! For the most part, the controls use standard egui styling, but some
//! components have special styling options.
//!   
//! ## Bar Height
//! The player bar has a default height, but it can be overridden with
//! [`SharkPlayer::bar_height`].
//!
//! ```no_run
//! # use eframe::egui;
//! # use egui_sharkplayer::{PlayerState, SharkPlayer, Sizing};
//! # fn ui(ui: &mut egui::Ui, player: &mut PlayerState) {
//! ui.add(
//!     SharkPlayer::new(player)
//!         .sizing(Sizing::Height(360.0))
//!         .bar_height(32.0),
//! );
//! # }
//! ```
//!
//! ## Custom icons
//!
//! To customize which icons are used for the controls, you need a
//! [`ControlsIconProvider`].
//!
//! ```no_run
//! # use eframe::egui;
//! use egui_sharkplayer::{ControlsIconProvider, PlayerState, SharkPlayer};
//!
//! struct Icons;
//!
//! impl ControlsIconProvider for Icons {
//!     fn play(&self) -> egui::WidgetText { "Play".into() }
//!     fn pause(&self) -> egui::WidgetText { "Pause".into() }
//! }
//!
//! # fn ui(ui: &mut egui::Ui, player: &mut PlayerState) {
//! ui.add(SharkPlayer::new_with_icons(player, Icons));
//! # }
//! ```
//!
//!
//! # Cleaning Up
//!
//! To clean up the player's OpenGL resources, call
//! [`PlayerState::destroy_gl_resources`] from [`eframe::App::on_exit`].
//!
//! This isn't strictly necessary it's fine to leak VRAM, like if the process
//! stops when the UI closes.

mod backend;
mod widget;

pub use backend::{BackendError, PlayerState};
pub use widget::{ControlsIconProvider, DefaultControlsIconProvider, Keybinds, SharkPlayer, Sizing};
