const INACTIVE_HIGHLIGHT_SPAN_CAP_LIMIT: usize = 24 * 1024;
const INACTIVE_HIGHLIGHT_COMPLETION_CAP_LIMIT: usize = 2048;
const INACTIVE_HIGHLIGHT_SMALL_CAP_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabDragPlacement {
    pub dragged_x: f32,
    pub destination: usize,
}

pub(crate) fn tab_drag_placement(
    base_x: f32,
    widths: &[f32],
    drag: Option<&TabDragState>,
) -> Option<TabDragPlacement> {
    let drag = drag?;
    if !drag.threshold_passed || drag.start_idx >= widths.len() {
        return None;
    }

    let start_x = base_x + widths[..drag.start_idx].iter().sum::<f32>();
    let dragged_x = start_x + (drag.current_x - drag.start_x);
    let dragged_center = dragged_x + widths[drag.start_idx] * 0.5;
    let mut destination = drag.start_idx;
    let mut x = base_x;

    for (idx, &width) in widths.iter().enumerate() {
        if idx != drag.start_idx {
            let other_center = x + width * 0.5;
            if idx < drag.start_idx {
                if dragged_center < other_center {
                    destination = destination.min(idx);
                }
            } else if dragged_center > other_center {
                destination = destination.max(idx);
            }
        }
        x += width;
    }

    Some(TabDragPlacement {
        dragged_x,
        destination,
    })
}

pub(crate) fn tab_drag_layout(
    base_x: f32,
    widths: &[f32],
    drag: Option<&TabDragState>,
    actual_xs: &mut Vec<f32>,
    order: &mut Vec<usize>,
) -> Option<usize> {
    actual_xs.clear();
    order.clear();
    actual_xs.reserve(widths.len());
    order.reserve(widths.len());

    let mut x = base_x;
    for (idx, &width) in widths.iter().enumerate() {
        actual_xs.push(x);
        order.push(idx);
        x += width;
    }

    let drag = drag?;
    let placement = tab_drag_placement(base_x, widths, Some(drag))?;
    order.retain(|&idx| idx != drag.start_idx);
    order.insert(placement.destination, drag.start_idx);

    let mut x = base_x;
    for &idx in order.iter() {
        if idx != drag.start_idx {
            actual_xs[idx] = x;
        }
        x += widths[idx];
    }
    actual_xs[drag.start_idx] = placement.dragged_x;
    Some(drag.start_idx)
}

pub(crate) fn tab_drag_render_order(
    order: &[usize],
    dragged_idx: Option<usize>,
    render_order: &mut Vec<usize>,
) {
    render_order.clear();
    render_order.reserve(order.len());
    render_order.extend(order.iter().copied().filter(|idx| Some(*idx) != dragged_idx));
    if let Some(idx) = dragged_idx {
        render_order.push(idx);
    }
}

pub(crate) fn active_index_after_move(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else if from < to && active > from && active <= to {
        active - 1
    } else if to < from && active >= to && active < from {
        active + 1
    } else {
        active
    }
}

pub(crate) fn active_index_after_remove(active: usize, removed: usize, remaining: usize) -> usize {
    if remaining == 0 {
        return 0;
    }
    if active == removed {
        removed.min(remaining - 1)
    } else if active > removed {
        active - 1
    } else {
        active.min(remaining - 1)
    }
}

fn take_terminal_creation_number(next: &mut u64) -> u64 {
    let number = *next;
    *next = number.saturating_add(1);
    number
}

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

fn ready_terminal_presentation<I>(
    states: I,
) -> Option<(usize, crate::app::terminal::TerminalPresentationIntent)>
where
    I: IntoIterator<
        Item = (
            usize,
            crate::app::terminal::TerminalPresentationIntent,
            bool,
        ),
    >,
{
    states.into_iter().find_map(|(idx, intent, ready)| {
        (intent != crate::app::terminal::TerminalPresentationIntent::None && ready)
            .then_some((idx, intent))
    })
}

