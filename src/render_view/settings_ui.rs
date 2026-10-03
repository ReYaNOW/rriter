fn clamped_settings_tab(active_tab: usize, tab_count: usize) -> usize {
    active_tab.min(tab_count.saturating_sub(1))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SettingsSidebarTabMetrics {
    top: f32,
    row_h: f32,
    gap: f32,
}

fn settings_sidebar_tab_metrics(
    inner_h: f32,
    tab_count: usize,
    scale: f32,
) -> SettingsSidebarTabMetrics {
    // Whole-pixel top, gap and row height: rows are placed at
    // `top + i * (row_h + gap)` and must land on integral y at any scale.
    let top = (20.0 * scale).min((inner_h * 0.10).max(0.0)).round();
    if tab_count == 0 {
        return SettingsSidebarTabMetrics {
            top,
            row_h: 0.0,
            gap: 0.0,
        };
    }
    let bottom = (20.0 * scale).min((inner_h - top).max(0.0) * 0.12);
    let available = (inner_h - top - bottom).max(0.0);
    let desired_gap = (4.0 * scale).round();
    let gap = if tab_count > 1 {
        desired_gap.min((available / (tab_count - 1) as f32).floor())
    } else {
        0.0
    };
    let row_h = ((available - gap * tab_count.saturating_sub(1) as f32)
        / tab_count as f32)
        .clamp(0.0, 36.0 * scale)
        .floor();
    SettingsSidebarTabMetrics { top, row_h, gap }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SettingsModalLayout {
    pub outer: crate::ui_system::UiClipRect,
    pub inner: crate::ui_system::UiClipRect,
    pub sidebar_w: f32,
}

pub(crate) fn settings_modal_layout(
    width: f32,
    height: f32,
    scale: f32,
) -> SettingsModalLayout {
    let outer = crate::ui_system::fit_centered_rect(
        width, height, 1000.0 * scale, 700.0 * scale, 20.0 * scale,
    );
    let pad_top = (35.0 * scale).min(outer.h * 0.2);
    let pad_bottom = (30.0 * scale).min((outer.h - pad_top).max(0.0) * 0.2);
    let pad_h = (40.0 * scale).min(outer.w * 0.2);
    let inner = crate::ui_system::UiClipRect::new(
        outer.x + pad_h,
        outer.y + pad_top,
        (outer.w - pad_h * 2.0).max(0.0),
        (outer.h - pad_top - pad_bottom).max(0.0),
    );
    let sidebar_w = (200.0 * scale).min((inner.w * 0.35).max(0.0));
    SettingsModalLayout { outer, inner, sidebar_w }
}

pub(crate) fn animated_settings_modal_layout(
    width: f32,
    height: f32,
    scale: f32,
    anim_progress: f32,
) -> SettingsModalLayout {
    let mut layout = settings_modal_layout(width, height, scale);
    let start_y = height + 100.0 * scale;
    let target_y = layout.outer.y;
    let animated_y = (start_y + (target_y - start_y) * anim_progress.clamp(0.0, 1.0)).round();
    let dy = animated_y - target_y;
    layout.outer.y += dy;
    layout.inner.y += dy;
    layout
}

/// Vertical steps of the IDE tab scroll content, in unscaled pixels. The render
/// code and the scroll model must walk the same steps: the content height is
/// built from these values, so a change in `draw_settings` that is not mirrored
/// here would make the scrollbar thumb drift from what is drawn.
const SETTINGS_IDE_SCROLL_TOP: f32 = 52.0; // clip top, just below the tab pill
/// `draw_settings` starts the tab content cursor at `inner.y + 40 * s` and
/// advances it by the tab block step (`46 * s`) before the IDE rows, so the
/// first IDE row sits this far below the clip top.
const SETTINGS_IDE_CONTENT_TOP_INSET: f32 = 86.0 - SETTINGS_IDE_SCROLL_TOP;
const SETTINGS_IDE_SECTION_TITLE_STEP: f32 = 40.0;
const SETTINGS_IDE_WORKSPACE_ROW_STEP: f32 = 46.0;
const SETTINGS_IDE_ADD_WORKSPACE_STEP: f32 = 56.0;
const SETTINGS_IDE_DIVIDER_STEP: f32 = 20.0;
const SETTINGS_IDE_IGNORE_TITLE_STEP: f32 = 28.0;
const SETTINGS_IDE_IGNORE_HINT_STEP: f32 = 22.0;
const SETTINGS_IDE_IGNORE_EXAMPLES_STEP: f32 = 20.0;
const SETTINGS_IDE_IGNORE_INPUT_H: f32 = 34.0;
const SETTINGS_IDE_IGNORE_INPUT_GAP: f32 = 16.0;
const SETTINGS_IDE_CHIP_H: f32 = 28.0;
const SETTINGS_IDE_CHIP_GAP_X: f32 = 8.0;
const SETTINGS_IDE_CHIP_GAP_Y: f32 = 8.0;
/// Air kept under the last chip row when the tab is scrolled to the bottom.
const SETTINGS_IDE_CONTENT_BOTTOM_PAD: f32 = 24.0;

pub(crate) fn settings_ignore_input_rect(
    layout: SettingsModalLayout,
    scale: f32,
    workspace_count: usize,
    ide_scroll_y: f32,
) -> crate::ui_system::UiClipRect {
    let content_x = layout.inner.x + layout.sidebar_w + 30.0 * scale;
    let content_available_w =
        (layout.inner.x + layout.inner.w - content_x - 18.0 * scale).max(1.0);
    let add_gap = 10.0 * scale;
    let button_w = (110.0 * scale).min((content_available_w * 0.32).max(0.0));
    let effective_gap = add_gap.min((content_available_w - button_w).max(0.0));
    let input_w = (content_available_w - effective_gap - button_w).max(0.0);
    // Every step the IDE tab render walks from the modal inner top down to the
    // input row, so hit-testing and drawing share one vertical model.
    let input_top = (SETTINGS_IDE_CONTENT_TOP_INSET
        + SETTINGS_IDE_SCROLL_TOP
        + SETTINGS_IDE_SECTION_TITLE_STEP
        + workspace_count as f32 * SETTINGS_IDE_WORKSPACE_ROW_STEP
        + SETTINGS_IDE_ADD_WORKSPACE_STEP
        + SETTINGS_IDE_DIVIDER_STEP
        + SETTINGS_IDE_IGNORE_TITLE_STEP
        + SETTINGS_IDE_IGNORE_HINT_STEP
        + SETTINGS_IDE_IGNORE_EXAMPLES_STEP)
        * scale;
    let input_y = layout.inner.y + input_top - ide_scroll_y.round();
    crate::ui_system::UiClipRect::new(
        content_x,
        input_y,
        input_w,
        SETTINGS_IDE_IGNORE_INPUT_H * scale,
    )
}

pub(crate) fn settings_ide_content_height(
    workspace_count: usize,
    ignore_chip_widths: impl IntoIterator<Item = f32>,
    max_row_width: f32,
    scale: f32,
) -> f32 {
    let max_row_w = max_row_width.max(1.0);
    let chip_gap_x = SETTINGS_IDE_CHIP_GAP_X * scale;
    let mut chip_rows = 1usize;
    let mut row_w = 0.0;
    for width in ignore_chip_widths {
        if row_w + width > max_row_w && row_w > 0.0 {
            chip_rows += 1;
            row_w = 0.0;
        }
        row_w += width + chip_gap_x;
    }
    // Content below the clip top: the same steps `draw_settings` walks, the
    // chip rows it wraps, and the air kept under the last row.
    let fixed = SETTINGS_IDE_CONTENT_TOP_INSET
        + SETTINGS_IDE_SECTION_TITLE_STEP
        + SETTINGS_IDE_ADD_WORKSPACE_STEP
        + SETTINGS_IDE_DIVIDER_STEP
        + SETTINGS_IDE_IGNORE_TITLE_STEP
        + SETTINGS_IDE_IGNORE_HINT_STEP
        + SETTINGS_IDE_IGNORE_EXAMPLES_STEP
        + SETTINGS_IDE_IGNORE_INPUT_H
        + SETTINGS_IDE_IGNORE_INPUT_GAP
        + SETTINGS_IDE_CHIP_H
        + SETTINGS_IDE_CONTENT_BOTTOM_PAD;
    let workspaces_h = workspace_count as f32 * SETTINGS_IDE_WORKSPACE_ROW_STEP;
    // Extra chip rows advance by the row height plus the vertical gap.
    let extra_chip_rows_h = chip_rows.saturating_sub(1) as f32
        * (SETTINGS_IDE_CHIP_H + SETTINGS_IDE_CHIP_GAP_Y);
    (fixed + workspaces_h + extra_chip_rows_h) * scale
}

pub(crate) fn settings_ide_viewport_height(layout: SettingsModalLayout, scale: f32) -> f32 {
    (layout.inner.h - SETTINGS_IDE_SCROLL_TOP * scale).max(0.0)
}

/// Vertical steps of the Help tab, in unscaled pixels. `draw_settings` and
/// `get_faq_max_scroll` both walk the text with `Renderer::settings_faq_line_units`
/// and these steps, so the scrollbar range matches the wrapped rows drawn.
const SETTINGS_FAQ_TOP_PAD: f32 = 20.0;
const SETTINGS_FAQ_HEADER_STEP: f32 = 50.0;
const SETTINGS_FAQ_SHORTCUT_STEP: f32 = 38.0;
const SETTINGS_FAQ_TEXT_STEP: f32 = 30.0;
const SETTINGS_FAQ_BLANK_STEP: f32 = 15.0;
/// Extra height of every wrapped continuation row.
const SETTINGS_FAQ_WRAP_STEP: f32 = 24.0;
/// Baseline of the first description row when it is stacked under the key.
const SETTINGS_FAQ_STACKED_DESC_TOP: f32 = 30.0;
const SETTINGS_FAQ_KEY_COL_W: f32 = 260.0;
/// Narrowest description column kept beside the key before stacking below it.
const SETTINGS_FAQ_MIN_DESC_W: f32 = 160.0;
/// Space under the text when scrolled to the bottom (includes the top pad).
const SETTINGS_FAQ_BOTTOM_PAD: f32 = 80.0;

/// Width of the Help text column, from its left edge to the scrollbar.
fn settings_faq_column_w(inner_w: f32, sidebar_w: f32, scale: f32) -> f32 {
    inner_w - sidebar_w - 76.0 * scale
}

/// Borrows the FAQ text when it is contiguous in the gap buffer (the normal
/// case for this read-only editor), so the Help tab does not copy it per frame.
fn settings_faq_text(faq_editor: &Editor) -> std::borrow::Cow<'_, str> {
    match faq_editor.text_parts() {
        (text, "") | ("", text) => std::borrow::Cow::Borrowed(text),
        _ => std::borrow::Cow::Owned(faq_editor.get_full_text()),
    }
}

pub(crate) fn settings_faq_viewport_height(layout: SettingsModalLayout, scale: f32) -> f32 {
    // FAQ starts 70 px below the inner top and keeps a 20 px bottom inset.
    (layout.inner.h - 90.0 * scale).max(0.0)
}

pub(crate) fn settings_ide_max_scroll(
    layout: SettingsModalLayout,
    workspace_count: usize,
    ignore_chip_widths: impl IntoIterator<Item = f32>,
    scale: f32,
) -> f32 {
    let max_row_width = (layout.inner.w - layout.sidebar_w - 48.0 * scale).max(1.0);
    (settings_ide_content_height(
        workspace_count,
        ignore_chip_widths,
        max_row_width,
        scale,
    ) - settings_ide_viewport_height(layout, scale))
        .max(0.0)
}

pub(crate) fn settings_scrollbar(
    lane: (f32, f32, f32, f32),
    viewport: f32,
    max_scroll: f32,
    current: f32,
    thumb_thickness: f32,
    min_thumb: f32,
    thumb_color: [f32; 4],
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle};
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness,
            edge_gap: Some(5.0),
            track_pad: 0.0,
            min_thumb,
            radius: Some(3.0),
            track_color: None,
            thumb_color,
        },
        axis: ScrollbarAxis::Vertical,
        lane,
        extent: ScrollbarExtent::with_max(viewport, max_scroll, current),
    }
}

