use std::ops::Range;

use super::{App, EditorTabKind};
use crate::render_view::markdown_read::MarkdownSourceAnchor;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownMode {
    #[default]
    Edit,
    Read,
}

include!("markdown_scroll_transition.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownReadWheelResult {
    NotRead,
    Blocked,
    Scrolled,
}

pub(crate) fn scroll_markdown_read(
    scroll: &mut crate::scroll::ScrollState,
    read_max_scroll: Option<f32>,
    dy: f32,
) {
    scroll.anim_speed = 7.0;
    scroll.scroll_by(dy);
    if let Some(max_scroll) = read_max_scroll {
        scroll.clamp_target(0.0, max_scroll);
    }
}

pub(crate) fn handle_markdown_read_wheel(
    mode: MarkdownMode,
    hovered: Option<crate::ui_system::UiId>,
    allow_stale_editor_surface: bool,
    scroll: &mut crate::scroll::ScrollState,
    read_max_scroll: Option<f32>,
    dy: f32,
) -> MarkdownReadWheelResult {
    if mode != MarkdownMode::Read {
        return MarkdownReadWheelResult::NotRead;
    }
    let read_surface = matches!(
        hovered,
        Some(
            crate::ui_system::UiId::MarkdownReadBody
                | crate::ui_system::UiId::MarkdownReadScrollbar
                | crate::ui_system::UiId::MarkdownCodeCopy(_)
                | crate::ui_system::UiId::MarkdownCodeScrollbarX(_)
        )
    );
    let stale_editor_surface = allow_stale_editor_surface
        && matches!(
            hovered,
            Some(
                crate::ui_system::UiId::EditorTextBody
                    | crate::ui_system::UiId::EditorScrollbarY
                    | crate::ui_system::UiId::EditorMinimap
            )
        );
    if !read_surface && !stale_editor_surface {
        return MarkdownReadWheelResult::Blocked;
    }
    scroll_markdown_read(scroll, read_max_scroll, dy);
    MarkdownReadWheelResult::Scrolled
}

pub(crate) fn is_markdown_extension(extension: &str) -> bool {
    matches!(extension, "md" | "markdown")
}

pub struct MarkdownTabState {
    pub mode: MarkdownMode,
    pub(crate) read_model: Option<crate::languages::markdown::MarkdownDocument>,
    pub(crate) read_source: String,
    pub(crate) read_model_version: Option<u64>,
    pub(crate) read_parser: Option<crate::languages::markdown::MarkdownParseState>,
    #[cfg(test)]
    parser_creation_count: usize,
    #[cfg(test)]
    semantic_refresh_count: usize,
    pub(crate) read_layout: crate::render_view::markdown_read::MarkdownReadLayoutCache,
    pub(crate) read_max_scroll: f32,
    pub(crate) read_scroll_bounds_valid: bool,
    pub(crate) scroll_transition: Option<MarkdownScrollTransition>,
    pub(crate) scroll_carry: Option<MarkdownScrollCarry>,
    pub(crate) scroll_navigation_revision: u64,
    pub(crate) scroll_target_navigation_revision: Option<u64>,
    pub(crate) scroll_target_navigation: Option<MarkdownAbsoluteScrollTargetNavigation>,
    pub(crate) deferred_current_geometry: Option<MarkdownDeferredCurrentGeometry>,
    pub(crate) last_read_geometry: Option<MarkdownReadDisplayedGeometry>,
    pub(crate) last_edit_geometry: Option<MarkdownEditDisplayedGeometry>,
    pub(crate) read_selection_anchor: Option<usize>,
    pub(crate) read_selection_cursor: Option<usize>,
    pub(crate) read_selecting: bool,
    pub(crate) read_selection_autoscrolling: bool,
    pub(crate) copied_code_block: Option<usize>,
    pub(crate) code_copy_hover_valid: bool,
    pub(crate) code_scroll_x: Vec<MarkdownCodeScrollX>,
    pub(crate) code_scroll_drag: Option<usize>,
}

// Горизонтальный скролл code block в Reader; хранится только пока активен.
#[derive(Clone, Debug)]
pub(crate) struct MarkdownCodeScrollX {
    pub block_id: usize,
    pub scroll: crate::scroll::ScrollState,
}