fn terminal_tab_reveal_target(
    widths_with_add: &[f32],
    active_idx: usize,
    reveal_right_tail: bool,
    viewport_w: f32,
    current_target: f32,
    margin: f32,
) -> f32 {
    let reveal_idx = if reveal_right_tail {
        widths_with_add.len().saturating_sub(1)
    } else {
        active_idx
    };
    crate::render_view::tabs_ui::tab_strip_reveal_target(
        widths_with_add,
        reveal_idx,
        viewport_w,
        current_target,
        margin,
    )
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

    fn active_terminal_presentation_ready(&self) -> bool {
        self.ide_panel
            .terminals
            .get(self.ide_panel.active_terminal)
            .is_some_and(|terminal| {
                crate::app::terminal::lock_terminal_grid(&terminal.grid).presentation_ready
            })
    }

    fn cancel_terminal_presentation_intents(&mut self) {
        for terminal in &mut self.ide_panel.terminals {
            terminal.presentation_intent =
                crate::app::terminal::TerminalPresentationIntent::None;
            terminal.reveal_right_tail_when_presented = false;
        }
    }

    fn hide_terminal_panel_for_pending_presentation(&mut self) {
        if let Some(slot) = self
            .ide_panel
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::Terminal)
        {
            slot.open = false;
        }
        self.ide_panel.terminal_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.enforce_single_open_per_group();
    }

    pub(crate) fn process_terminal_presentation_intents(&mut self) -> bool {
        let ready = ready_terminal_presentation(
            self.ide_panel
                .terminals
                .iter()
                .enumerate()
                .filter_map(|(idx, terminal)| {
                    let intent = terminal.presentation_intent;
                    (intent != crate::app::terminal::TerminalPresentationIntent::None).then(|| {
                        let ready = crate::app::terminal::lock_terminal_grid(&terminal.grid)
                            .presentation_ready;
                        (idx, intent, ready)
                    })
                }),
        );
        let Some((idx, intent)) = ready else {
            return false;
        };

        let reveal_right_tail = self.ide_panel.terminals[idx].reveal_right_tail_when_presented
            && idx + 1 == self.ide_panel.terminals.len();
        self.cancel_terminal_presentation_intents();
        self.ide_panel.active_terminal = idx;
        if intent == crate::app::terminal::TerminalPresentationIntent::OpenPanelWhenReady {
            self.ide_panel.open(PanelId::Terminal);
            crate::save_panel_state(&self.ide_panel);
        }
        self.reveal_active_terminal_tab_now_for_presentation(reveal_right_tail);
        true
    }

    pub(crate) fn defer_terminal_panel_until_ready(&mut self) {
        if !self.ide_panel.is_open(PanelId::Terminal) || self.active_terminal_presentation_ready() {
            return;
        }
        if self.process_terminal_presentation_intents() {
            return;
        }
        let pending = self.ide_panel.terminals.iter().rposition(|terminal| {
            terminal.presentation_intent
                != crate::app::terminal::TerminalPresentationIntent::None
        });
        if let Some(idx) = pending {
            self.ide_panel.terminals[idx].presentation_intent =
                crate::app::terminal::TerminalPresentationIntent::OpenPanelWhenReady;
            self.hide_terminal_panel_for_pending_presentation();
        }
    }

    pub(crate) fn select_terminal_tab_from_user(&mut self, idx: usize) {
        if idx >= self.ide_panel.terminals.len() {
            return;
        }
        self.cancel_terminal_presentation_intents();
        let ready = crate::app::terminal::lock_terminal_grid(&self.ide_panel.terminals[idx].grid)
            .presentation_ready;
        if ready {
            self.ide_panel.active_terminal = idx;
            self.reveal_active_terminal_tab_now();
        } else {
            self.ide_panel.terminals[idx].presentation_intent =
                crate::app::terminal::TerminalPresentationIntent::ActivateWhenReady;
        }
    }

    pub(crate) fn close_terminal_tab_at(&mut self, idx: usize) {
        if idx >= self.ide_panel.terminals.len() {
            return;
        }
        let active = self.ide_panel.active_terminal;
        self.ide_panel.terminals.remove(idx);
        if self.ide_panel.terminals.is_empty() {
            self.add_terminal();
        } else {
            self.ide_panel.active_terminal = crate::app::active_index_after_remove(
                active,
                idx,
                self.ide_panel.terminals.len(),
            );
            self.reveal_active_terminal_tab_now();
        }
        self.defer_terminal_panel_until_ready();
    }

    pub(crate) fn add_terminal(&mut self) -> usize {
        let reveal_panel_when_ready = (self.ide_panel.is_open(PanelId::Terminal)
            && !self.active_terminal_presentation_ready())
            || self.ide_panel.terminals.iter().any(|terminal| {
                terminal.presentation_intent
                    == crate::app::terminal::TerminalPresentationIntent::OpenPanelWhenReady
            });
        self.cancel_terminal_presentation_intents();

        let cwd = self.terminal_working_directory();
        let display_number =
            take_terminal_creation_number(&mut self.ide_panel.next_terminal_creation_number);
        let mut terminal = crate::app::terminal::Terminal::spawn(
            self.window.clone(),
            cwd.as_deref(),
            display_number,
        );
        terminal.presentation_intent = if reveal_panel_when_ready {
            crate::app::terminal::TerminalPresentationIntent::OpenPanelWhenReady
        } else {
            crate::app::terminal::TerminalPresentationIntent::ActivateWhenReady
        };
        terminal.reveal_right_tail_when_presented = true;
        let idx = self.ide_panel.terminals.len();
        self.ide_panel.terminals.push(terminal);
        if reveal_panel_when_ready {
            self.hide_terminal_panel_for_pending_presentation();
        }
        self.process_terminal_presentation_intents();
        idx
    }

    pub(crate) fn reveal_active_terminal_tab_now(&mut self) {
        self.reveal_active_terminal_tab_now_for_presentation(false);
    }

    fn reveal_active_terminal_tab_now_for_presentation(&mut self, reveal_right_tail: bool) {
        let idx = self.ide_panel.active_terminal;
        if idx >= self.ide_panel.terminals.len() {
            return;
        }
        let Some(s) = self.renderer.as_ref().map(|renderer| renderer.scale_factor) else {
            return;
        };
        let (_, _, panel_w, _, _) = crate::app::mouse::app_panel_scroll_rect(
            self,
            crate::app::PanelId::Terminal,
            s,
        );
        if panel_w <= 0.0 {
            return;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        let mut title = String::new();
        let mut widths = Vec::with_capacity(self.ide_panel.terminals.len());
        for terminal in &self.ide_panel.terminals {
            terminal.write_display_title(&mut title);
            widths.push(crate::render_view::terminal_ui::terminal_tab_width_from_title_width(
                renderer.measure_ui_width(&title, 1.0),
                s,
            ));
        }
        let add_size = crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, s);
        widths.push(8.0 * s + add_size + 8.0 * s);
        let target = terminal_tab_reveal_target(
            &widths,
            idx,
            reveal_right_tail,
            panel_w - 8.0 * s,
            self.ide_panel.terminal_tab_scroll.target,
            12.0 * s,
        );
        self.ide_panel.terminal_tab_scroll.jump_to(target);
    }

    pub(crate) fn shutdown_background_services(&mut self) {
        self.tool_installer.shutdown();
        for terminal in &mut self.ide_panel.terminals {
            terminal.shutdown();
        }
        if let Some(lsp) = self.lsp.take() {
            lsp.shutdown();
        }
        self.ide_panel.api.shutdown_background_tasks();
        self.shutdown_database_runtime();
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
            tool_paths: self.tool_paths.clone(),
            dart_settings: self.dart_settings.clone(),
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
    pub(crate) fn get_clipboard_file_list(&mut self) -> Option<Vec<PathBuf>> {
        self.clipboard.as_mut()?.get_file_list().ok()
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

        if self.is_automation_mode() {
            self.ide_panel = crate::app::IdePanelState::default();
        } else {
            self.ide_panel = crate::load_panel_state();
            self.ide_panel.api = crate::app::api_client::ApiClientState::load_persisted();
            self.load_database_panel_state();
        }
        self.ide_panel.enforce_single_open_per_group();

        if self.ide_panel.is_open(PanelId::Database) {
            self.reconcile_expanded_database_connections();
        }

        if self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {
            self.add_terminal();
        }

        if self.lsp.is_none() {
            let mut lsp = crate::lsp::LspManager::new(self.ide_workspaces.clone());
            lsp.set_dart_workspace_analysis_enabled(self.dart_settings.workspace_analysis);
            if !self.dart_settings.enabled {
                lsp.set_server_enabled("dart", false);
            }
            self.lsp = Some(lsp);
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
                closing_hints: Default::default(),
                kind: EditorTabKind::Normal,
            });
            self.active_tab = 0;
        }

        let (saved_tabs, saved_active) = if self.scroll_render_bench.is_some()
            || self.is_automation_mode()
        {
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
                    crate::OpenTabSnapshot::DatabaseTable {
                        connection_id,
                        database_name,
                        table_name,
                    } => {
                        if self.ide_panel.database.connection(connection_id).is_some() {
                            self.open_database_table_tab(connection_id, &database_name, &table_name);
                            loaded_any = true;
                        }
                    }
                    crate::OpenTabSnapshot::DatabaseQuery {
                        connection_id,
                        database_name,
                        console_id,
                    } => {
                        if self.ide_panel.database.connection(connection_id).is_some() {
                            self.restore_database_query_tab(connection_id, &database_name, console_id);
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
                    crate::editor::lsp_document_version(self.editor.version),
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
        if !self.is_ide_mode || self.is_automation_mode() {
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
        std::mem::swap(
            &mut self.closing_hint_state,
            &mut self.tabs[ai].closing_hints,
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
            let tab_x = (48.0 * s + self.ide_panel.visible_left_width(s)).round() + 1.0;
            let viewport_w = (r.width - tab_x).max(0.0);
            if viewport_w <= 0.0 {
                return;
            }

            let widths = titles
                .iter()
                .map(|title| {
                    let title_w = r.measure_ui_width(title, 1.0);
                    16.0 * s * 2.0 + 20.0 * s + 8.0 * s + title_w + 30.0 * s
                })
                .collect::<Vec<_>>();
            let target = crate::render_view::tabs_ui::tab_strip_reveal_target(
                &widths,
                idx,
                viewport_w,
                self.tab_scroll.target,
                12.0 * s,
            );
            self.tab_scroll.jump_to(target);
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
            self.tab_scroll.reset();
            return;
        }

        let titles = self.tab_display_titles();
        let Some(r) = self.renderer.as_mut() else {
            self.tab_scroll.reset();
            return;
        };
        let s = r.scale_factor;
        let tab_x = (48.0 * s + self.ide_panel.visible_left_width(s)).round() + 1.0;
        let viewport_w = (r.width - tab_x).max(0.0);
        if viewport_w <= 0.0 {
            self.tab_scroll.reset();
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
        self.save_active_database_query();
        self.commit_api_focus();
        self.ide_panel.api.focused = None;
        self.sync_active_tab();
        self.active_tab = new_idx;
        self.sync_active_tab();
        self.prefetch_active_tab_git_graph();

        if self.active_tab_is_api_client() || self.active_tab_is_database_table() {
            while self.highlighter.rx.try_recv().is_ok() {}
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

        if self.is_ide_mode && !self.active_tab_is_api_client() && !self.active_tab_is_database() {
            if let Some(lsp) = &mut self.lsp {
                if let Some(path) = &self.file_path {
                    let text = self.editor.get_full_text();
                    lsp.notify_open(
                        path,
                        &self.file_extension,
                        &text,
                        crate::editor::lsp_document_version(self.editor.version),
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

    fn drag(start_idx: usize, start_x: f32, current_x: f32, threshold_passed: bool) -> TabDragState {
        TabDragState {
            start_idx,
            start_x,
            current_x,
            threshold_passed,
        }
    }

    #[test]
    fn shared_tab_drag_math_crosses_variable_width_centers() {
        let widths = [80.0, 140.0, 60.0, 110.0];
        let move_right = drag(1, 150.0, 350.0, true);
        let placement = tab_drag_placement(10.0, &widths, Some(&move_right)).unwrap();
        assert_eq!(placement.destination, 3);

        let move_left = drag(3, 335.0, 20.0, true);
        let placement = tab_drag_placement(10.0, &widths, Some(&move_left)).unwrap();
        assert_eq!(placement.destination, 0);
    }

    #[test]
    fn terminal_creation_routes_share_the_same_session_factory() {
        let app_tabs = include_str!("app_ide_tab_methods.rs");
        let app_production = app_tabs.split("\n#[cfg(test)]").next().unwrap_or(app_tabs);
        let ui_handlers = include_str!("ui_handlers.rs");
        let about = include_str!("events/about.rs");

        assert_eq!(app_production.matches("Terminal::spawn(").count(), 1);
        assert!(app_production.contains("take_terminal_creation_number("));
        assert!(app_production.contains("self.ide_panel.terminals.is_empty() {\n            self.add_terminal();"));
        assert!(ui_handlers.contains("UiId::TerminalAdd => {\n                self.add_terminal();"));
        assert!(about.matches("app.add_terminal();").count() >= 2);
    }

    #[test]
    fn terminal_creation_numbers_are_monotonic_and_survive_close_and_reorder() {
        let mut panels = crate::app::IdePanelState::default();
        let mut numbers = vec![
            take_terminal_creation_number(&mut panels.next_terminal_creation_number),
            take_terminal_creation_number(&mut panels.next_terminal_creation_number),
            take_terminal_creation_number(&mut panels.next_terminal_creation_number),
        ];
        assert_eq!(numbers, vec![1, 2, 3]);

        numbers.remove(1);
        assert_eq!(numbers, vec![1, 3]);
        numbers.push(take_terminal_creation_number(
            &mut panels.next_terminal_creation_number,
        ));
        assert_eq!(numbers, vec![1, 3, 4]);

        let moved = numbers.remove(1);
        numbers.insert(0, moved);
        assert_eq!(numbers, vec![3, 1, 4]);
    }

    #[test]
    fn terminal_tab_close_routes_share_one_app_lifecycle() {
        let app_tabs = include_str!("app_ide_tab_methods.rs");
        let app_production = app_tabs.split("\n#[cfg(test)]").next().unwrap_or(app_tabs);
        let close = app_production
            .split("    pub(crate) fn close_terminal_tab_at")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn add_terminal")
            .next()
            .unwrap();
        let ui_handlers = include_str!("ui_handlers.rs");
        let main_keys = include_str!("keyboard/main_keys.rs");
        let editor_keys = include_str!("keyboard/editor_keys.rs");

        assert!(close.contains("self.ide_panel.terminals.remove(idx);"));
        assert!(close.contains("self.add_terminal();"));
        assert!(close.contains("crate::app::active_index_after_remove("));
        assert!(close.contains("self.reveal_active_terminal_tab_now();"));
        assert!(close.contains("self.defer_terminal_panel_until_ready();"));
        assert!(!close.contains("Terminal::spawn("));

        let mouse_close = ui_handlers
            .split("UiId::TerminalTabClose(idx) => {")
            .nth(1)
            .unwrap()
            .split("UiId::TerminalAdd => {")
            .next()
            .unwrap();
        assert!(mouse_close.contains("self.close_terminal_tab_at(idx);"));
        assert!(!mouse_close.contains("terminals.remove("));

        let shortcut_close = main_keys
            .find("self.close_terminal_tab_at(self.ide_panel.active_terminal);")
            .unwrap();
        let search_dispatch = main_keys
            .find("self.handle_terminal_search_keyboard_input(key_event);")
            .unwrap();
        let terminal_dispatch = main_keys
            .find("self.handle_terminal_keyboard_input(key_event);")
            .unwrap();
        assert!(shortcut_close < search_dispatch);
        assert!(shortcut_close < terminal_dispatch);
        assert!(main_keys[shortcut_close..search_dispatch].contains("return;"));

        assert!(editor_keys.contains(
            "PhysicalKey::Code(KeyCode::Digit4) if ctrl => {\n                self.close_tab_at(self.active_tab);"
        ));
    }

    #[test]
    fn terminal_pending_activation_waits_for_parser_ready_and_latest_request_wins() {
        use crate::app::terminal::TerminalPresentationIntent as Intent;

        assert_eq!(
            ready_terminal_presentation([(0, Intent::None, true), (1, Intent::ActivateWhenReady, false)]),
            None
        );
        assert_eq!(
            ready_terminal_presentation([(0, Intent::None, true), (1, Intent::ActivateWhenReady, true)]),
            Some((1, Intent::ActivateWhenReady))
        );

        assert_eq!(
            ready_terminal_presentation([
                (0, Intent::None, true),
                (1, Intent::None, true),
                (2, Intent::ActivateWhenReady, false),
            ]),
            None
        );
        assert_eq!(
            ready_terminal_presentation([
                (0, Intent::None, true),
                (1, Intent::None, true),
                (2, Intent::ActivateWhenReady, true),
            ]),
            Some((2, Intent::ActivateWhenReady))
        );
    }

    #[test]
    fn terminal_panel_reveal_intent_uses_parser_readiness_not_layout_readiness() {
        use crate::app::terminal::TerminalPresentationIntent as Intent;

        assert_eq!(
            ready_terminal_presentation([(0, Intent::OpenPanelWhenReady, false)]),
            None
        );
        assert_eq!(
            ready_terminal_presentation([(0, Intent::OpenPanelWhenReady, true)]),
            Some((0, Intent::OpenPanelWhenReady))
        );
    }

    #[test]
    fn terminal_presentation_routes_share_one_non_blocking_lifecycle() {
        let app_tabs = include_str!("app_ide_tab_methods.rs");
        let app_production = app_tabs.split("\n#[cfg(test)]").next().unwrap_or(app_tabs);
        let lifecycle = app_production
            .split("    fn active_terminal_presentation_ready")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn shutdown_background_services")
            .next()
            .unwrap();
        let ui_handlers = include_str!("ui_handlers.rs");
        let main_keys = include_str!("keyboard/main_keys.rs");
        let about = include_str!("events/about.rs");
        let renderer = include_str!("../render_view/terminal_ui.rs");

        assert!(!lifecycle.contains("presentation_visible"));
        assert!(!lifecycle.contains("sleep("));
        assert!(lifecycle.contains("cancel_terminal_presentation_intents();"));
        assert!(lifecycle.contains("TerminalPresentationIntent::ActivateWhenReady"));
        assert!(lifecycle.contains("TerminalPresentationIntent::OpenPanelWhenReady"));
        assert!(lifecycle.contains("hide_terminal_panel_for_pending_presentation();"));
        assert!(lifecycle.contains("self.process_terminal_presentation_intents();"));
        assert!(!lifecycle.contains(
            "self.ide_panel.active_terminal = self.ide_panel.terminals.len().saturating_sub(1)"
        ));

        assert!(ui_handlers.contains(
            "UiId::TerminalTab(idx) => {\n                self.select_terminal_tab_from_user(idx);"
        ));
        assert!(ui_handlers.contains("self.defer_terminal_panel_until_ready();"));
        assert!(main_keys.contains("self.defer_terminal_panel_until_ready();"));
        assert!(about.contains("app.process_terminal_presentation_intents()"));
        assert!(about.matches("app.add_terminal();").count() >= 2);
        assert!(app_production.contains(
            "self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {\n            self.add_terminal();"
        ));

        let layout_ready = renderer.find("grid.mark_presentation_layout_ready();").unwrap();
        let visible = renderer.find("let presentation_visible = grid.presentation_visible();").unwrap();
        let rendered = renderer
            .find("let rendered_lines = if presentation_visible { total_lines } else { 0 };")
            .unwrap();
        assert!(layout_ready < visible && visible < rendered);
    }

    #[test]
    fn pending_terminal_panel_reveal_uses_current_panel_group() {
        let source = include_str!("app_ide_tab_methods.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        let process = production
            .split("    pub(crate) fn process_terminal_presentation_intents")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn defer_terminal_panel_until_ready")
            .next()
            .unwrap();

        let open = process
            .find("self.ide_panel.open(PanelId::Terminal);")
            .unwrap();
        let save = process.find("crate::save_panel_state(&self.ide_panel);").unwrap();
        assert!(open < save);
        assert!(!process.contains("open_terminal_exclusive"));
    }

    #[test]
    fn manual_terminal_selection_cancels_stale_auto_activation_before_switching() {
        let app_tabs = include_str!("app_ide_tab_methods.rs");
        let app_production = app_tabs.split("\n#[cfg(test)]").next().unwrap_or(app_tabs);
        let selection = app_production
            .split("    pub(crate) fn select_terminal_tab_from_user")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn add_terminal")
            .next()
            .unwrap();

        let cancel = selection.find("self.cancel_terminal_presentation_intents();").unwrap();
        let ready = selection.find(".presentation_ready;").unwrap();
        assert!(cancel < ready);
        assert!(selection.contains("self.ide_panel.active_terminal = idx;"));
        assert!(selection.contains("TerminalPresentationIntent::ActivateWhenReady"));
    }

    #[test]
    fn terminal_tab_reveal_uses_shared_panel_geometry() {
        let source = include_str!("app_ide_tab_methods.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        let reveal = production
            .split("pub(crate) fn reveal_active_terminal_tab_now")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn shutdown_background_services").next())
            .expect("terminal tab reveal method must remain present");

        assert!(reveal.contains("crate::app::mouse::app_panel_scroll_rect("));
        assert!(reveal.contains("crate::app::PanelId::Terminal"));
        assert!(!reveal.contains("renderer.width - 48.0 * s"));
    }

    #[test]
    fn new_terminal_reveal_targets_plus_tail_at_fractional_scales() {
        for scale in [1.0, 1.32, 1.333_333_3] {
            let panel_x = 20.0;
            let panel_w = 260.0;
            let tab_widths = [30.0, 50.0, 80.0].map(|title_w| {
                crate::render_view::terminal_ui::terminal_tab_width_from_title_width(
                    title_w * scale,
                    scale,
                )
            });
            let logical_tabs_width = tab_widths.iter().sum::<f32>();
            let add_size =
                crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, scale);
            let mut widths_with_add = tab_widths.to_vec();
            widths_with_add.push(8.0 * scale + add_size + 8.0 * scale);
            let viewport_w = panel_w - 8.0 * scale;
            let max_scroll = crate::render_view::terminal_ui::terminal_tab_strip_max_scroll(
                panel_w,
                logical_tabs_width,
                add_size,
                scale,
            );

            let target = terminal_tab_reveal_target(
                &widths_with_add,
                tab_widths.len() - 1,
                true,
                viewport_w,
                0.0,
                12.0 * scale,
            );
            assert!((target - max_scroll).abs() < 0.001);

            let base_x = crate::render_view::terminal_ui::terminal_tab_base_x(
                panel_x,
                target,
                max_scroll,
                scale,
            );
            let last_left = base_x + tab_widths[..tab_widths.len() - 1].iter().sum::<f32>();
            let last_right = base_x + logical_tabs_width;
            let add_x = crate::render_view::terminal_ui::terminal_add_x_after_tabs(
                base_x,
                logical_tabs_width,
                scale,
            );
            let viewport_right = panel_x + panel_w;

            assert!(last_left >= panel_x - 0.5);
            assert!(last_right <= viewport_right + 0.5);
            assert!(add_x >= panel_x - 0.5);
            assert!(add_x + add_size <= viewport_right + 0.5);
        }
    }

    #[test]
    fn terminal_tail_reveal_stays_zero_without_overflow_and_old_tab_selection_stays_ordinary() {
        let scale = 1.0;
        let panel_w = 600.0;
        let add_size = crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, scale);
        let mut widths = vec![90.0, 110.0, 100.0];
        widths.push(8.0 * scale + add_size + 8.0 * scale);
        assert_eq!(
            terminal_tab_reveal_target(
                &widths,
                2,
                true,
                panel_w - 8.0 * scale,
                0.0,
                12.0 * scale,
            ),
            0.0
        );

        let panel_w = 220.0;
        let add_size = crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, scale);
        let mut widths = vec![100.0, 120.0, 130.0];
        widths.push(8.0 * scale + add_size + 8.0 * scale);
        let viewport_w = panel_w - 8.0 * scale;
        let tail_target = terminal_tab_reveal_target(
            &widths,
            2,
            true,
            viewport_w,
            0.0,
            12.0 * scale,
        );
        let first_tab_target = terminal_tab_reveal_target(
            &widths,
            0,
            false,
            viewport_w,
            tail_target,
            12.0 * scale,
        );

        assert!(tail_target > 0.0);
        assert_eq!(first_tab_target, 0.0);
    }

    #[test]
    fn terminal_tail_reveal_clamps_at_max_when_last_tab_is_wider_than_viewport() {
        let scale = 1.0;
        let panel_x = 20.0;
        let panel_w = 220.0;
        let tab_widths = [
            crate::render_view::terminal_ui::terminal_tab_width_from_title_width(34.0, scale),
            crate::render_view::terminal_ui::terminal_tab_width_from_title_width(320.0, scale),
        ];
        let logical_tabs_width = tab_widths.iter().sum::<f32>();
        let add_size = crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, scale);
        let mut widths_with_add = tab_widths.to_vec();
        widths_with_add.push(8.0 * scale + add_size + 8.0 * scale);
        let max_scroll = crate::render_view::terminal_ui::terminal_tab_strip_max_scroll(
            panel_w,
            logical_tabs_width,
            add_size,
            scale,
        );
        let target = terminal_tab_reveal_target(
            &widths_with_add,
            1,
            true,
            panel_w - 8.0 * scale,
            0.0,
            12.0 * scale,
        );
        assert!((target - max_scroll).abs() < 0.001);

        let base_x = crate::render_view::terminal_ui::terminal_tab_base_x(
            panel_x,
            target,
            max_scroll,
            scale,
        );
        let last_left = base_x + tab_widths[0];
        let last_right = base_x + logical_tabs_width;
        let add_x = crate::render_view::terminal_ui::terminal_add_x_after_tabs(
            base_x,
            logical_tabs_width,
            scale,
        );
        let viewport_right = panel_x + panel_w;

        assert!(last_left < panel_x);
        assert!(last_right >= panel_x && last_right <= viewport_right + 0.5);
        assert!(add_x >= panel_x && add_x + add_size <= viewport_right + 0.5);
    }

    #[test]
    fn new_terminal_tail_reveal_uses_the_existing_deferred_presentation_lifecycle() {
        let source = include_str!("app_ide_tab_methods.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        let add = production
            .split("    pub(crate) fn add_terminal")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn reveal_active_terminal_tab_now")
            .next()
            .unwrap();
        let process = production
            .split("    pub(crate) fn process_terminal_presentation_intents")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn defer_terminal_panel_until_ready")
            .next()
            .unwrap();
        let cancel = production
            .split("    fn cancel_terminal_presentation_intents")
            .nth(1)
            .unwrap()
            .split("    fn hide_terminal_panel_for_pending_presentation")
            .next()
            .unwrap();

        assert!(add.contains("terminal.reveal_right_tail_when_presented = true;"));
        let capture = process
            .find("let reveal_right_tail = self.ide_panel.terminals[idx].reveal_right_tail_when_presented")
            .unwrap();
        assert!(process.contains("&& idx + 1 == self.ide_panel.terminals.len();"));
        let cancel_intents = process.find("self.cancel_terminal_presentation_intents();").unwrap();
        let activate = process.find("self.ide_panel.active_terminal = idx;").unwrap();
        let reveal = process
            .find("self.reveal_active_terminal_tab_now_for_presentation(reveal_right_tail);")
            .unwrap();
        assert!(capture < cancel_intents && cancel_intents < activate && activate < reveal);
        assert!(cancel.contains("terminal.reveal_right_tail_when_presented = false;"));
    }

    #[test]
    fn deferred_terminal_activation_reveals_only_after_active_index_switch() {
        let source = include_str!("app_ide_tab_methods.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        let process = production
            .split("    pub(crate) fn process_terminal_presentation_intents")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn defer_terminal_panel_until_ready")
            .next()
            .unwrap();
        let activate = process.find("self.ide_panel.active_terminal = idx;").unwrap();
        let reveal = process
            .find("self.reveal_active_terminal_tab_now_for_presentation(reveal_right_tail);")
            .unwrap();
        assert!(activate < reveal);

        let selection = production
            .split("    pub(crate) fn select_terminal_tab_from_user")
            .nth(1)
            .unwrap()
            .split("    pub(crate) fn add_terminal")
            .next()
            .unwrap();
        assert!(selection.contains(
            "self.ide_panel.active_terminal = idx;\n            self.reveal_active_terminal_tab_now();"
        ));
    }

    #[test]
    fn shared_tab_drag_math_handles_first_last_and_single_tab_edges() {
        let widths = [100.0, 100.0, 100.0];
        let first_to_last = drag(0, 50.0, 350.0, true);
        assert_eq!(
            tab_drag_placement(0.0, &widths, Some(&first_to_last))
                .unwrap()
                .destination,
            2
        );

        let last_to_first = drag(2, 250.0, -50.0, true);
        assert_eq!(
            tab_drag_placement(0.0, &widths, Some(&last_to_first))
                .unwrap()
                .destination,
            0
        );

        let single = drag(0, 50.0, 200.0, true);
        assert_eq!(
            tab_drag_placement(0.0, &[100.0], Some(&single))
                .unwrap()
                .destination,
            0
        );
    }

    #[test]
    fn shared_tab_drag_math_keeps_pending_and_stale_drags_inert() {
        let widths = [90.0, 120.0, 70.0];
        assert!(
            tab_drag_placement(0.0, &widths, Some(&drag(1, 100.0, 104.0, false))).is_none()
        );
        assert!(
            tab_drag_placement(0.0, &widths, Some(&drag(9, 100.0, 250.0, true))).is_none()
        );
    }

    #[test]
    fn shared_tab_drag_layout_matches_file_tab_temporary_order() {
        let widths = [100.0, 60.0, 140.0];
        let drag = drag(0, 40.0, 235.0, true);
        let mut actual = Vec::new();
        let mut order = Vec::new();
        let dragged = tab_drag_layout(0.0, &widths, Some(&drag), &mut actual, &mut order);

        assert_eq!(dragged, Some(0));
        assert_eq!(order, vec![1, 2, 0]);
        assert_eq!(actual[1], 0.0);
        assert_eq!(actual[2], 60.0);
        assert_eq!(actual[0], 195.0);

        let mut render_order = Vec::new();
        tab_drag_render_order(&order, dragged, &mut render_order);
        assert_eq!(render_order, vec![1, 2, 0]);
    }

    #[test]
    fn terminal_active_index_helpers_preserve_identity_across_reorder_and_close() {
        let mut ids = vec!['a', 'b', 'c', 'd'];
        let mut active = 2;
        let moved = ids.remove(0);
        ids.insert(3, moved);
        active = active_index_after_move(active, 0, 3);
        assert_eq!(ids[active], 'c');

        ids.remove(0);
        active = active_index_after_remove(active, 0, ids.len());
        assert_eq!(ids[active], 'c');
    }

    #[test]
    fn terminal_background_close_preserves_ready_pending_session_identity() {
        use crate::app::terminal::TerminalPresentationIntent as Intent;

        let mut ids = vec!['a', 'b', 'c'];
        let (mut active, _) = ready_terminal_presentation([
            (0, Intent::None, true),
            (1, Intent::ActivateWhenReady, true),
            (2, Intent::None, true),
        ])
        .unwrap();
        assert_eq!(ids[active], 'b');

        ids.remove(0);
        active = active_index_after_remove(active, 0, ids.len());
        assert_eq!(active, 0);
        assert_eq!(ids[active], 'b');

        let about = include_str!("events/about.rs");
        let cleanup = about
            .split("for idx in closed_terminals.into_iter().rev()")
            .nth(1)
            .unwrap()
            .split("app.defer_terminal_panel_until_ready();")
            .next()
            .unwrap();
        assert!(cleanup.contains("crate::app::active_index_after_remove("));
        assert!(!cleanup.contains("active_terminal >= app.ide_panel.terminals.len()"));
    }

    #[test]
    fn terminal_background_close_keeps_active_identity_for_all_relative_removals() {
        let mut ids = vec!['a', 'b', 'c', 'd'];
        let mut active = 2;

        ids.remove(1);
        active = active_index_after_remove(active, 1, ids.len());
        ids.remove(0);
        active = active_index_after_remove(active, 0, ids.len());
        assert_eq!(ids[active], 'c');

        let mut ids = vec!['a', 'b', 'c'];
        let mut active = 0;
        ids.remove(2);
        active = active_index_after_remove(active, 2, ids.len());
        assert_eq!(ids[active], 'a');

        let mut ids = vec!['a', 'b', 'c'];
        let mut active = 1;
        ids.remove(1);
        active = active_index_after_remove(active, 1, ids.len());
        assert_eq!(ids[active], 'c');
    }

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
            closing_hints: Default::default(),
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
