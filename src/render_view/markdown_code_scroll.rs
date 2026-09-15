// Markdown Reader code-block horizontal overflow chunk.
// Included by markdown_read.rs so cached layout structs stay private to the parent module.

const CODE_HSCROLLBAR_RESERVE: f32 = 10.0;
const CODE_HSCROLLBAR_TRACK_H: f32 = 4.0;
const CODE_HSCROLLBAR_HIT_PAD: f32 = 3.0;
const CODE_HSCROLLBAR_MIN_THUMB: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodeScrollGeometry {
    pub view_x: f32,
    pub view_w: f32,
    pub content_w: f32,
    pub max_scroll: f32,
    pub track_x: f32,
    pub track_y: f32,
    pub track_w: f32,
    pub track_h: f32,
}

fn code_hscrollbar_reserve(scale: f32) -> f32 {
    (CODE_HSCROLLBAR_RESERVE * scale).round()
}

fn code_hscrollbar_track_h(scale: f32) -> f32 {
    (CODE_HSCROLLBAR_TRACK_H * scale).round().max(2.0)
}

// Единственная формула ширины видимой области кода: layout (резерв под
// скроллбар) и draw/hit-test (max_scroll) обязаны совпадать на границе.
#[inline]
fn code_block_view_w(content_w: f32, code_x: f32, scale: f32) -> f32 {
    (content_w - CONTENT_PAD * scale - code_x - 2.0 * code_block_padding(scale))
        .max(0.0)
        .round()
}

#[inline]
fn code_block_max_scroll(content_width: f32, view_w: f32) -> f32 {
    (content_width - view_w).max(0.0).round()
}

#[inline]
fn code_block_overflow_reserve(content_width: f32, width: f32, code_x: f32, scale: f32) -> f32 {
    if code_block_max_scroll(content_width, code_block_view_w(width, code_x, scale)) > 0.0 {
        code_hscrollbar_reserve(scale)
    } else {
        0.0
    }
}

/// Отрисованное смещение: clamp к текущему layout и округление, общее для draw и hit-test.
#[inline]
fn code_block_scroll_offset(scroll_x: f32, code: &CodeBlock, content_w: f32, scale: f32) -> f32 {
    clamp_code_scroll_offset(
        scroll_x,
        code_block_max_scroll(
            code.content_width,
            code_block_view_w(content_w, code.x, scale),
        ),
    )
}

#[inline]
fn clamp_code_scroll_offset(scroll_x: f32, max_scroll: f32) -> f32 {
    if !scroll_x.is_finite() {
        return 0.0;
    }
    scroll_x.clamp(0.0, max_scroll).round()
}

/// Один thumb для draw и drag: иначе захват смещался бы относительно нарисованного.
#[inline]
fn code_scroll_thumb(
    g: &CodeScrollGeometry,
    offset: f32,
    scale: f32,
) -> Option<crate::scroll::ScrollbarThumb> {
    crate::scroll::scrollbar_thumb(
        g.track_x,
        g.track_w,
        g.view_w,
        g.content_w,
        offset,
        (CODE_HSCROLLBAR_MIN_THUMB * scale).round(),
    )
}

fn code_line_pixel_width<F: FnMut(char, bool, Option<f32>) -> f32>(
    text: &str,
    advance: &mut F,
) -> f32 {
    text.chars()
        .map(|ch| mono_char_pixel_advance(ch, 1.0, || advance(ch, true, None)))
        .sum()
}

fn code_scroll_geometry(
    frame_x: f32,
    content_w: f32,
    code: &CodeBlock,
    block_bottom_screen: f32,
    scale: f32,
) -> CodeScrollGeometry {
    let pad = code_block_padding(scale);
    let view_x = (frame_x + code.x + pad).round();
    let view_w = code_block_view_w(content_w, code.x, scale);
    let track_h = code_hscrollbar_track_h(scale);
    let track_y = (block_bottom_screen - pad * 0.5 - track_h).round();
    CodeScrollGeometry {
        view_x,
        view_w,
        content_w: code.content_width,
        max_scroll: code_block_max_scroll(code.content_width, view_w),
        track_x: view_x,
        track_y,
        track_w: view_w,
        track_h,
    }
}

impl MarkdownReadLayoutCache {
    fn code_block_by_id(&self, block_id: usize) -> Option<(&ReadBlock, &CodeBlock)> {
        let start = self
            .blocks
            .partition_point(|block| block.source_range.start < block_id);
        self.blocks[start..]
            .iter()
            .take_while(|block| block.source_range.start == block_id)
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
    }
}

impl Renderer {
    pub(crate) fn markdown_code_scroll_geometry(
        &self,
        markdown: &MarkdownTabState,
        editor_version: u64,
        frame: (f32, f32, f32, f32),
        scroll_y: f32,
        block_id: usize,
    ) -> Option<CodeScrollGeometry> {
        if markdown.mode != MarkdownMode::Read
            || markdown.read_layout.key?.version != editor_version
        {
            return None;
        }
        let (frame_x, frame_y, frame_w, _) = frame;
        let (block, code) = markdown.read_layout.code_block_by_id(block_id)?;
        Some(code_scroll_geometry(
            frame_x,
            frame_w.max(1.0),
            code,
            block.bottom + frame_y - scroll_y.round(),
            self.scale_factor,
        ))
    }

