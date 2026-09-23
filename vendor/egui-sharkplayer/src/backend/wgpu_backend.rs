use eframe::glow::{self, HasContext as _};
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};
use tracing::debug;

use super::{BackendError, FramebufferSize, GpaFn, RenderedFrame};

pub(super) static OFFSCREEN_EGL: OnceLock<Arc<OffscreenEglState>> = OnceLock::new();

/// Holds the EGL display/surface/context and the dynamic EGL library needed
/// to provide a `get_proc_address` to libmpv when the egui renderer is wgpu.
///
/// This lives in a `OnceLock` for the process lifetime. EGL handles are
/// opaque driver-managed identifiers; they are intentionally not explicitly
/// released on drop (the driver cleans them up at process exit).
pub(super) struct OffscreenEglState {
    egl:     khronos_egl::DynamicInstance<khronos_egl::EGL1_5>,
    display: khronos_egl::Display,
    /// Surface must be kept alive; the surface is only a 1×1 pbuffer used to
    /// satisfy `make_current`'s requirement for a draw surface.
    surface: khronos_egl::Surface,
    // Hold the context so it is not invalidated.
    context: khronos_egl::Context,
    /// Shared glow context built from this EGL context. Cloned into each
    /// [`PlayerState`] that uses the offscreen path.
    gl:      Arc<glow::Context>,
}

// SAFETY: EGL handles are opaque integers referencing driver-managed state.
// EGL is thread-safe by spec. `DynamicInstance` contains only function
// pointers and a `libloading::Library`, both of which are `Send` on Linux.
unsafe impl Send for OffscreenEglState {}
unsafe impl Sync for OffscreenEglState {}

impl OffscreenEglState {
    pub fn make_current(&self) -> Result<(), BackendError> {
        self.egl.make_current(
            self.display,
            Some(self.surface),
            Some(self.surface),
            Some(self.context),
        )?;
        Ok(())
    }

    pub fn unbind(&self) -> Result<(), BackendError> {
        self.egl.make_current(self.display, None, None, None)?;
        Ok(())
    }
}

pub(super) struct WgpuState {
    /// `true` when we are using our own offscreen EGL context rather than
    /// eframe's glow context (i.e. the wgpu renderer is active).
    /// Only present when the `wgpu` feature is enabled.
    pub is_offscreen:  bool,
    /// The egui texture used to display the current video frame on the slow
    /// (wgpu / CPU-readback) path. Kept alive here between frames.
    pub frame_texture: Option<eframe::egui::TextureHandle>,
    pub pixel_buffer:  Vec<u8>,
}

impl super::PlayerState {
    /// Initialize the shared offscreen EGL/OpenGL context. Returns a clone of
    /// the shared `Arc<glow::Context>` plus a fresh `Arc<GpaFn>` for this
    /// particular `PlayerState` instance. Subsequent calls skip
    /// re-initialisation and return clones from the lready-stored global.
    ///
    /// The EGL context is made current on the first call and remains current on
    /// the main thread permanently; wgpu's own backends on Linux do not use
    /// OpenGL on the main thread, so no context conflicts arise in practice.
    pub(super) fn init_offscreen_egl() -> Result<(Arc<glow::Context>, Arc<GpaFn>), BackendError> {
        use khronos_egl as egl;

        const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
        const EGL_PLATFORM_DEVICE_EXT: u32 = 0x313F;

        // If a previous PlayerState already set this up, just clone out the
        // shared state and build a fresh GPA arc (same backing EGL, independent arc).
        if let Some(state) = OFFSCREEN_EGL.get() {
            debug!("Reusing shared offscreen EGL context");
            let gpa: Arc<GpaFn> = Arc::new(|name: &str| -> *mut c_void {
                OFFSCREEN_EGL
                    .get()
                    .and_then(|s| s.egl.get_proc_address(name))
                    .map_or(std::ptr::null(), |f| f as usize as *const c_void)
                    .cast_mut()
            });
            return Ok((Arc::clone(&state.gl), gpa));
        }

        debug!("Initialising shared offscreen EGL context for libmpv (wgpu path)");

        let lib = unsafe { libloading::Library::new("libEGL.so.1") }.map_err(egl::LoadError::Library)?;
        let egl_inst = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required_from(lib) }?;

