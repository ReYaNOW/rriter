use crate::app::App;
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

mod cursor;
mod input;
mod wheel;
#[derive(Debug, Clone)]
pub struct HoverPopup {
    pub text: String,
    pub spans: Vec<crate::highlighter::ColorSpan>,
    pub line_kinds: Vec<crate::lsp::HoverLineKindPublic>,
    pub inline_code_ranges: Vec<(usize, usize)>,
    pub byte_offset: usize,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub anim_progress: f32,
    pub scroll: crate::scroll::ScrollState,
    pub layout_cache: Option<HoverLayoutCache>,
}

#[derive(Debug, Clone)]
pub struct HoverVisualLine {
    pub glyphs: Vec<(char, [f32; 4], usize)>,
    pub kind: crate::lsp::HoverLineKindPublic,
}

#[derive(Debug, Clone)]
pub struct HoverLayoutCache {
    pub scale_factor: f32,
    pub max_text_w: f32,
    pub span_count: usize,
    pub text_len: usize,
    pub lines: Vec<HoverVisualLine>,
    pub max_line_w: f32,
    pub total_text_h: f32,
}

pub type DiagnosticPopupRect = (f32, f32, f32, f32, f32, f32, f32);
pub type HoveredDiagnostic = (usize, f32, f32, f32, f32);

pub struct HoverState {
    pub request_id: Option<i32>,
    pub definition_request_id: Option<i32>,
    pub popup: Option<HoverPopup>,
    pub pending_popup: Option<HoverPopup>,
    pub timer: f32,
    pub byte_offset: Option<usize>,
    pub rect: Option<(f32, f32, f32, f32)>,
    pub max_scroll: f32,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
    pub selecting: bool,
    pub diag_selection_anchor: Option<usize>,
    pub diag_selection_cursor: Option<usize>,
    pub diag_selecting: bool,
    pub diag_rect: Option<DiagnosticPopupRect>,
    pub diag_scroll: crate::scroll::ScrollState,
    pub diag_max_scroll: f32,
    pub diag_hover_timer: f32,
    pub diag_hover_timer_idx: Option<usize>,
    pub hovered_diags: Vec<usize>,
    pub hovered_diags_cache: Vec<HoveredDiagnostic>,
    pub stale_hovered_diags_cache: Vec<HoveredDiagnostic>,
    pub hovered_diag_type_target: Option<usize>,
    pub popup_diag_type_target: Option<usize>,
    pub stale_combined_popup: bool,
    pub diag_hover_ready_after_stale: bool,
    pub diag_anim_progress: f32,
    pub diag_text: String,
    pub diag_href: Option<String>,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            request_id: None,
            definition_request_id: None,
            popup: None,
            pending_popup: None,
            timer: 0.0,
            byte_offset: None,
            rect: None,
            max_scroll: 0.0,
            selection_anchor: None,
            selection_cursor: None,
            selecting: false,
            diag_selection_anchor: None,
            diag_selection_cursor: None,
            diag_selecting: false,
            diag_rect: None,
            diag_scroll: crate::scroll::ScrollState::new(15.0),
            diag_max_scroll: 0.0,
            diag_hover_timer: 0.0,
            diag_hover_timer_idx: None,
            hovered_diags: Vec::new(),
            hovered_diags_cache: Vec::with_capacity(16),
            stale_hovered_diags_cache: Vec::with_capacity(16),
            hovered_diag_type_target: None,
            popup_diag_type_target: None,
            stale_combined_popup: false,
            diag_hover_ready_after_stale: false,
            diag_anim_progress: 0.0,
            diag_text: String::new(),
            diag_href: None,
        }
    }
}

impl HoverState {
    pub fn reset_diagnostic_popup(&mut self) {
        self.diag_rect = None;
        self.diag_scroll.target = 0.0;
        self.diag_scroll.current = 0.0;
        self.diag_max_scroll = 0.0;
        self.diag_hover_timer = 0.0;
        self.diag_hover_timer_idx = None;
        self.hovered_diags.clear();
        self.hovered_diags_cache.clear();
        self.stale_hovered_diags_cache.clear();
        self.hovered_diag_type_target = None;
        self.popup_diag_type_target = None;
        self.stale_combined_popup = false;
        self.diag_hover_ready_after_stale = false;
        self.diag_anim_progress = 0.0;
        self.diag_selection_anchor = None;
        self.diag_selection_cursor = None;
        self.diag_selecting = false;
        self.diag_text.clear();
        self.diag_href = None;
    }

    pub fn hide_diagnostic_popup_until_ready(&mut self) {
        if self.stale_combined_popup {
            return;
        }
        self.diag_rect = None;
        self.diag_scroll.target = 0.0;
        self.diag_scroll.current = 0.0;
        self.diag_max_scroll = 0.0;
        self.diag_anim_progress = 0.0;
        self.diag_selection_anchor = None;
        self.diag_selection_cursor = None;
        self.diag_selecting = false;
        self.diag_text.clear();
        self.diag_href = None;
    }

    pub fn advance_diagnostic_hover_timer(
        &mut self,
        first_diag_idx: Option<usize>,
        has_type_popup: bool,
        type_in_progress: bool,
        dt: f32,
    ) -> bool {
        if first_diag_idx != self.diag_hover_timer_idx {
            self.diag_hover_timer_idx = first_diag_idx;
            if self.diag_hover_ready_after_stale && first_diag_idx.is_some() {
                self.diag_hover_timer = 0.2;
                self.diag_hover_ready_after_stale = false;
            } else {
                self.diag_hover_timer = 0.0;
                if first_diag_idx.is_some() {
                    self.diag_hover_ready_after_stale = false;
                }
            }
        } else if first_diag_idx.is_some() || has_type_popup || type_in_progress {
            self.diag_hover_timer += dt;
        }

        self.diag_hover_timer >= 0.2
    }

    pub fn take_type_popup_for_draw(
        &mut self,
        show_combined: bool,
    ) -> (
        Option<HoverPopup>,
        Option<(usize, usize)>,
        Option<(f32, f32, f32, f32)>,
    ) {
        let selection = match (self.selection_anchor, self.selection_cursor) {
            (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
            _ => None,
        };
        let attached_diag = if show_combined {
            self.diag_rect
                .map(|(rx, ry, rw, rh, _, _, _)| (rx, ry, rw, rh))
        } else {
            None
        };
        (self.popup.take(), selection, attached_diag)
    }

    pub fn put_type_popup_after_draw(
        &mut self,
        popup: Option<HoverPopup>,
        rect: Option<(f32, f32, f32, f32)>,
        max_scroll: f32,
    ) {
        if popup.is_none() {
            if let Some(p) = self.popup.as_mut() {
                p.anim_progress = 0.0;
            }
        }
        self.popup = popup;
        self.rect = rect;
        self.max_scroll = max_scroll;
    }

    pub fn should_show_stale_popup_while_target_loads(&self, show_type: bool) -> bool {
        let popup_byte = self.popup.as_ref().map(|p| p.byte_offset);
        let stale_diagnostic_popup = self
            .combined_type_target()
            .is_some_and(|target| popup_byte == Some(target) && self.byte_offset != Some(target));
        self.popup.is_some()
            && !show_type
            && !self.stale_combined_popup
            && popup_byte != self.byte_offset
            && !stale_diagnostic_popup
    }

    pub fn begin_type_hover_transition(&mut self, byte_offset: usize) -> bool {
        let popup_has_diagnostic_context = self.combined_type_target().is_some();
        let old_popup_is_different = self
            .popup
            .as_ref()
            .is_some_and(|popup| popup.byte_offset != byte_offset);
        if popup_has_diagnostic_context && old_popup_is_different {
            self.stale_combined_popup = true;
            self.popup_diag_type_target = self.combined_type_target();
            self.stale_hovered_diags_cache.clear();
            self.stale_hovered_diags_cache
                .extend_from_slice(&self.hovered_diags_cache);
        }
        let keep_visible_popup = self.popup.is_some();
        self.byte_offset = Some(byte_offset);
        self.timer = 0.0;
        self.request_id = None;
        self.definition_request_id = None;
        self.pending_popup = None;
        self.selection_anchor = None;
        self.selection_cursor = None;
        self.selecting = false;
        if !keep_visible_popup {
            self.popup = None;
            self.rect = None;
        }
        !keep_visible_popup
    }

    pub fn record_hovered_diagnostic(
        &mut self,
        diagnostic: HoveredDiagnostic,
        type_target: Option<usize>,
    ) -> Option<usize> {
        if self.stale_combined_popup {
            return self.effective_hovered_diag_type_target(type_target);
        }
        if let Some(active_target) = self.combined_type_target() {
            if self
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == active_target)
                && type_target != Some(active_target)
            {
                return Some(active_target);
            }
        }
        if !self
            .hovered_diags_cache
            .iter()
            .any(|existing| existing.0 == diagnostic.0)
        {
            self.hovered_diags_cache.push(diagnostic);
        }
        type_target
    }

