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
    let top = (20.0 * scale).min((inner_h * 0.10).max(0.0));
    if tab_count == 0 {
        return SettingsSidebarTabMetrics {
            top,
            row_h: 0.0,
            gap: 0.0,
        };
    }
    let bottom = (20.0 * scale).min((inner_h - top).max(0.0) * 0.12);
    let available = (inner_h - top - bottom).max(0.0);
    let desired_gap = 4.0 * scale;
    let gap = if tab_count > 1 {
        desired_gap.min(available / (tab_count - 1) as f32)
    } else {
        0.0
    };
    let row_h = ((available - gap * tab_count.saturating_sub(1) as f32)
        / tab_count as f32)
        .clamp(0.0, 36.0 * scale);
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
    let input_y = layout.inner.y
        + (272.0 + workspace_count as f32 * 46.0) * scale
        - ide_scroll_y.round();
    crate::ui_system::UiClipRect::new(content_x, input_y, input_w, 34.0 * scale)
}


pub(crate) fn settings_ide_content_height(
    workspace_count: usize,
    ignore_chip_widths: impl IntoIterator<Item = f32>,
    max_row_width: f32,
    scale: f32,
) -> f32 {
    let workspace_h = workspace_count as f32 * 46.0 * scale + 126.0 * scale;
    let chip_h = 28.0 * scale;
    let chip_gap_y = 8.0 * scale;
    let chip_gap_x = 8.0 * scale;
    let max_row_w = max_row_width.max(1.0);
    let mut chip_rows = 1usize;
    let mut row_w = 0.0;
    for width in ignore_chip_widths {
        if row_w + width > max_row_w && row_w > 0.0 {
            chip_rows += 1;
            row_w = 0.0;
        }
        row_w += width + chip_gap_x;
    }
    let ignore_h = 160.0 * scale + chip_rows as f32 * (chip_h + chip_gap_y);
    workspace_h + ignore_h
}

pub(crate) fn settings_ide_viewport_height(layout: SettingsModalLayout, scale: f32) -> f32 {
    (layout.inner.h - 52.0 * scale).max(0.0)
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

pub(crate) fn settings_scrollbar_thumb(
    track_y: f32,
    track_h: f32,
    max_scroll: f32,
    current: f32,
    scale: f32,
) -> Option<crate::scroll::ScrollbarThumb> {
    crate::scroll::scrollbar_thumb(
        track_y,
        track_h,
        track_h,
        track_h + max_scroll,
        current,
        40.0 * scale,
    )
}

use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

fn compact_settings_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let tail = text.chars().skip(count.saturating_sub(keep)).collect::<String>();
    format!("…{tail}")
}