        let display = unsafe {
            egl_inst.get_platform_display(EGL_PLATFORM_SURFACELESS_MESA, egl::DEFAULT_DISPLAY, &[])
        }
        .or_else(|_| {
            tracing::debug!("Surfaceless failed, trying EGL_EXT_platform_device (NVIDIA)");

            // 2. Try Device (NVIDIA Proprietary)
            let query_devices_ptr = egl_inst.get_proc_address("eglQueryDevicesEXT");
            if let Some(query_devices_ptr) = query_devices_ptr {
                // Cast the raw pointer to the correct EGL function signature
                let query_devices: unsafe extern "system" fn(
                    i32,
                    *mut *mut std::ffi::c_void,
                    *mut i32,
                ) -> u32 = unsafe { std::mem::transmute(query_devices_ptr) };

                let mut num_devices = 0;
                let mut device = std::ptr::null_mut();

                // Grab the first available physical GPU device
                unsafe {
                    if query_devices(1, &raw mut device, &raw mut num_devices) != 0 && num_devices > 0 {
                        return egl_inst.get_platform_display(
                            EGL_PLATFORM_DEVICE_EXT,
                            device,
                            &[egl::NONE as usize],
                        );
                    }
                }
            }

            tracing::warn!("Device platform failed, falling back to DEFAULT_DISPLAY.");
            Err(khronos_egl::Error::BadParameter)
        })
        .unwrap_or_else(|e| {
            tracing::warn!("Surfaceless EGL platform unavailable: {e}; falling back to default display.");
            // FIXME: Do not panic here!!!
            unsafe { egl_inst.get_display(egl::DEFAULT_DISPLAY).unwrap() }
        });

        egl_inst.initialize(display)?;
        egl_inst.bind_api(egl::OPENGL_API)?;

        #[rustfmt::skip]
        let config_attribs = [
            egl::RENDERABLE_TYPE, egl::OPENGL_BIT,
            egl::SURFACE_TYPE,    egl::PBUFFER_BIT,
            egl::RED_SIZE,        8,
            egl::GREEN_SIZE,      8,
            egl::BLUE_SIZE,       8,
            egl::ALPHA_SIZE,      8,
            egl::NONE,
        ];

        let config = egl_inst
            .choose_first_config(display, &config_attribs)?
            .ok_or(BackendError::NoEglConfig)?;

        // OpenGL 3.3 core profile context.
        #[rustfmt::skip]
        let ctx_attribs = [
            egl::CONTEXT_MAJOR_VERSION,       3,
            egl::CONTEXT_MINOR_VERSION,       3,
            egl::CONTEXT_OPENGL_PROFILE_MASK, egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            egl::NONE,
        ];
        let context = egl_inst.create_context(display, config, None, &ctx_attribs)?;

        // 1×1 pbuffer: we never render to the surface itself, it is only
        // required to make the context current.
        let surf_attribs = [egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE];
        let surface = egl_inst.create_pbuffer_surface(display, config, &surf_attribs)?;

        // Bind to this thread permanently (wgpu on Linux does not use GL on
        // the main thread, so no context conflicts arise in practice).
        egl_inst.make_current(display, Some(surface), Some(surface), Some(context))?;