    /// Scissor в экранных координатах (GL y снизу); общий для Reader и вложенного code viewport.
    fn set_markdown_read_scissor(&mut self, x: f32, y: f32, w: f32, h: f32) {
        unsafe {
            use glow::HasContext;
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round().max(0.0) as i32,
                h.round().max(0.0) as i32,
            );
        }
    }

    /// Байтовый префикс строки, целиком ушедший левее viewport: его глифы не пушим.
    fn mono_prefix_before(&mut self, text: &str, limit: f32) -> (usize, f32) {
        let mut width = 0.0;
        for (idx, ch) in text.char_indices() {
            let advance = mono_char_pixel_advance(ch, 1.0, || self.char_advance(ch));
            if width + advance > limit {
                return (idx, width);
            }
            width += advance;
        }
        (text.len(), width)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_markdown_code_lines(
        &mut self,
        code: &CodeBlock,
        source: &str,
        spans: &[ColorSpan],
        frame_x: f32,
        content_w: f32,
        block_bottom_screen: f32,
        offset_y: f32,
        visible_top: f32,
        visible_bottom: f32,
        highlights: ReadHighlights<'_>,
        code_scroll_x: f32,
        reader_clip: (f32, f32, f32, f32),
    ) {
        let pad = code_block_padding(self.scale_factor);
        let left = frame_x + code.x;
        let right = frame_x + content_w - CONTENT_PAD * self.scale_factor;
        let lines = visible_code_line_range(&code.lines, visible_top, visible_bottom);
        let g = code_scroll_geometry(
            frame_x,
            content_w,
            code,
            block_bottom_screen,
            self.scale_factor,
        );
        if g.max_scroll <= 0.0 {
            for idx in lines {
                let line = &code.lines[idx];
                let slice = source.get(line.source_range.clone()).unwrap_or("");
                self.draw_mono_source_highlights(
                    slice,
                    line.source_range.start,
                    left + pad,
                    line.top + offset_y,
                    code.line_height,
                    1.0,
                    highlights,
                );
                self.draw_spanned_editor_line_pixel_snapped_alpha(
                    slice,
                    spans,
                    Some(line.source_range.start),
                    left + pad,
                    line.y + offset_y,
                    right - pad,
                    1.0,
                );
            }
            return;
        }

        let sx = code_block_scroll_offset(code_scroll_x, code, content_w, self.scale_factor);
        let (clip_x, clip_y, clip_w, clip_h) = reader_clip;
        let inner_x = clip_x.max(g.view_x);
        let inner_w = ((clip_x + clip_w).min(g.view_x + g.view_w) - inner_x).max(0.0);
        self.flush();
        self.set_markdown_read_scissor(inner_x, clip_y, inner_w, clip_h);
        let origin_x = left + pad - sx;
        for idx in lines {
            let line = &code.lines[idx];
            let slice = source.get(line.source_range.clone()).unwrap_or("");
            let (skip, skip_w) = self.mono_prefix_before(slice, sx);
            let visible = slice.get(skip..).unwrap_or("");
            let start = line.source_range.start + skip;
            let x = origin_x + skip_w;
            self.draw_mono_source_highlights(
                visible,
                start,
                x,
                line.top + offset_y,
                code.line_height,
                1.0,
                highlights,
            );
            self.draw_spanned_editor_line_pixel_snapped_alpha(
                visible,
                spans,
                Some(start),
                x,
                line.y + offset_y,
                right - pad,
                1.0,
            );
        }
        self.flush();
        self.set_markdown_read_scissor(clip_x, clip_y, clip_w, clip_h);

        if let Some(thumb) = code_scroll_thumb(&g, sx, self.scale_factor) {
            self.push_rounded_rect(
                thumb.start.round(),
                g.track_y,
                thumb.len.round(),
                g.track_h,
                g.track_h * 0.5,
                faded(self.theme.fg, 0.32),
            );
        }
    }

    /// Трек горизонтального скроллбара: blocker (стрелка, не I-beam), только при переполнении.
    fn register_markdown_code_scrollbar(
        &self,
        block: &ReadBlock,
        frame_x: f32,
        offset_y: f32,
        content_w: f32,
        ui_registry: &mut UiRegistry,
    ) {
        let ReadBlockKind::Code(code) = &block.kind else {
            return;
        };
        let g = code_scroll_geometry(
            frame_x,
            content_w,
            code,
            block.bottom + offset_y,
            self.scale_factor,
        );
        if g.max_scroll <= 0.0 {
            return;
        }
        let hit_pad = (CODE_HSCROLLBAR_HIT_PAD * self.scale_factor).round();
        ui_registry.register_blocker(
            crate::ui_system::UiId::MarkdownCodeScrollbarX(block.source_range.start),
            g.track_x,
            g.track_y - hit_pad,
            g.track_w,
            g.track_h + 2.0 * hit_pad,
            self.last_mouse_x,
            self.last_mouse_y,
        );
    }
}

