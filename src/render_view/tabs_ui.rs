use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

pub(crate) const EXTERNAL_TAB_TITLE_COLOR: [f32; 4] = [1.0, 0.55, 0.18, 1.0];

pub(crate) fn tab_path_is_external(
    path: &std::path::Path,
    workspaces: &[std::path::PathBuf],
) -> bool {
    !workspaces.is_empty()
        && path.is_absolute()
        && !workspaces
            .iter()
            .any(|workspace| path.starts_with(workspace))
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
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
        ide_workspaces: &[std::path::PathBuf],
    ) -> Option<(std::path::PathBuf, f32, f32)> {
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
        let icon_size_tab = 20.0 * s;

        let mut paths: Vec<Option<&std::path::PathBuf>> =
            tabs.iter().map(|t| t.file_path.as_ref()).collect();
        paths[active_tab] = _editor_path;

        let mut display_titles = vec![String::new(); tabs.len()];
        for i in 0..tabs.len() {
            if let Some(p1) = paths[i] {
                let mut diff_level = 0;
                let mut collision = false;
                for j in 0..tabs.len() {
                    if i == j {
                        continue;
                    }
                    if let Some(p2) = paths[j] {
                        if p1.file_name() == p2.file_name() {
                            collision = true;
                            let mut it1 = p1.components().rev();
                            let mut it2 = p2.components().rev();
                            let mut level = 0;
                            loop {
                                let c1 = it1.next();
                                let c2 = it2.next();
                                if c1 != c2 {
                                    diff_level = diff_level.max(level);
                                    break;
                                }
                                if c1.is_none() && c2.is_none() {
                                    break;
                                }
                                level += 1;
                            }
                        }
                    }
                }
                if collision && diff_level > 0 {
                    let comps: Vec<_> = p1.components().rev().collect();
                    if diff_level < comps.len() {
                        let diff_dir = comps[diff_level].as_os_str().to_string_lossy();
                        let file_name = comps[0].as_os_str().to_string_lossy();
                        if diff_level == 1 {
                            display_titles[i] = format!("{}/{}", diff_dir, file_name);
                        } else {
                            display_titles[i] = format!("{}/.../{}", diff_dir, file_name);
                        }
                    } else {
                        display_titles[i] = p1.to_string_lossy().into_owned();
                    }
                } else {
                    display_titles[i] = p1
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                }
            } else {
                let bt = if i == active_tab {
                    if editor_title.is_empty() {
                        "Безымянный"
                    } else {
                        editor_title
                    }
                } else {
                    if tabs[i].base_title.is_empty() {
                        "Безымянный"
                    } else {
                        &tabs[i].base_title
                    }
                };
                display_titles[i] = bt.to_string();
            }
        }

        let mut tab_widths = Vec::with_capacity(tabs.len());
        for title in &display_titles {
            let title_w = self.measure_ui_width(title, 1.0);
            tab_widths.push(tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s);
        }

        let mut hovered_tab_path = None;
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
            if drag.threshold_passed {
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
                if let Some(p) = paths[i] {
                    hovered_tab_path = Some(p.clone());
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

            let icon_key = if is_active {
                crate::app::file_icons::file_icon_key(&title.to_lowercase())
            } else {
                tab.icon_key
            };

            let icon_y = (y + (h - icon_size_tab) / 2.0 - 1.5 * s).round();
            self.draw_file_icon(icon_key, false, current_x + tab_pad, icon_y, icon_size_tab);

            let text_color =
                if paths[i].is_some_and(|path| tab_path_is_external(path, ide_workspaces)) {
                    EXTERNAL_TAB_TITLE_COLOR
                } else if is_active {
                    self.theme.fg
                } else {
                    self.theme.line_num
                };
            let text_x = current_x + tab_pad + icon_size_tab + 8.0 * s;
            self.draw_string_scaled(title, text_x, y + h / 2.0 + 5.0 * s, text_color, 1.0);

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

                if close_rect_right > x && close_rect_x < x + w {
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

        if let (Some(path), Some((anchor_x, anchor_y))) = (hovered_tab_path, tooltip_anchor) {
            if !self.hide_popups_until_mouse_move {
                return Some((path.clone(), anchor_x, anchor_y));
            }
        }
        None
    }

    pub fn draw_tab_tooltip(
        &mut self,
        path: &std::path::PathBuf,
        hovered_tab_x: f32,
        hovered_tab_y: f32,
        s: f32,
    ) {
        let mut path_str = path.to_string_lossy().into_owned();
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
        let workspaces = vec![std::path::PathBuf::from("/work/app")];

        assert!(!tab_path_is_external(
            std::path::Path::new("/work/app/pkg/file.py"),
            &workspaces
        ));
        assert!(tab_path_is_external(
            std::path::Path::new("/tmp/site-packages/lib.py"),
            &workspaces
        ));
        assert!(!tab_path_is_external(
            std::path::Path::new("relative.py"),
            &workspaces
        ));
        assert!(!tab_path_is_external(
            std::path::Path::new("/tmp/file.py"),
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
}
