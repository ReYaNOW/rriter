use crate::app::App;
use crate::render_view::editor_scroll_content_height;
use crate::render_view::minimap_ui::{minimap_thumb_height, minimap_view_metrics};
use crate::renderer::VisualLine;
use crate::ui_system::UiId;
use super::UiClickFlow;

/// Press on the editor horizontal bar: `(grab_offset, target)`. Input only needs the
/// along-axis span, so the lane has no height.
fn scrollbar_x_click_target(
    mouse_x: f32,
    track_x: f32,
    track_w: f32,
    current_scroll: f32,
    max_scroll: f32,
    scale: f32,
) -> Option<(f32, f32)> {
    crate::render_view::editor_horizontal_scrollbar(
        (track_x, 0.0, track_w, 0.0),
        max_scroll,
        current_scroll,
    )
    .geometry(scale)?
    .press_target(mouse_x)
}

pub(crate) fn repeated_ui_click(
    same_target: bool,
    elapsed: std::time::Duration,
    dx: f32,
    dy: f32,
) -> bool {
    same_target && elapsed < std::time::Duration::from_millis(400) && dx * dx + dy * dy < 25.0
}

fn content_y_hits_visual_text_row(
    content_y: f32,
    line_height: f32,
    visual_lines: &[VisualLine],
) -> bool {
    visual_lines
        .iter()
        .any(|line| content_y >= line.y_offset && content_y < line.y_offset + line_height)
}

fn editor_interaction_view_height(
    window_height: f32,
    tab_bar_height: f32,
    panel_bottom_height: f32,
    query_results_height: f32,
    is_ide_mode: bool,
    scale: f32,
) -> f32 {
    let reserved_bottom_height = if is_ide_mode {
        panel_bottom_height + query_results_height
    } else {
        0.0
    };
    crate::render_view::editor_view_height(
        window_height,
        tab_bar_height,
        reserved_bottom_height,
        is_ide_mode,
        scale,
    )
}

impl App {
    fn editor_interaction_bottom_heights(&self, window_height: f32, scale: f32) -> (f32, f32) {
        if !self.is_ide_mode {
            return (0.0, 0.0);
        }
        let panel_bottom_height = self.ide_panel.editor_reserved_bottom_height(scale);
        let open_panel_height = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * scale
        } else {
            0.0
        };
        let query_results_height =
            self.tabs
                .get(self.active_tab)
                .map_or(0.0, |tab| match &tab.kind {
                    crate::app::EditorTabKind::DatabaseQuery(_, state)
                        if crate::app::database::database_query_results_visible(state) =>
                    {
                        crate::app::database::database_query_results_height(
                            state.result_view.preferred_height,
                            window_height,
                            open_panel_height,
                            scale,
                        )
                    }
                    _ => 0.0,
                });
        (panel_bottom_height, query_results_height)
    }

    fn editor_interaction_metrics(&self) -> Option<(f32, f32, f32, f32)> {
        let renderer = self.renderer.as_ref()?;
        let window_height = self
            .window
            .as_ref()
            .map_or(renderer.height, |window| window.inner_size().height as f32);
        let scale = renderer.scale_factor;
        let (panel_bottom_height, query_results_height) =
            self.editor_interaction_bottom_heights(window_height, scale);
        Some((
            window_height,
            scale,
            panel_bottom_height,
            query_results_height,
        ))
    }
}