impl MarkdownTabState {
    pub(crate) fn code_scroll_x(&self, block_id: usize) -> f32 {
        self.code_scroll_x
            .iter()
            .find(|entry| entry.block_id == block_id)
            .map_or(0.0, |entry| entry.scroll.current)
    }

    pub(crate) fn code_scroll_state_mut(&mut self, block_id: usize) -> &mut crate::scroll::ScrollState {
        let idx = match self
            .code_scroll_x
            .iter()
            .position(|entry| entry.block_id == block_id)
        {
            Some(idx) => idx,
            None => {
                self.code_scroll_x.push(crate::app::MarkdownCodeScrollX {
                    block_id,
                    scroll: crate::scroll::ScrollState::new(7.0),
                });
                self.code_scroll_x.len() - 1
            }
        };
        &mut self.code_scroll_x[idx].scroll
    }

    pub(crate) fn update_code_scroll_x(&mut self, dt: f32) -> bool {
        let mut changed = false;
        for entry in &mut self.code_scroll_x {
            changed |= entry.scroll.update(dt);
        }
        self.retain_active_code_scroll_x();
        changed
    }

    pub(crate) fn end_code_scroll_drag(&mut self) -> bool {
        let Some(block_id) = self.code_scroll_drag.take() else {
            return false;
        };
        if let Some(entry) = self
            .code_scroll_x
            .iter_mut()
            .find(|entry| entry.block_id == block_id)
        {
            entry.scroll.end_drag();
        }
        self.retain_active_code_scroll_x();
        true
    }

    /// Wheel по переполненному блоку; false -> событие уходит в обычный Reader wheel.
    pub(crate) fn scroll_code_block_x(&mut self, block_id: usize, max_scroll: f32, delta: f32) -> bool {
        if !max_scroll.is_finite() || max_scroll <= 0.0 || !delta.is_finite() {
            return false;
        }
        let state = self.code_scroll_state_mut(block_id);
        state.anim_speed = 7.0;
        state.scroll_by(delta);
        state.clamp_target(0.0, max_scroll);
        state.clamp_current(0.0, max_scroll);
        state.target = state.target.round();
        true
    }

    pub(crate) fn begin_code_scroll_drag(
        &mut self,
        block_id: usize,
        g: &CodeScrollGeometry,
        pointer_x: f32,
        scale: f32,
    ) -> bool {
        self.end_code_scroll_drag();
        let state = self.code_scroll_state_mut(block_id);
        state.clamp_current(0.0, g.max_scroll);
        let offset = clamp_code_scroll_offset(state.current, g.max_scroll);
        let Some((drag_offset, target)) = code_scroll_thumb(g, offset, scale).and_then(|thumb| {
            crate::scroll::scrollbar_drag_target(
                pointer_x,
                g.track_x,
                g.track_w,
                thumb,
                g.max_scroll,
                None,
            )
        }) else {
            return false;
        };
        if !crate::app::mouse::apply_scrollbar_drag_target(state, target, drag_offset) {
            return false;
        }
        self.code_scroll_drag = Some(block_id);
        true
    }

    /// true -> нужен redraw (цель сменилась или drag завершён из-за пропавшей геометрии).
    pub(crate) fn drag_code_scroll_to(
        &mut self,
        g: &CodeScrollGeometry,
        pointer_x: f32,
        scale: f32,
    ) -> bool {
        let Some(block_id) = self.code_scroll_drag else {
            return false;
        };
        let state = self.code_scroll_state_mut(block_id);
        state.clamp_current(0.0, g.max_scroll);
        let drag_offset = state.drag_offset;
        let offset = clamp_code_scroll_offset(state.current, g.max_scroll);
        let target = state
            .is_dragging
            .then(|| code_scroll_thumb(g, offset, scale))
            .flatten()
            .and_then(|thumb| {
                crate::scroll::scrollbar_drag_target(
                    pointer_x,
                    g.track_x,
                    g.track_w,
                    thumb,
                    g.max_scroll,
                    Some(drag_offset),
                )
            });
        match target {
            Some((_, target))
                if crate::app::mouse::apply_scrollbar_drag_target(state, target, drag_offset) =>
            {
                true
            }
            _ => self.end_code_scroll_drag(),
        }
    }

    fn retain_active_code_scroll_x(&mut self) {
        self.code_scroll_x.retain(|entry| {
            entry.scroll.is_dragging || entry.scroll.current != 0.0 || entry.scroll.target != 0.0
        });
    }
}

