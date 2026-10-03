use crate::app::keymap_settings::KeymapSettingsState;
use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

const ROW_H: f32 = 38.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct KeymapRowMetrics {
    row_h: f32,
    first_baseline: f32,
    second_baseline: f32,
    chip_y: f32,
    chip_h: f32,
}

fn keymap_row_metrics(row_y: f32, scale: f32) -> KeymapRowMetrics {
    let row_y = row_y.round();
    KeymapRowMetrics {
        row_h: (ROW_H * scale).round().max(1.0),
        first_baseline: row_y + (17.0 * scale).round(),
        second_baseline: row_y + (31.0 * scale).round(),
        chip_y: row_y + (6.0 * scale).round(),
        chip_h: (25.0 * scale).round().max(1.0),
    }
}

fn row_is_visible(row_y: f32, row_h: f32, clip_y: f32, clip_h: f32) -> bool {
    row_y + row_h >= clip_y && row_y <= clip_y + clip_h
}

pub(super) fn draw(
    renderer: &mut Renderer,
    x: f32,
    width: f32,
    y: f32,
    clip: UiClipRect,
    state: &mut KeymapSettingsState,
    ui: &mut UiRegistry,
) {
    let s = renderer.scale_factor;
    let filter_y = y.round();
    let filter_placeholder = if state.filter.is_empty() { "Фильтр по команде или сочетанию" } else { "" };
    register_button(renderer, ui, UiId::SettingsKeymapFilter, x, filter_y, width * 0.66, 34.0 * s, filter_placeholder, [0.8, 0.8, 0.82, 1.0]);
    renderer.draw_string_scaled(&state.filter, x + 8.0 * s, (filter_y + 22.0 * s).round(), [1.0, 1.0, 1.0, 1.0], 0.8);
    if state.filter_focused {
        let scale = 0.8;
        let text_geometry = crate::app::single_line_input::single_line_text_geometry(
            x, width * 0.66, 8.0 * s, 0.0,
        );
        let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(s);
        let cursor = crate::app::single_line_input::single_line_cursor_geometry(
            &state.filter,
            state.filter_input.cursor,
            text_geometry.content_w,
            0.0,
            edge_pad,
            edge_pad,
            |ch| renderer.get_ui_glyph(ch).map(|glyph| glyph.advance * scale).unwrap_or(10.0 * scale),
        );
        renderer.push_rect(
            (text_geometry.text_start_x + cursor.cursor_x - cursor.scroll_x).round(),
            (filter_y + 8.0 * s).round(),
            crate::app::single_line_input::single_line_caret_width(s),
            (18.0 * s).round(),
            [1.0, 1.0, 1.0, 0.9],
        );
    }
    register_button(renderer, ui, UiId::SettingsKeymapResetAll, x + width * 0.69, filter_y, width * 0.31, 34.0 * s, "Сбросить все", [0.9, 0.78, 0.8, 1.0]);
    if let Some(label) = &state.skipped_label {
        renderer.draw_string_scaled(label, x, (filter_y + 54.0 * s).round(), [1.0, 0.38, 0.38, 1.0], 0.9);
    }
    if let Some(hint) = state.hint {
        renderer.draw_string_scaled(hint, x, (filter_y + 54.0 * s).round(), [0.9, 0.78, 0.56, 1.0], 0.9);
    }

    let list_y = filter_y + 72.0 * s - state.scroll.current.round();
    let mut row_y = list_y;
    let mut previous_context = None;
    let mut row_index = 0usize;
    let row_step = (ROW_H * s).round().max(1.0);
    ui.push_clip(clip);
    renderer.begin_tab_strip_scissor(clip.x, clip.y, clip.w, clip.h);
    for row in &state.rows {
        if !state.row_matches(row) { continue; }

        if previous_context != Some(row.context) {
            previous_context = Some(row.context);
            renderer.draw_string_scaled(context_label(row.context), x.round(), (row_y.round() + (18.0 * s).round()), [0.78, 0.65, 1.0, 1.0], 0.9);
            row_y += (25.0 * s).round();
        }
        if !row_is_visible(row_y, row_step, clip.y, clip.h) {
            row_y += row_step;
            row_index += 1;
            continue;
        }
        let metrics = keymap_row_metrics(row_y, s);
        if row_index.is_multiple_of(2) { renderer.push_rect(x.round(), row_y.round(), width.round(), metrics.row_h, [1.0, 1.0, 1.0, 0.025]); }
        let command_index = row.command as usize;
        let color = if row.conflicted { [1.0, 0.46, 0.42, 1.0] } else { [0.88, 0.89, 0.92, 1.0] };
        renderer.draw_string_scaled(row.label, (x + (5.0 * s).round()).round(), metrics.first_baseline, color, 0.83);
        renderer.draw_string_scaled(row.id, (x + (5.0 * s).round()).round(), metrics.second_baseline, [0.48, 0.49, 0.54, 1.0], 0.65);
        let mut chip_x = x + width * 0.49;
        if row.chords.is_empty() {
            renderer.draw_string_scaled("не назначено", chip_x, row_y.round() + (23.0 * s).round(), [0.52, 0.53, 0.58, 1.0], 0.74);
            chip_x += (82.0 * s).round();
        }
        for (chord_index, label) in row.chords.iter().enumerate() {
            let chip_w = (renderer.measure_ui_width(label, 0.76) + 25.0 * s).min(112.0 * s);
            renderer.push_rounded_rect(chip_x.round(), metrics.chip_y, chip_w.round(), metrics.chip_h, (5.0 * s).round(), [0.3, 0.27, 0.38, 1.0]);
            renderer.draw_string_scaled(label, (chip_x + (5.0 * s).round()).round(), row_y.round() + (23.0 * s).round(), [0.9, 0.88, 0.96, 1.0], 0.76);
            register_button(renderer, ui, UiId::SettingsKeymapRemove(command_index, chord_index), chip_x + chip_w - (19.0 * s).round(), metrics.chip_y, (19.0 * s).round(), metrics.chip_h, "×", [1.0, 0.58, 0.62, 1.0]);
            chip_x += chip_w + (4.0 * s).round();
        }
        if row.array_override {
            register_button(renderer, ui, UiId::SettingsKeymapAdd(command_index), chip_x.round(), metrics.chip_y, (28.0 * s).round(), metrics.chip_h, if state.recording.is_some_and(|recording| recording.command == row.command) { "…" } else { "+" }, [0.72, 0.78, 0.96, 1.0]);
        }
        if row.has_override {
            register_button(renderer, ui, UiId::SettingsKeymapReset(command_index), (x + width - (30.0 * s).round()).round(), metrics.chip_y, (26.0 * s).round(), metrics.chip_h, "↺", [0.84, 0.75, 0.96, 1.0]);
        }
        if row.terminal_warning {
            renderer.draw_string_scaled("терминал перехватывает", x + width * 0.76, (row_y + 34.0 * s).round(), [1.0, 0.7, 0.42, 1.0], 0.62);
        }
        row_y += row_step;
        row_index += 1;
    }
    renderer.end_tab_strip_scissor();
    state.max_scroll = (row_y - list_y - clip.h).max(0.0);
    state.scroll.clamp_target(0.0, state.max_scroll);
    if state.max_scroll > 0.0 {
        let bar = super::settings_ui::settings_scrollbar((x + width - 11.0 * s, clip.y, 14.0 * s, clip.h), clip.h, state.max_scroll, state.scroll.current, 6.0, super::settings_ui::KEYMAP_SCROLLBAR_MIN_THUMB, [0.7, 0.33, 0.54, 1.0]);
        renderer.draw_scrollbar(&bar, s, 1.0, Some(super::scrollbar_widget::ScrollbarHit { ui, id: UiId::SettingsKeymapScrollY, mx: renderer.last_mouse_x, my: renderer.last_mouse_y, blocker: false }));
    }
    ui.pop_clip();

    if let Some(conflict) = &state.pending_conflict {
        renderer.draw_string_scaled(&conflict.owner_labels, x, (clip.y + clip.h - 52.0 * s).round(), [1.0, 0.78, 0.55, 1.0], 0.9);
        register_button(renderer, ui, UiId::SettingsKeymapConflictAccept, x + width - 180.0 * s, clip.y + clip.h - 40.0 * s, 78.0 * s, 30.0 * s, "Да", [0.7, 0.88, 0.72, 1.0]);
        register_button(renderer, ui, UiId::SettingsKeymapConflictCancel, x + width - 92.0 * s, clip.y + clip.h - 40.0 * s, 78.0 * s, 30.0 * s, "Нет", [0.9, 0.65, 0.68, 1.0]);
    }
}