impl Default for MarkdownTabState {
    fn default() -> Self {
        Self {
            mode: MarkdownMode::Edit,
            read_model: None,
            read_source: String::new(),
            read_model_version: None,
            read_parser: None,
            #[cfg(test)]
            parser_creation_count: 0,
            #[cfg(test)]
            semantic_refresh_count: 0,
            read_layout: crate::render_view::markdown_read::MarkdownReadLayoutCache::default(),
            read_max_scroll: 0.0,
            read_scroll_bounds_valid: false,
            scroll_transition: None,
            scroll_carry: None,
            scroll_navigation_revision: 0,
            scroll_target_navigation_revision: None,
            scroll_target_navigation: None,
            deferred_current_geometry: None,
            last_read_geometry: None,
            last_edit_geometry: None,
            read_selection_anchor: None,
            read_selection_cursor: None,
            read_selecting: false,
            read_selection_autoscrolling: false,
            copied_code_block: None,
            code_copy_hover_valid: false,
            code_scroll_x: Vec::new(),
            code_scroll_drag: None,
        }
    }
}

impl MarkdownTabState {
    pub(crate) fn refresh_read_model(&mut self, version: u64, source: String) -> bool {
        #[cfg(test)]
        {
            self.semantic_refresh_count = self.semantic_refresh_count.saturating_add(1);
        }
        if self.read_model_version == Some(version)
            && self.read_source == source
            && self.read_model.is_some()
        {
            return true;
        }

        self.clear_read_selection();
        self.clear_code_copy_transient();

        let edit = if self.read_parser.is_some() && self.read_model_version.is_some() {
            Some(markdown_replacement_edit(&self.read_source, &source))
        } else {
            None
        };
        if self.read_parser.is_none() {
            self.read_parser = Some(crate::languages::markdown::MarkdownParseState::default());
            #[cfg(test)]
            {
                self.parser_creation_count = self.parser_creation_count.saturating_add(1);
            }
        }
        let Some(parser) = self.read_parser.as_mut() else {
            return false;
        };
        if let Some(edit) = edit.as_ref() {
            parser.apply_edit(edit);
        }

        let parsed = parser.parse(&source);
        if let Some(document) = parsed {
            self.read_model = Some(document);
            self.read_source = source;
            self.read_model_version = Some(version);
        } else {
            self.read_model = None;
            self.read_source = source;
            self.read_model_version = Some(version);
            self.read_parser = None;
        }
        self.read_layout.invalidate();
        self.invalidate_read_scroll_bounds();
        self.scroll_carry = self
            .scroll_carry
            .take()
            .filter(|carry| carry.version == version);
        if self
            .scroll_transition
            .as_ref()
            .is_some_and(|transition| transition.version != version)
        {
            self.scroll_transition = None;
        }
        self.read_model.is_some()
    }

    pub(crate) fn read_document(
        &self,
        version: u64,
    ) -> Option<&crate::languages::markdown::MarkdownDocument> {
        (self.read_model_version == Some(version))
            .then_some(self.read_model.as_ref())
            .flatten()
    }

    fn needs_read_model_refresh(&self, version: u64) -> bool {
        self.read_document(version).is_none()
    }

    pub(crate) fn read_selection_range(&self) -> Option<Range<usize>> {
        let anchor = self.read_selection_anchor?;
        let cursor = self.read_selection_cursor?;
        (anchor != cursor).then_some(anchor.min(cursor)..anchor.max(cursor))
    }

    pub(crate) fn begin_read_selection(&mut self, byte: usize) {
        let byte = byte.min(self.read_source.len());
        self.read_selection_anchor = Some(byte);
        self.read_selection_cursor = Some(byte);
        self.read_selecting = true;
        self.read_selection_autoscrolling = false;
    }

    pub(crate) fn update_read_selection(&mut self, byte: usize) -> bool {
        if !self.read_selecting {
            return false;
        }
        let byte = byte.min(self.read_source.len());
        let changed = self.read_selection_cursor != Some(byte);
        self.read_selection_cursor = Some(byte);
        changed
    }

    pub(crate) fn settle_read_selection_autoscroll(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
    ) -> bool {
        if !self.read_selection_autoscrolling {
            return false;
        }
        self.read_selection_autoscrolling = false;
        if !scroll.current.is_finite() {
            let changed = !scroll.is_settled();
            scroll.reset();
            return changed;
        }
        let changed = scroll.target != scroll.current || scroll.velocity != 0.0;
        scroll.target = scroll.current;
        scroll.velocity = 0.0;
        changed
    }