        // Build the glow context now while we have a local reference to egl_inst.
        // SAFETY: The EGL context is current on this thread.
        let gl = Arc::new(unsafe {
            glow::Context::from_loader_function(|name| {
                egl_inst
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as usize as *const _)
            })
        });

        let state = Arc::new(OffscreenEglState {
            egl: egl_inst,
            display,
            surface,
            context,
            gl: Arc::clone(&gl),
        });

        // Benign race: PlayerState::new is always called on the UI thread, so
        // in practice this never actually races. If it somehow did, the loser
        // would just drop its freshly-created (unused) Arc.
        let _ = OFFSCREEN_EGL.set(Arc::clone(&state));

        // Fresh GPA arc: no captures, reads from the now-populated global.
        let gpa: Arc<GpaFn> = Arc::new(|name: &str| -> *mut c_void {
            OFFSCREEN_EGL
                .get()
                .and_then(|s| s.egl.get_proc_address(name))
                .map_or(std::ptr::null(), |f| f as usize as *const c_void)
                .cast_mut()
        });

        state.unbind()?;
        debug!("Offscreen EGL/OpenGL 3.3 context ready");

        Ok((gl, gpa))
    }

    /// CPU-readback path: render into the offscreen GL framebuffer, read the
    /// pixels back, and upload them as an egui texture.
    #[cfg(feature = "wgpu")]
    pub(super) fn render_frame_offscreen(
        &mut self,
        ctx: &eframe::egui::Context,
        size: FramebufferSize,
    ) -> Result<RenderedFrame, BackendError> {
        let egl_state = OFFSCREEN_EGL.get().expect("Offscreen EGL uninitialized");
        egl_state.make_current()?;

        let res = {
            #[expect(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            if self.wgpu.is_offscreen {
                // Scale down to the source video's dimensions, and let the UI scale it back
                // up. This way, less data needs to make the roundtrip through system memory.
                let size = if let Ok(Some((vid_w, vid_h))) = self.dimensions() {
                    let req_w = f64::from(size.width);
                    let req_h = f64::from(size.height);

                    if req_w > 0.0 && req_h > 0.0 {
                        // Find the strictest scale factor to prevent exceeding native resolution whilst
                        // strictly preserving the UI's requested aspect ratio.
                        let scale = 1.0_f64.min((vid_w as f64 / req_w).min(vid_h as f64 / req_h));

                        FramebufferSize {
                            width:  (req_w * scale) as i32,
                            height: (req_h * scale) as i32,
                        }
                    } else {
                        size
                    }
                } else {
                    size
                };

                self.render_glow(size)
            } else {
                self.render_glow(size)
            }
        };

        let (res, frame_updated) = match res {
            Ok(res) => res,
            Err(e) => {
                egl_state.unbind()?;
                return Err(e);
            }
        };

        // Don't copy or replace anything if nothing changed between redraws.
        if !frame_updated && let Some(ref tex) = self.wgpu.frame_texture {
            egl_state.unbind()?;
            return Ok(RenderedFrame::EguiTexture(tex.id()));
        }

        let FramebufferSize { width, height } = res.framebuffer_size();
        let expected_len = (width * height * 4).cast_unsigned() as usize;
        if self.wgpu.pixel_buffer.len() != expected_len {
            self.wgpu.pixel_buffer.resize(expected_len, 0);
        }
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, Some(res.framebuffer()));
            self.gl.read_pixels(
                0,
                0,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut self.wgpu.pixel_buffer)),
            );
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }

        egl_state.unbind()?;

        let color_image = eframe::egui::ColorImage::from_rgba_unmultiplied(
            [width.cast_unsigned() as usize, height.cast_unsigned() as usize],
            &self.wgpu.pixel_buffer,
        );

        let tex_id = if let Some(ref mut tex) = self.wgpu.frame_texture {
            tex.set(color_image, eframe::egui::TextureOptions::LINEAR);
            tex.id()
        } else {
            let tex = ctx.load_texture(
                "sharkplayer_frame",
                color_image,
                eframe::egui::TextureOptions::LINEAR,
            );
            let id = tex.id();
            self.wgpu.frame_texture = Some(tex);
            id
        };

        Ok(RenderedFrame::EguiTexture(tex_id))
    }
}