fn compact_settings_path(path: &std::path::Path, max_chars: usize) -> String {
    compact_settings_text(&path.to_string_lossy(), max_chars)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn get_faq_max_scroll(&mut self, faq_editor: &Editor, viewport_height: f32) -> f32 {
        let scale = self.scale_factor;
        let mut total_h = 0.0;

        for line in faq_editor.get_full_text().split('\n') {
            if line.starts_with("# ") {
                total_h += 50.0 * scale;
            } else if line.contains('\t') {
                total_h += 38.0 * scale;
            } else if !line.trim().is_empty() {
                total_h += 30.0 * scale;
            } else {
                total_h += 15.0 * scale;
            }
        }

        total_h += 80.0 * scale;
        (total_h - viewport_height.max(0.0)).max(0.0)
    }

    pub(crate) fn draw_settings(
        &mut self,
        anim_progress: f32,
        active_tab: usize,
        faq_editor: &Editor,
        scroll_y: f32,
        ide_workspaces: &[std::path::PathBuf],
        ide_ignore_patterns: &[String],
        settings_ignore_editor: &Editor,
        settings_ignore_focused: bool,
        settings_ignore_scroll_x: &mut f32,
        ide_scroll_y: f32,
        blink_alpha: f32,
        tool_paths: &crate::platform::ToolPaths,
        tool_installer: &crate::app::tool_installer::ToolInstaller,
        database_settings: &crate::app::database::DatabaseSettings,
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

        let tabs = ["IDE", "Основные", "Редактор", "Внешний вид", "Помощь", "Базы данных"];
        let active_tab = clamped_settings_tab(active_tab, tabs.len());
        let tab_metrics = settings_sidebar_tab_metrics(ih, tabs.len(), s);
        let mut tab_y = iy + tab_metrics.top;
        for (i, title) in tabs.iter().enumerate() {
            let tab_rect_y = tab_y;
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
            self.draw_string_scaled_stable(
                title,
                (ix + 25.0 * s).round(),
                (tab_y + tab_rect_h * 0.5 + 5.0 * s).round(),
                color,
                0.95,
            );
            tab_y += tab_rect_h + tab_metrics.gap;
        }

        let content_x = ix + sidebar_w + 30.0 * s;
        let content_available_w = (ix + iw - content_x - 18.0 * s).max(1.0);
        let content_title_x = content_x - 14.0 * s;
        let mut content_y = iy + 40.0 * s;

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
        content_y += if active_tab == 4 { 30.0 * s } else { 46.0 * s };

        if active_tab == 0 {
            // ── Scissor для скролла вкладки IDE ──────────────────────────────
            // Начало scissor = iy + 52.0 * s (ниже пилюли заголовка iy+18..iy+48)
            let ide_content_area_x = ix + sidebar_w;
            let ide_content_area_w = iw - sidebar_w;
            let ide_content_area_h = ih - 52.0 * s;
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (iy + 52.0 * s + ide_content_area_h);
                self.gl.scissor(
                    ide_content_area_x.round() as i32,
                    scissor_y.round() as i32,
                    ide_content_area_w.round() as i32,
                    ide_content_area_h.round() as i32,
                );
            }
            ui_registry.push_clip(crate::ui_system::UiClipRect::new(
                ide_content_area_x,
                iy + 52.0 * s,
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
            content_y += 40.0 * s;

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
                    [0.306, 0.318, 0.341, 1.0],
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
                content_y += 46.0 * s;
            }

            let add_btn_y_reg = content_y.round();
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsIdeAddWorkspace,
                content_x,
                add_btn_y_reg,
                (190.0 * s).min(content_available_w),
                36.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            let btn_add = crate::widgets::Button {
                x: content_x,
                y: add_btn_y_reg,
                w: (190.0 * s).min(content_available_w),
                h: 36.0 * s,
                text: "Добавить папку".to_string(),
                icon: Some(crate::widgets::IconType::Plus),
                text_scale: 1.0,
                icon_size: 20.0 * s,
            };
            btn_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            content_y += 56.0 * s;
            // ── Разделитель ───────────────────────────────────────────────
            self.push_rect(content_x, content_y, content_available_w, 1.0, [1.0, 1.0, 1.0, 0.07]);
            content_y += 20.0 * s;

            // ── Заголовок секции игноров ──────────────────────────────────
            self.draw_string_scaled(
                "Игнорируемые файлы и папки",
                content_x,
                content_y.round(),
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 28.0 * s;

            // Пояснение
            self.draw_string_scaled(
                "Эти файлы и папки не будут показаны в дереве проекта.",
                content_x,
                content_y.round(),
                [0.45, 0.47, 0.55, 1.0],
                0.85,
            );
            content_y += 22.0 * s;
            self.draw_string_scaled(
                "Примеры: *.log  temp/  .DS_Store  *.min.js  build  dist",
                content_x,
                content_y.round(),
                [0.35, 0.37, 0.44, 1.0],
                0.82,
            );
            content_y += 20.0 * s;

            // ── Поле ввода + кнопка «Добавить» ───────────────────────────
            let content_w = content_available_w;
            let add_gap = 10.0 * s;
            let btn_add_w = (110.0 * s).min((content_w * 0.32).max(0.0));
            let effective_gap = add_gap.min((content_w - btn_add_w).max(0.0));
            let input_w = (content_w - effective_gap - btn_add_w).max(0.0);
            let input_h = 34.0 * s;
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
            content_y += input_h + 16.0 * s;

            // ── Чипы пользовательских паттернов ──────────────────────────
            let chip_h = 28.0 * s;
            let chip_r = chip_h / 2.0;
            let pad_x = 12.0 * s;
            let chip_gap_x = 8.0 * s;
            let chip_gap_y = 8.0 * s;
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
            if let Some(thumb) = settings_scrollbar_thumb(
                iy + 52.0 * s,
                ide_content_area_h,
                max_scroll,
                ide_scroll_y,
                s,
            ) {
                let track_h = ide_content_area_h;
                let sb_x = (ix + iw - 14.0 * s).round();
                self.push_rounded_rect(
                    sb_x,
                    thumb.start.round(),
                    6.0 * s,
                    thumb.len,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeScrollY,
                    sb_x - 5.0 * s,
                    iy + 52.0 * s,
                    16.0 * s,
                    track_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
        } else if active_tab == 1 {
            let tools_clip_y = iy + 52.0 * s;
            let tools_clip_h = (iy + ih - tools_clip_y).max(0.0);
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    (ix + sidebar_w).round() as i32,
                    (self.height - (tools_clip_y + tools_clip_h)).round() as i32,
                    (iw - sidebar_w).max(0.0).round() as i32,
                    tools_clip_h.round() as i32,
                );
            }
            ui_registry.push_clip(crate::ui_system::UiClipRect::new(
                ix + sidebar_w, tools_clip_y, (iw - sidebar_w).max(0.0), tools_clip_h,
            ));
            content_y = content_y.round();
            self.draw_string_scaled_stable(
                "Внешние инструменты",
                content_x.round(),
                content_y,
                [0.82, 0.82, 0.86, 1.0],
                1.0,
            );
            let refresh_w = (102.0 * s).min(content_available_w);
            let refresh_x = (content_x + content_available_w - refresh_w).round();
            let refresh_y = (content_y - 18.0 * s).round();
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsRefreshTools,
                refresh_x,
                refresh_y,
                refresh_w,
                29.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x: refresh_x,
                y: refresh_y,
                w: refresh_w,
                h: 29.0 * s,
                text: "Обновить",
                icon: Some(crate::widgets::IconType::Reload),
                text_scale: 0.72,
                icon_size: 14.0 * s,
            }
            .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            content_y = (content_y + (24.0 * s).round()).round();
            self.draw_string_scaled_stable(
                "Явный путь имеет приоритет над PATH. Переменные RRITER_*_PATH — выше настроек.",
                content_x.round(),
                content_y,
                [0.44, 0.46, 0.54, 1.0],
                0.76,
            );
            content_y = (content_y + (18.0 * s).round()).round();
            self.draw_string_scaled_stable(
                "uv, Ruff и Ty ставятся в каталог RRiter без изменения PATH и профиля shell.",
                content_x.round(),
                content_y,
                [0.44, 0.46, 0.54, 1.0],
                0.72,
            );
            content_y = (content_y + (18.0 * s).round()).round();

            for kind in crate::platform::ToolKind::ALL {
                let row_y = content_y.round();
                let stacked_actions = content_available_w < 430.0 * s;
                let row_h = ((if stacked_actions { 82.0 } else { 47.0 }) * s)
                    .round()
                    .max(1.0);
                let resolution = crate::platform::resolve_tool_kind(kind);
                let configured = tool_paths.get(kind);
                let compact_path_chars = if kind.supports_managed_install() { 24 } else { 47 };
                let status = if resolution.is_ready() {
                    let path = resolution.path.as_deref().unwrap_or(std::path::Path::new(""));
                    let source = resolution
                        .source
                        .map(|source| source.label())
                        .unwrap_or("авто");
                    format!("{source}: {}", compact_settings_path(path, compact_path_chars))
                } else if resolution.is_invalid_override() {
                    let path = resolution
                        .configured_path
                        .as_deref()
                        .unwrap_or(std::path::Path::new(""));
                    format!("Не найден: {}", compact_settings_path(path, compact_path_chars))
                } else {
                    "Не найден".to_string()
                };
                let status_color = if resolution.is_ready() {
                    [0.46, 0.82, 0.58, 1.0]
                } else {
                    [0.90, 0.52, 0.52, 1.0]
                };

                self.push_rounded_rect(
                    content_x,
                    row_y,
                    content_available_w.round(),
                    (row_h - (4.0 * s).round()).max(1.0),
                    5.0 * s,
                    [0.12, 0.13, 0.17, 1.0],
                );
                self.draw_string_scaled_stable(
                    kind.label(),
                    (content_x + 10.0 * s).round(),
                    (row_y + (17.0 * s).round()).round(),
                    [0.88, 0.88, 0.92, 1.0],
                    0.88,
                );
                self.draw_string_scaled_stable(
                    &status,
                    (content_x + 10.0 * s).round(),
                    (row_y + (35.0 * s).round()).round(),
                    status_color,
                    0.70,
                );

                let action_y = (row_y + if stacked_actions { 44.0 * s } else { 7.0 * s }).round();
                let action_left = content_x + 8.0 * s;
                let action_right = content_x + content_available_w - 8.0 * s;
                let action_gap = (6.0 * s).min((action_right - action_left).max(0.0) * 0.08);
                let action_count = usize::from(kind.supports_managed_install())
                    + 1
                    + usize::from(configured.is_some());
                let action_w = ((action_right - action_left
                    - action_gap * action_count.saturating_sub(1) as f32)
                    / action_count.max(1) as f32)
                    .max(0.0);
                let mut action_x = action_left;
                if kind.supports_managed_install() {
                    let install_x = action_x.round();
                    action_x += action_w + action_gap;
                    let install_disabled = tool_installer.is_running()
                        && !tool_installer.is_running_for(kind);
                    let install_text = if tool_installer.is_running_for(kind) {
                        "Отмена"
                    } else if resolution.is_ready() {
                        "Обновить"
                    } else {
                        "Установить"
                    };
                    if !install_disabled {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::SettingsToolInstall(kind.index()),
                            install_x,
                            action_y,
                            action_w,
                            29.0 * s,
                            self.last_mouse_x,
                            self.last_mouse_y,
                        );
                    }
                    crate::widgets::ButtonView {
                        x: install_x,
                        y: action_y,
                        w: action_w,
                        h: 29.0 * s,
                        text: install_text,
                        icon: None,
                        text_scale: 0.68,
                        icon_size: 0.0,
                    }
                    .render(
                        self,
                        self.last_mouse_x,
                        self.last_mouse_y,
                        s,
                        install_disabled,
                    );
                }

                let choose_x = action_x.round();
                action_x += action_w + action_gap;
                let button_y = action_y;
                let path_controls_disabled = tool_installer.is_running();
                if !path_controls_disabled {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::SettingsToolPick(kind.index()),
                        choose_x,
                        button_y,
                        action_w,
                        29.0 * s,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                }
                crate::widgets::ButtonView {
                    x: choose_x,
                    y: button_y,
                    w: action_w,
                    h: 29.0 * s,
                    text: "Выбрать",
                    icon: None,
                    text_scale: 0.72,
                    icon_size: 0.0,
                }
                .render(
                    self,
                    self.last_mouse_x,
                    self.last_mouse_y,
                    s,
                    path_controls_disabled,
                );

                if configured.is_some() {
                    let clear_x = action_x.round();
                    if !path_controls_disabled {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::SettingsToolClear(kind.index()),
                            clear_x,
                            button_y,
                            action_w,
                            29.0 * s,
                            self.last_mouse_x,
                            self.last_mouse_y,
                        );
                    }
                    crate::widgets::ButtonView {
                        x: clear_x,
                        y: button_y,
                        w: action_w,
                        h: 29.0 * s,
                        text: "×",
                        icon: None,
                        text_scale: 0.92,
                        icon_size: 0.0,
                    }
                    .render(
                        self,
                        self.last_mouse_x,
                        self.last_mouse_y,
                        s,
                        path_controls_disabled,
                    );
                }
                content_y = (row_y + row_h).round();
            }

            if let Some(target) = tool_installer.target() {
                content_y = (content_y + (3.0 * s).round()).round();
                let panel_h = (102.0 * s).round();
                self.push_rounded_rect(
                    content_x,
                    content_y,
                    content_available_w.round(),
                    panel_h,
                    5.0 * s,
                    [0.10, 0.11, 0.15, 1.0],
                );
                let heading = format!(
                    "{} · {}",
                    target.label(),
                    tool_installer.phase().label()
                );
                self.draw_string_scaled_stable(
                    &heading,
                    (content_x + 10.0 * s).round(),
                    (content_y + (18.0 * s).round()).round(),
                    [0.84, 0.84, 0.90, 1.0],
                    0.80,
                );
                self.draw_string_scaled_stable(
                    &compact_settings_text(tool_installer.detail(), 58),
                    (content_x + 10.0 * s).round(),
                    (content_y + (36.0 * s).round()).round(),
                    [0.56, 0.58, 0.68, 1.0],
                    0.70,
                );
                let logs = tool_installer.logs();
                let start = logs.len().saturating_sub(3);
                let preview_step = (14.0 * s).round().max(1.0);
                let preview_y = (content_y + (55.0 * s).round()).round();
                for (line_idx, line) in logs[start..].iter().enumerate() {
                    let color = match line.kind {
                        crate::app::tool_installer::ToolInstallLogKind::Error => {
                            [0.92, 0.50, 0.50, 1.0]
                        }
                        crate::app::tool_installer::ToolInstallLogKind::Success => {
                            [0.46, 0.82, 0.58, 1.0]
                        }
                        crate::app::tool_installer::ToolInstallLogKind::Info => {
                            [0.62, 0.64, 0.72, 1.0]
                        }
                        crate::app::tool_installer::ToolInstallLogKind::Output => {
                            [0.74, 0.74, 0.78, 1.0]
                        }
                    };
                    self.draw_string_scaled_stable(
                        &compact_settings_text(&line.text, 58),
                        (content_x + 10.0 * s).round(),
                        (preview_y + line_idx as f32 * preview_step).round(),
                        color,
                        0.65,
                    );
                }
                if !logs.is_empty() {
                    let button_y = (content_y + 7.0 * s).round();
                    let copy_log_w = (104.0 * s).min(content_available_w * 0.48);
                    let open_log_w = (100.0 * s).min(content_available_w * 0.48);
                    let copy_log_x = (content_x + content_available_w - copy_log_w).round();
                    let open_log_x = (copy_log_x - 6.0 * s - open_log_w)
                        .max(content_x)
                        .round();
                    ui_registry.register_rect(
                        crate::ui_system::UiId::SettingsOpenToolInstallLog,
                        open_log_x,
                        button_y,
                        open_log_w,
                        29.0 * s,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                    crate::widgets::ButtonView {
                        x: open_log_x,
                        y: button_y,
                        w: open_log_w,
                        h: 29.0 * s,
                        text: "Открыть лог",
                        icon: None,
                        text_scale: 0.66,
                        icon_size: 0.0,
                    }
                    .render(self, self.last_mouse_x, self.last_mouse_y, s, false);

                    ui_registry.register_rect(
                        crate::ui_system::UiId::SettingsCopyToolInstallLog,
                        copy_log_x,
                        button_y,
                        copy_log_w,
                        29.0 * s,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                    crate::widgets::ButtonView {
                        x: copy_log_x,
                        y: button_y,
                        w: copy_log_w,
                        h: 29.0 * s,
                        text: "Копировать",
                        icon: None,
                        text_scale: 0.66,
                        icon_size: 0.0,
                    }
                    .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
                }
                content_y = (content_y + panel_h + (3.0 * s).round()).round();
            }

            content_y = (content_y + (5.0 * s).round()).round();
            self.draw_string_scaled_stable(
                "Каталоги RRiter",
                content_x,
                content_y,
                [0.82, 0.82, 0.86, 1.0],
                0.92,
            );
            content_y += 13.0 * s;
            let directory_labels = ["Config", "Data", "Cache", "State"];
            let dir_gap = 8.0 * s;
            let dir_button_w = 102.0 * s;
            let dir_columns = (((content_available_w + dir_gap) / (dir_button_w + dir_gap))
                .floor() as usize)
                .clamp(1, directory_labels.len());
            for (idx, label) in directory_labels.iter().enumerate() {
                let col = idx % dir_columns;
                let row = idx / dir_columns;
                let button_x = content_x + col as f32 * (dir_button_w + dir_gap);
                let button_y = content_y + row as f32 * 37.0 * s;
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsOpenDirectory(idx),
                    button_x,
                    button_y,
                    dir_button_w,
                    29.0 * s,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                crate::widgets::ButtonView {
                    x: button_x,
                    y: button_y,
                    w: dir_button_w,
                    h: 29.0 * s,
                    text: label,
                    icon: None,
                    text_scale: 0.76,
                    icon_size: 0.0,
                }
                .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            }
            content_y += ((directory_labels.len() + dir_columns - 1) / dir_columns) as f32
                * 37.0
                * s
                + 3.0 * s;

            self.draw_string_scaled(
                "Графика",
                content_x,
                content_y,
                [0.82, 0.82, 0.86, 1.0],
                0.92,
            );
            content_y += 18.0 * s;
            let graphics_summary = format!(
                "{} · {} · scale {:.2}",
                self.graphics_diagnostics.renderer,
                self.graphics_diagnostics.version,
                self.graphics_diagnostics.scale_factor
            );
            self.draw_string_scaled(
                &compact_settings_text(&graphics_summary, 66),
                content_x,
                content_y,
                [0.56, 0.58, 0.66, 1.0],
                0.74,
            );
            let copy_w = (114.0 * s).min(content_available_w);
            let copy_x = content_x + content_available_w - copy_w;
            let copy_y = content_y - 17.0 * s;
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsCopyGraphicsDiagnostics,
                copy_x,
                copy_y,
                copy_w,
                29.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x: copy_x,
                y: copy_y,
                w: copy_w,
                h: 29.0 * s,
                text: "Скопировать",
                icon: Some(crate::widgets::IconType::Copy),
                text_scale: 0.72,
                icon_size: 14.0 * s,
            }
            .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            ui_registry.pop_clip();
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
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

            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (text_area_y + text_area_h);
                self.gl.scissor(
                    (content_x - 10.0 * s).round() as i32,
                    scissor_y.round() as i32,
                    (iw - sidebar_w - 10.0 * s).round() as i32,
                    text_area_h.round() as i32,
                );
            }

            let start_x = content_x;
            let main_header_x = content_x - 14.0 * s;
            let render_scroll_y = scroll_y.round();
            let mut text_y = text_area_y + 20.0 * s - render_scroll_y;
            let text = faq_editor.get_full_text();

            let left_col_w = 260.0 * s;
            let cw = iw - sidebar_w - 76.0 * s;
            let mut main_header_drawn = false;

            for line in text.split('\n') {
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

                    text_y += 50.0 * s;
                    continue;
                }

                if let Some(tab_idx) = line.find('\t') {
                    let shortcut = &line[..tab_idx];
                    let description = &line[tab_idx + 1..];

                    let kbd_bg = [0.224, 0.231, 0.251, 1.0];
                    let kbd_border = [0.306, 0.318, 0.341, 1.0];
                    let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

                    let kbd_w = self.measure_ui_width(shortcut, 0.95) + 20.0 * s;
                    let kbd_h = 24.0 * s;
                    let kbd_x = start_x;
                    let kbd_y = text_y - 18.0 * s;

                    self.push_rounded_rect(
                        kbd_x - 1.0,
                        kbd_y - 1.0,
                        kbd_w + 2.0,
                        kbd_h + 2.0,
                        4.0 * s,
                        kbd_border,
                    );
                    self.push_rounded_rect(kbd_x, kbd_y, kbd_w, kbd_h, 4.0 * s, kbd_bg);
                    self.draw_string_scaled(
                        shortcut,
                        kbd_x + 10.0 * s,
                        text_y - 1.0 * s,
                        kbd_text_color,
                        0.95,
                    );

                    let desc_color = [0.663, 0.690, 0.729, 1.0];
                    self.draw_string_scaled(
                        description,
                        start_x + left_col_w,
                        text_y,
                        desc_color,
                        1.0,
                    );

                    text_y += 38.0 * s;
                    continue;
                }

                if !line.trim().is_empty() {
                    let normal_color = [0.875, 0.882, 0.902, 1.0];
                    self.draw_string_scaled(line.trim(), start_x, text_y, normal_color, 1.0);
                    text_y += 30.0 * s;
                } else {
                    text_y += 15.0 * s;
                }
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            let max_scroll = self.get_faq_max_scroll(faq_editor, text_area_h);
            if let Some(thumb) = settings_scrollbar_thumb(
                text_area_y, text_area_h, max_scroll, scroll_y, s,
            ) {
                let track_h = text_area_h;
                let scroll_x = (start_x + cw + 5.0 * s).round();

                self.push_rounded_rect(
                    scroll_x,
                    thumb.start.round(),
                    6.0 * s,
                    thumb.len,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsFaqScrollY,
                    scroll_x - 5.0 * s,
                    text_area_y,
                    16.0 * s,
                    track_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
        } else if active_tab == 5 {
            self.draw_database_settings_tab(
                database_settings,
                content_x,
                content_available_w,
                content_y,
                ui_registry,
            );
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
        if let Some(thumb) = crate::scroll::scrollbar_thumb(
            log_y + 6.0 * s,
            log_h - 12.0 * s,
            log_h,
            content_h,
            scroll,
            28.0 * s,
        ) {
            self.push_rounded_rect(
                (log_x + log_w - 7.0 * s).round(),
                thumb.start.round(),
                4.0 * s,
                thumb.len,
                2.0 * s,
                [0.56, 0.38, 0.70, 0.95],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsToolInstallLogScrollY,
                log_x + log_w - 16.0 * s,
                log_y,
                16.0 * s,
                log_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
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
        settings_faq_viewport_height, settings_modal_layout, settings_scrollbar_thumb,
        settings_sidebar_tab_metrics,
    };

    #[test]
    fn invalid_settings_tab_is_clamped_before_rendering() {
        assert_eq!(clamped_settings_tab(0, 6), 0);
        assert_eq!(clamped_settings_tab(99, 6), 5);
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
    fn settings_scrollbar_thumb_matches_shared_scroll_geometry() {
        let thumb = settings_scrollbar_thumb(50.0, 300.0, 600.0, 300.0, 1.0)
            .expect("scrollbar should be visible");
        assert_eq!(thumb.start, 150.0);
        assert_eq!(thumb.len, 100.0);
    }
}