pub(crate) fn scroll_settings_content(
    scroll: &mut crate::scroll::ScrollState,
    delta_y: f32,
    max_scroll: f32,
) {
    scroll.anim_speed = 7.0;
    scroll.scroll_by(delta_y);
    scroll.clamp_target(0.0, max_scroll);
}

use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

pub(super) fn compact_settings_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let tail = text.chars().skip(count.saturating_sub(keep)).collect::<String>();
    format!("…{tail}")
}

pub(super) fn compact_settings_path(path: &std::path::Path, max_chars: usize) -> String {
    compact_settings_text(&path.to_string_lossy(), max_chars)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn get_faq_max_scroll(&mut self, faq_editor: &Editor, viewport_height: f32) -> f32 {
        let scale = self.scale_factor;
        let layout = settings_modal_layout(self.width, self.height, scale);
        let cw = settings_faq_column_w(layout.inner.w, layout.sidebar_w, scale);
        let text = settings_faq_text(faq_editor);
        let mut units = 0.0;
        for line in text.split('\n') {
            units += self.settings_faq_line_units(line, cw, None);
        }
        ((units + SETTINGS_FAQ_BOTTOM_PAD) * scale - viewport_height.max(0.0)).max(0.0)
    }

    /// Byte end of the first row of `text` that fits `max_w`, breaking after
    /// whitespace when possible. Uses the advances of `draw_string_scaled_stable`.
    fn settings_faq_wrap_end(&mut self, text: &str, max_w: f32, text_scale: f32) -> usize {
        let mut used = 0.0;
        let mut last_break = None;
        let mut end = 0usize;
        for (idx, ch) in text.char_indices() {
            let adv = if matches!(ch, '\n' | '\r' | '\u{FE0F}' | '\u{200D}') {
                0.0
            } else {
                self.get_ui_glyph(ch)
                    .map_or(0.0, |g| Self::snapped_text_advance(g.advance, text_scale))
            };
            if end > 0 && used + adv > max_w {
                return last_break.filter(|&break_at| break_at > 0).unwrap_or(end);
            }
            used += adv;
            end = idx + ch.len_utf8();
            if ch.is_whitespace() {
                last_break = Some(end);
            }
        }
        end
    }

    /// Word-wraps `text` into `max_w`; draws the rows when `origin` (x, first
    /// baseline) is given. Returns the number of rows, at least one.
    fn settings_faq_wrapped_text(
        &mut self,
        mut text: &str,
        max_w: f32,
        origin: Option<(f32, f32)>,
        color: [f32; 4],
    ) -> usize {
        let s = self.scale_factor;
        let mut rows = 0usize;
        loop {
            let end = self.settings_faq_wrap_end(text, max_w, 1.0);
            if let Some((x, y)) = origin {
                let row_y = y + (rows as f32 * SETTINGS_FAQ_WRAP_STEP * s).round();
                self.draw_string_scaled(text[..end].trim_end(), x, row_y, color, 1.0);
            }
            rows += 1;
            if end >= text.len() {
                return rows;
            }
            text = text[end..].trim_start();
            if text.is_empty() {
                return rows;
            }
        }
    }

    /// Height of one non-header FAQ line in unscaled pixels; draws it when
    /// `origin` (column x, baseline y) is given. Shortcut lines put the key in
    /// a left column and wrap the description beside it, or below the key when
    /// the column would be too narrow.
    fn settings_faq_line_units(
        &mut self,
        line: &str,
        cw: f32,
        origin: Option<(f32, f32)>,
    ) -> f32 {
        let s = self.scale_factor;
        let text_w = (cw - 8.0 * s).max(1.0);
        if line.starts_with("# ") {
            return SETTINGS_FAQ_HEADER_STEP;
        }
        if let Some(tab_idx) = line.find('\t') {
            let shortcut = &line[..tab_idx];
            let description = &line[tab_idx + 1..];
            let kbd_w = self.measure_ui_width(shortcut, 0.95) + 20.0 * s;
            let key_col_w = SETTINGS_FAQ_KEY_COL_W * s;
            let stacked = kbd_w + 12.0 * s > key_col_w
                || text_w - key_col_w < SETTINGS_FAQ_MIN_DESC_W * s;
            if let Some((x, y)) = origin {
                let kbd_h = 24.0 * s;
                let kbd_y = y - 18.0 * s;
                self.push_rounded_rect(
                    x - 1.0,
                    kbd_y - 1.0,
                    kbd_w + 2.0,
                    kbd_h + 2.0,
                    4.0 * s,
                    [0.306, 0.3176, 0.341, 1.0],
                );
                self.push_rounded_rect(x, kbd_y, kbd_w, kbd_h, 4.0 * s, [0.224, 0.231, 0.251, 1.0]);
                self.draw_string_scaled(
                    shortcut,
                    x + 10.0 * s,
                    y - 1.0 * s,
                    [0.875, 0.882, 0.902, 1.0],
                    0.95,
                );
            }
            let desc_color = [0.663, 0.690, 0.729, 1.0];
            if stacked {
                let desc_origin =
                    origin.map(|(x, y)| (x, y + (SETTINGS_FAQ_STACKED_DESC_TOP * s).round()));
                let rows = self.settings_faq_wrapped_text(description, text_w, desc_origin, desc_color);
                return SETTINGS_FAQ_STACKED_DESC_TOP
                    + SETTINGS_FAQ_SHORTCUT_STEP
                    + rows.saturating_sub(1) as f32 * SETTINGS_FAQ_WRAP_STEP;
            }
            let desc_origin = origin.map(|(x, y)| (x + key_col_w, y));
            let rows = self.settings_faq_wrapped_text(
                description,
                text_w - key_col_w,
                desc_origin,
                desc_color,
            );
            return SETTINGS_FAQ_SHORTCUT_STEP
                + rows.saturating_sub(1) as f32 * SETTINGS_FAQ_WRAP_STEP;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return SETTINGS_FAQ_BLANK_STEP;
        }
        let rows = self.settings_faq_wrapped_text(trimmed, text_w, origin, [0.875, 0.882, 0.902, 1.0]);
        SETTINGS_FAQ_TEXT_STEP + rows.saturating_sub(1) as f32 * SETTINGS_FAQ_WRAP_STEP
    }

    pub(crate) fn draw_settings(
        &mut self,
        anim_progress: f32,
        active_tab: usize,
        faq_editor: &Editor,
        scroll_y: f32,
        general_scroll_y: f32,
        database_scroll_y: f32,
        general_max_scroll: &mut f32,
        database_max_scroll: &mut f32,
        ide_workspaces: &[std::path::PathBuf],
        ide_ignore_patterns: &[String],
        settings_ignore_editor: &Editor,
        settings_ignore_focused: bool,
        settings_ignore_scroll_x: &mut f32,
        ide_scroll_y: f32,
        blink_alpha: f32,
        tool_paths: &crate::platform::ToolPaths,
        tool_installer: &crate::app::tool_installer::ToolInstaller,
        dart_settings: &crate::app::DartSettings,
        rust_settings: &crate::app::RustSettings,
        dart_tool_state: &crate::app::tool_installer::DartToolState,
        dart_lsp_status: Option<crate::lsp::LspServerStatus>,
        rust_row: Option<&crate::lsp::RustRowInfo>,
        database_settings: &crate::app::database::DatabaseSettings,
        ctrl_wheel_multiplier: f32,
        keymap_settings: &mut crate::app::keymap_settings::KeymapSettingsState,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> u8 {
        if anim_progress <= 0.0 {
            return 0;
        }
        // Элементы основного draw() (сайдбар, панели) не должны влиять
        // на курсор внутри модального окна настроек.
        ui_registry.reset_cursor_state();
        let s = self.scale_factor;
        // Smoothstep для предотвращения "вспышки" (резкого скачка яркости) при отсечении анимации в конце
        let smooth_p = anim_progress * anim_progress * (3.0 - 2.0 * anim_progress);
        let overlay_alpha = (smooth_p * 0.6).clamp(0.0, 1.0);
        self.push_rect(
            0.0,
            0.0,
            self.width,
            self.height,
            [0.0, 0.0, 0.0, overlay_alpha],
        );

        let layout = animated_settings_modal_layout(
            self.width,
            self.height,
            s,
            anim_progress,
        );
        let fitted = layout.outer;
        let w = fitted.w;
        let h = fitted.h;
        let y = fitted.y;
        let x = fitted.x;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        // 1. Внешнее окно с градиентом
        self.push_rounded_rect(
            x - 1.0,
            y - 1.0,
            w + 2.0,
            h + 2.0,
            10.0 * s,
            [0.224, 0.231, 0.251, 1.0],
        );
        self.push_rounded_rect_gradient(x, y, w, h, 10.0 * s, top_color, bottom_color);

        // 2. Внутренняя панель
        let ix = layout.inner.x;
        let iy = layout.inner.y;
        let iw = layout.inner.w;
        let ih = layout.inner.h;

        self.push_rounded_rect(
            ix - 1.0,
            iy - 1.0,
            iw + 2.0,
            ih + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(ix, iy, iw, ih, 8.0 * s, [0.15, 0.16, 0.20, 1.0]);

        self.flush();

        let sidebar_w = layout.sidebar_w;
        self.push_rect(ix + sidebar_w, iy, 1.0, ih, [1.0, 1.0, 1.0, 0.05]);

        let tabs = ["IDE", "Основные", "Редактор", "Внешний вид", "Помощь", "Базы данных", "Горячие клавиши"];
        let active_tab = clamped_settings_tab(active_tab, tabs.len());
        let tab_metrics = settings_sidebar_tab_metrics(ih, tabs.len(), s);
        let tab_step = tab_metrics.row_h + tab_metrics.gap;
        // Labels are clipped to the tab hitbox minus the text inset on the left
        // and a small right pad, so narrow sidebars never run into the divider.
        let tab_label_x = (ix + 25.0 * s).round();
        let tab_label_max_w = (ix + sidebar_w - 16.0 * s - tab_label_x).max(0.0);
        let mut label_scratch = std::mem::take(&mut self.scratch_buffer);
        for (i, title) in tabs.iter().enumerate() {
            let tab_rect_y = (iy + tab_metrics.top + i as f32 * tab_step).round();
            let tab_rect_h = tab_metrics.row_h;
            if tab_rect_h <= 0.0 {
                break;
            }

            let is_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::SettingsTab(i),
                ix + 10.0 * s,
                tab_rect_y,
                (sidebar_w - 20.0 * s).max(0.0),
                tab_rect_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );

            if i == active_tab {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    (sidebar_w - 20.0 * s).max(0.0),
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.1],
                );
            } else if is_hovered {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    (sidebar_w - 20.0 * s).max(0.0),
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let color = if i == active_tab {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.7, 0.7, 0.7, 1.0]
            };
            self.draw_tree_label_clipped(
                title,
                tab_label_x,
                (tab_rect_y + tab_rect_h * 0.5 + 5.0 * s).round(),
                tab_label_max_w,
                color,
                0.95,
                &mut label_scratch,
            );
        }
        self.scratch_buffer = label_scratch;

        let content_x = ix + sidebar_w + 30.0 * s;
        let content_available_w = (ix + iw - content_x - 18.0 * s).max(1.0);
        let content_title_x = content_x - 14.0 * s;
        let mut content_y = (iy + 40.0 * s).round();

        let tab_title = tabs[active_tab];
        let pill_w = self.measure_ui_width(tab_title, 1.1) + 28.0 * s;
        let pill_h = 30.0 * s;
        let pill_y = content_y - 22.0 * s;
        self.push_rounded_rect(
            content_title_x - 1.0,
            pill_y - 1.0,
            pill_w + 2.0,
            pill_h + 2.0,
            6.0 * s,
            [0.35, 0.26, 0.48, 1.0],
        );
        self.push_rounded_rect(
            content_title_x,
            pill_y,
            pill_w,
            pill_h,
            6.0 * s,
            [0.26, 0.20, 0.36, 1.0],
        );
        self.draw_string_scaled(
            tab_title,
            content_title_x + 14.0 * s,
            content_y,
            [1.0, 1.0, 1.0, 1.0],
            1.1,
        );
        content_y = (content_y + if active_tab == 4 { 30.0 * s } else { 46.0 * s }).round();
        // Content column below the tab title pill, shared by the clipped tabs.
        let settings_content_clip = crate::ui_system::UiClipRect::new(
            ix + sidebar_w, iy + 52.0 * s, (iw - sidebar_w).max(0.0), (ih - 52.0 * s).max(0.0),
        );

        if active_tab == 0 {
            // ── Scissor для скролла вкладки IDE ──────────────────────────────
            // Начало scissor = iy + SETTINGS_IDE_SCROLL_TOP * s (ниже пилюли заголовка iy+18..iy+48)
            let ide_content_area_x = ix + sidebar_w;
            let ide_content_area_w = iw - sidebar_w;
            let ide_content_area_h = ih - SETTINGS_IDE_SCROLL_TOP * s;
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (iy + SETTINGS_IDE_SCROLL_TOP * s + ide_content_area_h);
                self.gl.scissor(
                    ide_content_area_x.round() as i32,
                    scissor_y.round() as i32,
                    ide_content_area_w.round() as i32,
                    ide_content_area_h.round() as i32,
                );
            }
            ui_registry.push_clip(crate::ui_system::UiClipRect::new(
                ide_content_area_x,
                iy + SETTINGS_IDE_SCROLL_TOP * s,
                ide_content_area_w,
                ide_content_area_h,
            ));

            content_y -= ide_scroll_y.round();

            self.draw_string_scaled(
                "Рабочие области",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += SETTINGS_IDE_SECTION_TITLE_STEP * s;

            for (ws_idx, path) in ide_workspaces.iter().enumerate() {
                let path_str = path.to_string_lossy();
                let item_w = content_available_w;
                let item_h = 36.0 * s;

                self.push_rounded_rect(
                    content_x - 1.0,
                    content_y - 1.0,
                    item_w + 2.0,
                    item_h + 2.0,
                    6.0 * s,
                    [0.306, 0.3176, 0.341, 1.0],
                );
                self.push_rounded_rect(
                    content_x,
                    content_y,
                    item_w,
                    item_h,
                    6.0 * s,
                    [0.224, 0.231, 0.251, 1.0],
                );

                let mut path_scratch = String::new();
                self.draw_tree_label_clipped(
                    &path_str,
                    (content_x + 10.0 * s).round(),
                    (content_y + item_h * 0.70).round(),
                    (item_w - 54.0 * s).max(1.0),
                    self.theme.fg,
                    0.85,
                    &mut path_scratch,
                );

                let del_btn_x = content_x + item_w - 34.0 * s;
                let del_btn_y = content_y + 3.0 * s;
                let del_btn_size = 30.0 * s;
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeRemoveWorkspace(ws_idx),
                    del_btn_x,
                    del_btn_y,
                    del_btn_size,
                    del_btn_size,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                let btn_del = crate::widgets::IconButton {
                    x: del_btn_x,
                    y: del_btn_y,
                    size: del_btn_size,
                    icon: Some(crate::widgets::IconType::Discard),
                    is_active: false,
                    icon_size: Some(18.0 * s),
                    active_square_width: None,
                    custom_color: None,
                };
                btn_del.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
                content_y += SETTINGS_IDE_WORKSPACE_ROW_STEP * s;
            }

            let add_btn_y_reg = content_y.round();
            let add_btn_w = (190.0 * s).min(content_available_w);
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsIdeAddWorkspace,
                content_x,
                add_btn_y_reg,
                add_btn_w,
                36.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            let add_btn_text = "Добавить папку";
            let add_btn_icon_size = 20.0 * s;
            // `ButtonView` centres icon + text; when that content is wider than
            // the button, the centred start lands left of the button and the
            // «+» sticks out. Drop the icon then and keep the label.
            let add_btn_content_w = self.measure_ui_width(add_btn_text, 1.0)
                + add_btn_icon_size.round()
                + (8.0 * s).round()
                + 2.0 * (8.0 * s).round();
            let btn_add = crate::widgets::ButtonView {
                x: content_x,
                y: add_btn_y_reg,
                w: add_btn_w,
                h: 36.0 * s,
                text: add_btn_text,
                icon: (add_btn_content_w <= add_btn_w.round())
                    .then_some(crate::widgets::IconType::Plus),
                text_scale: 1.0,
                icon_size: add_btn_icon_size,
            };
            btn_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            content_y += SETTINGS_IDE_ADD_WORKSPACE_STEP * s;
            // ── Разделитель ───────────────────────────────────────────────
            self.push_rect(content_x, content_y, content_available_w, 1.0, [1.0, 1.0, 1.0, 0.07]);
            content_y += SETTINGS_IDE_DIVIDER_STEP * s;

            // ── Заголовок секции игноров ──────────────────────────────────
            self.draw_string_scaled(
                "Игнорируемые файлы и папки",
                content_x,
                content_y.round(),
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += SETTINGS_IDE_IGNORE_TITLE_STEP * s;

            // Пояснение
            self.draw_string_scaled(
                "Эти файлы и папки не будут показаны в дереве проекта.",
                content_x,
                content_y.round(),
                [0.45, 0.47, 0.55, 1.0],
                0.85,
            );
            content_y += SETTINGS_IDE_IGNORE_HINT_STEP * s;
            self.draw_string_scaled(
                "Примеры: *.log  temp/  .DS_Store  *.min.js  build  dist",
                content_x,
                content_y.round(),
                [0.35, 0.37, 0.44, 1.0],
                0.82,
            );
            content_y += SETTINGS_IDE_IGNORE_EXAMPLES_STEP * s;

            // ── Поле ввода + кнопка «Добавить» ───────────────────────────
            let content_w = content_available_w;
            let add_gap = 10.0 * s;
            let btn_add_w = (110.0 * s).min((content_w * 0.32).max(0.0));
            let effective_gap = add_gap.min((content_w - btn_add_w).max(0.0));
            let input_w = (content_w - effective_gap - btn_add_w).max(0.0);
            let input_h = SETTINGS_IDE_IGNORE_INPUT_H * s;
            let text_scale_input = 0.95f32; // Округленный скейл для ровного бейзлайна

            ui_registry.register_text_input(
                crate::ui_system::UiId::SettingsIdeIgnoreInput,
                content_x,
                content_y,
                input_w,
                input_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );

            let full_text = settings_ignore_editor.get_full_text();
            *settings_ignore_scroll_x = self.one_line_scroll_for_cursor(
                &full_text,
                settings_ignore_editor.cursor,
                text_scale_input,
                input_w - 16.0 * s,
                *settings_ignore_scroll_x,
            );
            self.draw_one_line_input_with_chrome(
                &full_text,
                settings_ignore_editor.cursor,
                settings_ignore_editor.selection_anchor,
                false,
                settings_ignore_focused,
                content_x,
                content_y,
                input_w,
                input_h,
                *settings_ignore_scroll_x,
                blink_alpha,
                text_scale_input,
                0.0,
                8.0 * s,
                6.0 * s,
            );
            if full_text.is_empty() {
                let mut placeholder_scratch = String::new();
                self.draw_tree_label_clipped(
                    "Паттерн или имя файла...",
                    (content_x + 8.0 * s).round(),
                    (content_y + input_h * 0.70).round(),
                    (input_w - 16.0 * s).max(1.0),
                    [0.30, 0.32, 0.40, 1.0],
                    text_scale_input,
                    &mut placeholder_scratch,
                );
            }

            // Кнопка «Добавить» — неактивна если поле пустое или только пробелы
            let trimmed_input = full_text.trim();
            let btn_add_x = content_x + input_w + effective_gap;
            let btn_add_y = content_y;
            if !trimmed_input.is_empty() {
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeAddIgnore,
                    btn_add_x,
                    btn_add_y,
                    btn_add_w,
                    input_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }

            let btn_ignore_add = crate::widgets::ButtonView {
                x: btn_add_x,
                y: btn_add_y,
                w: btn_add_w,
                h: input_h,
                text: "Добавить",
                icon: Some(crate::widgets::IconType::Plus),
                text_scale: 0.88,
                icon_size: 15.0 * s,
            };
            if trimmed_input.is_empty() {
                btn_ignore_add.render_disabled(self, s);
            } else {
                btn_ignore_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            }
            content_y += input_h + SETTINGS_IDE_IGNORE_INPUT_GAP * s;

            // ── Чипы пользовательских паттернов ──────────────────────────
            let chip_h = SETTINGS_IDE_CHIP_H * s;
            let chip_r = chip_h / 2.0;
            let pad_x = 12.0 * s;
            let chip_gap_x = SETTINGS_IDE_CHIP_GAP_X * s;
            let chip_gap_y = SETTINGS_IDE_CHIP_GAP_Y * s;
            let max_row_w = content_available_w;
            let mut chip_x = content_x;

            for (chip_idx, pattern) in ide_ignore_patterns.iter().enumerate() {
                let text_w = self.measure_ui_width(pattern, 0.88);
                let close_area = 22.0 * s;
                let chip_w = text_w + pad_x * 2.0 + close_area;

                if chip_x + chip_w > content_x + max_row_w && chip_x > content_x {
                    chip_x = content_x;
                    content_y += chip_h + chip_gap_y;
                }

                let chip_hov = self.last_mouse_x >= chip_x
                    && self.last_mouse_x <= chip_x + chip_w
                    && self.last_mouse_y >= content_y
                    && self.last_mouse_y <= content_y + chip_h;

                let close_hov = ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeRemoveIgnore(chip_idx),
                    chip_x + chip_w - close_area - 2.0 * s,
                    content_y,
                    close_area + 2.0 * s,
                    chip_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );

                let bg = if chip_hov {
                    [0.30, 0.18, 0.44, 1.0]
                } else {
                    [0.20, 0.13, 0.30, 1.0]
                };
                let border = if chip_hov {
                    [0.58, 0.34, 0.82, 1.0]
                } else {
                    [0.35, 0.22, 0.52, 1.0]
                };

                self.push_rounded_rect(
                    chip_x - 1.0,
                    content_y - 1.0,
                    chip_w + 2.0,
                    chip_h + 2.0,
                    chip_r + 1.0,
                    border,
                );
                self.push_rounded_rect(chip_x, content_y, chip_w, chip_h, chip_r, bg);

                self.draw_string_scaled(
                    pattern,
                    chip_x + pad_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.82, 0.68, 1.0, 1.0],
                    0.88,
                );

                let cross_color = if close_hov {
                    [1.0, 0.38, 0.58, 1.0]
                } else {
                    [0.50, 0.40, 0.65, 1.0]
                };
                self.draw_string_scaled(
                    "×",
                    chip_x + chip_w - close_area + 1.0 * s,
                    (content_y + chip_h * 0.70).round(),
                    cross_color,
                    0.95,
                );

                chip_x += chip_w + chip_gap_x;
            }

            if ide_ignore_patterns.is_empty() {
                self.draw_string_scaled(
                    "Нет пользовательских правил",
                    content_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.28, 0.30, 0.36, 1.0],
                    0.88,
                );
            }

            ui_registry.pop_clip();
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            // ── Скроллбар для вкладки IDE ─────────────────────────────────
            let ide_total_h = settings_ide_content_height(
                ide_workspaces.len(),
                ide_ignore_patterns.iter().map(|pattern| {
                    self.measure_ui_width(pattern, 0.88) + pad_x * 2.0 + 22.0 * s
                }),
                content_available_w,
                s,
            );
            let max_scroll = (ide_total_h - ide_content_area_h).max(0.0);
            if max_scroll > 0.0 {
                let track_h = ide_content_area_h;
                let sb_x = (ix + iw - 14.0 * s).round();
                let bar = settings_scrollbar(
                    (sb_x - 5.0 * s, iy + SETTINGS_IDE_SCROLL_TOP * s, 16.0 * s, track_h),
                    track_h, max_scroll, ide_scroll_y, 6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
                );
                self.draw_scrollbar(&bar, s, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                    ui: &mut *ui_registry,
                    id: crate::ui_system::UiId::SettingsIdeScrollY,
                    mx: self.last_mouse_x,
                    my: self.last_mouse_y,
                    blocker: false,
                }));
            }
        } else if active_tab == 1 {
            self.draw_settings_general_tab(
                content_x,
                content_available_w,
                content_y,
                layout.inner,
                settings_content_clip,
                general_scroll_y,
                general_max_scroll,
                tool_paths,
                tool_installer,
                dart_settings,
                rust_settings,
                dart_tool_state,
                dart_lsp_status,
                rust_row,
                ui_registry,
            );
        } else if active_tab == 2 {
            self.draw_string_scaled(
                "Размер шрифта: 14px",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 30.0 * s;
            self.draw_string_scaled(
                "Межстрочный интервал: 1.5",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 42.0 * s;
            self.draw_editor_ctrl_wheel_setting(
                content_x,
                content_available_w,
                content_y,
                ctrl_wheel_multiplier,
                ui_registry,
            );
        } else if active_tab == 3 {
            self.draw_string_scaled(
                "Тема: Dracula (По умолчанию)",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
        } else if active_tab == 4 {
            self.flush();
            let text_area_y = content_y;
            let text_area_h = ih - (text_area_y - iy) - 20.0 * s;

            let start_x = content_x;
            let main_header_x = content_x - 14.0 * s;
            let render_scroll_y = scroll_y.round();
            let text_base_y = (text_area_y - render_scroll_y).round();
            let text = settings_faq_text(faq_editor);

            let cw = settings_faq_column_w(iw, sidebar_w, s);
            let mut main_header_drawn = false;
            let mut units = 0.0;

            // The clip ends at the scrollbar column: text never reaches it,
            // and an unbreakable word is cut there instead of past the panel.
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (text_area_y + text_area_h);
                let clip_x = (content_x - 10.0 * s).round();
                self.gl.scissor(
                    clip_x as i32,
                    scissor_y.round() as i32,
                    ((start_x + cw).round() - clip_x).max(0.0) as i32,
                    text_area_h.round() as i32,
                );
            }

            for line in text.split('\n') {
                let text_y = text_base_y + ((SETTINGS_FAQ_TOP_PAD + units) * s).round();
                let is_header = line.starts_with("# ");

                if is_header {
                    let header_text = &line[2..];
                    let is_main = !main_header_drawn && header_text == tab_title;

                    if is_main {
                        let pill_w = self.measure_ui_width(header_text, 1.05) + 24.0 * s;
                        let pill_h = 26.0 * s;
                        let pill_y = text_y - 19.0 * s;

                        self.push_rounded_rect(
                            main_header_x - 1.0,
                            pill_y - 1.0,
                            pill_w + 2.0,
                            pill_h + 2.0,
                            5.0 * s,
                            [0.35, 0.26, 0.48, 1.0],
                        );
                        self.push_rounded_rect(
                            main_header_x,
                            pill_y,
                            pill_w,
                            pill_h,
                            5.0 * s,
                            [0.26, 0.20, 0.36, 1.0],
                        );
                        self.draw_string_scaled(
                            header_text,
                            main_header_x + 12.0 * s,
                            text_y,
                            [1.0, 1.0, 1.0, 1.0],
                            1.05,
                        );
                        main_header_drawn = true;
                    } else {
                        let sep_y = text_y + 10.0 * s;
                        let sep_x = main_header_x;
                        let sep_w = (cw - 10.0 * s).max(0.0);
                        self.draw_string_scaled(
                            header_text,
                            start_x,
                            text_y,
                            [0.875, 0.882, 0.902, 1.0],
                            1.05,
                        );
                        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.10]);
                    }

                    units += SETTINGS_FAQ_HEADER_STEP;
                    continue;
                }

                units += self.settings_faq_line_units(line, cw, Some((start_x, text_y)));
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            // Same walk as `get_faq_max_scroll`, without a second pass.
            let max_scroll = ((units + SETTINGS_FAQ_BOTTOM_PAD) * s - text_area_h.max(0.0)).max(0.0);
            if max_scroll > 0.0 {
                let track_h = text_area_h;
                let scroll_x = (start_x + cw + 5.0 * s).round();
                let bar = settings_scrollbar(
                    (scroll_x - 5.0 * s, text_area_y, 16.0 * s, track_h),
                    track_h, max_scroll, scroll_y, 6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
                );
                self.draw_scrollbar(&bar, s, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                    ui: &mut *ui_registry,
                    id: crate::ui_system::UiId::SettingsFaqScrollY,
                    mx: self.last_mouse_x,
                    my: self.last_mouse_y,
                    blocker: false,
                }));
            }
        } else if active_tab == 5 {
            // Ten fixed-height rows overflow short windows (1280x720 at 4/3): clip them
            // to the modal like the tools tab instead of drawing past its bottom.
            self.begin_settings_content_clip(ui_registry, settings_content_clip);
            let unscrolled_content_y = content_y.round();
            self.draw_database_settings_tab(
                database_settings,
                content_x,
                content_available_w,
                content_y,
                database_scroll_y,
                ui_registry,
            );
            *database_max_scroll = (super::settings_database_ui::database_settings_content_bottom(
                unscrolled_content_y,
                s,
            ) - (iy + ih))
                .max(0.0);
            if *database_max_scroll > 0.0 {
                let sb_x = (ix + iw - 14.0 * s).round();
                let bar = settings_scrollbar(
                    (sb_x - 5.0 * s, settings_content_clip.y, 16.0 * s, settings_content_clip.h),
                    settings_content_clip.h, *database_max_scroll, database_scroll_y,
                    6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
                );
                self.draw_scrollbar(&bar, s, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                    ui: &mut *ui_registry,
                    id: crate::ui_system::UiId::SettingsDatabaseScrollY,
                    mx: self.last_mouse_x,
                    my: self.last_mouse_y,
                    blocker: false,
                }));
            }
            self.end_settings_content_clip(ui_registry);
        } else if active_tab == 6 {
            super::settings_keymap_ui::draw(self, content_x, content_available_w, content_y, settings_content_clip, keymap_settings, ui_registry);
        }

        if tool_installer.is_log_open() {
            self.draw_tool_install_log_modal(tool_installer, ui_registry);
        }

        self.flush();
        if ui_registry.wants_text() {
            2
        } else if ui_registry.wants_pointer() {
            1
        } else {
            0
        }
    }

    fn draw_tool_install_log_modal(
        &mut self,
        tool_installer: &crate::app::tool_installer::ToolInstaller,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let s = self.scale_factor;
        let fitted = crate::ui_system::fit_centered_rect(
            self.width,
            self.height,
            720.0 * s,
            crate::app::tool_installer::log_modal_height(self.height, s),
            16.0 * s,
        );
        let modal_w = fitted.w;
        let modal_h = fitted.h;
        let modal_x = fitted.x;
        let modal_y = fitted.y;

        self.flush();
        ui_registry.reset_cursor_state();
        ui_registry.register_blocker(
            crate::ui_system::UiId::SettingsToolInstallLogBackdrop,
            0.0,
            0.0,
            self.width,
            self.height,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.70]);
        self.push_rounded_rect(
            modal_x - 1.0,
            modal_y - 1.0,
            modal_w + 2.0,
            modal_h + 2.0,
            9.0 * s,
            [0.38, 0.30, 0.52, 1.0],
        );
        self.push_rounded_rect(
            modal_x,
            modal_y,
            modal_w,
            modal_h,
            9.0 * s,
            [0.095, 0.102, 0.14, 1.0],
        );

        let target = tool_installer
            .target()
            .map(crate::platform::ToolKind::label)
            .unwrap_or("Инструмент");
        let heading = format!("{target} · {}", tool_installer.phase().label());
        self.draw_string_scaled_stable(
            &heading,
            (modal_x + 18.0 * s).round(),
            (modal_y + 28.0 * s).round(),
            [0.92, 0.92, 0.96, 1.0],
            0.95,
        );
        self.draw_string_scaled_stable(
            &compact_settings_text(tool_installer.detail(), 86),
            (modal_x + 18.0 * s).round(),
            (modal_y + 50.0 * s).round(),
            [0.60, 0.62, 0.72, 1.0],
            0.72,
        );

        let log_x = (modal_x + 18.0 * s).round();
        let log_y = (modal_y + 66.0 * s).round();
        let log_w = (modal_w - 36.0 * s).round();
        let log_h = crate::app::tool_installer::log_viewport_height(self.height, s);
        self.push_rounded_rect(
            log_x - 1.0,
            log_y - 1.0,
            log_w + 2.0,
            log_h + 2.0,
            5.0 * s,
            [0.22, 0.23, 0.30, 1.0],
        );
        self.push_rounded_rect(
            log_x,
            log_y,
            log_w,
            log_h,
            5.0 * s,
            [0.055, 0.060, 0.083, 1.0],
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::SettingsToolInstallLogBody,
            log_x,
            log_y,
            log_w,
            log_h,
            self.last_mouse_x,
            self.last_mouse_y,
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                log_x.round() as i32,
                (self.height - log_y - log_h).round() as i32,
                log_w.round().max(0.0) as i32,
                log_h.round().max(0.0) as i32,
            );
        }

        let line_h = crate::app::tool_installer::log_line_height(s);
        let scroll = tool_installer.log_scroll_y().max(0.0).round();
        let first = (scroll / line_h).floor() as usize;
        let mut draw_y =
            (log_y + (15.0 * s).round() - (scroll % line_h)).round();
        let visible = (log_h / line_h).ceil() as usize + 2;
        let max_chars = ((log_w / (7.0 * s)).floor() as usize).max(24);
        for line in tool_installer.logs().iter().skip(first).take(visible) {
            let (prefix, color) = match line.kind {
                crate::app::tool_installer::ToolInstallLogKind::Info => {
                    ("[info] ", [0.62, 0.66, 0.76, 1.0])
                }
                crate::app::tool_installer::ToolInstallLogKind::Output => {
                    ("[out] ", [0.80, 0.81, 0.84, 1.0])
                }
                crate::app::tool_installer::ToolInstallLogKind::Error => {
                    ("[error] ", [0.96, 0.52, 0.52, 1.0])
                }
                crate::app::tool_installer::ToolInstallLogKind::Success => {
                    ("[ok] ", [0.48, 0.86, 0.60, 1.0])
                }
            };
            let text = format!("{prefix}{}", line.text);
            self.draw_string_scaled_stable(
                &compact_settings_text(&text, max_chars),
                (log_x + 10.0 * s).round(),
                draw_y.round(),
                color,
                0.70,
            );
            draw_y = (draw_y + line_h).round();
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let content_h = (tool_installer.logs().len().max(1) as f32 * line_h
            + (12.0 * s).round())
        .round();
        if content_h > log_h {
            let bar = settings_scrollbar(
                (log_x + log_w - 16.0 * s, log_y + 6.0 * s, 16.0 * s, log_h - 12.0 * s),
                log_h, content_h - log_h, scroll, 4.0, 28.0, [0.56, 0.38, 0.70, 0.95],
            );
            self.draw_scrollbar(&bar, s, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                ui: &mut *ui_registry,
                id: crate::ui_system::UiId::SettingsToolInstallLogScrollY,
                mx: self.last_mouse_x,
                my: self.last_mouse_y,
                blocker: false,
            }));
        }

        let button_y = (modal_y + modal_h - 43.0 * s).round();
        let inner_w = (modal_w - 36.0 * s).max(1.0);
        let button_gap = (8.0 * s).min(inner_w * 0.04);
        let running = tool_installer.is_running();
        let base_widths: &[f32] = if running { &[90.0, 108.0, 78.0] } else { &[108.0, 78.0] };
        let usable = (inner_w - button_gap * base_widths.len().saturating_sub(1) as f32).max(1.0);
        let base_total = base_widths.iter().sum::<f32>() * s;
        let ratio = (usable / base_total.max(1.0)).min(1.0);
        let mut button_x = modal_x + 18.0 * s;
        let mut draw_button = |renderer: &mut Self,
                               ui_registry: &mut crate::ui_system::UiRegistry,
                               id: crate::ui_system::UiId,
                               label: &str,
                               base_w: f32| {
            let width = base_w * s * ratio;
            ui_registry.register_rect(
                id,
                button_x,
                button_y,
                width,
                29.0 * s,
                renderer.last_mouse_x,
                renderer.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x: button_x,
                y: button_y,
                w: width,
                h: 29.0 * s,
                text: label,
                icon: None,
                text_scale: (0.70 * ratio).clamp(0.50, 0.70),
                icon_size: 0.0,
            }
            .render(
                renderer,
                renderer.last_mouse_x,
                renderer.last_mouse_y,
                s,
                false,
            );
            button_x += width + button_gap;
        };
        if running {
            draw_button(
                self,
                ui_registry,
                crate::ui_system::UiId::SettingsCancelToolInstall,
                "Отменить",
                90.0,
            );
        }
        draw_button(
            self,
            ui_registry,
            crate::ui_system::UiId::SettingsCopyToolInstallLog,
            "Копировать",
            108.0,
        );
        draw_button(
            self,
            ui_registry,
            crate::ui_system::UiId::SettingsCloseToolInstallLog,
            "Закрыть",
            78.0,
        );
    }
}