fn register_button(renderer: &mut Renderer, ui: &mut UiRegistry, id: UiId, x: f32, y: f32, w: f32, h: f32, label: &str, color: [f32; 4]) {
    let hovered = ui.register_rect(id, x, y, w, h, renderer.last_mouse_x, renderer.last_mouse_y);
    let background = if hovered {
        [0.38, 0.34, 0.46, 0.9]
    } else {
        [0.25, 0.26, 0.31, 0.8]
    };
    renderer.push_rounded_rect(x, y, w, h, 4.0 * renderer.scale_factor, background);
    renderer.draw_string_scaled(
        label,
        (x + 6.0 * renderer.scale_factor).round(),
        (y + h * 0.68).round(),
        color,
        0.78,
    );
}

fn context_label(context: crate::keymap::KeyContext) -> &'static str {
    use crate::keymap::KeyContext::*;
    match context {
        Global => "Общие",
        Editor => "Редактор",
        FileTree => "Дерево файлов",
        Terminal => "Терминал",
        Pdf => "PDF",
        Image => "Изображения",
        Git => "Git",
        DatabaseQuery => "SQL-консоль",
        DatabaseTable => "Таблица БД",
        ApiClient => "API Client",
        Markdown => "Markdown",
        Settings => "Настройки",
        Welcome => "Стартовый экран",
        ProjectSearch => "Поиск по проекту",
    }
}