    pub(crate) fn finish_read_selection_gesture(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
    ) -> bool {
        let was_selecting = self.read_selecting;
        let changed = self.settle_read_selection_autoscroll(scroll);
        self.finish_read_selection();
        was_selecting || changed
    }

    pub(crate) fn finish_read_selection(&mut self) {
        self.read_selecting = false;
        self.read_selection_autoscrolling = false;
    }

    pub(crate) fn clear_read_selection(&mut self) {
        self.read_selection_anchor = None;
        self.read_selection_cursor = None;
        self.read_selecting = false;
        self.read_selection_autoscrolling = false;
    }

    pub(crate) fn mark_code_copy_hover_valid(&mut self) -> bool {
        let changed = !self.code_copy_hover_valid;
        self.code_copy_hover_valid = true;
        changed
    }

    pub(crate) fn update_code_copy_hover(&mut self, hovered_block: Option<usize>) -> bool {
        if self.copied_code_block.is_some() && self.copied_code_block != hovered_block {
            self.copied_code_block = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn clear_code_copy_transient(&mut self) -> bool {
        let changed = self.code_copy_hover_valid || self.copied_code_block.is_some();
        self.code_copy_hover_valid = false;
        self.copied_code_block = None;
        changed
    }

    pub(crate) fn selected_read_text(&self) -> Option<String> {
        let range = self.read_selection_range()?;
        let text = self
            .read_layout
            .copy_source_selection(self.read_source.as_str(), &range);
        (!text.is_empty()).then_some(text)
    }

    #[cfg(test)]
    fn parser_creation_count(&self) -> usize {
        self.parser_creation_count
    }

    #[cfg(test)]
    fn semantic_refresh_count(&self) -> usize {
        self.semantic_refresh_count
    }
}

fn markdown_replacement_edit(old: &str, new: &str) -> tree_sitter::InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while start > 0 && (!old.is_char_boundary(start) || !new.is_char_boundary(start)) {
        start -= 1;
    }
    let max_suffix = old
        .len()
        .saturating_sub(start)
        .min(new.len().saturating_sub(start));
    let mut suffix = old.as_bytes()[old.len() - max_suffix..]
        .iter()
        .rev()
        .zip(new.as_bytes()[new.len() - max_suffix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while suffix > 0 {
        let old_end = old.len() - suffix;
        let new_end = new.len() - suffix;
        if old.is_char_boundary(old_end) && new.is_char_boundary(new_end) {
            break;
        }
        suffix -= 1;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    tree_sitter::InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: markdown_point(old, start),
        old_end_position: markdown_point(old, old_end),
        new_end_position: markdown_point(new, new_end),
    }
}

fn markdown_point(text: &str, byte: usize) -> tree_sitter::Point {
    let prefix = &text[..byte.min(text.len())];
    let row = prefix.as_bytes().iter().filter(|&&b| b == b'\n').count();
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len());
    tree_sitter::Point::new(row, column)
}

impl App {
    pub fn active_document_is_markdown(&self) -> bool {
        let markdown_extension = is_markdown_extension(&self.file_extension);
        let normal_document = !self.is_ide_mode
            || self
                .tabs
                .get(self.active_tab)
                .is_some_and(|tab| matches!(tab.kind, EditorTabKind::Normal));
        markdown_extension && normal_document && self.file_path.is_some()
    }

    pub fn markdown_mode(&self) -> MarkdownMode {
        if self.active_document_is_markdown() {
            self.markdown.mode
        } else {
            MarkdownMode::Edit
        }
    }

    pub fn set_markdown_mode(&mut self, mode: MarkdownMode) {
        if !self.active_document_is_markdown() || self.markdown.mode == mode {
            return;
        }

        self.finish_markdown_read_selection_gesture();
        let from = self.markdown.mode;
        let reverse_policy = self
            .markdown
            .scroll_transition
            .as_ref()
            .filter(|transition| transition.from == mode && transition.to == from)
            .and_then(|transition| {
                self.markdown
                    .transition_navigation_policy(transition, self.editor.version)
            });
        let reverse_unresolved = match reverse_policy {
            Some(MarkdownPendingNavigationPolicy::PreserveMotion) => true,
            Some(MarkdownPendingNavigationPolicy::PreserveDestinationTarget) => {
                self.scroll_y.cancel_unapplied_deferred_current_rebase()
            }
            None => false,
        };
        if reverse_unresolved {
            self.markdown.cancel_stale_scroll_transition();
            self.scroll_y.end_drag();
            self.markdown.end_code_scroll_drag();
            self.markdown.mode = mode;
            self.markdown.clear_code_copy_transient();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }

        // A deferred current rebase can be consumed by the single physics tick
        // before the destination surface is ever drawn. In that phase `current`
        // belongs to the prepared destination geometry, not to the last displayed
        // frame. Capture through that exact geometry before clearing the old
        // transition so an immediate inverse toggle never decodes destination
        // pixels with stale origin metrics.
        let deferred_current_geometry = (self.scroll_y.deferred_current_rebase_applied()
            == Some(true))
        .then_some(self.markdown.deferred_current_geometry)
        .flatten();
        let origin_scroll_y = self.scroll_y.current;
        let (anchor, origin_read_width, origin_sticky_lines) =
            self.capture_markdown_scroll_anchor(from, mode, deferred_current_geometry);
        if deferred_current_geometry.is_some() {
            self.scroll_y.clear_deferred_current_rebase();
            self.markdown.deferred_current_geometry = None;
            self.markdown.scroll_target_navigation_revision = None;
            self.markdown.scroll_target_navigation = None;
        }
        self.markdown.scroll_transition = None;
        self.scroll_y.end_drag();
        self.markdown.end_code_scroll_drag();
        self.markdown.scroll_transition = Some(MarkdownScrollTransition {
            from,
            to: mode,
            version: self.editor.version,
            origin_scroll_y,
            origin_sticky_lines,
            origin_navigation_revision: self.markdown.scroll_navigation_revision(),
            origin_read_width,
            anchor,
        });

        self.markdown.clear_code_copy_transient();
        if mode == MarkdownMode::Read {
            self.close_autocomplete();
            self.lsp_actions_menu = None;
            self.pending_fix_all_id = None;
            self.markdown.invalidate_read_scroll_bounds();
        }
        if mode == MarkdownMode::Read && self.markdown.needs_read_model_refresh(self.editor.version)
        {
            let source = self.editor.get_full_text();
            self.markdown
                .refresh_read_model(self.editor.version, source);
        }
        self.markdown.mode = mode;
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn toggle_markdown_mode(&mut self) {
        let next = match self.markdown_mode() {
            MarkdownMode::Edit => MarkdownMode::Read,
            MarkdownMode::Read => MarkdownMode::Edit,
        };
        self.set_markdown_mode(next);
    }

    pub(crate) fn refresh_markdown_read_model_if_stale(&mut self) -> bool {
        if self.markdown_mode() != MarkdownMode::Read
            || !self.markdown.needs_read_model_refresh(self.editor.version)
        {
            return false;
        }
        self.finish_markdown_read_selection_gesture();
        let source = self.editor.get_full_text();
        self.markdown
            .refresh_read_model(self.editor.version, source);
        true
    }

    pub(crate) fn begin_markdown_read_selection_at(&mut self, x: f32, y: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return false;
        };
        let version = self.editor.version;
        let byte = {
            let markdown = &self.markdown;
            let Some(renderer) = self.renderer.as_mut() else {
                return false;
            };
            renderer.markdown_read_source_byte_at(
                markdown,
                version,
                frame,
                self.scroll_y.current,
                x,
                y,
            )
        };
        let Some(byte) = byte else {
            return false;
        };
        self.markdown.begin_read_selection(byte);
        self.is_dragging = false;
        self.is_editor_drag_pending = false;
        true
    }

    pub(crate) fn update_markdown_read_selection_at(&mut self, x: f32, y: f32) -> bool {
        if !self.markdown.read_selecting || self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return false;
        };
        let version = self.editor.version;
        let byte = {
            let markdown = &self.markdown;
            let Some(renderer) = self.renderer.as_mut() else {
                return false;
            };
            renderer.markdown_read_source_byte_at(
                markdown,
                version,
                frame,
                self.scroll_y.current,
                x,
                y,
            )
        };
        let Some(byte) = byte else {
            return false;
        };
        self.markdown.update_read_selection(byte)
    }

    pub(crate) fn settle_markdown_read_selection_autoscroll(&mut self) -> bool {
        self.markdown
            .settle_read_selection_autoscroll(&mut self.scroll_y)
    }

    pub(crate) fn finish_markdown_read_selection_gesture(&mut self) -> bool {
        self.markdown
            .finish_read_selection_gesture(&mut self.scroll_y)
    }

    fn markdown_read_scrollbar_geometry(
        &self,
    ) -> Option<crate::render_view::scrollbar_widget::ScrollbarGeometry> {
        if self.markdown_mode() != MarkdownMode::Read
            || self.markdown.read_document(self.editor.version).is_none()
        {
            return None;
        }
        let frame = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)?;
        let renderer = self.renderer.as_ref()?;
        let content_width = frame.2.max(1.0);
        if !self.markdown.read_layout.is_valid_for_geometry(
            self.editor.version,
            content_width,
            renderer.scale_factor,
            renderer.font_size,
        ) {
            return None;
        }
        let stored_max_scroll = self.markdown.read_scroll_bounds()?;
        let content_height = self.markdown.read_layout.content_height();
        let max_scroll = (content_height - frame.3).max(0.0);
        if !max_scroll.is_finite()
            || max_scroll <= 0.0
            || (stored_max_scroll - max_scroll).abs() > 1.0
        {
            return None;
        }
        crate::render_view::markdown_read::markdown_read_scrollbar(
            frame,
            content_height,
            self.scroll_y.current.round(),
            renderer.scale_factor,
            [0.0; 4],
        )
        .geometry(renderer.scale_factor)
    }