impl App {
    pub(super) fn handle_editor_ui_click(&mut self, id: UiId, same_click_target: bool) -> UiClickFlow {
        match id {

            // Tabs
            UiId::EditorTab(idx) => {
                self.switch_to_tab(idx);
            }
            UiId::EditorTabClose(idx) => {
                self.close_tab_at(idx);
            }

            // Editor
            UiId::EditorFoldArrow(phys_idx) => {
                if self.editor.folded_lines.contains(&phys_idx) {
                    self.editor.folded_lines.remove(&phys_idx);
                    self.editor
                        .folded_start_bytes
                        .remove(&self.editor.line_offsets[phys_idx]);
                } else if self.editor.foldable_lines.contains_key(&phys_idx) {
                    self.editor.folded_lines.insert(phys_idx);
                    self.editor
                        .folded_start_bytes
                        .insert(self.editor.line_offsets[phys_idx]);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorGitHunk(hunk_idx, clicked_line) => {
                self.show_inline_git_hunk_popup(hunk_idx, clicked_line.saturating_add(1));
            }
            UiId::InlineGitPrevHunk => {
                self.jump_inline_git_hunk(-1);
            }
            UiId::InlineGitNextHunk => {
                self.jump_inline_git_hunk(1);
            }
            UiId::InlineGitRollbackHunk => {
                self.rollback_inline_git_hunk();
            }
            UiId::GitDiffRollbackHunk(tab_idx, hunk_idx) => {
                if tab_idx == self.active_tab {
                    self.rollback_active_git_diff_hunk(hunk_idx);
                }
            }
            UiId::GitDiffPrevHunk => {
                self.jump_active_git_diff_hunk(-1);
            }
            UiId::GitDiffNextHunk => {
                self.jump_active_git_diff_hunk(1);
            }
            UiId::EditorFoldDots(phys_idx) => {
                self.editor.folded_lines.remove(&phys_idx);
                self.editor
                    .folded_start_bytes
                    .remove(&self.editor.line_offsets[phys_idx]);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::StickyLine(target_byte, slot_index) => {
                if self.active_document_is_markdown() {
                    self.markdown
                        .mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
                }
                self.editor.cursor = target_byte;
                self.editor.selection_anchor = None;
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let phys_line = self
                        .editor
                        .line_offsets
                        .partition_point(|&o| o <= target_byte)
                        .saturating_sub(1);
                    let visual_line = r
                        .phys_to_visual
                        .get(phys_line)
                        .copied()
                        .unwrap_or(phys_line);
                    let line_y = visual_line as f32 * r.line_height;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);
                    let ry = slot_index as f32 * r.line_height;
                    let padding = r.line_height * 3.0;
                    self.scroll_y.animate_to(
                        (line_y - ry - padding)
                            .max(0.0)
                            .clamp(0.0, max_scroll)
                            .round(),
                    );
                    self.scroll_y.anim_speed = 15.0;
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorScrollbarY => {
                if self.active_document_is_markdown() {
                    self.markdown
                        .mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
                }
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    self.scroll_y.is_dragging = true;
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);
                    let scrollbar_w = 10.0 * s;
                    let geometry = crate::render_view::editor_vertical_scrollbar(
                        (
                            r.width - r.minimap_width - scrollbar_w,
                            tab_bar_h,
                            scrollbar_w,
                            editor_height,
                        ),
                        editor_scroll_content_height(
                            self.editor.get_visible_lines_count(),
                            r.line_height,
                            editor_height,
                        ),
                        max_scroll,
                        self.scroll_y.current,
                    )
                    .geometry(s);
                    let pressed = geometry
                        .and_then(|g| Some((g.press_target(my)?, g.on_thumb(my))));

                    match pressed {
                        Some(((grab_offset, _), true)) => {
                            self.scroll_y.drag_offset = grab_offset;
                            self.last_click_time = std::time::Instant::now();
                        }
                        Some(((grab_offset, target), false)) => {
                            // Track click: jump now; the drag branch then follows at once.
                            self.scroll_y.drag_offset = grab_offset;
                            self.scroll_y.target = target.round();
                            self.scroll_y.anim_speed = 15.0;
                            self.last_click_time =
                                std::time::Instant::now() - std::time::Duration::from_millis(200);
                        }
                        None => self.last_click_time = std::time::Instant::now(),
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorMinimap => {
                if self.active_document_is_markdown() {
                    self.markdown
                        .mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
                }
                self.scroll_y.is_dragging = true;
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);
                    self.last_click_time =
                        std::time::Instant::now() - std::time::Duration::from_millis(200);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);

                    if max_scroll > 0.0 {
                        // Same geometry as `draw_minimap`, so the click targets the line
                        // drawn under the pointer.
                        let minimap = minimap_view_metrics(
                            r.minimap_total_visual_lines(&self.editor),
                            editor_height,
                            r.line_height,
                            self.scroll_y.current.min(max_scroll),
                            max_scroll,
                        );
                        let minimap_line_h = minimap.line_height;

                        let scroll_ratio_y = (self.scroll_y.current / max_scroll).clamp(0.0, 1.0);
                        let thumb_h =
                            minimap_thumb_height(editor_height, r.line_height, minimap_line_h);
                        let viewport_y = tab_bar_h + scroll_ratio_y * (editor_height - thumb_h);

                        if my >= viewport_y && my <= viewport_y + thumb_h {
                            self.scroll_y.drag_offset = my - viewport_y;
                        } else {
                            let minimap_y = my - tab_bar_h;
                            let abs_minimap_y = minimap_y + minimap.scroll;
                            let target_line = abs_minimap_y / minimap_line_h;

                            let target_scroll = target_line * r.line_height - editor_height / 2.0;
                            let clamped_scroll = target_scroll.clamp(0.0, max_scroll).round();

                            self.scroll_y.target = clamped_scroll;
                            self.scroll_y.anim_speed = 15.0;

                            let target_ratio = clamped_scroll / max_scroll;
                            let thumb_visual_y = target_ratio * (editor_height - thumb_h);
                            self.scroll_y.drag_offset = my - tab_bar_h - thumb_visual_y;
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorScrollbarX => {
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                let started = if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let mx = r.last_mouse_x;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_y = r.get_max_scroll(&self.editor, editor_height);
                    let scrollbar_w = if max_y > 0.0 { 10.0 * s } else { 0.0 };
                    let track_x = r.left_padding;
                    let track_w = r.width - r.minimap_width - scrollbar_w - track_x;
                    scrollbar_x_click_target(
                        mx,
                        track_x,
                        track_w,
                        self.scroll_x.current,
                        r.max_scroll_x,
                        s,
                    )
                } else {
                    None
                };
                if let Some((drag_offset, target)) = started {
                    let _ = crate::app::mouse::apply_scrollbar_drag_target(
                        &mut self.scroll_x,
                        target.round(),
                        drag_offset,
                    );
                } else {
                    self.scroll_x.end_drag();
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::EditorTextBody => {
                let database_query_tab = self.active_tab_is_database_query();
                if let Some(r) = self.renderer.as_mut() {
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        r.scale_factor,
                    );
                    let content_y = r.last_mouse_y - tab_bar_h + self.scroll_y.current;
                    if !content_y_hits_visual_text_row(content_y, r.line_height, &r.visual_lines) {
                        self.is_dragging = false;
                        self.window.as_ref().unwrap().request_redraw();
                        return UiClickFlow::Return;
                    }

                    r.suppress_popups_until_next_mouse_move();
                }
                self.is_dragging = false;
                self.is_editor_drag_pending = true;
                self.focus_document_text_surface();
                crate::app::mouse::clear_hover_popup(&mut self.hover);
                self.scroll_y.anim_speed = 15.0;
                self.scroll_y.stop_anim();

                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let now = std::time::Instant::now();
                    let dx = mx - self.last_click_pos.0;
                    let dy = my - self.last_click_pos.1;
                    if repeated_ui_click(
                        same_click_target,
                        now.duration_since(self.last_click_time),
                        dx,
                        dy,
                    ) {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }

                    self.last_click_time = now;
                    self.last_click_pos = (mx, my);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        r.scale_factor,
                    );
                    self.editor.set_cursor_at_pos(
                        mx,
                        my - tab_bar_h + self.scroll_y.current,
                        r,
                        true,
                    );
                }

                if self.click_count == 2 {
                    self.editor.select_word();
                } else if self.click_count >= 3 {
                    self.editor.select_line();
                    self.click_count = 3;
                }
                self.window.as_ref().unwrap().request_redraw();
            }

            // Panels
            UiId::BottomPanelBody => {
                // Поглощаем клик — непрозрачная панель блокирует взаимодействие с редактором под ней
            }
            UiId::ResizeLeft => {
                // Блокируем resize, когда терминал в фокусе
                if !self.ide_panel.terminal_focused {
                    self.ide_panel.is_resizing_left = true;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::ResizeBottom => {
                // Блокируем resize, когда терминал в фокусе
                if !self.ide_panel.terminal_focused {
                    self.ide_panel.is_resizing_bottom = true;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            _ => return UiClickFlow::NotMine,
        }
        UiClickFlow::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_x_click_target_drags_thumb_or_jumps_to_pointer() {
        let on_thumb = scrollbar_x_click_target(120.0, 100.0, 400.0, 0.0, 800.0, 1.0)
            .expect("x scrollbar visible");
        assert_eq!(on_thumb, (20.0, 0.0));

        let jump = scrollbar_x_click_target(420.0, 100.0, 400.0, 0.0, 800.0, 1.0)
            .expect("x scrollbar visible");
        assert!(jump.0 > 0.0);
        assert!(jump.1 > 0.0);
        assert!(jump.1 <= 800.0);

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 18.0;
        scroll.target = 18.0;
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            jump.1.round(),
            jump.0,
        ));
        assert_eq!(scroll.current, 18.0);
        assert_eq!(scroll.target, jump.1.round());
        assert_ne!(scroll.current, scroll.target);
        assert_eq!(scroll.drag_offset, jump.0);
        assert_eq!(scroll.anim_speed, 15.0);

        assert!(scrollbar_x_click_target(120.0, 100.0, 400.0, 0.0, 0.0, 1.0).is_none());
    }
    #[test]
    fn editor_pointer_viewport_matches_rendered_bottom_reservations() {
        let viewport = editor_interaction_view_height(1000.0, 40.0, 300.0, 160.0, true, 1.0);
        assert_eq!(
            viewport,
            1000.0 - 40.0 - 300.0 - 160.0 - crate::render_view::ide_status_bar_height(1.0)
        );

        assert_eq!(
            editor_interaction_view_height(1000.0, 40.0, 300.0, 160.0, false, 1.0),
            960.0
        );
    }
    #[test]
    fn repeated_click_requires_the_same_ui_target() {
        let elapsed = std::time::Duration::from_millis(100);
        assert!(repeated_ui_click(true, elapsed, 2.0, 1.0));
        assert!(!repeated_ui_click(false, elapsed, 0.0, 0.0));
        assert!(!repeated_ui_click(
            true,
            std::time::Duration::from_millis(400),
            0.0,
            0.0,
        ));
        assert!(!repeated_ui_click(true, elapsed, 5.0, 0.0));
    }
    #[test]
    fn editor_text_row_hit_test_rejects_blank_viewport_edges() {
        let lines = [
            VisualLine {
                byte_idx: 0,
                physical_line: 1,
                is_soft_wrap: false,
                whitespace_px_width: 0.0,
                text_px_width: 10.0,
                y_offset: 24.0,
                is_folded: false,
                fold_suffix: ['\0'; 4],
                fold_suffix_len: 0,
            },
            VisualLine {
                byte_idx: 4,
                physical_line: 2,
                is_soft_wrap: false,
                whitespace_px_width: 0.0,
                text_px_width: 10.0,
                y_offset: 48.0,
                is_folded: false,
                fold_suffix: ['\0'; 4],
                fold_suffix_len: 0,
            },
        ];

        assert!(!content_y_hits_visual_text_row(12.0, 24.0, &lines));
        assert!(content_y_hits_visual_text_row(24.0, 24.0, &lines));
        assert!(content_y_hits_visual_text_row(71.9, 24.0, &lines));
        assert!(!content_y_hits_visual_text_row(72.0, 24.0, &lines));
    }
}