#[cfg(test)]
mod tests {
    use super::{keymap_row_metrics, row_is_visible};

    #[test]
    fn row_baselines_and_chips_land_on_integer_pixels_at_fractional_scale() {
        for scale in [1.0, 1.25, 1.5, 1.75] {
            let metrics = keymap_row_metrics(81.35, scale);
            assert_eq!(metrics.row_h.fract(), 0.0);
            assert_eq!(metrics.first_baseline.fract(), 0.0);
            assert_eq!(metrics.second_baseline.fract(), 0.0);
            assert_eq!(metrics.chip_y.fract(), 0.0);
            assert_eq!(metrics.chip_h.fract(), 0.0);
        }
    }

    #[test]
    fn row_metrics_round_the_origin_before_applying_offsets() {
        let metrics = keymap_row_metrics(20.7, 1.25);
        assert_eq!(metrics.first_baseline, 42.0);
        assert_eq!(metrics.second_baseline, 60.0);
        assert_eq!(metrics.chip_y, 29.0);
    }

    #[test]
    fn row_height_stays_nonzero_for_small_scales() {
        assert_eq!(keymap_row_metrics(0.0, 0.01).row_h, 1.0);
        assert!(keymap_row_metrics(0.0, 0.01).chip_h >= 1.0);
    }

    #[test]
    fn row_clip_intersection_includes_partial_rows_only() {
        assert!(row_is_visible(99.0, 10.0, 100.0, 30.0));
        assert!(row_is_visible(129.0, 10.0, 100.0, 30.0));
        assert!(!row_is_visible(80.0, 10.0, 100.0, 30.0));
        assert!(!row_is_visible(131.0, 10.0, 100.0, 30.0));
    }

    #[test]
    fn row_clipping_accepts_a_zero_height_viewport_at_its_boundary() {
        assert!(row_is_visible(20.0, 1.0, 20.0, 0.0));
        assert!(!row_is_visible(22.0, 1.0, 20.0, 0.0));
    }

    #[test]
    fn row_clipping_keeps_a_row_touching_the_bottom_edge() {
        assert!(row_is_visible(129.0, 1.0, 100.0, 30.0));
    }
}