    pub(crate) fn begin_markdown_read_scrollbar_drag_at(&mut self, pointer_y: f32) -> bool {
        let Some((drag_offset, target)) = self
            .markdown_read_scrollbar_geometry()
            .and_then(|geometry| geometry.press_target(pointer_y))
        else {
            return false;
        };

        self.finish_markdown_read_selection_gesture();
        self.markdown
            .mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
        let changed = self.scroll_y.target != target;
        if !crate::app::mouse::apply_scrollbar_drag_target(&mut self.scroll_y, target, drag_offset)
        {
            return false;
        }
        if changed {
            self.markdown.on_shared_vertical_scroll_changed();
        }
        true
    }

    pub(crate) fn drag_markdown_read_scrollbar_to(&mut self, pointer_y: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read || !self.scroll_y.is_dragging {
            return false;
        }
        let drag_offset = self.scroll_y.drag_offset;
        let Some(target) = self
            .markdown_read_scrollbar_geometry()
            .and_then(|geometry| geometry.drag_target(pointer_y, drag_offset))
        else {
            self.scroll_y.end_drag();
            return true;
        };
        let changed = self.scroll_y.target != target;
        if !crate::app::mouse::apply_scrollbar_drag_target(&mut self.scroll_y, target, drag_offset)
        {
            return false;
        }
        if changed {
            self.markdown.on_shared_vertical_scroll_changed();
        }
        true
    }