    pub fn diagnostic_popup_cache(&self) -> &[HoveredDiagnostic] {
        if self.stale_combined_popup && !self.stale_hovered_diags_cache.is_empty() {
            &self.stale_hovered_diags_cache
        } else {
            &self.hovered_diags_cache
        }
    }

    pub fn diagnostic_popup_cache_is_empty(&self) -> bool {
        self.diagnostic_popup_cache().is_empty()
    }

    pub fn has_active_combined_type_popup(&self) -> bool {
        self.combined_type_target().is_some_and(|target| {
            self.popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == target)
        })
    }

    pub fn combined_type_target(&self) -> Option<usize> {
        self.popup_diag_type_target
            .or(self.hovered_diag_type_target)
            .or_else(|| {
                if self.diag_rect.is_some() || !self.hovered_diags_cache.is_empty() {
                    self.popup.as_ref().map(|popup| popup.byte_offset)
                } else {
                    None
                }
            })
    }

    pub fn update_hovered_diag_type_target_for_frame(&mut self, type_target: Option<usize>) {
        if !self.stale_combined_popup && !(type_target.is_none() && self.has_active_combined_type_popup()) {
            self.hovered_diag_type_target = type_target;
        }
    }

    pub fn effective_hovered_diag_type_target(&self, type_target: Option<usize>) -> Option<usize> {
        if self.stale_combined_popup || (type_target.is_none() && self.has_active_combined_type_popup()) {
            self.combined_type_target()
        } else {
            type_target
        }
    }

    pub fn mark_type_popup_drawn(&mut self, show_combined: bool, type_target: Option<usize>) {
        if show_combined {
            self.popup_diag_type_target = type_target;
        } else if !self.stale_combined_popup
            && !self.has_active_combined_type_popup()
        {
            self.popup_diag_type_target = None;
        }
    }

    pub fn finish_stale_combined_transition(&mut self) {
        if self.stale_combined_popup {
            self.diag_hover_ready_after_stale = true;
            self.diag_rect = None;
            self.diag_scroll.target = 0.0;
            self.diag_scroll.current = 0.0;
            self.diag_max_scroll = 0.0;
            self.diag_anim_progress = 0.0;
            self.hovered_diags.clear();
            self.hovered_diags_cache.clear();
            self.stale_hovered_diags_cache.clear();
            self.hovered_diag_type_target = None;
            self.diag_selection_anchor = None;
            self.diag_selection_cursor = None;
            self.diag_selecting = false;
            self.diag_text.clear();
            self.diag_href = None;
        }
        self.stale_combined_popup = false;
        self.popup_diag_type_target = None;
    }

    pub fn should_keep_popup_through_empty_space(&self) -> bool {
        self.stale_combined_popup
            || self.popup_diag_type_target.is_some()
            || (self.popup.is_some()
                && (self.pending_popup.is_some()
                    || self.request_id.is_some()
                    || self.definition_request_id.is_some()))
    }

    pub fn keep_active_combined_popup_on_empty_space(&mut self) -> bool {
        let Some(target) = self.combined_type_target() else {
            return false;
        };
        if self.byte_offset.is_none() {
            self.byte_offset = self.popup.as_ref().map(|popup| popup.byte_offset).or(Some(target));
        }
        self.timer = 0.0;
        self.request_id = None;
        true
    }

    pub fn clear_type_popup_transition_markers(&mut self) {
        self.popup_diag_type_target = None;
        self.stale_combined_popup = false;
        self.stale_hovered_diags_cache.clear();
        self.diag_hover_ready_after_stale = false;
    }

    pub fn popup_or_bridge_contains(
        &self,
        px: f32,
        py: f32,
        viewport_w: f32,
        scale: f32,
    ) -> (bool, bool) {
        let mut inside = false;
        let mut source_line = false;

        if let Some((rx, ry, rw, rh, anchor_x_start, anchor_x_end, anchor_y)) = self.diag_rect {
            let anchor_x = (anchor_x_start + anchor_x_end) * 0.5;
            let line_top_y = anchor_y - 10.0 * scale;
            let line_bottom_y = anchor_y + 10.0 * scale;
            if is_in_hover_popup_or_bridge(
                px,
                py,
                (rx, ry, rw, rh),
                anchor_x,
                anchor_y,
                line_top_y,
                line_bottom_y,
                viewport_w,
                scale,
            ) {
                inside = true;
                source_line |= py >= line_top_y && py <= line_bottom_y;
            }
        }

        if let (Some((rx, ry, rw, rh)), Some(popup)) = (self.rect, self.popup.as_ref()) {
            let line_top_y = popup.anchor_y - 10.0 * scale;
            let line_bottom_y = popup.anchor_y + 10.0 * scale;
            if is_in_hover_popup_or_bridge(
                px,
                py,
                (rx, ry, rw, rh),
                popup.anchor_x,
                popup.anchor_y,
                line_top_y,
                line_bottom_y,
                viewport_w,
                scale,
            ) {
                inside = true;
                source_line |= py >= line_top_y && py <= line_bottom_y;
            }
        }

        (inside, source_line)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        advance_hover_anim_progress, compute_hover_visibility,
        compute_hover_visibility_from_matches, diagnostic_hover_byte_range_on_line,
        diagnostic_hover_range_on_line,
        diagnostic_hover_target_byte_on_line, hover_bytes_share_token, hover_token_bounds,
        hover_token_text, is_hover_target_byte, is_in_hover_popup_or_bridge,
        is_python_hover_keyword, normalize_hover_byte, HoverState,
    };

    #[test]
    fn hover_visibility_linter_error_only() {
        let (show_err, show_type, show_comb) = compute_hover_visibility(
            true,  // is_error_hovered
            true,  // error_timer_ready
            false, // has_type_popup
            None,  // hovered_diag_type_target
            None,  // type_popup_byte
            None,  // hover_byte_offset
            false, // stale_combined_popup
        );
        assert!(show_err);
        assert!(!show_type);
        assert!(!show_comb);
    }

    #[test]
    fn hover_visibility_type_only() {
        let (show_err, show_type, show_comb) = compute_hover_visibility(
            false,     // is_error_hovered
            false,     // error_timer_ready
            true,      // has_type_popup
            None,      // hovered_diag_type_target
            Some(100), // type_popup_byte
            Some(100), // hover_byte_offset
            false,     // stale_combined_popup
        );
        assert!(!show_err);
        assert!(show_type);
        assert!(!show_comb);
    }

    #[test]
    fn hover_visibility_combined() {
        let (show_err, show_type, show_comb) = compute_hover_visibility(
            true,      // is_error_hovered
            true,      // error_timer_ready
            true,      // has_type_popup
            Some(100), // hovered_diag_type_target
            Some(100), // type_popup_byte
            Some(100), // hover_byte_offset
            false,     // stale_combined_popup
        );
        assert!(show_err);
        assert!(show_type);
        assert!(show_comb);
    }

    #[test]
    fn hover_visibility_during_transition() {
        let (show_err, show_type, show_comb) = compute_hover_visibility(
            true,      // is_error_hovered
            true,      // error_timer_ready
            true,      // has_type_popup
            Some(200), // hovered_diag_type_target (new location)
            Some(100), // type_popup_byte (old location)
            Some(200), // hover_byte_offset (new location)
            false,     // stale_combined_popup
        );
        assert!(!show_err);
        assert!(!show_type);
        assert!(!show_comb);
    }

    #[test]
    fn hover_visibility_waits_for_matching_type_before_showing_combined_type() {
        let (show_err, show_type, show_comb) =
            compute_hover_visibility(true, false, true, Some(100), Some(100), Some(100), false);
        assert!(!show_err);
        assert!(!show_type);
        assert!(!show_comb);

        let (show_err, show_type, show_comb) =
            compute_hover_visibility(true, true, true, Some(100), Some(200), Some(100), false);
        assert!(!show_err);
        assert!(!show_type);
        assert!(!show_comb);

        let (show_err, show_type, show_comb) =
            compute_hover_visibility(true, true, true, Some(100), Some(100), Some(100), false);
        assert!(show_err);
        assert!(show_type);
        assert!(show_comb);
    }

    #[test]
    fn hover_animation_progress_uses_shared_slightly_slower_curve() {
        let next = advance_hover_anim_progress(0.0, 0.016);

        assert!((next - 0.192).abs() < 0.0001);
        assert!(next < 0.24);
        assert_eq!(advance_hover_anim_progress(0.995, 0.016), 1.0);
        assert_eq!(advance_hover_anim_progress(1.0, 0.016), 1.0);
    }

    #[test]
    fn hover_visibility_combines_offsets_inside_same_identifier() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    handlers\n");
        let text = editor.get_full_text();
        let handlers_start = text.find("handlers").unwrap();
        let handlers_middle = handlers_start + 7;

        assert!(hover_bytes_share_token(
            &editor,
            Some(handlers_start),
            Some(handlers_middle)
        ));

        let (show_err, show_type, show_comb) = compute_hover_visibility_from_matches(
            true,
            true,
            true,
            true,
            hover_bytes_share_token(&editor, Some(handlers_start), Some(handlers_middle)),
            hover_bytes_share_token(&editor, Some(handlers_start), Some(handlers_middle)),
            true,
            false,
        );

        assert!(show_err);
        assert!(show_type);
        assert!(show_comb);
    }

    #[test]
    fn hover_state_resets_all_diagnostic_popup_fields() {
        let mut state = HoverState::default();
        state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
        state.diag_scroll.current = 12.0;
        state.diag_scroll.target = 24.0;
        state.diag_max_scroll = 99.0;
        state.diag_hover_timer = 0.3;
        state.diag_hover_timer_idx = Some(7);
        state.hovered_diags.push(7);
        state.hovered_diags_cache.push((7, 1.0, 2.0, 3.0, 4.0));
        state.hovered_diag_type_target = Some(99);
        state.popup_diag_type_target = Some(99);
        state.stale_combined_popup = true;
        state.diag_hover_ready_after_stale = true;
        state.diag_anim_progress = 1.0;
        state.diag_selection_anchor = Some(1);
        state.diag_selection_cursor = Some(3);
        state.diag_selecting = true;
        state.diag_text.push_str("diagnostic");
        state.diag_href = Some("https://example.invalid".to_string());

        state.reset_diagnostic_popup();

        assert!(state.diag_rect.is_none());
        assert_eq!(state.diag_scroll.current, 0.0);
        assert_eq!(state.diag_scroll.target, 0.0);
        assert_eq!(state.diag_max_scroll, 0.0);
        assert_eq!(state.diag_hover_timer, 0.0);
        assert!(state.diag_hover_timer_idx.is_none());
        assert!(state.hovered_diags.is_empty());
        assert!(state.hovered_diags_cache.is_empty());
        assert!(state.hovered_diag_type_target.is_none());
        assert!(state.popup_diag_type_target.is_none());
        assert!(!state.stale_combined_popup);
        assert!(!state.diag_hover_ready_after_stale);
        assert_eq!(state.diag_anim_progress, 0.0);
        assert!(state.diag_selection_anchor.is_none());
        assert!(state.diag_selection_cursor.is_none());
        assert!(!state.diag_selecting);
        assert!(state.diag_text.is_empty());
        assert!(state.diag_href.is_none());
    }

    #[test]
    fn diagnostic_hover_timer_resets_when_hovered_diagnostic_changes() {
        let mut state = HoverState::default();

        assert!(!state.advance_diagnostic_hover_timer(Some(1), false, false, 0.21));
        assert_eq!(state.diag_hover_timer_idx, Some(1));
        assert_eq!(state.diag_hover_timer, 0.0);
        assert!(state.advance_diagnostic_hover_timer(Some(1), false, false, 0.21));

        assert!(!state.advance_diagnostic_hover_timer(Some(2), false, false, 0.21));
        assert_eq!(state.diag_hover_timer_idx, Some(2));
        assert_eq!(state.diag_hover_timer, 0.0);
    }

    #[test]
    fn diagnostic_hover_timer_keeps_ticking_during_type_popup_transition() {
        let mut state = HoverState::default();

        assert!(!state.advance_diagnostic_hover_timer(None, false, true, 0.11));
        assert!(state.advance_diagnostic_hover_timer(None, false, true, 0.10));

        state.reset_diagnostic_popup();
        assert!(!state.advance_diagnostic_hover_timer(None, true, false, 0.19));
        assert!(state.advance_diagnostic_hover_timer(None, true, false, 0.02));
    }

    #[test]
    fn pending_linter_popup_hide_does_not_reset_hover_timer_or_cache() {
        let mut state = HoverState::default();
        state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
        state.diag_scroll.current = 9.0;
        state.diag_scroll.target = 9.0;
        state.diag_max_scroll = 30.0;
        state.diag_hover_timer = 0.11;
        state.diag_hover_timer_idx = Some(3);
        state.hovered_diags.push(3);
        state
            .hovered_diags_cache
            .push((3, 100.0, 200.0, 220.0, 140.0));
        state.diag_anim_progress = 0.7;
        state.diag_selection_anchor = Some(1);
        state.diag_selection_cursor = Some(2);
        state.diag_selecting = true;
        state.diag_text.push_str("pending");
        state.diag_href = Some("https://example.invalid".to_string());

        state.hide_diagnostic_popup_until_ready();

        assert!(state.diag_rect.is_none());
        assert_eq!(state.diag_scroll.current, 0.0);
        assert_eq!(state.diag_scroll.target, 0.0);
        assert_eq!(state.diag_max_scroll, 0.0);
        assert_eq!(state.diag_hover_timer, 0.11);
        assert_eq!(state.diag_hover_timer_idx, Some(3));
        assert_eq!(state.hovered_diags, vec![3]);
        assert_eq!(
            state.hovered_diags_cache,
            vec![(3, 100.0, 200.0, 220.0, 140.0)]
        );
        assert_eq!(state.diag_anim_progress, 0.0);
        assert!(state.diag_selection_anchor.is_none());
        assert!(state.diag_selection_cursor.is_none());
        assert!(!state.diag_selecting);
        assert!(state.diag_text.is_empty());
        assert!(state.diag_href.is_none());
    }

    #[test]
    fn keyword_linter_popup_becomes_visible_after_delay_without_type_target() {
        let mut state = HoverState::default();
        state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
        state.hovered_diags.push(0);

        let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0);
        let (show_err, show_type, show_combined) =
            compute_hover_visibility(true, ready, false, None, None, None, false);
        assert!(!show_err);
        assert!(!show_type);
        assert!(!show_combined);

        state.hide_diagnostic_popup_until_ready();
        assert!(!state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0));
        let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21);
        let (show_err, show_type, show_combined) =
            compute_hover_visibility(true, ready, false, None, None, None, false);

        assert!(show_err);
        assert!(!show_type);
        assert!(!show_combined);
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 40.0, 60.0)]);
    }

    #[test]
    fn type_popup_draw_flow_does_not_hold_hover_state_borrow() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "type info".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 42,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.selection_anchor = Some(9);
        state.selection_cursor = Some(3);
        state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 1.0, 2.0, 3.0));

        let (mut popup, selection, attached_diag) = state.take_type_popup_for_draw(true);

        assert!(state.popup.is_none());
        assert_eq!(selection, Some((3, 9)));
        assert_eq!(attached_diag, Some((10.0, 20.0, 30.0, 40.0)));

        popup
            .as_mut()
            .expect("popup must be detached for draw")
            .offset_x = Some(5.0);
        state.put_type_popup_after_draw(popup, Some((1.0, 2.0, 3.0, 4.0)), 12.0);

        assert!(state.popup.is_some());
        assert_eq!(state.rect, Some((1.0, 2.0, 3.0, 4.0)));
        assert_eq!(state.max_scroll, 12.0);
        assert_eq!(state.popup.as_ref().and_then(|p| p.offset_x), Some(5.0));
    }

    #[test]
    fn stale_type_popup_stays_visible_while_new_target_loads() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old ValueError hover".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 17,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.byte_offset = Some(25);

        assert!(state.should_show_stale_popup_while_target_loads(false));
        assert!(!state.should_show_stale_popup_while_target_loads(true));

        state.byte_offset = None;
        assert!(state.should_show_stale_popup_while_target_loads(false));
    }

    #[test]
    fn stale_type_popup_stays_visible_when_cursor_moves_to_whitespace() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "some text".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 17,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.byte_offset = None;

        assert!(state.should_show_stale_popup_while_target_loads(false));
        assert!(!state.should_show_stale_popup_while_target_loads(true));
    }

    #[test]
    fn combined_popup_stays_visible_while_next_word_loads() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "Literal[\"513\"]".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 20,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.rect = Some((90.0, 80.0, 240.0, 120.0));
        state.diag_rect = Some((90.0, 210.0, 240.0, 80.0, 100.0, 130.0, 145.0));
        state.hovered_diags.push(0);
        state
            .hovered_diags_cache
            .push((0, 100.0, 140.0, 162.0, 130.0));
        state.hovered_diag_type_target = Some(17);
        state.popup_diag_type_target = Some(17);

        let should_reset_diagnostics = state.begin_type_hover_transition(7);

        assert!(!should_reset_diagnostics);
        assert_eq!(state.byte_offset, Some(7));
        assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(20));
        assert_eq!(state.rect, Some((90.0, 80.0, 240.0, 120.0)));
        assert_eq!(
            state.diag_rect,
            Some((90.0, 210.0, 240.0, 80.0, 100.0, 130.0, 145.0))
        );
        assert_eq!(
            state.hovered_diags_cache,
            vec![(0, 100.0, 140.0, 162.0, 130.0)]
        );
        assert!(state.stale_combined_popup);
        assert!(!state.should_show_stale_popup_while_target_loads(false));
    }

    #[test]
    fn combined_popup_stays_visible_when_diag_rect_was_cleared_before_transition() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "list[...]".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: Some(20.0),
            offset_y: Some(-80.0),
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.rect = Some((120.0, 40.0, 260.0, 120.0));
        state.diag_rect = None;
        state.hovered_diag_type_target = None;
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 90.0, 100.0, 126.0, 180.0));

        let should_reset_diagnostics = state.begin_type_hover_transition(3659);

        assert!(!should_reset_diagnostics);
        assert!(state.stale_combined_popup);
        assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(3717));
        assert_eq!(state.rect, Some((120.0, 40.0, 260.0, 120.0)));
        assert_eq!(state.effective_hovered_diag_type_target(Some(3659)), Some(3717));
        assert_eq!(state.record_hovered_diagnostic((1, 10.0, 20.0, 46.0, 60.0), Some(3659)), Some(3717));

        let (show_error, show_type, show_combined) =
            compute_hover_visibility(true, true, true, Some(3717), Some(3717), Some(3659), true);
        assert!(show_error);
        assert!(show_type);
        assert!(show_combined);
    }

    #[test]
    fn combined_popup_stays_visible_when_only_diag_cache_survived() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: Some(20.0),
            offset_y: Some(-80.0),
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.rect = Some((120.0, 40.0, 260.0, 120.0));
        state.diag_rect = None;
        state.hovered_diag_type_target = None;
        state.popup_diag_type_target = None;
        state.hovered_diags_cache.push((0, 90.0, 100.0, 126.0, 180.0));

        state.mark_type_popup_drawn(false, None);
        let should_reset_diagnostics = state.begin_type_hover_transition(3659);

        assert!(!should_reset_diagnostics);
        assert!(state.stale_combined_popup);
        assert_eq!(state.popup_diag_type_target, Some(3717));
        assert_eq!(state.combined_type_target(), Some(3717));
        assert_eq!(state.effective_hovered_diag_type_target(Some(3659)), Some(3717));

        let (show_error, show_type, show_combined) =
            compute_hover_visibility(true, true, true, Some(3717), Some(3717), Some(3659), true);
        assert!(show_error);
        assert!(show_type);
        assert!(show_combined);
    }

    #[test]
    fn type_only_popup_stays_visible_while_next_word_loads() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old hover".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 20,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });

        let should_reset_diagnostics = state.begin_type_hover_transition(7);

        assert!(!should_reset_diagnostics);
        assert_eq!(state.byte_offset, Some(7));
        assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(20));
        assert!(state.should_show_stale_popup_while_target_loads(false));
    }

    #[test]
    fn stale_visibility_can_still_show_existing_combined_popup() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.hovered_diag_type_target = Some(17);

        assert_eq!(
            state.record_hovered_diagnostic((0, 100.0, 140.0, 162.0, 130.0), Some(99)),
            Some(17)
        );
        assert!(state.hovered_diags_cache.is_empty());

        let (show_error, show_type, show_combined) =
            compute_hover_visibility_from_matches(true, true, true, true, true, false, false, true);

        assert!(show_error);
        assert!(show_type);
        assert!(show_combined);
    }

    #[test]
    fn stale_combined_popup_freezes_diagnostic_target_during_new_hover() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.hovered_diag_type_target = Some(3768);
        state.popup_diag_type_target = Some(3768);

        state.update_hovered_diag_type_target_for_frame(Some(3717));

        assert_eq!(state.hovered_diag_type_target, Some(3768));
        assert_eq!(state.effective_hovered_diag_type_target(Some(3717)), Some(3768));
    }

    #[test]
    fn active_combined_popup_keeps_target_during_whitespace_frame() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: Some(20.0),
            offset_y: Some(-80.0),
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.hovered_diag_type_target = Some(3717);
        state.popup_diag_type_target = Some(3717);
        state.byte_offset = None;

        assert!(state.has_active_combined_type_popup());
        state.update_hovered_diag_type_target_for_frame(None);

        assert_eq!(state.hovered_diag_type_target, Some(3717));
        assert_eq!(state.effective_hovered_diag_type_target(None), Some(3717));
    }

    #[test]
    fn non_stale_frame_accepts_new_diagnostic_target() {
        let mut state = HoverState::default();
        state.hovered_diag_type_target = Some(3768);

        state.update_hovered_diag_type_target_for_frame(Some(3717));

        assert_eq!(state.hovered_diag_type_target, Some(3717));
        assert_eq!(state.effective_hovered_diag_type_target(Some(3717)), Some(3717));
    }

    #[test]
    fn mark_type_popup_drawn_tracks_combined_target_only() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });

        state.mark_type_popup_drawn(true, Some(3717));
        assert_eq!(state.popup_diag_type_target, Some(3717));

        state.mark_type_popup_drawn(false, None);
        assert_eq!(state.popup_diag_type_target, Some(3717));

        state.popup = None;
        state.mark_type_popup_drawn(false, None);
        assert!(state.popup_diag_type_target.is_none());

        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.popup_diag_type_target = Some(3717);
        state.stale_combined_popup = true;
        state.mark_type_popup_drawn(false, None);
        assert_eq!(state.popup_diag_type_target, Some(3717));
    }

    #[test]
    fn stale_transition_completion_keeps_diagnostic_timer_ready_for_new_popup() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.popup_diag_type_target = Some(3717);
        state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
        state.hovered_diags.push(0);
        state.hovered_diags_cache.push((0, 11.0, 20.0, 46.0, 41.0));
        state.hovered_diag_type_target = Some(3717);
        state.diag_hover_timer_idx = Some(0);
        state.diag_hover_timer = 0.2;
        state.diag_anim_progress = 1.0;
        state.diag_text.push_str("old diagnostic");

        state.finish_stale_combined_transition();

        assert!(!state.stale_combined_popup);
        assert!(state.popup_diag_type_target.is_none());
        assert!(state.diag_rect.is_none());
        assert!(state.hovered_diags.is_empty());
        assert!(state.hovered_diags_cache.is_empty());
        assert!(state.hovered_diag_type_target.is_none());
        assert!(state.diag_text.is_empty());
        assert_eq!(state.diag_anim_progress, 0.0);
        assert!(state.diag_hover_ready_after_stale);
        assert!(state.advance_diagnostic_hover_timer(Some(1), true, false, 0.0));
        assert_eq!(state.diag_hover_timer_idx, Some(1));
        assert_eq!(state.diag_hover_timer, 0.2);
        assert!(!state.diag_hover_ready_after_stale);
    }

    #[test]
    fn empty_space_keeps_popup_while_hover_transition_is_pending() {
        let mut state = HoverState::default();

        assert!(!state.should_keep_popup_through_empty_space());

        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });

        assert!(!state.should_keep_popup_through_empty_space());

        state.request_id = Some(1);
        assert!(state.should_keep_popup_through_empty_space());

        state.request_id = None;
        state.definition_request_id = Some(2);
        assert!(state.should_keep_popup_through_empty_space());

        state.definition_request_id = None;
        state.pending_popup = Some(crate::app::mouse::HoverPopup {
            text: "new hover".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3659,
            anchor_x: 140.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        assert!(state.should_keep_popup_through_empty_space());

        state.pending_popup = None;
        state.stale_combined_popup = true;
        assert!(state.should_keep_popup_through_empty_space());
    }

    #[test]
    fn empty_space_keeps_active_combined_popup_without_pending_request() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.popup_diag_type_target = Some(3717);
        state.byte_offset = None;
        state.timer = 0.19;

        assert!(state.keep_active_combined_popup_on_empty_space());
        assert_eq!(state.byte_offset, Some(3717));
        assert_eq!(state.timer, 0.0);
        assert!(state.should_keep_popup_through_empty_space());
    }

    #[test]
    fn stale_combined_popup_uses_frozen_diagnostics_not_live_cache() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old openapi_config_arg hover".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: Some(20.0),
            offset_y: Some(-80.0),
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.rect = Some((120.0, 40.0, 260.0, 120.0));
        state.diag_rect = Some((120.0, 160.0, 260.0, 80.0, 130.0, 170.0, 180.0));
        state.hovered_diag_type_target = Some(3717);
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 130.0, 170.0, 192.0, 180.0));

        state.begin_type_hover_transition(3659);
        state.hovered_diags_cache.push((1, 90.0, 110.0, 132.0, 140.0));

        assert!(state.stale_combined_popup);
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(0, 130.0, 170.0, 192.0, 180.0)]
        );
    }

    #[test]
    fn stale_combined_popup_ignores_new_hovered_diagnostic_until_new_popup_ready() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        state.begin_type_hover_transition(3659);
        let target = state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659));

        assert_eq!(target, Some(3717));
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 42.0, 60.0)]);
        assert_eq!(state.diagnostic_popup_cache(), &[(0, 10.0, 20.0, 42.0, 60.0)]);
    }

    #[test]
    fn active_combined_popup_does_not_collect_different_target_diagnostic() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3717,
            anchor_x: 100.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        let target = state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659));

        assert_eq!(target, Some(3717));
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 42.0, 60.0)]);
        assert_eq!(state.diagnostic_popup_cache(), &[(0, 10.0, 20.0, 42.0, 60.0)]);
    }

    #[test]
    fn stale_completion_clears_frozen_diagnostics_before_handlers_popup() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.popup_diag_type_target = Some(3717);
        state
            .stale_hovered_diags_cache
            .push((0, 10.0, 20.0, 42.0, 60.0));
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        state.finish_stale_combined_transition();

        assert!(!state.stale_combined_popup);
        assert!(state.stale_hovered_diags_cache.is_empty());
        assert!(state.hovered_diags_cache.is_empty());

        assert_eq!(
            state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659)),
            Some(3659)
        );
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(1, 80.0, 90.0, 112.0, 120.0)]
        );
    }


    #[test]
    fn hover_response_preserves_pending_diagnostic_context_for_combined_popup() {
        let mut state = HoverState::default();
        state.hovered_diags.push(0);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
        state.diag_hover_timer = 0.21;
        state.diag_hover_timer_idx = Some(0);

        state.hide_diagnostic_popup_until_ready();

        assert_eq!(state.hovered_diags, vec![0]);
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 40.0, 60.0)]);
        assert_eq!(state.diag_hover_timer, 0.21);
        assert_eq!(state.diag_hover_timer_idx, Some(0));
    }

    #[test]
    fn diagnostic_hover_range_expands_when_target_is_string_prefix() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("raise ValueError(f'513')\n");
        let text = editor.get_full_text();
        let f_string_offset = text.find("f'513'").unwrap();
        let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
        let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

        let byte_range =
            diagnostic_hover_byte_range_on_line(&editor, 0, f_string_col, f_string_end_col)
                .unwrap();

        assert_eq!(byte_range.0, f_string_offset);
        assert_eq!(byte_range.1, f_string_offset + "f'513'".len());
    }

    #[test]
    fn hover_visibility_shows_only_one_popup_during_conflict() {
        let (show_err, show_type, show_comb) = compute_hover_visibility(
            true,      // is_error_hovered
            true,      // error_timer_ready
            true,      // has_type_popup
            Some(100), // hovered_diag_type_target (e.g. byte of 'f')
            Some(103), // type_popup_byte (e.g. byte of '5')
            Some(103), // hover_byte_offset (e.g. byte of '5')
            false,     // stale_combined_popup
        );
        // Мы требуем ровно 1 попап в любой момент времени.
        // Приоритет отдается Type Popup для конкретного слова под курсором!
        assert!(!show_err);
        assert!(show_type);
        assert!(!show_comb);
    }

    #[test]
    fn hover_state_bridge_keeps_diagnostic_and_type_as_one_popup_area() {
        let mut state = HoverState::default();
        state.diag_rect = Some((220.0, 100.0, 500.0, 120.0, 440.0, 480.0, 305.0));
        state.rect = Some((220.0, 220.0, 500.0, 180.0));
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "type info".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 0,
            anchor_x: 460.0,
            anchor_y: 305.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });

        let (inside_diag, diag_line) = state.popup_or_bridge_contains(450.0, 150.0, 1000.0, 1.0);
        let (inside_type, type_line) = state.popup_or_bridge_contains(450.0, 310.0, 1000.0, 1.0);
        let (outside, outside_line) = state.popup_or_bridge_contains(450.0, 460.0, 1000.0, 1.0);

        assert!(inside_diag);
        assert!(!diag_line);
        assert!(inside_type);
        assert!(type_line);
        assert!(!outside);
        assert!(!outside_line);
    }

    #[test]
    fn hover_byte_ignores_whitespace_next_to_identifier() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    handlers\n");
        let text = editor.get_full_text();
        let handlers = text.find("handlers").unwrap();
        let after_handlers = handlers + "handlers".len();

        assert_eq!(normalize_hover_byte(&editor, handlers), Some(handlers));
        assert_eq!(
            normalize_hover_byte(&editor, after_handlers - 1),
            Some(after_handlers - 1)
        );
        assert_eq!(normalize_hover_byte(&editor, after_handlers), None);
        assert_eq!(normalize_hover_byte(&editor, handlers - 1), None);
    }

    #[test]
    fn hover_byte_ignores_python_keywords_so_diagnostics_can_show() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    else:\n        raise ValueError()\n");
        let text = editor.get_full_text();
        let else_offset = text.find("else").unwrap();

        assert_eq!(normalize_hover_byte(&editor, else_offset), None);
        assert_eq!(normalize_hover_byte(&editor, else_offset + 2), None);
    }

    #[test]
    fn diagnostic_hover_range_expands_to_whole_f_string_literal() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("raise ValueError(f'513')\n");
        let text = editor.get_full_text();
        let literal_offset = text.find("513").unwrap();
        let literal_col = text[..literal_offset].encode_utf16().count() as u32;
        let f_string_offset = text.find("f'513'").unwrap();
        let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
        let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

        let range =
            diagnostic_hover_range_on_line(&editor, 0, literal_col, literal_col + 3).unwrap();
        let byte_range =
            diagnostic_hover_byte_range_on_line(&editor, 0, literal_col, literal_col + 3).unwrap();

        assert_eq!(range.0, f_string_col);
        assert_eq!(range.1, f_string_end_col);
        assert_eq!(byte_range.0, f_string_offset);
        assert_eq!(byte_range.1, f_string_offset + "f'513'".len());
        assert!(literal_offset >= byte_range.0);
        assert!(literal_offset + "513".len() <= byte_range.1);
        assert_eq!(normalize_hover_byte(&editor, range.2), Some(range.2));
    }

    #[test]
    fn diagnostic_hover_target_is_stable_for_expanded_f_string_literal() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("raise ValueError(f'513')\n");
        let text = editor.get_full_text();
        let f_string_offset = text.find("f'513'").unwrap();
        let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
        let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

        let first_target =
            diagnostic_hover_target_byte_on_line(&editor, 0, f_string_col, f_string_end_col);
        let second_target =
            diagnostic_hover_target_byte_on_line(&editor, 0, f_string_col, f_string_end_col);

        assert_eq!(first_target, Some(f_string_offset));
        assert_eq!(second_target, Some(f_string_offset));
    }

    #[test]
    fn diagnostic_hover_range_does_not_create_type_target_for_keyword() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("else:\n");

        assert_eq!(diagnostic_hover_range_on_line(&editor, 0, 0, 4), None);
    }

    #[test]
    fn hover_bridge_reaches_full_source_line_when_popup_is_above() {
        let popup_rect = (220.0, 100.0, 500.0, 180.0);
        let line_top_y = 288.0;
        let line_bottom_y = 316.0;

        assert!(is_in_hover_popup_or_bridge(
            450.0,
            305.0,
            popup_rect,
            460.0,
            305.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_handles_popup_shifted_away_from_anchor() {
        let popup_rect = (20.0, 100.0, 520.0, 180.0);
        let line_top_y = 288.0;
        let line_bottom_y = 316.0;

        assert!(is_in_hover_popup_or_bridge(
            520.0,
            247.0,
            popup_rect,
            760.0,
            305.0,
            line_top_y,
            line_bottom_y,
            800.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_keeps_popup_when_moving_up_from_token_anchor() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(is_in_hover_popup_or_bridge(
            620.0,
            318.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_does_not_capture_next_line_when_popup_is_above() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(!is_in_hover_popup_or_bridge(
            620.0,
            394.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_keeps_popup_when_cursor_moves_slightly_sideways() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(is_in_hover_popup_or_bridge(
            676.0,
            300.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    // --- ГЛОБАЛЬНЫЕ ТЕСТЫ НА ВЕСЬ МОДУЛЬ HOVER ---

    #[test]
    fn global_test_hover_token_bounds() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("def my_func(arg1: int = 42):\n    return f'hello {arg1}'\n");
        let text = editor.get_full_text();

        let my_func_pos = text.find("my_func").unwrap();
        let (s, e) = hover_token_bounds(&editor, my_func_pos + 2);
        assert_eq!(&text[s..e], "my_func");

        let f_pos = text.find("f'hello").unwrap();
        let (s, e) = hover_token_bounds(&editor, f_pos);
        assert_eq!(&text[s..e], "f'hello {arg1}'");
    }

    #[test]
    fn global_test_hover_identifier_pipeline_end_to_end() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("value = my_func(42)\n");
        let text = editor.get_full_text();
        let symbol_pos = text.find("my_func").unwrap();
        let after_symbol = symbol_pos + "my_func".len();

        let normalized = normalize_hover_byte(&editor, after_symbol - 1).unwrap();
        let (start, end) = hover_token_bounds(&editor, normalized);
        let range =
            diagnostic_hover_byte_range_on_line(&editor, 0, symbol_pos as u32, after_symbol as u32)
                .unwrap();

        assert_eq!(normalize_hover_byte(&editor, after_symbol), None);
        assert_eq!(normalized, after_symbol - 1);
        assert_eq!(&text[start..end], "my_func");
        assert_eq!(
            hover_token_text(&editor, normalized).as_deref(),
            Some("my_func")
        );
        assert_eq!(
            (range.0, range.1, range.2),
            (symbol_pos, after_symbol, symbol_pos)
        );
    }

    #[test]
    fn global_test_hover_f_string_pipeline_end_to_end() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("message = f'hello {arg1}'\n");
        let text = editor.get_full_text();
        let literal_start = text.find("f'hello").unwrap();
        let literal_end = literal_start + "f'hello {arg1}'".len();
        let arg_pos = text.find("arg1").unwrap();

        let normalized = normalize_hover_byte(&editor, arg_pos + 2).unwrap();
        let (start, end) = hover_token_bounds(&editor, normalized);
        let range =
            diagnostic_hover_byte_range_on_line(&editor, 0, arg_pos as u32, (arg_pos + 4) as u32)
                .unwrap();

        assert_eq!(&text[start..end], "f'hello {arg1}'");
        assert_eq!(
            hover_token_text(&editor, normalized).as_deref(),
            Some("f'hello {arg1}'")
        );
        assert_eq!((range.0, range.1), (literal_start, literal_end));
        assert!(range.2 >= arg_pos && range.2 < arg_pos + 4);
    }

    #[test]
    fn global_test_hover_popup_state_end_to_end_without_glow() {
        let mut state = HoverState::default();
        state.hovered_diags.push(0);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
        state.byte_offset = Some(42);
        state.request_id = Some(7);

        assert!(!state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0));
        let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21);
        let popup = crate::app::mouse::HoverPopup {
            text: "value: int".to_string(),
            spans: vec![],
            line_kinds: vec![],
            inline_code_ranges: vec![],
            byte_offset: 42,
            anchor_x: 120.0,
            anchor_y: 64.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        };
        state.put_type_popup_after_draw(Some(popup), Some((80.0, 40.0, 220.0, 140.0)), 0.0);

        let (show_err, show_type, show_combined) = compute_hover_visibility(
            true,
            ready,
            state.popup.is_some(),
            Some(42),
            Some(42),
            Some(42),
            false,
        );
        let (inside_popup, _) = state.popup_or_bridge_contains(100.0, 60.0, 800.0, 1.0);

        assert!(show_err);
        assert!(show_type);
        assert!(show_combined);
        assert!(inside_popup);
        assert_eq!(
            state.popup.as_ref().map(|popup| popup.byte_offset),
            Some(42)
        );
    }

    #[test]
    fn global_test_normalize_hover_byte() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    class MyClass:\n");
        let text = editor.get_full_text();

        let class_pos = text.find("class").unwrap();
        assert_eq!(normalize_hover_byte(&editor, class_pos), None);

        let myclass_pos = text.find("MyClass").unwrap();
        assert_eq!(
            normalize_hover_byte(&editor, myclass_pos),
            Some(myclass_pos)
        );
    }

    #[test]
    fn global_test_is_python_hover_keyword() {
        assert!(is_python_hover_keyword("def"));
        assert!(is_python_hover_keyword("class"));
        assert!(is_python_hover_keyword("yield"));
        assert!(!is_python_hover_keyword("my_var"));
        assert!(!is_python_hover_keyword("int"));
    }

    #[test]
    fn global_test_is_hover_target_byte() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("word 123 _var =\n");
        let text = editor.get_full_text();

        assert!(is_hover_target_byte(&editor, text.find('w').unwrap()));
        assert!(is_hover_target_byte(&editor, text.find('1').unwrap()));
        assert!(is_hover_target_byte(&editor, text.find('_').unwrap()));
        assert!(!is_hover_target_byte(&editor, text.find('=').unwrap()));
        assert!(!is_hover_target_byte(&editor, text.find(' ').unwrap()));
    }

    #[test]
    fn global_test_hover_state_machine() {
        let mut state = HoverState::default();

        state.diag_hover_timer_idx = Some(0);
        let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.3);
        assert!(ready);

        state.diag_rect = Some((10.0, 20.0, 100.0, 50.0, 15.0, 25.0, 100.0));
        state.byte_offset = Some(42);
        state.request_id = Some(1);

        let popup = crate::app::mouse::HoverPopup {
            text: "test".to_string(),
            spans: vec![],
            line_kinds: vec![],
            inline_code_ranges: vec![],
            byte_offset: 42,
            anchor_x: 10.0,
            anchor_y: 20.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        };
        state.put_type_popup_after_draw(Some(popup), Some((10.0, 20.0, 100.0, 50.0)), 0.0);
        assert!(state.popup.is_some());
        assert_eq!(state.rect.unwrap().0, 10.0);

        state.hide_diagnostic_popup_until_ready();
        assert!(state.diag_rect.is_none());
        assert!(state.popup.is_some());

        state.reset_diagnostic_popup();
        assert!(state.diag_rect.is_none());
    }
}

thread_local! {
    pub static HOVER_STATE: std::cell::RefCell<HoverState> = std::cell::RefCell::new(HoverState::default());
}

pub const HOVER_REQUEST_DELAY_SEC: f32 = 0.34;
pub const HOVER_POPUP_ANIM_SPEED: f32 = 12.0;

pub fn advance_hover_anim_progress(progress: f32, dt: f32) -> f32 {
    if progress >= 1.0 {
        return 1.0;
    }
    let next = progress + (1.0 - progress) * HOVER_POPUP_ANIM_SPEED * dt;
    if next > 0.99 {
        1.0
    } else {
        next
    }
}

#[allow(dead_code)]
pub fn compute_hover_visibility(
    is_error_hovered: bool,
    error_timer_ready: bool,
    has_type_popup: bool,
    hovered_diag_type_target: Option<usize>,
    type_popup_byte: Option<usize>,
    hover_byte_offset: Option<usize>,
    stale_combined_popup: bool,
) -> (bool, bool, bool) {
    let diagnostic_needs_type = is_error_hovered && hovered_diag_type_target.is_some();
    let type_matches_diag = hovered_diag_type_target == type_popup_byte;
    let hover_matches_diag = hovered_diag_type_target == hover_byte_offset;
    let type_matches_hover = type_popup_byte == hover_byte_offset;

    compute_hover_visibility_from_matches(
        is_error_hovered,
        error_timer_ready,
        has_type_popup,
        diagnostic_needs_type,
        type_matches_diag,
        hover_matches_diag,
        type_matches_hover,
        stale_combined_popup,
    )
}

#[allow(dead_code)]
pub fn compute_hover_visibility_from_matches(
    is_error_hovered: bool,
    error_timer_ready: bool,
    has_type_popup: bool,
    diagnostic_needs_type: bool,
    type_matches_diag: bool,
    hover_matches_diag: bool,
    type_matches_hover: bool,
    stale_combined_popup: bool,
) -> (bool, bool, bool) {
    let show_stale_combined =
        stale_combined_popup && diagnostic_needs_type && has_type_popup && type_matches_diag;
    let show_combined = (diagnostic_needs_type
        && hover_matches_diag
        && has_type_popup
        && type_matches_diag
        && error_timer_ready)
        || show_stale_combined;

    let mut show_error = if diagnostic_needs_type {
        if hover_matches_diag {
            show_combined
        } else if show_stale_combined {
            true
        } else {
            false
        }
    } else {
        is_error_hovered && error_timer_ready
    };

    let show_type = if diagnostic_needs_type {
        if hover_matches_diag {
            has_type_popup && type_matches_diag && error_timer_ready
        } else if show_stale_combined {
            true
        } else {
            has_type_popup && type_matches_hover
        }
    } else {
        has_type_popup && type_matches_hover
    };

    // Строгое правило 1 окна: если всплывают два независимых попапа,
    // скрываем ошибку в угоду детальной информации (типу) того слова, на которое наведен курсор.
    if show_error && show_type && !show_combined {
        show_error = false;
    }

    (show_error, show_type, show_combined)
}

#[cfg(test)]
fn diagnostic_hover_target_byte_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<usize> {
    diagnostic_hover_byte_range_on_line(editor, line, start_col, end_col)
        .map(|(_, _, type_target)| type_target)
}

pub fn clear_hover_popup(_renderer: Option<&mut crate::renderer::Renderer>) -> bool {
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let had_popup = state.popup.is_some()
            || state.request_id.is_some()
            || state.definition_request_id.is_some()
            || state.byte_offset.is_some()
            || state.rect.is_some()
            || state.diag_rect.is_some();
        state.request_id = None;
        state.definition_request_id = None;
        state.popup = None;
        state.pending_popup = None;
        state.timer = 0.0;
        state.byte_offset = None;
        state.rect = None;
        state.max_scroll = 0.0;
        state.selection_anchor = None;
        state.selection_cursor = None;
        state.selecting = false;
        state.diag_selection_anchor = None;
        state.diag_selection_cursor = None;
        state.diag_selecting = false;
        state.reset_diagnostic_popup();
        had_popup
    })
}

fn is_hover_target_byte(editor: &crate::editor::Editor, byte_offset: usize) -> bool {
    if byte_offset >= editor.len() {
        return false;
    }
    let b = editor.byte_at(byte_offset);
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') || b >= 0x80
}

fn is_python_hover_keyword(token: &str) -> bool {
    matches!(
        token,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn hover_token_text(editor: &crate::editor::Editor, byte_offset: usize) -> Option<String> {
    let (start, end) = hover_token_bounds(editor, byte_offset);
    let text = editor.get_full_text();
    text.get(start..end).map(|s| s.to_string())
}

pub(crate) fn diagnostic_hover_byte_range_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<(usize, usize, usize)> {
    if line >= editor.line_offsets.len() {
        return None;
    }

    let line_start = editor.line_offsets[line];
    let line_end = editor
        .line_offsets
        .get(line + 1)
        .copied()
        .unwrap_or(editor.len());

    let mut start_byte = line_start;
    let mut end_byte = line_end;
    let mut start_byte_found = false;
    let mut end_byte_found = false;

    editor.utf16_col_to_byte_advance(line, |_ch, utf16_before, pos| {
        if !start_byte_found && utf16_before >= start_col {
            start_byte = pos;
            start_byte_found = true;
        }
        if !end_byte_found && utf16_before >= end_col {
            end_byte = pos;
            end_byte_found = true;
        }
    });

    if !start_byte_found {
        start_byte = line_start;
    }
    if !end_byte_found {
        end_byte = line_end;
    }

    let scan_start = start_byte.min(line_end);
    let scan_end = end_byte.max(scan_start).min(line_end);
    let mut target_raw_byte = None;
    let mut target_byte = None;

    for byte in scan_start..scan_end {
        if let Some(normalized) = normalize_hover_byte(editor, byte) {
            target_raw_byte = Some(byte);
            target_byte = Some(normalized);
            break;
        }
    }

    let target_raw_byte = target_raw_byte?;
    let target_byte = target_byte?;

    let mut range_start = target_byte;
    let mut range_end = target_byte.saturating_add(1).min(line_end);

    while range_start > line_start && is_hover_target_byte(editor, range_start - 1) {
        range_start -= 1;
    }
    while range_end < line_end && is_hover_target_byte(editor, range_end) {
        range_end += 1;
    }

    let mut quote_start = None;
    let mut scan = target_raw_byte.min(line_end.saturating_sub(1));
    loop {
        let b = editor.byte_at(scan);
        if b == b'\'' || b == b'"' {
            quote_start = Some(scan);
            break;
        }
        if scan == line_start {
            break;
        }
        scan -= 1;
    }

    if quote_start.is_none() {
        let mut forward_scan = target_raw_byte;
        while forward_scan < target_raw_byte + 4 && forward_scan < line_end {
            let b = editor.byte_at(forward_scan);
            if b == b'\'' || b == b'"' {
                quote_start = Some(forward_scan);
                break;
            }
            forward_scan += 1;
        }
    }

    if let Some(qs) = quote_start {
        let quote = editor.byte_at(qs);
        let mut quote_end = None;
        let mut qe = qs + 1;
        while qe < line_end {
            if editor.byte_at(qe) == quote {
                quote_end = Some(qe);
                break;
            }
            qe += 1;
        }

        if let Some(qe) = quote_end {
            let mut prefix_start = qs;
            while prefix_start > line_start {
                let b = editor.byte_at(prefix_start - 1);
                if matches!(b, b'f' | b'F' | b'r' | b'R' | b'u' | b'U' | b'b' | b'B') {
                    prefix_start -= 1;
                } else {
                    break;
                }
            }

            if target_raw_byte >= prefix_start && target_raw_byte < qe {
                range_start = prefix_start;
                range_end = (qe + 1).min(line_end);
            }
        }
    }

    Some((range_start, range_end, target_byte))
}

#[cfg(test)]
pub(crate) fn diagnostic_hover_range_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<(u32, u32, usize)> {
    let (range_start, range_end, target_byte) =
        diagnostic_hover_byte_range_on_line(editor, line, start_col, end_col)?;

    let mut out_start_col = start_col;
    let mut out_end_col = end_col;
    let mut out_start_found = false;
    let mut out_end_found = false;

    editor.utf16_col_to_byte_advance(line, |_ch, utf16_before, pos| {
        if !out_start_found && pos >= range_start {
            out_start_col = utf16_before;
            out_start_found = true;
        }
        if !out_end_found && pos >= range_end {
            out_end_col = utf16_before;
            out_end_found = true;
        }
    });

    if !out_start_found {
        out_start_col = start_col;
    }
    if !out_end_found {
        out_end_col = end_col.max(out_start_col + 1);
    }

    Some((out_start_col, out_end_col, target_byte))
}

pub(crate) fn normalize_hover_byte(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> Option<usize> {
    let normalized = is_hover_target_byte(editor, byte_offset).then_some(byte_offset)?;

    if hover_token_text(editor, normalized)
        .as_deref()
        .is_some_and(is_python_hover_keyword)
    {
        return None;
    }

    Some(normalized)
}

pub(crate) fn hover_token_bounds(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> (usize, usize) {
    let text = editor.get_full_text();
    if !text.is_empty() {
        let bytes = text.as_bytes();
        let idx = byte_offset.min(bytes.len().saturating_sub(1));
        let line_start = bytes[..=idx]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let line_end = bytes[idx..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|pos| idx + pos)
            .unwrap_or(bytes.len());

        let mut quote_pos = None;
        if matches!(bytes[idx], b'\'' | b'"') {
            quote_pos = Some(idx);
        } else {
            let mut pos = idx;
            while pos > line_start {
                pos -= 1;
                if matches!(bytes[pos], b'\'' | b'"') {
                    quote_pos = Some(pos);
                    break;
                }
            }
            if quote_pos.is_none() && idx + 1 < line_end && matches!(bytes[idx + 1], b'\'' | b'"') {
                quote_pos = Some(idx + 1);
            }
        }

        if let Some(quote_start) = quote_pos {
            let quote = bytes[quote_start];
            let mut quote_end = quote_start + 1;
            while quote_end < line_end {
                if bytes[quote_end] == quote
                    && bytes.get(quote_end.saturating_sub(1)) != Some(&b'\\')
                {
                    break;
                }
                quote_end += 1;
            }

            if quote_end < line_end {
                let mut prefix_start = quote_start;
                while prefix_start > line_start {
                    let prev = bytes[prefix_start - 1];
                    if matches!(prev, b'f' | b'F' | b'r' | b'R' | b'b' | b'B' | b'u' | b'U') {
                        prefix_start -= 1;
                    } else {
                        break;
                    }
                }

                if idx >= prefix_start && idx <= quote_end {
                    return (prefix_start, (quote_end + 1).min(line_end));
                }
            }
        }
    }

    let len = editor.len();
    let mut start = byte_offset.min(len);
    while start > 0 && is_hover_target_byte(editor, start - 1) {
        start -= 1;
    }

    let mut end = byte_offset.saturating_add(1).min(len);
    while end < len && is_hover_target_byte(editor, end) {
        end += 1;
    }

    (start, end)
}

pub(crate) fn hover_bytes_share_token(
    editor: &crate::editor::Editor,
    first: Option<usize>,
    second: Option<usize>,
) -> bool {
    match (first, second) {
        (Some(first), Some(second)) => {
            hover_token_bounds(editor, first) == hover_token_bounds(editor, second)
        }
        _ => false,
    }
}

pub fn hover_anchor_for_byte(
    renderer: &mut crate::renderer::Renderer,
    editor: &crate::editor::Editor,
    byte_offset: usize,
    render_scroll_y: f32,
) -> (f32, f32) {
    let (start, end) = hover_token_bounds(editor, byte_offset);
    let text = editor.get_full_text();
    if text.is_empty() {
        return (renderer.last_mouse_x, renderer.last_mouse_y);
    }

    let phys_line = editor
        .line_offsets
        .partition_point(|&o| o <= start)
        .saturating_sub(1);
    let line_start = editor.line_offsets.get(phys_line).copied().unwrap_or(0);

    let mut token_start = start.min(text.len());
    while token_start > line_start && !text.is_char_boundary(token_start) {
        token_start -= 1;
    }

    let mut token_end = end.min(text.len());
    while token_end < text.len() && !text.is_char_boundary(token_end) {
        token_end += 1;
    }

    if token_start > token_end || line_start > token_start {
        return (renderer.last_mouse_x, renderer.last_mouse_y);
    }

    let mut prefix_w = 0.0;
    if let Some(prefix) = text.get(line_start..token_start) {
        for c in prefix.chars() {
            if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                prefix_w += renderer.char_advance(c);
            }
        }
    }

    let mut token_w = 0.0;
    if let Some(token) = text.get(token_start..token_end) {
        for c in token.chars() {
            if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                token_w += renderer.char_advance(c);
            }
        }
    }

    let vis_line_idx = renderer
        .phys_to_visual
        .get(phys_line)
        .copied()
        .unwrap_or(phys_line) as f32;
    let x = renderer.left_padding - renderer.last_scroll_x + prefix_w + token_w * 0.5;
    let y = (vis_line_idx * renderer.line_height) - render_scroll_y + renderer.line_height * 0.5;

    (x, y)
}

pub fn is_in_hover_popup_or_bridge(
    px: f32,
    py: f32,
    popup_rect: (f32, f32, f32, f32),
    anchor_x: f32,
    anchor_y: f32,
    line_top_y: f32,
    line_bottom_y: f32,
    _viewport_w: f32,
    scale: f32,
) -> bool {
    let (rx, ry, rw, rh) = popup_rect;
    if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
        return true;
    }

    let bridge_radius = 72.0 * scale;
    let bridge_margin = 16.0 * scale;

    if ry + rh <= line_top_y {
        if py > line_bottom_y + bridge_margin {
            return false;
        }
    } else if ry >= line_bottom_y && py < line_top_y - bridge_margin {
        return false;
    }

    let target_x = anchor_x.clamp(rx, rx + rw);
    let target_y = anchor_y.clamp(ry, ry + rh);

    let dx = target_x - anchor_x;
    let dy = target_y - anchor_y;
    let len_sq = dx * dx + dy * dy;

    let t = if len_sq == 0.0 {
        0.0
    } else {
        ((px - anchor_x) * dx + (py - anchor_y) * dy) / len_sq
    };

    let t = t.clamp(0.0, 1.0);
    let proj_x = anchor_x + t * dx;
    let proj_y = anchor_y + t * dy;

    let dist_sq = (px - proj_x) * (px - proj_x) + (py - proj_y) * (py - proj_y);

    if dist_sq <= bridge_radius * bridge_radius {
        return true;
    }

    let on_line_x = (px - anchor_x).abs() < bridge_radius;
    let on_line_y = if ry + rh <= line_top_y {
        py >= ry + rh - bridge_radius * 0.5 && py <= line_bottom_y + bridge_margin
    } else if ry >= line_bottom_y {
        py >= line_top_y - bridge_margin && py <= ry + bridge_radius * 0.5
    } else {
        py >= line_top_y - bridge_radius * 0.5 && py <= line_bottom_y + bridge_radius * 0.5
    };

    on_line_x && on_line_y
}

fn hover_popup_byte_at(
    renderer: &mut crate::renderer::Renderer,
    popup: &HoverPopup,
    rect: (f32, f32, f32, f32),
    x: f32,
    y: f32,
) -> usize {
    let s = renderer.scale_factor;
    let pad = 12.0 * s;
    let line_h = 22.0 * s;
    let max_text_w = (renderer.width - 80.0 * s).min(820.0 * s).max(320.0 * s);
    let (bx, by, _bw, _bh) = rect;

    let computed_layout;
    let layout = if let Some(cache) = popup.layout_cache.as_ref().filter(|cache| {
        cache.scale_factor == renderer.scale_factor
            && cache.max_text_w == max_text_w
            && cache.span_count == popup.spans.len()
            && cache.text_len == popup.text.len()
    }) {
        cache
    } else {
        computed_layout = renderer.build_hover_popup_layout(popup, max_text_w, line_h);
        &computed_layout
    };
    let lines = &layout.lines;

    if lines.is_empty() {
        return 0;
    }

    let mut current_top = by + pad - popup.scroll.current;
    let mut found_line_idx = lines.len().saturating_sub(1);

    for (i, line) in lines.iter().enumerate() {
        let scale_mul = match line.kind {
            crate::lsp::HoverLineKindPublic::Header1 => 1.15,
            crate::lsp::HoverLineKindPublic::Header2 => 1.05,
            _ => 1.0,
        };
        let cur_line_h = line_h * scale_mul;

        if y >= current_top && y < current_top + cur_line_h {
            found_line_idx = i;
            break;
        }
        current_top += cur_line_h;
    }

    let found_visual_line = &lines[found_line_idx];
    let found_line = &found_visual_line.glyphs;
    let found_kind = found_visual_line.kind;
    let found_scale = match found_kind {
        crate::lsp::HoverLineKindPublic::Header1 => 1.15,
        crate::lsp::HoverLineKindPublic::Header2 => 1.05,
        _ => 1.0,
    };

    if found_line.is_empty() {
        if found_line_idx > 0 {
            if let Some(prev_line) = lines.get(found_line_idx - 1) {
                if let Some(&(prev_ch, _, prev_off)) = prev_line.glyphs.last() {
                    return prev_off + prev_ch.len_utf8();
                }
            }
        }
        for next_idx in (found_line_idx + 1)..lines.len() {
            if let Some(&(_next_ch, _, next_off)) = lines[next_idx].glyphs.first() {
                return next_off;
            }
        }
        return 0;
    }

    let is_code = found_kind == crate::lsp::HoverLineKindPublic::Code;
    let is_module_header = found_kind == crate::lsp::HoverLineKindPublic::Text
        && found_line.len() >= 11
        && found_line
            .iter()
            .take(11)
            .map(|&(c, _, _)| c)
            .collect::<String>()
            == "[[MODULE]] ";
    let is_header = matches!(
        found_kind,
        crate::lsp::HoverLineKindPublic::Header1 | crate::lsp::HoverLineKindPublic::Header2
    );

    let mut start_x = if is_code {
        bx + pad + 8.0 * s
    } else {
        bx + pad
    };
    let mut glyph_start = 0;

    if is_module_header {
        let icon_size = 18.0 * s;
        start_x = bx + pad + icon_size + 4.0 * s;
        glyph_start = 11;
    }

    let target_x = (x - start_x).max(0.0);
    let mut draw_x = 0.0;

    for i in glyph_start..found_line.len() {
        let (ch, _, off) = found_line[i];
        let adv = if is_header {
            renderer.get_ui_glyph(ch).map(|g| g.advance).unwrap_or(10.0) * found_scale
        } else {
            renderer.char_advance(ch) * found_scale
        };
        if target_x <= draw_x + adv * 0.5 {
            return off;
        }
        draw_x += adv;
    }
    let (last_ch, _, last_off) = found_line[found_line.len() - 1];
    last_off + last_ch.len_utf8()
}
