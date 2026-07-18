use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

pub(crate) const EXTERNAL_TAB_TITLE_COLOR: [f32; 4] = [1.0, 0.55, 0.18, 1.0];

const TAB_ICON_SLOT_SIZE: f32 = 20.0;
const DATABASE_QUERY_TAB_ICON_SIZE: f32 = 28.0;
const DATABASE_TABLE_TAB_ICON_SIZE: f32 = 24.0;

fn tab_file_icon_key(
    path: Option<&std::path::Path>,
    title: &str,
    fallback: &'static str,
) -> &'static str {
    let name = path
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| title.trim_start_matches('*').trim());
    if name.is_empty() {
        fallback
    } else {
        crate::app::file_icons::file_icon_key_for_name(name)
    }
}

fn tab_icon_rect(
    slot_x: f32,
    bar_y: f32,
    bar_h: f32,
    slot_size: f32,
    visual_size: f32,
    s: f32,
) -> (f32, f32, f32) {
    let slot_size = (slot_size * s).round().max(1.0);
    let visual_size = (visual_size * s).round().max(1.0);
    let x = (slot_x + (slot_size - visual_size) * 0.5).round();
    let y = (bar_y + (bar_h - visual_size) * 0.5 - 1.5 * s).round();
    (x, y, visual_size)
}

fn tab_diagnostic_severity_for_path(
    lsp: Option<&crate::lsp::LspManager>,
    path: Option<&std::path::PathBuf>,
) -> Option<crate::lsp::DiagSeverity> {
    let lsp = lsp?;
    let path = path?;
    lsp.diagnostic_severity_for_path(path)
}