// App wheel/drag input: frame + geometry резолвятся здесь, физика в MarkdownTabState.
impl crate::app::App {
    fn markdown_code_scroll_geometry_for(
        &self,
        block_id: usize,
    ) -> Option<(crate::render_view::markdown_read::CodeScrollGeometry, f32)> {
        if self.markdown_mode() != MarkdownMode::Read {
            return None;
        }
        let frame = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)?;
        let renderer = self.renderer.as_ref()?;
        let g = renderer.markdown_code_scroll_geometry(
            &self.markdown,
            self.editor.version,
            frame,
            self.scroll_y.current,
            block_id,
        )?;
        Some((g, renderer.scale_factor))
    }

    /// Shift/горизонтальный wheel над Reader; true -> событие поглощено code block (redraw уже запрошен).
    pub(crate) fn try_markdown_code_wheel(
        &mut self,
        hovered: Option<crate::ui_system::UiId>,
        mx: f32,
        my: f32,
        dx: f32,
        dy: f32,
        shift: bool,
    ) -> bool {
        if self.markdown_mode() == MarkdownMode::Read
            && (shift || dx.abs() > dy.abs())
            && matches!(
                hovered,
                Some(
                    crate::ui_system::UiId::MarkdownReadBody
                        | crate::ui_system::UiId::MarkdownCodeCopy(_)
                        | crate::ui_system::UiId::MarkdownCodeScrollbarX(_)
                )
            )
            && self.scroll_markdown_code_block_x_at(mx, my, if shift { dy } else { dx })
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
        false
    }

    pub(crate) fn scroll_markdown_code_block_x_at(&mut self, x: f32, y: f32, delta: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(block_id) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .zip(self.renderer.as_ref())
            .and_then(|(frame, renderer)| {
                renderer.markdown_read_code_block_at(
                    &self.markdown,
                    self.editor.version,
                    frame,
                    self.scroll_y.current,
                    x,
                    y,
                )
            })
        else {
            return false;
        };
        let Some((g, _)) = self.markdown_code_scroll_geometry_for(block_id) else {
            return false;
        };
        self.markdown
            .scroll_code_block_x(block_id, g.max_scroll, delta)
    }

    pub(crate) fn begin_markdown_code_scrollbar_drag_at(
        &mut self,
        block_id: usize,
        pointer_x: f32,
    ) -> bool {
        let Some((g, scale)) = self.markdown_code_scroll_geometry_for(block_id) else {
            self.markdown.end_code_scroll_drag();
            return false;
        };
        self.markdown
            .begin_code_scroll_drag(block_id, &g, pointer_x, scale)
    }

    pub(crate) fn drag_markdown_code_scrollbar_to(&mut self, pointer_x: f32) -> bool {
        let Some(block_id) = self.markdown.code_scroll_drag else {
            return false;
        };
        match self.markdown_code_scroll_geometry_for(block_id) {
            Some((g, scale)) => self.markdown.drag_code_scroll_to(&g, pointer_x, scale),
            None => self.markdown.end_code_scroll_drag(),
        }
    }
}

#[cfg(test)]
mod markdown_code_scroll_tests {
    use super::*;
    use crate::languages::markdown::MarkdownParseState;

    fn code_layout(
        source: &str,
        width: f32,
        scale: f32,
        advance: f32,
    ) -> MarkdownReadLayoutCache {
        let doc = MarkdownParseState::default()
            .parse(source)
            .expect("markdown parse");
        let mut builder = LayoutBuilder::new(
            source,
            width,
            scale,
            test_layout_text_metrics(scale),
            |_, _, _| advance,
        );
        builder.append_blocks(&doc.blocks, 0.0, 0, None);
        let (blocks, content_height) = builder.finish();
        let mut cache = MarkdownReadLayoutCache::default();
        cache.replace_layout(
            LayoutKey::new(1, width, scale, 16.0),
            blocks,
            content_height,
            source.len(),
        );
        cache
    }

    fn layout_with_advance(source: &str, width: f32, advance: f32) -> MarkdownReadLayoutCache {
        code_layout(source, width, 1.0, advance)
    }

    fn first_code(cache: &MarkdownReadLayoutCache) -> (&ReadBlock, &CodeBlock) {
        cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("code block")
    }

    fn first_code_block(cache: &MarkdownReadLayoutCache) -> &CodeBlock {
        first_code(cache).1
    }

    #[test]
    fn code_block_content_width_uses_longest_line_mono_advance() {
        let cache = layout_with_advance("```\nab\nabcdefghij\n```\n", 900.0, 10.0);
        let code = first_code_block(&cache);
        assert_eq!(code.content_width, 100.0);
    }

    #[test]
    fn only_overflowing_code_block_reserves_hscrollbar_and_has_max_scroll() {
        let long = format!("```\n{}\n```\n", "x".repeat(200));
        let short = "```\nx\n```\n";
        let wide = layout_with_advance(short, 900.0, 10.0);
        let narrow = layout_with_advance(&long, 900.0, 10.0);
        let (wb, wc) = first_code(&wide);
        let (nb, nc) = first_code(&narrow);
        let reserve = code_hscrollbar_reserve(1.0);
        assert_eq!((nb.bottom - nb.top) - (wb.bottom - wb.top), reserve);
        assert_eq!(
            code_scroll_geometry(0.0, 900.0, wc, wb.bottom, 1.0).max_scroll,
            0.0
        );
        let g = code_scroll_geometry(0.0, 900.0, nc, nb.bottom, 1.0);
        assert!(g.max_scroll > 0.0);
        assert_eq!(g.max_scroll, (nc.content_width - g.view_w).max(0.0));
        assert!(g.track_y + g.track_h <= nb.bottom);
        assert!(g.track_y >= nc.lines.last().unwrap().bottom);
        assert_eq!((g.track_x, g.track_w), (g.view_x, g.view_w));
        assert_eq!(g.track_h, code_hscrollbar_track_h(1.0));
    }

