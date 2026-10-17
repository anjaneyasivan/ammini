        // Floating inspector overlay
        if self.inspector_open {
            let proxy_url = self.proxy_url_mut().clone();
            let pos_line = self
                .player_mut()
                .time_pos()
                .ok()
                .flatten()
                .map(|pos| format!("pos: {:.1}s", pos));
            let info_lines = {
                let mpv = self.player_mut().mpv();
                let mut lines: Vec<String> = Vec::new();
                if let Ok(w) = mpv.get_property::<i64>("width")
                    && let Ok(h) = mpv.get_property::<i64>("height")
                {
                    lines.push(format!("resolution: {}x{}", w, h));
                }
                if let Ok(c) = mpv.get_property::<String>("video-codec")
                    && !c.is_empty()
                {
                    lines.push(format!("video codec: {}", c));
                }
                if let Ok(c) = mpv.get_property::<String>("audio-codec")
                    && !c.is_empty()
                {
                    lines.push(format!("audio codec: {}", c));
                }
                if let Ok(fs) = mpv.get_property::<i64>("file-size")
                    && fs > 0
                {
                    lines.push(format!("file size: {:.1} MB", fs as f64 / 1_048_576.0));
                }
                if let Ok(vb) = mpv.get_property::<f64>("video-bitrate")
                    && vb > 0.0
                {
                    lines.push(format!("video bitrate: {:.0} kbps", vb / 1000.0));
                }
                if let Ok(ab) = mpv.get_property::<f64>("audio-bitrate")
                    && ab > 0.0
                {
                    lines.push(format!("audio bitrate: {:.0} kbps", ab / 1000.0));
                }
                if let Ok(fps) = mpv.get_property::<f64>("container-fps")
                    && fps > 0.0
                {
                    lines.push(format!("fps: {:.2}", fps));
                }
                lines
            };
            let dismiss = right_sidebar::show(
                &ctx,
                proxy_url.as_deref(),
                pos_line,
                &info_lines,
                video_rect.min.y + 40.0,
                60.0,
            );
            if dismiss {
                self.inspector_open = false;
            }
        }

        // URL dialog
        if self.show_url_dialog {
            let mut close = false;
            egui::Window::new("Open URL")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut self.url_input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Load").clicked() && !self.url_input.trim().is_empty() {
                            events.push(PlayerEvent::OpenUrl(self.url_input.trim().to_string()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.show_url_dialog = false;
                self.url_input.clear();
            }
        }

        for event in events {
            if matches!(
                event,
                PlayerEvent::OpenFile(_)
                    | PlayerEvent::OpenUrl(_)
                    | PlayerEvent::OpenTelegramUrl(_)
            ) {
                self.byte_time_map = ammini::telegram::cache::ByteTimeMap::default();
                self.byte_time_record_last = None;
                let speed = self.playback_speed;
                let _ = self.player_mut().mpv().set_property("speed", speed);
            }
            match event {
                PlayerEvent::Poll => trace!("poll"),
                _ => debug!("{event:?}"),
            }
            self.fsm.handle(&event);
        }
        // VideoReady handling from drained messages needs OpenTelegramUrl event; handle via telegram fsm data?
        // We'll handle OpenTelegramUrl by checking if a video was just readied — skip for now; telegram panel handles play.

        if (self.audio_menu_open || self.subs_menu_open || self.speed_menu_open)
            && self.frame_count > self.menu_opened_frame
        {
            if ctx.input(|i| i.pointer.any_click()) {
                self.audio_menu_open = false;
                self.subs_menu_open = false;
                self.speed_menu_open = false;
            }
        }
        self.poll_seek();
        self.update_display_sleep();
