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
                let is_same_diagnostic = self
                    .hovered_diags_cache
                    .iter()
                    .any(|existing| existing.0 == diagnostic.0);
                if !is_same_diagnostic {
                    return Some(active_target);
                }
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
        if !self.stale_combined_popup
            && !(type_target.is_none() && self.has_active_combined_type_popup())
        {
            self.hovered_diag_type_target = type_target;
        }
    }

    pub fn effective_hovered_diag_type_target(&self, type_target: Option<usize>) -> Option<usize> {
        if self.stale_combined_popup
            || (type_target.is_none() && self.has_active_combined_type_popup())
        {
            self.combined_type_target()
        } else {
            type_target
        }
    }

    pub fn mark_type_popup_drawn(&mut self, show_combined: bool, type_target: Option<usize>) {
        if show_combined {
            self.popup_diag_type_target = type_target;
        } else if !self.stale_combined_popup && !self.has_active_combined_type_popup() {
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
            || (self.popup.is_some()
                && (self.pending_popup.is_some()
                    || self.request_id.is_some()
                    || self.definition_request_id.is_some()))
    }

    pub fn should_lock_hover_target_while_popup_opens(&self, next_byte: Option<usize>) -> bool {
        let Some(next_byte) = next_byte else {
            return false;
        };
        let current_byte = self
            .byte_offset
            .or_else(|| self.pending_popup.as_ref().map(|popup| popup.byte_offset))
            .or_else(|| self.popup.as_ref().map(|popup| popup.byte_offset));
        let Some(current_byte) = current_byte else {
            return false;
        };
        if current_byte == next_byte {
            return false;
        }
        self.pending_popup.is_some()
            || self.request_id.is_some()
            || self.definition_request_id.is_some()
            || self
                .popup
                .as_ref()
                .is_some_and(|popup| self.rect.is_none() || popup.anim_progress < 1.0)
    }

    pub fn keep_active_combined_popup_on_empty_space(&mut self) -> bool {
        if !self.stale_combined_popup {
            return false;
        }
        let Some(target) = self.combined_type_target() else {
            return false;
        };
        if self.byte_offset.is_none() {
            self.byte_offset = self
                .popup
                .as_ref()
                .map(|popup| popup.byte_offset)
                .or(Some(target));
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
            let (line_top_y, line_bottom_y) = hover_source_line_y_band(anchor_y, scale);
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
            let (line_top_y, line_bottom_y) = hover_source_line_y_band(popup.anchor_y, scale);
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

pub fn hover_source_line_y_band(anchor_y: f32, scale: f32) -> (f32, f32) {
    let half_h = 10.0 * scale;
    (anchor_y - half_h, anchor_y + half_h)
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

    if ry + rh <= line_top_y {
        if py > line_bottom_y {
            return false;
        }
    } else if ry >= line_bottom_y && py < line_top_y {
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
        py >= ry + rh - bridge_radius * 0.5 && py <= line_bottom_y
    } else if ry >= line_bottom_y {
        py >= line_top_y && py <= ry + bridge_radius * 0.5
    } else {
        py >= line_top_y && py <= line_bottom_y
    };

    on_line_x && on_line_y
}