    #[test]
    fn code_scroll_overflow_reserve_and_geometry_agree_across_boundary() {
        let source = format!("```\n{}\n```\n", "x".repeat(40));
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let mut saw_overflow = false;
            let mut saw_fit = false;
            // content_width = 400px (unscaled mono advance); boundary ≈ 400 + 80 * scale.
            let mut width = 400.0 + 40.0 * scale;
            while width < 400.0 + 120.0 * scale {
                let cache = code_layout(&source, width, scale, 10.0);
                let (block, code) = first_code(&cache);
                let last_bottom = code.lines.last().unwrap().bottom;
                let unreserved = (last_bottom + code_block_padding(scale)).round();
                let reserve_applied = block.bottom != unreserved;
                if reserve_applied {
                    assert_eq!(block.bottom - unreserved, code_hscrollbar_reserve(scale));
                }
                let g = code_scroll_geometry(0.0, width, code, block.bottom, scale);
                assert_eq!(
                    reserve_applied,
                    g.max_scroll > 0.0,
                    "scale={scale} width={width} content={} view_w={} max={}",
                    code.content_width,
                    g.view_w,
                    g.max_scroll
                );
                assert_eq!(g.view_x.fract(), 0.0);
                assert_eq!(g.view_w.fract(), 0.0);
                assert_eq!(g.track_y.fract(), 0.0);
                saw_overflow |= reserve_applied;
                saw_fit |= !reserve_applied;
                width += 0.25;
            }
            assert!(saw_overflow && saw_fit, "scale={scale} sweep must span boundary");
        }
    }

    #[test]
    fn code_scroll_offset_clamps_to_layout_max_and_rounds() {
        let source = format!("```\n{}\n```\n", "x".repeat(200));
        let cache = layout_with_advance(&source, 900.0, 10.0);
        let (block, code) = first_code(&cache);
        let g = code_scroll_geometry(0.0, 900.0, code, block.bottom, 1.0);
        assert_eq!(code_block_scroll_offset(-5.0, code, 900.0, 1.0), 0.0);
        assert_eq!(code_block_scroll_offset(10.4, code, 900.0, 1.0), 10.0);
        assert_eq!(
            code_block_scroll_offset(1.0e9, code, 900.0, 1.0),
            g.max_scroll
        );
        assert_eq!(code_block_scroll_offset(f32::NAN, code, 900.0, 1.0), 0.0);
    }

    #[test]
    fn code_scroll_state_keeps_only_active_blocks() {
        let mut md = crate::app::MarkdownTabState::default();
        assert_eq!(md.code_scroll_x(7), 0.0);
        md.code_scroll_state_mut(7).jump_to(40.0);
        md.code_scroll_state_mut(9); // untouched
        assert!(!md.update_code_scroll_x(0.016));
        assert_eq!(md.code_scroll_x(7), 40.0);
        assert!(md.code_scroll_x.iter().all(|e| e.block_id != 9));
        assert_eq!(md.code_scroll_state_mut(7).anim_speed, 7.0);
    }

    #[test]
    fn code_scroll_drag_end_releases_state_and_drops_idle_entries() {
        let mut md = crate::app::MarkdownTabState::default();
        assert!(!md.end_code_scroll_drag());
        md.code_scroll_state_mut(3).is_dragging = true;
        md.code_scroll_drag = Some(3);
        assert!(!md.update_code_scroll_x(0.016));
        assert!(md.code_scroll_x.iter().any(|e| e.block_id == 3));
        assert!(md.end_code_scroll_drag());
        assert_eq!(md.code_scroll_drag, None);
        assert!(md.code_scroll_x.iter().all(|e| e.block_id != 3));

        md.code_scroll_state_mut(4).target = 30.0;
        assert!(md.update_code_scroll_x(0.016));
        assert!(md.code_scroll_x(4) > 0.0);
    }

    #[test]
    fn code_scroll_input_clamps_current_after_layout_shrinks_max_scroll() {
        let mut md = crate::app::MarkdownTabState::default();
        md.code_scroll_state_mut(5).jump_to(500.0);
        assert!(md.scroll_code_block_x(5, 100.0, -10.0));
        let state = md.code_scroll_state_mut(5);
        assert_eq!((state.current, state.target), (100.0, 100.0));

        md.code_scroll_state_mut(7).current = f32::NAN;
        assert!(md.scroll_code_block_x(7, 100.0, 10.0));
        assert_eq!(md.code_scroll_state_mut(7).current, 0.0);

        let g = CodeScrollGeometry {
            view_x: 0.0,
            view_w: 200.0,
            content_w: 300.0,
            max_scroll: 100.0,
            track_x: 0.0,
            track_y: 0.0,
            track_w: 200.0,
            track_h: 4.0,
        };
        md.code_scroll_state_mut(6).jump_to(500.0);
        let thumb = code_scroll_thumb(&g, 100.0, 1.0).expect("thumb");
        assert!(md.begin_code_scroll_drag(6, &g, thumb.start + thumb.len * 0.5, 1.0));
        assert_eq!(md.code_scroll_state_mut(6).current, 100.0);
        md.code_scroll_state_mut(6).current = 400.0;
        assert!(md.drag_code_scroll_to(&g, thumb.start + thumb.len * 0.5, 1.0));
        assert_eq!(md.code_scroll_state_mut(6).current, 100.0);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod markdown_code_scroll_gl_tests {
    use super::*;
    use crate::render_view::reviewer_stage2_integration::{fixture, read_frame};
    use crate::ui_system::UiId;

    #[test]
    fn scrolled_code_block_hit_test_and_scrollbar_registry_follow_scroll_x() {
        let source = format!("intro\n\n```\n{}\n```\n", "abcdefghij".repeat(40));
        let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        read_frame(&mut app);
        let (block_id, line_mid_y) = app
            .markdown
            .read_layout
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((
                    block.source_range.start,
                    ((code.lines[0].top + code.lines[0].bottom) * 0.5).round(),
                )),
                _ => None,
            })
            .expect("code block");
        let track = app
            .ui_registry
            .rect_for(UiId::MarkdownCodeScrollbarX(block_id))
            .expect("overflowing code block registers its horizontal scrollbar");

        let version = app.editor.version;
        let scroll_y = app.scroll_y.current;
        let renderer = app.renderer.as_mut().expect("renderer");
        let frame = (0.0, 0.0, renderer.width, renderer.height);
        let g = renderer
            .markdown_code_scroll_geometry(&app.markdown, version, frame, scroll_y, block_id)
            .expect("code scroll geometry");
        assert!(g.max_scroll > 0.0);
        let hit_pad = (3.0 * renderer.scale_factor).round();
        assert_eq!(
            track,
            (g.track_x, g.track_y - hit_pad, g.track_w, g.track_h + 2.0 * hit_pad)
        );
        let adv = mono_char_pixel_advance('a', 1.0, || renderer.char_advance('a'));
        assert_eq!(adv, renderer.char_advance('a').round());
        let mouse_x = g.view_x + 3.0 * adv + adv * 0.25;
        let mouse_y = line_mid_y - scroll_y.round();
        let before = renderer
            .markdown_read_source_byte_at(&app.markdown, version, frame, scroll_y, mouse_x, mouse_y)
            .expect("byte before scroll");

        app.markdown.code_scroll_state_mut(block_id).jump_to(adv * 5.0);
        read_frame(&mut app);
        let renderer = app.renderer.as_mut().expect("renderer");
        let after = renderer
            .markdown_read_source_byte_at(&app.markdown, version, frame, scroll_y, mouse_x, mouse_y)
            .expect("byte after scroll");
        assert_eq!(after, before + 5);

        // Thumb hover is an arrow-cursor blocker, not text I-beam or pointer hand.
        renderer.last_mouse_x = g.track_x + g.track_w * 0.5;
        renderer.last_mouse_y = g.track_y + g.track_h * 0.5;
        read_frame(&mut app);
        assert_eq!(
            app.ui_registry
                .find_at(g.track_x + g.track_w * 0.5, g.track_y + g.track_h * 0.5),
            Some(UiId::MarkdownCodeScrollbarX(block_id))
        );
        assert_eq!(app.ui_registry.cursor_code(), 0);
    }

    fn code_blocks_with_first_line_mid(app: &crate::app::App) -> Vec<(usize, f32)> {
        app.markdown
            .read_layout
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((
                    block.source_range.start,
                    ((code.lines[0].top + code.lines[0].bottom) * 0.5).round(),
                )),
                _ => None,
            })
            .collect()
    }

    fn code_scroll_target(app: &crate::app::App, block_id: usize) -> f32 {
        app.markdown
            .code_scroll_x
            .iter()
            .find(|entry| entry.block_id == block_id)
            .map_or(0.0, |entry| entry.scroll.target)
    }

    fn wheel(app: &mut crate::app::App, dx: f64, dy: f64) {
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(dx, dy),
        ));
        crate::app::mouse::clear_hover_popup(None);
    }

    #[test]
    fn shift_wheel_over_overflowing_code_block_scrolls_block_not_reader() {
        let mut source = format!("```\n{}\n```\n\n```\nshort\n```\n\n", "abcdefghij".repeat(40));
        for i in 0..60 {
            source.push_str(&format!("paragraph {i:03} alpha beta gamma delta\n\n"));
        }
        let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        read_frame(&mut app);
        let blocks = code_blocks_with_first_line_mid(&app);
        let (long_id, long_mid) = blocks[0];
        let (short_id, short_mid) = blocks[1];
        let body = app
            .ui_registry
            .rect_for(UiId::MarkdownReadBody)
            .expect("reader body");
        let version = app.editor.version;
        let scroll_y = app.scroll_y.current;
        let renderer = app.renderer.as_mut().expect("renderer");
        let long_g = renderer
            .markdown_code_scroll_geometry(&app.markdown, version, body, scroll_y, long_id)
            .expect("long geometry");
        let short_g = renderer
            .markdown_code_scroll_geometry(&app.markdown, version, body, scroll_y, short_id)
            .expect("short geometry");
        assert!(long_g.max_scroll > 100.0);
        assert_eq!(short_g.max_scroll, 0.0);
        renderer.last_mouse_x = long_g.view_x + 40.0;
        renderer.last_mouse_y = body.1 + long_mid - scroll_y.round();

        // Shift + vertical wheel (dy = +60 like the editor's shift → scroll_x.scroll_by(dy)).
        app.modifiers = winit::keyboard::ModifiersState::SHIFT;
        let reader_target = app.scroll_y.target;
        wheel(&mut app, 0.0, -60.0);
        assert_eq!(code_scroll_target(&app, long_id), 60.0);
        assert_eq!(app.scroll_y.target, reader_target);
        assert_eq!(app.markdown.code_scroll_state_mut(long_id).anim_speed, 7.0);

        // Horizontal-dominant trackpad delta without Shift also scrolls the block.
        app.modifiers = winit::keyboard::ModifiersState::empty();
        wheel(&mut app, -25.4, 3.0);
        assert_eq!(code_scroll_target(&app, long_id), 85.0);
        assert_eq!(app.scroll_y.target, reader_target);

        // Clamped and rounded to the layout max.
        app.modifiers = winit::keyboard::ModifiersState::SHIFT;
        wheel(&mut app, 0.0, -1.0e6);
        assert_eq!(code_scroll_target(&app, long_id), long_g.max_scroll);
        assert_eq!(app.scroll_y.target, reader_target);

        // Plain vertical wheel stays a Reader scroll.
        app.modifiers = winit::keyboard::ModifiersState::empty();
        wheel(&mut app, 0.0, -60.0);
        assert!(app.scroll_y.target > reader_target);
        assert_eq!(code_scroll_target(&app, long_id), long_g.max_scroll);

        // Shift wheel over a non-overflowing block falls through to the Reader path.
        let renderer = app.renderer.as_mut().expect("renderer");
        renderer.last_mouse_x = short_g.view_x + 10.0;
        renderer.last_mouse_y = body.1 + short_mid - scroll_y.round();
        app.modifiers = winit::keyboard::ModifiersState::SHIFT;
        let reader_target = app.scroll_y.target;
        wheel(&mut app, 0.0, -30.0);
        assert!(app.scroll_y.target > reader_target);
        assert!(app.markdown.code_scroll_x.iter().all(|e| e.block_id != short_id));
    }

    #[test]
    fn code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag() {
        let source = format!("intro\n\n```\n{}\n```\n", "abcdefghij".repeat(40));
        let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        read_frame(&mut app);
        let (block_id, _) = code_blocks_with_first_line_mid(&app)[0];
        let body = app
            .ui_registry
            .rect_for(UiId::MarkdownReadBody)
            .expect("reader body");
        let version = app.editor.version;
        let scroll_y = app.scroll_y.current;
        let renderer = app.renderer.as_mut().expect("renderer");
        let g = renderer
            .markdown_code_scroll_geometry(&app.markdown, version, body, scroll_y, block_id)
            .expect("code scroll geometry");
        let thumb = crate::scroll::scrollbar_thumb(
            g.track_x,
            g.track_w,
            g.view_w,
            g.content_w,
            0.0,
            (CODE_HSCROLLBAR_MIN_THUMB * renderer.scale_factor).round(),
        )
        .expect("thumb");
        renderer.last_mouse_x = thumb.start + thumb.len * 0.25;
        renderer.last_mouse_y = g.track_y + g.track_h * 0.5;
        read_frame(&mut app);
        assert_eq!(
            app.ui_registry
                .find_at(thumb.start + thumb.len * 0.25, g.track_y + g.track_h * 0.5),
            Some(UiId::MarkdownCodeScrollbarX(block_id))
        );

        app.reviewer_markdown_read_mouse_input(
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
        );
        assert_eq!(app.markdown.code_scroll_drag, Some(block_id));
        assert!(!app.markdown.read_selecting);
        {
            let state = app.markdown.code_scroll_state_mut(block_id);
            assert!(state.is_dragging);
            assert!((state.drag_offset - thumb.len * 0.25).abs() < 0.01);
            assert!(state.target.abs() < 0.01);
            assert_eq!(state.current, 0.0);
        }

        // handle_main_cursor_moved unwraps the native window before Reader routing,
        // so the headless fixture drives the drag entrypoint it delegates to.
        assert!(app.drag_markdown_code_scrollbar_to(g.track_x + g.track_w));
        {
            let state = app.markdown.code_scroll_state_mut(block_id);
            assert!(state.is_dragging);
            assert_eq!(state.target, g.max_scroll);
            assert_eq!(state.current, 0.0);
            assert_eq!(state.anim_speed, 15.0);
        }
        assert_eq!(app.scroll_y.target, scroll_y);

        // Release outside the thumb still ends the captured drag.
        let renderer = app.renderer.as_mut().expect("renderer");
        renderer.last_mouse_x = body.0 + 30.0;
        renderer.last_mouse_y = body.1 + body.3 * 0.5;
        app.reviewer_markdown_read_mouse_input(
            winit::event::ElementState::Released,
            winit::event::MouseButton::Left,
        );
        assert_eq!(app.markdown.code_scroll_drag, None);
        assert!(!app.markdown.read_selecting);
        assert!(!app.markdown.code_scroll_state_mut(block_id).is_dragging);
        assert!(app.markdown.update_code_scroll_x(0.016));
        assert!(app.markdown.code_scroll_x(block_id) > 0.0);
    }

    #[test]
    fn overflowing_code_block_draw_restores_reader_scissor() {
        let source = format!("```\n{}\n```\n", "abcdefghij".repeat(40));
        let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        read_frame(&mut app);
        let block = app
            .markdown
            .read_layout
            .blocks
            .iter()
            .find(|block| matches!(block.kind, ReadBlockKind::Code(_)))
            .cloned()
            .expect("code block");
        let renderer = app.renderer.as_mut().expect("renderer");
        let clip = (0.0, 0.0, renderer.width, renderer.height);
        renderer.flush();
        let restored = unsafe {
            use glow::HasContext;
            renderer.gl.enable(glow::SCISSOR_TEST);
            renderer.set_markdown_read_scissor(clip.0, clip.1, clip.2, clip.3);
            let mut expected = [0i32; 4];
            renderer
                .gl
                .get_parameter_i32_slice(glow::SCISSOR_BOX, &mut expected);
            renderer.draw_markdown_block(
                &block,
                app.markdown.read_source.as_str(),
                &[],
                0.0,
                0.0,
                0.0,
                clip.2,
                0.0,
                f32::MAX,
                ReadHighlights {
                    selection: None,
                    search_results: &[],
                    search_current_idx: None,
                },
                50.0,
                clip,
            );
            let mut actual = [0i32; 4];
            renderer
                .gl
                .get_parameter_i32_slice(glow::SCISSOR_BOX, &mut actual);
            renderer.gl.disable(glow::SCISSOR_TEST);
            (expected, actual)
        };
        assert_eq!(restored.0, [0, 0, 600, 180]);
        assert_eq!(restored.1, restored.0);
    }

    fn hold_code_scroll_drag(md: &mut crate::app::MarkdownTabState, block_id: usize) {
        md.code_scroll_state_mut(block_id).is_dragging = true;
        md.code_scroll_drag = Some(block_id);
    }

    fn assert_code_scroll_drag_ended(md: &crate::app::MarkdownTabState) {
        assert_eq!(md.code_scroll_drag, None);
        assert!(md.code_scroll_x.iter().all(|e| !e.scroll.is_dragging));
    }

    #[test]
    fn markdown_mode_switch_ends_code_scrollbar_drag() {
        let source = format!("intro\n\n```\n{}\n```\n", "abcdefghij".repeat(40));
        let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        read_frame(&mut app);
        hold_code_scroll_drag(&mut app.markdown, 7);
        app.set_markdown_mode(crate::app::MarkdownMode::Edit);
        assert_code_scroll_drag_ended(&app.markdown);
    }

    #[test]
    fn tab_switch_ends_code_scrollbar_drag() {
        let (_ctx, mut app) = fixture("intro\n", 600.0, 1.0);
        app.is_ide_mode = true;
        app.open_new_tab();
        app.open_new_tab();
        assert_eq!((app.tabs.len(), app.active_tab), (2, 1));
        hold_code_scroll_drag(&mut app.markdown, 7);
        app.switch_to_tab(0);
        assert_eq!(app.active_tab, 0);
        assert_code_scroll_drag_ended(&app.tabs[1].markdown);
        assert_code_scroll_drag_ended(&app.markdown);
    }

    #[test]
    fn external_changes_check_keeps_code_scrollbar_drag() {
        let (_ctx, mut app) = fixture("intro\n", 600.0, 1.0);
        app.is_ide_mode = true;
        app.open_new_tab();
        assert_eq!(app.tabs.len(), 1);
        hold_code_scroll_drag(&mut app.markdown, 7);
        // File-watcher path: swap-out/swap-in bookkeeping must not touch a held drag.
        app.start_external_changes_check();
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.markdown.code_scroll_drag, Some(7));
        assert!(
            app.markdown
                .code_scroll_x
                .iter()
                .any(|e| e.block_id == 7 && e.scroll.is_dragging)
        );
    }
}
