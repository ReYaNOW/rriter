const INACTIVE_HIGHLIGHT_SPAN_CAP_LIMIT: usize = 24 * 1024;
const INACTIVE_HIGHLIGHT_COMPLETION_CAP_LIMIT: usize = 2048;
const INACTIVE_HIGHLIGHT_SMALL_CAP_LIMIT: usize = 4096;

fn inactive_highlight_cache_over_limit(tab: &EditorTab) -> bool {
    tab.spans.capacity() > INACTIVE_HIGHLIGHT_SPAN_CAP_LIMIT
        || tab.completions.capacity() > INACTIVE_HIGHLIGHT_COMPLETION_CAP_LIMIT
        || tab.foldable_ranges.capacity() > INACTIVE_HIGHLIGHT_SMALL_CAP_LIMIT
        || tab.syntax_errors.capacity() > INACTIVE_HIGHLIGHT_SMALL_CAP_LIMIT
}

fn compact_tab_highlight_cache(tab: &mut EditorTab) {
    tab.spans.clear();
    tab.spans.shrink_to_fit();
    tab.completions.clear();
    tab.completions.shrink_to_fit();
    tab.foldable_ranges.clear();
    tab.foldable_ranges.shrink_to_fit();
    tab.syntax_errors.clear();
    tab.syntax_errors.shrink_to_fit();
    tab.is_highlight_complete = false;
}

impl App {
    pub(crate) fn terminal_working_directory(&self) -> Option<PathBuf> {
        crate::app::terminal_process::select_terminal_working_directory(
            self.file_path.as_deref(),
            &self.ide_workspaces,
        )
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
    }

    pub(crate) fn add_terminal(&mut self) -> usize {
        let cwd = self.terminal_working_directory();
        let terminal = crate::app::terminal::Terminal::spawn(
            self.window.clone(),
            cwd.as_deref(),
        );
        self.ide_panel.terminals.push(terminal);
        self.ide_panel.active_terminal = self.ide_panel.terminals.len().saturating_sub(1);
        self.ide_panel.active_terminal
    }

    pub(crate) fn shutdown_background_services(&mut self) {
        for terminal in &mut self.ide_panel.terminals {
            terminal.shutdown();
        }
        if let Some(lsp) = self.lsp.take() {
            lsp.shutdown();
        }
        self.ide_panel.api.shutdown_background_tasks();
        crate::app::api_mock::server::stop_api_mock_server();
    }