#[cfg(test)]
mod settings_ui_tests {
    use super::{
        clamped_settings_tab, settings_ide_content_height, settings_ide_max_scroll,
        settings_faq_viewport_height, settings_modal_layout, settings_scrollbar,
        settings_sidebar_tab_metrics,
    };

    #[test]
    fn invalid_settings_tab_is_clamped_before_rendering() {
        assert_eq!(clamped_settings_tab(0, 6), 0);
        assert_eq!(clamped_settings_tab(99, 7), 6);
        assert_eq!(clamped_settings_tab(4, 0), 0);
    }

    #[test]
    fn settings_sidebar_tabs_fit_inside_short_modal() {
        let layout = settings_modal_layout(720.0, 300.0, 1.0);
        let metrics = settings_sidebar_tab_metrics(layout.inner.h, 6, 1.0);
        let bottom = metrics.top + metrics.row_h * 6.0 + metrics.gap * 5.0;

        assert!(metrics.row_h < 36.0);
        assert!(bottom <= layout.inner.h + f32::EPSILON);

        let normal = settings_sidebar_tab_metrics(635.0, 6, 1.0);
        assert_eq!(normal.top, 20.0);
        assert_eq!(normal.row_h, 36.0);
        assert_eq!(normal.gap, 4.0);
    }

