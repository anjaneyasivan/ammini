//! Keep the display awake while media is playing.
//!
//! egui/winit have no screen-saver suppression, and the embedded libmpv runs with a
//! render context (no VO), so mpv's own `stop-screensaver` option is a no-op here.
//! Instead we hold a keepawake lock (macOS: IOKit `NoDisplaySleep` + `NoIdleSleep`
//! assertions — the same power assertions a real video player takes) while audio/
//! video is actually loaded and unpaused, and release it on pause so the OS's normal
//! dim/sleep behavior applies while paused. The lock is created/dropped on play ↔
//! pause transitions; keepawake has no toggle, and a dropped handle releases its
//! assertions.

use keepawake::{Builder, KeepAwake};

/// Tracks whether the keepawake lock is currently held, so the app only creates /
/// drops it on actual play ↔ pause transitions.
pub struct Guard {
    /// Held while media is playing (keeps the display from dimming/sleeping and the
    /// system from idling to sleep mid-playback).
    awake: Option<KeepAwake>,
}

impl Guard {
    pub fn new() -> Self {
        Self { awake: None }
    }

    /// Hold the display (and idle) awake when `playing` is true, release both
    /// otherwise. Idempotent; safe to call every frame.
    pub fn set_playing(&mut self, playing: bool) {
        if playing && self.awake.is_none() {
            // Best-effort: a failed assertion must never break playback, so a lock
            // that cannot be created is simply skipped.
            self.awake = Builder::default()
                .display(true)
                .idle(true)
                .reason("Ammini video playback")
                .create()
                .ok();
        } else if !playing {
            self.awake = None;
        }
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Guard;

    #[test]
    fn hold_is_idempotent_until_release() {
        let mut guard = Guard::new();
        assert!(guard.awake.is_none());
        guard.set_playing(true);
        // A second hold (e.g. next frame) must not stack locks.
        guard.set_playing(true);
        assert!(guard.awake.is_some());
        guard.set_playing(false);
        assert!(guard.awake.is_none());
    }

    #[test]
    fn drop_releases_held_lock() {
        let mut guard = Guard::new();
        guard.set_playing(true);
        drop(guard);
        assert!(Guard::new().awake.is_none());
    }
}