pub(crate) fn tab_path_is_external(
    path: &std::path::Path,
    workspaces: &[std::path::PathBuf],
) -> bool {
    !workspaces.is_empty()
        && path.is_absolute()
        && !workspaces
            .iter()
            .any(|workspace| crate::platform::path_is_within(path, workspace))
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn editor_tab_width(
        &mut self,
        tab: &crate::app::EditorTab,
        title: &str,
        scale: f32,
    ) -> f32 {
        let tab_pad = 16.0 * scale;
        let icon_size_tab = TAB_ICON_SLOT_SIZE * scale;
        if let crate::app::EditorTabKind::ApiClient(meta, _) = &tab.kind
            && let Some(method) = meta.route_method
        {
            let api_title = if meta.title.is_empty() {
                "API"
            } else {
                meta.title.as_str()
            };
            let mut path = String::new();
            crate::app::api_client::write_api_path_display(&meta.route_path, &mut path);
            let title_w = self.measure_ui_width(api_title, 1.0);
            let chip_w = (self.measure_ui_width(method.chip_str(), 0.62) + 16.0 * scale)
                .max(34.0 * scale);
            let path_w = self.measure_ui_width(&path, 0.88);
            tab_pad * 2.0
                + icon_size_tab
                + 8.0 * scale
                + title_w
                + 8.0 * scale
                + chip_w
                + 8.0 * scale
                + path_w
                + 30.0 * scale
        } else {
            let title_w = self.measure_ui_width(title, 1.0);
            tab_pad * 2.0 + icon_size_tab + 8.0 * scale + title_w + 30.0 * scale
        }
    }

    pub fn draw_tab_bar(
        &mut self,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        editor: &Editor,
        editor_title: &str,
        _editor_path: Option<&std::path::PathBuf>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        mx: f32,
        my: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_scroll_x: f32,
        tab_drag: Option<&crate::app::TabDragState>,
        lsp: Option<&crate::lsp::LspManager>,
        api: &crate::app::api_client::ApiClientState,
        ide_workspaces: &[std::path::PathBuf],
    ) -> Option<(String, f32, f32)> {
        let tab_bar_bg = self.theme.minimap_bg;
        self.push_rect(x, y, w, h, tab_bar_bg);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (y + h)).round() as i32;
            self.gl
                .scissor(x.round() as i32, sy, w.round() as i32, h.round() as i32);
        }

        let tab_pad = 16.0 * s;
        let icon_size_tab = TAB_ICON_SLOT_SIZE * s;

        let path_for_tab = |idx: usize| {
            if idx == active_tab {
                _editor_path
            } else {
                tabs[idx].file_path.as_ref()
            }
        };
        let mut display_titles = std::mem::take(&mut self.tab_display_titles);
        crate::app::write_tab_display_titles_for(
            tabs,
            active_tab,
            _editor_path,
            editor_title,
            &mut display_titles,
        );

        let mut tab_widths = Vec::with_capacity(tabs.len());
        for (idx, title) in display_titles.iter().enumerate() {
            tab_widths.push(self.editor_tab_width(&tabs[idx], title, s));
        }

        let mut hovered_tab_tooltip = None;
        let mut hovered_tab_x = 0.0;
        let mut hovered_tab_y = 0.0;
        let mut current_hovered_idx = None;

        let mut initial_xs = vec![0.0; tabs.len()];
        let mut cx = x - tab_scroll_x;
        for i in 0..tabs.len() {
            initial_xs[i] = cx;
            cx += tab_widths[i];
        }

        let mut order: Vec<usize> = (0..tabs.len()).collect();
        let mut actual_xs = initial_xs.clone();

        let is_dragging = tab_drag.map(|d| d.threshold_passed).unwrap_or(false);
        let dragged_idx = tab_drag.map(|d| d.start_idx);

        if let Some(drag) = tab_drag {
            if drag.threshold_passed && drag.start_idx < tabs.len() {
                let dragged_x = initial_xs[drag.start_idx] + (drag.current_x - drag.start_x);
                let dragged_w = tab_widths[drag.start_idx];
                let dragged_center = dragged_x + dragged_w / 2.0;
                let mut dst = drag.start_idx;

                for i in 0..tabs.len() {
                    if i == drag.start_idx {
                        continue;
                    }
                    let other_center = initial_xs[i] + tab_widths[i] / 2.0;

                    if i < drag.start_idx {
                        if dragged_center < other_center {
                            dst = dst.min(i);
                        }
                    } else {
                        if dragged_center > other_center {
                            dst = dst.max(i);
                        }
                    }
                }

                order.retain(|&idx| idx != drag.start_idx);
                order.insert(dst, drag.start_idx);

                let mut cur_x = x - tab_scroll_x;
                for &idx in &order {
                    if idx != drag.start_idx {
                        actual_xs[idx] = cur_x;
                    }
                    cur_x += tab_widths[idx];
                }
                actual_xs[drag.start_idx] = dragged_x;
            }
        }

        if self.tab_x_anim.len() != tabs.len() || !is_dragging {
            self.tab_x_anim = actual_xs.clone();
        } else {
            for i in 0..tabs.len() {
                if Some(i) == dragged_idx {
                    self.tab_x_anim[i] = actual_xs[i];
                } else {
                    let diff = actual_xs[i] - self.tab_x_anim[i];
                    if diff.abs() > 0.5 {
                        self.tab_x_anim[i] += diff * 0.12;
                    } else {
                        self.tab_x_anim[i] = actual_xs[i];
                    }
                }
            }
        }

        let mut render_order = order.clone();
        if let Some(d_idx) = dragged_idx {
            if is_dragging {
                render_order.retain(|&idx| idx != d_idx);
                render_order.push(d_idx);
            }
        }

        for &i in &render_order {
            let tab = &tabs[i];
            let is_active = i == active_tab;
            let title = &display_titles[i];
            let tab_w = tab_widths[i];
            let current_x = self.tab_x_anim[i];
            let is_last_in_order = order.last() == Some(&i);

            let is_hovered = mx >= current_x.max(x)
                && mx <= (current_x + tab_w).min(x + w)
                && my >= y
                && my <= y + h;

            if is_hovered {
                if let Some(p) = path_for_tab(i) {
                    hovered_tab_tooltip = Some(p.to_string_lossy().into_owned());
                    hovered_tab_x = current_x.max(x);
                    hovered_tab_y = y + h;
                    current_hovered_idx = Some(i);
                } else if let crate::app::EditorTabKind::GitDiff(meta, _) = &tab.kind {
                    hovered_tab_tooltip = Some(format!(
                        "{} · режим чтения",
                        meta.repo_root.join(&meta.rel_path).to_string_lossy()
                    ));
                    hovered_tab_x = current_x.max(x);
                    hovered_tab_y = y + h;
                    current_hovered_idx = Some(i);
                } else if let crate::app::EditorTabKind::ApiClient(meta, _) = &tab.kind {
                    let source = api.specs.iter().find(|entry| entry.id == meta.spec_id).map(
                        |entry| match &entry.source {
                            crate::app::api_client::ApiSpecSource::Local(path) => {
                                path.to_string_lossy().into_owned()
                            }
                            crate::app::api_client::ApiSpecSource::Url(url) => url.clone(),
                        },
                    );
                    let tooltip = if let Some(source) = source {
                        if let Some(method) = meta.route_method {
                            let mut path = String::new();
                            crate::app::api_client::write_api_path_display(
                                &meta.route_path,
                                &mut path,
                            );
                            format!("{source} · {} {path}", method.chip_str())
                        } else {
                            source
                        }
                    } else {
                        format!("{} · API клиент", meta.title)
                    };
                    hovered_tab_tooltip = Some(tooltip);
                    hovered_tab_x = current_x.max(x);
                    hovered_tab_y = y + h;
                    current_hovered_idx = Some(i);
                }
            }

            let bg_color = if is_active {
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0]
            } else if is_hovered {
                [
                    self.theme.bg[0] + 0.02,
                    self.theme.bg[1] + 0.02,
                    self.theme.bg[2] + 0.02,
                    1.0,
                ]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };

            if bg_color[3] > 0.0 {
                self.push_rect(current_x, y, tab_w, h, bg_color);
            }

            if is_active {
                self.push_rect(
                    current_x,
                    y + h - 2.0 * s,
                    tab_w,
                    2.0 * s,
                    [0.60, 0.35, 0.85, 1.0],
                );
            }

            if !is_last_in_order {
                let sep_h = h * 0.4;
                let sep_y = y + (h - sep_h) / 2.0;
                let sep_color = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.15];
                self.push_rect(current_x + tab_w - 1.0, sep_y, 1.0, sep_h, sep_color);
            }

            let is_dirty = if is_active {
                editor.is_dirty()
            } else {
                tab.editor.is_dirty()
            };

            let slot_x = current_x + tab_pad;
            let (_, icon_y, _) = tab_icon_rect(
                slot_x,
                y,
                h,
                TAB_ICON_SLOT_SIZE,
                TAB_ICON_SLOT_SIZE,
                s,
            );
            if tab.kind.is_git_diff() {
                self.draw_atlas_icon(
                    crate::widgets::IconType::GitCompare,
                    slot_x.round(),
                    icon_y,
                    icon_size_tab.round(),
                    self.theme.fg,
                );
            } else if tab.kind.is_api_client() {
                self.draw_atlas_icon(
                    crate::widgets::IconType::Api,
                    slot_x.round(),
                    icon_y,
                    icon_size_tab.round(),
                    [1.0, 1.0, 1.0, 1.0],
                );
            } else if tab.kind.is_database_table() {
                let (icon_x, icon_y, icon_size) = tab_icon_rect(
                    slot_x,
                    y,
                    h,
                    TAB_ICON_SLOT_SIZE,
                    DATABASE_TABLE_TAB_ICON_SIZE,
                    s,
                );
                self.draw_atlas_icon(
                    crate::widgets::IconType::DatabaseTable,
                    icon_x,
                    icon_y,
                    icon_size,
                    [0.22, 0.84, 0.78, 1.0],
                );
            } else if tab.kind.is_database_query() {
                let (icon_x, icon_y, icon_size) = tab_icon_rect(
                    slot_x,
                    y,
                    h,
                    TAB_ICON_SLOT_SIZE,
                    DATABASE_QUERY_TAB_ICON_SIZE,
                    s,
                );
                self.draw_atlas_icon(
                    crate::widgets::IconType::Database,
                    icon_x,
                    icon_y,
                    icon_size,
                    [1.0, 0.67, 0.16, 1.0],
                );
            } else {
                let icon_key = if is_active {
                    tab_file_icon_key(
                        _editor_path.map(std::path::PathBuf::as_path),
                        editor_title,
                        tab.icon_key,
                    )
                } else {
                    tab_file_icon_key(tab.file_path.as_deref(), &tab.base_title, tab.icon_key)
                };
                self.draw_file_icon(
                    icon_key,
                    false,
                    slot_x.round(),
                    icon_y,
                    icon_size_tab.round(),
                );
            }

            let text_color =
                if path_for_tab(i).is_some_and(|path| tab_path_is_external(path, ide_workspaces)) {
                    EXTERNAL_TAB_TITLE_COLOR
                } else if is_active {
                    self.theme.fg
                } else {
                    self.theme.line_num
            };
            let text_x = current_x + tab_pad + icon_size_tab + 8.0 * s;
            let text_y = y + h / 2.0 + 5.0 * s;
            if let crate::app::EditorTabKind::ApiClient(meta, _) = &tab.kind
                && let Some(method) = meta.route_method
            {
                let api_title = if meta.title.is_empty() {
                    "API"
                } else {
                    meta.title.as_str()
                };
                let mut path = String::new();
                crate::app::api_client::write_api_path_display(&meta.route_path, &mut path);
                self.draw_string_scaled(api_title, text_x, text_y, text_color, 1.0);
                let title_w = self.measure_ui_width(api_title, 1.0);
                let chip_w = (self.measure_ui_width(method.chip_str(), 0.62) + 16.0 * s)
                    .max(34.0 * s);
                let chip_x = text_x + title_w + 8.0 * s;
                let chip_h = 18.0 * s;
                self.draw_api_method_chip(
                    method,
                    chip_x,
                    y + (h - chip_h) * 0.5,
                    chip_w,
                    chip_h,
                    s,
                    0.62,
                );
                self.draw_string_scaled(
                    &path,
                    chip_x + chip_w + 8.0 * s,
                    text_y,
                    text_color,
                    0.88,
                );
            } else {
                self.draw_string_scaled(title, text_x, text_y, text_color, 1.0);
                if let Some(severity) = tab_diagnostic_severity_for_path(lsp, path_for_tab(i)) {
                    let title_w =
                        (tab_w - (tab_pad * 2.0 + icon_size_tab + 8.0 * s + 30.0 * s)).max(0.0);
                    let color = match severity {
                        crate::lsp::DiagSeverity::Error => self.theme.diag_error,
                        crate::lsp::DiagSeverity::Warning => self.theme.diag_warn,
                        _ => self.theme.diag_warn,
                    };
                    self.push_squiggle(
                        text_x,
                        text_y + 2.0 * s,
                        title_w,
                        color,
                    );
                }
            }

            let tab_right = current_x + tab_w;
            if tab_right > x && current_x < x + w {
                ui_registry.register_rect(
                    crate::ui_system::UiId::EditorTab(i),
                    current_x.max(x),
                    y,
                    (tab_right.min(x + w) - current_x.max(x)).max(0.0),
                    h,
                    mx,
                    my,
                );
            }

            {
                let close_size = 20.0 * s;
                let close_x = current_x + tab_w - tab_pad - close_size;
                let close_y = (y + (h - close_size) / 2.0 - 1.5 * s).round();

                let close_rect_x = close_x - 4.0 * s;
                let close_rect_y = close_y - 4.0 * s;
                let close_rect_w = close_size + 8.0 * s;
                let close_rect_h = close_size + 8.0 * s;
                let close_rect_right = close_rect_x + close_rect_w;

                let close_hovered = mx >= close_rect_x.max(x)
                    && mx <= close_rect_right.min(x + w)
                    && my >= close_rect_y
                    && my <= close_rect_y + close_rect_h;

                let show_close = is_active || is_hovered;
                if show_close {
                    if is_dirty && !close_hovered {
                        // Точка вместо крестика (VS Code стиль)
                        self.draw_string_scaled(
                            "●",
                            close_x + close_size / 2.0 - 4.0 * s,
                            close_y + close_size / 2.0 + 4.0 * s,
                            [0.9, 0.9, 0.9, 1.0],
                            0.8,
                        );
                    } else {
                        if close_hovered {
                            self.push_rounded_rect(
                                close_rect_x,
                                close_rect_y,
                                close_rect_w,
                                close_rect_h,
                                4.0 * s,
                                [1.0, 1.0, 1.0, 0.1],
                            );
                        }
                        let icon_col = if close_hovered {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.8]
                        };
                        self.draw_atlas_icon(
                            crate::widgets::IconType::Close,
                            close_x,
                            close_y,
                            close_size,
                            icon_col,
                        );
                    }
                }

                if show_close && close_rect_right > x && close_rect_x < x + w {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::EditorTabClose(i),
                        close_rect_x.max(x),
                        close_rect_y,
                        (close_rect_right.min(x + w) - close_rect_x.max(x)).max(0.0),
                        close_rect_h,
                        mx,
                        my,
                    );
                }
            }
        }

        let total_tabs_w: f32 = tab_widths.iter().sum();
        self.max_tab_scroll_x = (total_tabs_w - w).max(0.0);

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let base_shadow_alpha = 0.4;
        let transparent = [0.0, 0.0, 0.0, 0.0];
        let fade_w = 40.0 * s;

        let left_alpha = (tab_scroll_x / fade_w).clamp(0.0, 1.0) * base_shadow_alpha;
        if left_alpha > 0.001 {
            let shadow_color = [0.0, 0.0, 0.0, left_alpha];
            self.push_horizontal_gradient(x, y, fade_w, h, shadow_color, transparent);
        }

        if self.max_tab_scroll_x > 0.0 {
            let right_alpha = ((self.max_tab_scroll_x - tab_scroll_x) / fade_w).clamp(0.0, 1.0)
                * base_shadow_alpha;
            if right_alpha > 0.001 {
                let shadow_color = [0.0, 0.0, 0.0, right_alpha];
                self.push_horizontal_gradient(
                    x + w - fade_w,
                    y,
                    fade_w,
                    h,
                    transparent,
                    shadow_color,
                );
            }
        }

        self.tab_hover_idx = current_hovered_idx;
        const TAB_TOOLTIP_NAMESPACE: u64 = 1u64 << 60;
        let tooltip_anchor = if let Some(idx) = current_hovered_idx {
            self.delayed_tooltip_anchor(
                Some(TAB_TOOLTIP_NAMESPACE | idx as u64),
                hovered_tab_x,
                hovered_tab_y,
                std::time::Instant::now(),
            )
        } else {
            self.reset_delayed_tooltip_anchor_namespace(TAB_TOOLTIP_NAMESPACE);
            None
        };

        let tooltip = if let (Some(text), Some((anchor_x, anchor_y))) =
            (hovered_tab_tooltip, tooltip_anchor)
        {
            (!self.hide_popups_until_mouse_move).then_some((text, anchor_x, anchor_y))
        } else {
            None
        };
        self.tab_display_titles = display_titles;
        tooltip
    }

    pub fn draw_tab_tooltip(&mut self, text: &str, hovered_tab_x: f32, hovered_tab_y: f32, s: f32) {
        let mut path_str = text.to_string();
        if let Some(home) = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
        {
            if path_str.starts_with(&home) {
                path_str = path_str.replacen(&home, "~", 1);
            }
        }

        let tooltip_scale = 0.95;
        let tooltip_w = self.measure_ui_width(&path_str, tooltip_scale) + 24.0 * s;
        let tooltip_h = 32.0 * s;

        let mut tooltip_x = hovered_tab_x + 10.0 * s;
        if tooltip_x + tooltip_w > self.width {
            tooltip_x = (self.width - tooltip_w - 4.0 * s).max(0.0);
        }

        let tooltip_y = hovered_tab_y + 8.0 * s;

        let border_col = self.theme.sel;
        let bg_col = [
            self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            0.98,
        ];

        self.push_rounded_rect(
            tooltip_x,
            tooltip_y,
            tooltip_w,
            tooltip_h,
            6.0 * s,
            border_col,
        );
        self.push_rounded_rect(
            tooltip_x + 1.0,
            tooltip_y + 1.0,
            tooltip_w - 2.0,
            tooltip_h - 2.0,
            5.0 * s,
            bg_col,
        );
        self.draw_string_scaled(
            &path_str,
            tooltip_x + 12.0 * s,
            tooltip_y + tooltip_h / 2.0 + 5.0 * s,
            self.theme.fg,
            tooltip_scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_path_external_detection_requires_absolute_outside_workspace() {
        let root = std::env::temp_dir().join("rriter-tab-workspace");
        let workspaces = vec![root.clone()];

        assert!(!tab_path_is_external(
            &root.join("pkg/file.py"),
            &workspaces
        ));
        assert!(tab_path_is_external(
            &std::env::temp_dir().join("rriter-external-site-packages/lib.py"),
            &workspaces
        ));
        assert!(!tab_path_is_external(
            std::path::Path::new("relative.py"),
            &workspaces
        ));
        assert!(!tab_path_is_external(
            &std::env::temp_dir().join("rriter-no-workspace-file.py"),
            &[]
        ));
    }

    #[test]
    fn external_tab_title_color_is_orange() {
        assert!(EXTERNAL_TAB_TITLE_COLOR[0] > 0.9);
        assert!(EXTERNAL_TAB_TITLE_COLOR[1] > 0.4);
        assert!(EXTERNAL_TAB_TITLE_COLOR[1] < 0.7);
        assert!(EXTERNAL_TAB_TITLE_COLOR[2] < 0.3);
        assert_eq!(EXTERNAL_TAB_TITLE_COLOR[3], 1.0);
    }

    #[test]
    fn python_tab_icon_uses_real_path_instead_of_stale_default_icon() {
        assert_eq!(
            tab_file_icon_key(
                Some(std::path::Path::new("/workspace/app/service.py")),
                "Безымянный",
                "default_file",
            ),
            "python"
        );
    }

    #[test]
    fn database_tab_icon_is_larger_and_pixel_snapped() {
        let (x, y, size) = tab_icon_rect(
            31.35,
            4.2,
            36.0,
            TAB_ICON_SLOT_SIZE,
            DATABASE_QUERY_TAB_ICON_SIZE,
            1.25,
        );
        assert!(size > (TAB_ICON_SLOT_SIZE * 1.25).round());
        assert_eq!(x.fract(), 0.0);
        assert_eq!(y.fract(), 0.0);
        assert_eq!(size.fract(), 0.0);
    }
}