    pub(crate) fn copy_markdown_read_selection(&mut self) -> bool {
        let Some(text) = self.markdown.selected_read_text() else {
            return false;
        };
        self.set_clipboard_text(text);
        true
    }

    pub(crate) fn copy_markdown_read_code_block(&mut self, block_id: usize) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(text) = self
            .markdown
            .read_layout
            .code_block_copy_text(self.markdown.read_source.as_str(), block_id)
        else {
            return false;
        };
        self.set_clipboard_text(text);
        self.markdown.copied_code_block = Some(block_id);
        true
    }

    pub(crate) fn update_markdown_code_copy_hover_at(&mut self, x: f32, y: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return self.markdown.clear_code_copy_transient();
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return self.markdown.clear_code_copy_transient();
        };
        let mut changed = self.markdown.mark_code_copy_hover_valid();
        if self.markdown.copied_code_block.is_none() {
            return changed;
        }
        let hovered = self.renderer.as_ref().and_then(|renderer| {
            renderer.markdown_read_code_block_at(
                &self.markdown,
                self.editor.version,
                frame,
                self.scroll_y.current,
                x,
                y,
            )
        });
        changed |= self.markdown.update_code_copy_hover(hovered);
        changed
    }
}

#[cfg(test)]
mod tests {
    include!("markdown_app_tests.rs");
}