    pub(crate) fn save_current_config(&self) {
        let config = crate::Config {
            window_width: self.window_width,
            window_height: self.window_height,
            maximized: self
                .window
                .as_ref()
                .map(|w| w.is_maximized())
                .unwrap_or(false),
            ide_workspaces: self.ide_workspaces.clone(),
            ide_ignore_patterns: self.ide_ignore_patterns.clone(),
            enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                .load(std::sync::atomic::Ordering::Relaxed),
        };
        crate::save_config(&config);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn set_clipboard_text(&mut self, text: impl Into<String>) {
        if let Some(clipboard) = self.clipboard.as_mut() {
            let _ = clipboard.set_text(text.into());
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_clipboard_text(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn enter_ide_mode(&mut self) {
        self.is_ide_mode = true;

        let was_welcome = self.show_welcome;
        self.show_welcome = false;
        if was_welcome && self.base_title == "Добро пожаловать" {
            self.base_title = "Безымянный".to_string();
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
        }

        self.ide_panel = crate::load_panel_state();
        self.ide_panel.api = crate::app::api_client::ApiClientState::load_persisted();
        self.ide_panel.enforce_single_open_per_group();

        if self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {
            self.add_terminal();
            self.ide_panel.terminal_focused = true;
        }

        if self.lsp.is_none() {
            self.lsp = Some(crate::lsp::LspManager::new(self.ide_workspaces.clone()));
        }

        let has_startup_file =
            self.file_path.is_some() || self.editor.len() > 0 || self.editor.is_dirty();

        if has_startup_file && self.tabs.is_empty() {
            self.tabs.push(EditorTab {
                editor: crate::editor::Editor::new(128),
                file_path: self.file_path.clone(),
                file_key: self.file_key.clone(),
                text_file_format: self.text_file_format,
                base_title: self.base_title.clone(),
                file_extension: self.file_extension.clone(),
                scroll_y: crate::scroll::ScrollState::new(15.0),
                scroll_x: crate::scroll::ScrollState::new(15.0),
                spans: Vec::new(),
                completions: Vec::new(),
                foldable_ranges: Vec::new(),
                last_sent_version: u64::MAX,
                search_results: Vec::new(),
                search_current_idx: None,
                is_highlighted_once: false,
                is_highlight_complete: false,
                icon_key: "default_file",
                syntax_errors: Vec::new(),
                kind: EditorTabKind::Normal,
            });
            self.active_tab = 0;
        }

        let (saved_tabs, saved_active) = if self.scroll_render_bench.is_some() {
            (Vec::new(), 0)
        } else {
            crate::load_open_tabs(true)
        };

        if !saved_tabs.is_empty() {
            let mut loaded_any = false;
            for saved_tab in saved_tabs {
                match saved_tab {
                    crate::OpenTabSnapshot::File(path) => {
                        if path.exists() {
                            self.open_file_in_tab_bg(path, false);
                            loaded_any = true;
                        }
                    }
                    crate::OpenTabSnapshot::Empty => {
                        self.open_new_tab();
                        loaded_any = true;
                    }
                    crate::OpenTabSnapshot::Api {
                        spec_id,
                        route_idx,
                        auth_view,
                    } => {
                        if self
                            .ide_panel
                            .api
                            .specs
                            .iter()
                            .any(|entry| entry.id == spec_id)
                        {
                            if auth_view {
                                self.open_api_auth_tab(spec_id);
                            } else {
                                if let Some(route_idx) = route_idx {
                                    self.open_api_route_with_new_tab(spec_id, route_idx, true);
                                } else {
                                    self.open_api_spec_tab(spec_id);
                                }
                            }
                            loaded_any = true;
                        }
                    }
                }
            }

            if loaded_any {
                let target = if has_startup_file {
                    0
                } else {
                    saved_active.min(self.tabs.len().saturating_sub(1))
                };
                self.switch_to_tab(target);
                self.save_tabs_state();
                if !self.is_highlighted_once {
                    self.wait_for_current_highlight();
                }
            }
        }

        let title = self.base_title.clone();
        if !self.tabs.is_empty() {
            self.tabs[self.active_tab].icon_key =
                crate::app::file_icons::file_icon_key_for_name(&title);
        }

        if let Some(path) = &self.file_path {
            if let Some(lsp) = &mut self.lsp {
                let text = self.editor.get_full_text();
                lsp.notify_open(
                    path,
                    &self.file_extension,
                    &text,
                    self.editor.version as i32,
                );
            }
            self.refresh_current_editor_git_base();
        }

        self.refresh_file_tree();
        self.start_file_watcher();
        if self.ide_panel.is_open(PanelId::Git) {
            self.refresh_git_panel();
        }

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }
    pub fn save_tabs_state(&mut self) {
        if !self.is_ide_mode {
            return;
        }
        self.sync_active_tab();
        crate::save_open_tabs(&self.tabs, self.active_tab, self.is_ide_mode);
        self.sync_active_tab();
    }

    pub fn sync_active_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let ai = self.active_tab;
        std::mem::swap(&mut self.editor, &mut self.tabs[ai].editor);
        std::mem::swap(&mut self.highlighter.spans, &mut self.tabs[ai].spans);
        std::mem::swap(
            &mut self.highlighter.completions,
            &mut self.tabs[ai].completions,
        );
        std::mem::swap(
            &mut self.highlighter.foldable_ranges,
            &mut self.tabs[ai].foldable_ranges,
        );
        std::mem::swap(
            &mut self.highlighter.syntax_errors,
            &mut self.tabs[ai].syntax_errors,
        );
        std::mem::swap(&mut self.file_path, &mut self.tabs[ai].file_path);
        std::mem::swap(&mut self.file_key, &mut self.tabs[ai].file_key);
        std::mem::swap(
            &mut self.text_file_format,
            &mut self.tabs[ai].text_file_format,
        );
        std::mem::swap(&mut self.base_title, &mut self.tabs[ai].base_title);
        std::mem::swap(&mut self.file_extension, &mut self.tabs[ai].file_extension);
        std::mem::swap(&mut self.scroll_y, &mut self.tabs[ai].scroll_y);
        std::mem::swap(&mut self.scroll_x, &mut self.tabs[ai].scroll_x);
        std::mem::swap(&mut self.search_results, &mut self.tabs[ai].search_results);
        std::mem::swap(
            &mut self.search_current_idx,
            &mut self.tabs[ai].search_current_idx,
        );
        std::mem::swap(
            &mut self.last_sent_version,
            &mut self.tabs[ai].last_sent_version,
        );
        std::mem::swap(
            &mut self.is_highlighted_once,
            &mut self.tabs[ai].is_highlighted_once,
        );
        std::mem::swap(
            &mut self.is_highlight_complete,
            &mut self.tabs[ai].is_highlight_complete,
        );

        let title_to_use = if self.base_title.len() > self.tabs[ai].base_title.len() {
            &self.base_title
        } else {
            &self.tabs[ai].base_title
        };
        let icon_title = title_to_use.trim_start_matches('*').trim_start();
        let icon_key = crate::app::file_icons::file_icon_key_for_name(icon_title);
        self.tabs[ai].icon_key = icon_key;
    }

    fn next_tab_highlight_version(&self) -> u64 {
        self.tabs
            .iter()
            .map(|t| t.editor.version)
            .max()
            .unwrap_or(0)
            .max(self.editor.version)
            .max(self.highlighter.current_version)
            .saturating_add(1)
    }

    fn tab_display_titles(&self) -> Vec<String> {
        tab_display_titles_for(
            &self.tabs,
            self.active_tab,
            self.file_path.as_ref(),
            &self.base_title,
        )
    }

    fn reveal_tab_now(&mut self, idx: usize) {
        if !self.is_ide_mode || idx >= self.tabs.len() {
            return;
        }

        let titles = self.tab_display_titles();
        if let Some(r) = self.renderer.as_mut() {
            let s = r.scale_factor;
            let tab_x = (48.0 * s + self.ide_panel.left_width * s).round() + 1.0;
            let viewport_w = (r.width - tab_x).max(0.0);
            if viewport_w <= 0.0 {
                return;
            }

            let tab_pad = 16.0 * s;
            let icon_size_tab = 20.0 * s;
            let mut tab_left = 0.0;
            let mut total_w = 0.0;
            for (i, title) in titles.iter().enumerate() {
                let title_w = r.measure_ui_width(title, 1.0);
                let tab_w = tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
                if i < idx {
                    tab_left += tab_w;
                }
                total_w += tab_w;
            }

            let title_w = r.measure_ui_width(&titles[idx], 1.0);
            let tab_w = tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
            let tab_right = tab_left + tab_w;
            let max_scroll = (total_w - viewport_w).max(0.0);
            let margin = (12.0 * s).min(viewport_w * 0.25);
            let mut target = self.tab_scroll.target;

            if tab_left < target + margin {
                target = tab_left - margin;
            } else if tab_right > target + viewport_w - margin {
                target = tab_right + margin - viewport_w;
            }

            let target = target.clamp(0.0, max_scroll);
            self.tab_scroll.target = target;
            self.tab_scroll.current = target;
        }
    }

    fn reveal_active_tab_now(&mut self) {
        self.reveal_tab_now(self.active_tab);
    }

    fn compact_inactive_highlight_caches(&mut self, recent_tab_idx: Option<usize>) {
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            if idx == self.active_tab || Some(idx) == recent_tab_idx {
                continue;
            }
            if inactive_highlight_cache_over_limit(tab) {
                compact_tab_highlight_cache(tab);
            }
        }
    }

    fn clamp_tab_scroll_to_content_now(&mut self) {
        if !self.is_ide_mode || self.tabs.is_empty() {
            self.tab_scroll.current = 0.0;
            self.tab_scroll.target = 0.0;
            return;
        }

        let titles = self.tab_display_titles();
        let Some(r) = self.renderer.as_mut() else {
            self.tab_scroll.current = 0.0;
            self.tab_scroll.target = 0.0;
            return;
        };
        let s = r.scale_factor;
        let tab_x = (48.0 * s + self.ide_panel.left_width * s).round() + 1.0;
        let viewport_w = (r.width - tab_x).max(0.0);
        if viewport_w <= 0.0 {
            self.tab_scroll.current = 0.0;
            self.tab_scroll.target = 0.0;
            return;
        }

        let tab_pad = 16.0 * s;
        let icon_size_tab = 20.0 * s;
        let total_w = titles
            .iter()
            .map(|title| {
                tab_pad * 2.0 + icon_size_tab + 8.0 * s + r.measure_ui_width(title, 1.0) + 30.0 * s
            })
            .sum::<f32>();
        let max_scroll = (total_w - viewport_w).max(0.0);
        self.tab_scroll.current = self.tab_scroll.current.clamp(0.0, max_scroll);
        self.tab_scroll.target = self.tab_scroll.target.clamp(0.0, max_scroll);
    }

    pub fn switch_to_tab(&mut self, new_idx: usize) {
        if !self.is_ide_mode || self.tabs.is_empty() {
            return;
        }
        if new_idx == self.active_tab || new_idx >= self.tabs.len() {
            return;
        }

        let previous_tab = self.active_tab;
        self.commit_api_focus();
        self.ide_panel.api.focused = None;
        self.sync_active_tab();
        self.active_tab = new_idx;
        self.sync_active_tab();
        self.prefetch_active_tab_git_graph();

        if self.active_tab_is_api_client() {
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.autocomplete_active = false;
            self.inline_git_popup = None;
        } else if self.active_tab_is_git_diff() {
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            if !self.is_highlighted_once {
                self.editor.version = self.next_tab_highlight_version();
                self.prepare_active_git_diff_highlight_after_switch();
            } else {
                self.highlighter.restore_cached_view(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                );
            }
        } else if self.is_highlighted_once && self.is_highlight_complete {
            self.highlighter.restore_cached_view(
                self.editor.version,
                self.editor.get_full_text(),
                self.file_extension.clone(),
            );
        } else if self.is_highlighted_once {
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.is_highlight_complete = false;
            self.highlighter.restart_cached_view(
                self.editor.version,
                self.editor.get_full_text(),
                self.file_extension.clone(),
                self.editor.cursor,
            );
        } else {
            self.editor.version = self.next_tab_highlight_version();
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            self.wait_for_current_highlight();
        }

        if self.is_ide_mode && !self.active_tab_is_api_client() {
            if let Some(lsp) = &mut self.lsp {
                if let Some(path) = &self.file_path {
                    let text = self.editor.get_full_text();
                    lsp.notify_open(
                        path,
                        &self.file_extension,
                        &text,
                        self.editor.version as i32,
                    );
                }
            }
        }

        self.autocomplete_active = false;
        self.show_welcome =
            self.tabs.is_empty() && self.file_path.is_none() && self.editor.len() == 0;
        self.inline_git_popup = None;
        self.reveal_active_tab_now();
        self.compact_inactive_highlight_caches(Some(previous_tab));
        self.start_file_watcher();
        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.save_tabs_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab_with_large_highlight_cache() -> EditorTab {
        let mut spans = Vec::with_capacity(INACTIVE_HIGHLIGHT_SPAN_CAP_LIMIT + 1);
        spans.push(crate::highlighter::ColorSpan {
            start: 0,
            end: 1,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        EditorTab {
            editor: crate::editor::Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: "large.rs".to_string(),
            file_extension: "rs".to_string(),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans,
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: 0,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "default_file",
            kind: EditorTabKind::Normal,
        }
    }

    #[test]
    fn inactive_highlight_cache_compaction_drops_retained_buffers() {
        let mut tab = tab_with_large_highlight_cache();

        assert!(inactive_highlight_cache_over_limit(&tab));
        compact_tab_highlight_cache(&mut tab);

        assert!(tab.spans.is_empty());
        assert_eq!(tab.spans.capacity(), 0);
        assert!(tab.is_highlighted_once);
        assert!(!tab.is_highlight_complete);
    }
}