    #[test]
    fn ide_scroll_height_uses_the_same_wrapping_model_as_rendering() {
        let no_wrap = settings_ide_content_height(2, [100.0, 120.0], 460.0, 1.0);
        let wrapped = settings_ide_content_height(2, [430.0, 120.0, 450.0], 460.0, 1.0);
        assert!(wrapped > no_wrap);

        let layout = settings_modal_layout(1000.0, 500.0, 1.0);
        let max_row_width = (layout.inner.w - layout.sidebar_w - 48.0).max(1.0);
        let rendered_height =
            settings_ide_content_height(2, [430.0, 120.0, 450.0], max_row_width, 1.0);
        let max_scroll = settings_ide_max_scroll(layout, 2, [430.0, 120.0, 450.0], 1.0);
        assert_eq!(max_scroll, (rendered_height - (layout.inner.h - 52.0)).max(0.0));
    }

    #[test]
    fn faq_viewport_matches_the_rendered_text_clip() {
        let layout = settings_modal_layout(1000.0, 700.0, 1.0);
        assert_eq!(settings_faq_viewport_height(layout, 1.0), layout.inner.h - 90.0);
    }

    #[test]
    fn settings_scrollbar_uses_shared_widget_geometry() {
        let bar = settings_scrollbar(
            (0.0, 50.0, 16.0, 300.0), 300.0, 600.0, 300.0,
            6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
        );
        let geometry = bar.geometry(1.0).expect("scrollbar should be visible");
        assert_eq!(geometry.thumb.start, 150.0);
        assert_eq!(geometry.thumb.len, 100.0);
    }

    #[test]
    fn settings_scrollbar_drag_uses_shared_target_only_motion() {
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 300.0;
        scroll.target = 300.0;
        let bar = settings_scrollbar(
            (0.0, 50.0, 16.0, 300.0), 300.0, 600.0, scroll.current,
            6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
        );
        let geometry = bar.geometry(1.0).expect("scrollbar should be visible");
        let pointer = geometry.thumb.start + geometry.thumb.len * 0.25;

        assert!(crate::app::mouse::press_scrollbar(
            &mut scroll,
            Some(geometry),
            0.0,
            pointer,
        ).is_some());
        assert_eq!(scroll.current, 300.0);
        let drag_offset = scroll.drag_offset;
        let first_target = scroll.target;

        assert!(crate::app::mouse::drag_scrollbar(
            &mut scroll,
            Some(geometry),
            0.0,
            pointer + 80.0,
        ).is_some());
        assert_eq!(scroll.current, 300.0);
        assert_ne!(scroll.target, first_target);
        assert_eq!(scroll.drag_offset, drag_offset);
    }
}
