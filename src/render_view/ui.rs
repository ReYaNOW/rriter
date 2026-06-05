use crate::renderer::{IconAtlasEntry, Renderer};
use glow::HasContext;
use std::borrow::Cow;

mod hover_widget;
mod problems_panel;

pub(crate) use hover_widget::diag_popup_byte_at;

#[derive(Clone, Copy, Debug, PartialEq)]
struct AutocompleteBadgeStyle {
    letter: Option<char>,
    bg: [f32; 4],
    fg: [f32; 4],
}

fn autocomplete_badge_style(kind: crate::highlighter::SymbolKind) -> AutocompleteBadgeStyle {
    match kind {
        crate::highlighter::SymbolKind::Class => AutocompleteBadgeStyle {
            letter: Some('C'),
            bg: [0.14, 0.33, 0.42, 1.0],
            fg: [0.58, 0.88, 1.0, 1.0],
        },
        crate::highlighter::SymbolKind::Function => AutocompleteBadgeStyle {
            letter: Some('F'),
            bg: [0.19, 0.36, 0.22, 1.0],
            fg: [0.66, 0.94, 0.68, 1.0],
        },
        crate::highlighter::SymbolKind::Variable => AutocompleteBadgeStyle {
            letter: Some('V'),
            bg: [0.32, 0.23, 0.42, 1.0],
            fg: [0.78, 0.64, 1.0, 1.0],
        },
        crate::highlighter::SymbolKind::Parameter => AutocompleteBadgeStyle {
            letter: Some('P'),
            bg: [0.42, 0.31, 0.16, 1.0],
            fg: [1.0, 0.79, 0.42, 1.0],
        },
        crate::highlighter::SymbolKind::Argument => AutocompleteBadgeStyle {
            letter: Some('A'),
            bg: [0.44, 0.25, 0.12, 1.0],
            fg: [1.0, 0.68, 0.34, 1.0],
        },
        crate::highlighter::SymbolKind::Property => AutocompleteBadgeStyle {
            letter: Some('P'),
            bg: [0.35, 0.25, 0.14, 1.0],
            fg: [1.0, 0.72, 0.38, 1.0],
        },
        crate::highlighter::SymbolKind::Module => AutocompleteBadgeStyle {
            letter: Some('M'),
            bg: [0.17, 0.28, 0.43, 1.0],
            fg: [0.62, 0.78, 1.0, 1.0],
        },
        crate::highlighter::SymbolKind::Builtin => AutocompleteBadgeStyle {
            letter: None,
            bg: [0.29, 0.24, 0.48, 1.0],
            fg: [0.74, 0.66, 1.0, 1.0],
        },
        crate::highlighter::SymbolKind::Keyword => AutocompleteBadgeStyle {
            letter: Some('K'),
            bg: [0.43, 0.20, 0.30, 1.0],
            fg: [1.0, 0.61, 0.80, 1.0],
        },
        crate::highlighter::SymbolKind::Unknown => AutocompleteBadgeStyle {
            letter: Some('U'),
            bg: [0.25, 0.26, 0.29, 1.0],
            fg: [0.70, 0.72, 0.76, 1.0],
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutocompleteRowEdges {
    top: bool,
    bottom: bool,
}

fn autocomplete_row_edges(
    row_y: f32,
    popup_y: f32,
    row_h: f32,
    popup_h: f32,
) -> AutocompleteRowEdges {
    let eps = 0.5;
    AutocompleteRowEdges {
        top: row_y <= popup_y + eps,
        bottom: row_y + row_h >= popup_y + popup_h - eps,
    }
}

fn autocomplete_scrollbar_track_margin(scale: f32) -> f32 {
    3.0 * scale
}

fn autocomplete_popup_width(screen_w: f32, x: f32, min_width: f32, scale: f32) -> f32 {
    let min_w = 400.0 * scale;
    let target_w = 590.0 * scale;
    let max_w = 760.0 * scale;
    let edge_margin = 8.0 * scale;
    let available_w = (screen_w - x - edge_margin).max(195.0 * scale);
    target_w
        .max(min_width)
        .max(min_w)
        .min(max_w)
        .min(available_w)
}

fn autocomplete_source_is_type_or_signature(source: &str, class_repr: bool) -> bool {
    if class_repr {
        return false;
    }
    source.contains('|')
        || source.contains('[')
        || source.contains(']')
        || source.contains("->")
        || source.starts_with("def ")
        || source.starts_with("async def ")
        || source.starts_with("overload[")
        || source.starts_with('(')
        || matches!(
            source,
            "Any"
                | "None"
                | "bool"
                | "bytes"
                | "dict"
                | "float"
                | "int"
                | "list"
                | "set"
                | "str"
                | "tuple"
                | "type"
        )
}

fn autocomplete_source_label<'a>(source: &'a str, word: &str) -> Option<Cow<'a, str>> {
    let source = source.trim();
    let class_repr = source.starts_with("<class '");
    let label = source
        .strip_prefix("<class '")
        .and_then(|s| s.strip_suffix("'>"))
        .or_else(|| {
            source
                .strip_prefix("<module '")
                .and_then(|s| s.strip_suffix("'>"))
        })
        .unwrap_or(source)
        .trim();
    if label.is_empty()
        || label == word
        || label.contains('/')
        || label.contains('\\')
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        || autocomplete_source_is_type_or_signature(label, class_repr)
    {
        None
    } else {
        Some(Cow::Borrowed(label))
    }
}

fn autocomplete_source_is_type_label(source: &str) -> bool {
    let source = source.trim();
    !source.contains('.')
        && source
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        && source.chars().any(|c| c.is_ascii_lowercase())
        && source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn autocomplete_row_source<'a>(item: &'a crate::app::AutocompleteItem) -> Option<&'a str> {
    match (item.module.as_deref(), item.module_path.as_deref()) {
        (Some(module), Some(module_path))
            if autocomplete_source_is_type_label(module)
                && autocomplete_source_label(module_path, &item.word).is_some() =>
        {
            Some(module_path)
        }
        (Some(module), _) if autocomplete_source_label(module, &item.word).is_some() => {
            Some(module)
        }
        (_, Some(module_path)) if autocomplete_source_label(module_path, &item.word).is_some() => {
            Some(module_path)
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
struct AutocompleteModuleLayout {
    x: f32,
    width: f32,
    text: String,
}

fn autocomplete_module_layout(
    source: &str,
    word: &str,
    min_x: f32,
    right_limit: f32,
    scale: f32,
    ellipsis_w: f32,
    mut char_width: impl FnMut(char, f32) -> f32,
) -> Option<AutocompleteModuleLayout> {
    let label = autocomplete_source_label(source, word)?;
    let limit = (right_limit - min_x).max(0.0);
    if limit < 34.0 * scale {
        return None;
    }
    let mut width = 0.0;
    let mut end = label.len();
    let mut truncated = false;
    for (idx, c) in label.char_indices() {
        if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
            continue;
        }
        let next_w = width + char_width(c, scale);
        let next_idx = idx + c.len_utf8();
        let suffix_w = if next_idx < label.len() {
            ellipsis_w
        } else {
            0.0
        };
        if next_w + suffix_w > limit {
            end = idx;
            truncated = true;
            break;
        }
        width = next_w;
    }
    let text = if truncated {
        let mut s = String::with_capacity(end + 3);
        s.push_str(&label[..end]);
        s.push_str("...");
        width += ellipsis_w;
        s
    } else {
        label.to_string()
    };
    Some(AutocompleteModuleLayout {
        x: (right_limit - width).max(min_x),
        width,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlighter::SymbolKind;

    #[test]
    fn autocomplete_badges_match_lapce_style_letters() {
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Class).letter,
            Some('C')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Function).letter,
            Some('F')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Variable).letter,
            Some('V')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Parameter).letter,
            Some('P')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Argument).letter,
            Some('A')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Property).letter,
            Some('P')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Module).letter,
            Some('M')
        );
        assert_eq!(autocomplete_badge_style(SymbolKind::Builtin).letter, None);
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Keyword).letter,
            Some('K')
        );
        assert_eq!(
            autocomplete_badge_style(SymbolKind::Unknown).letter,
            Some('U')
        );
    }

    #[test]
    fn autocomplete_module_layout_right_aligns_and_truncates_to_slot() {
        let layout = autocomplete_module_layout(
            "car_wash.domains.washes.bookings.repositories.deep.output",
            "BookingRead",
            120.0,
            160.0,
            1.0,
            3.0,
            |_, _| 1.0,
        )
        .unwrap();
        assert!(layout.text.ends_with("..."));
        assert!(layout.x >= 120.0);
        assert_eq!(layout.x + layout.width, 160.0);

        let full = autocomplete_module_layout(
            "car_wash.core",
            "RepoBase",
            80.0,
            180.0,
            1.0,
            3.0,
            |_, _| 1.0,
        )
        .unwrap();
        assert_eq!(full.text, "car_wash.core");
        assert_eq!(full.x + full.width, 180.0);
    }

    #[test]
    fn autocomplete_row_edges_mark_visible_popup_corners() {
        assert_eq!(
            autocomplete_row_edges(20.0, 20.0, 36.0, 108.0),
            AutocompleteRowEdges {
                top: true,
                bottom: false
            }
        );
        assert_eq!(
            autocomplete_row_edges(56.0, 20.0, 36.0, 108.0),
            AutocompleteRowEdges {
                top: false,
                bottom: false
            }
        );
        assert_eq!(
            autocomplete_row_edges(92.0, 20.0, 36.0, 108.0),
            AutocompleteRowEdges {
                top: false,
                bottom: true
            }
        );
        assert_eq!(
            autocomplete_row_edges(20.0, 20.0, 36.0, 36.0),
            AutocompleteRowEdges {
                top: true,
                bottom: true
            }
        );
    }

    #[test]
    fn autocomplete_scrollbar_track_margin_stays_close_to_popup_edges() {
        assert_eq!(autocomplete_scrollbar_track_margin(1.0), 3.0);
        assert_eq!(autocomplete_scrollbar_track_margin(2.0), 6.0);
    }

    #[test]
    fn autocomplete_popup_width_is_stable_before_details_arrive() {
        assert_eq!(autocomplete_popup_width(1000.0, 100.0, 0.0, 1.0), 590.0);
        assert_eq!(autocomplete_popup_width(1000.0, 100.0, 580.0, 1.0), 590.0);
        assert_eq!(autocomplete_popup_width(1000.0, 100.0, 700.0, 1.0), 700.0);
        assert_eq!(autocomplete_popup_width(430.0, 100.0, 0.0, 1.0), 322.0);
    }

    #[test]
    fn autocomplete_source_label_strips_duplicate_python_class_repr() {
        assert_eq!(
            autocomplete_source_label("<class 'RepoBase'>", "RepoBase").as_deref(),
            None
        );
        assert_eq!(
            autocomplete_source_label("<class 'str'>", "strip").as_deref(),
            Some("str")
        );
        assert_eq!(
            autocomplete_source_label("car_wash.core.db.repo_base", "RepoBase").as_deref(),
            Some("car_wash.core.db.repo_base")
        );
        assert_eq!(
            autocomplete_source_label("def dir(o: object = ..., /) -> list[str]", "dir").as_deref(),
            None
        );
        assert_eq!(
            autocomplete_source_label("overload[(*values: object, sep: str) -> None]", "print")
                .as_deref(),
            None
        );
        assert_eq!(
            autocomplete_source_label("builtins", "print").as_deref(),
            Some("builtins")
        );
        assert_eq!(
            autocomplete_source_label("builtins", "bool").as_deref(),
            Some("builtins")
        );
    }

    #[test]
    fn autocomplete_row_source_prefers_module_path_over_type_label() {
        let item = crate::app::AutocompleteItem {
            word: "cars_router".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: 0,
            module: Some("Router".to_string()),
            module_path: Some("car_wash.domains.cars.controller".to_string()),
            detail: Some("Router".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        };
        assert_eq!(
            autocomplete_row_source(&item),
            Some("car_wash.domains.cars.controller")
        );

        let builtin = crate::app::AutocompleteItem {
            word: "issubclass".to_string(),
            kind: SymbolKind::Function,
            scope_start: 0,
            scope_end: 0,
            module: Some("builtins".to_string()),
            module_path: Some("builtins.issubclass".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        };
        assert_eq!(autocomplete_row_source(&builtin), Some("builtins"));

        let generic_builtin = crate::app::AutocompleteItem {
            word: "map".to_string(),
            kind: SymbolKind::Class,
            scope_start: 0,
            scope_end: 0,
            module: Some("builtins".to_string()),
            module_path: Some("builtins.map".to_string()),
            detail: Some("class map(Generic[_S])\n---\nMake an iterator.".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        };
        assert_eq!(autocomplete_row_source(&generic_builtin), Some("builtins"));
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn draw_icon(&mut self, tex: &glow::Texture, x: f32, y: f32, w: f32, h: f32) {
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
        }
        self.push_quad(x, y, w, h, 0.0, 0.0, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        }
    }

    pub fn draw_atlas_icon(
        &mut self,
        icon: crate::widgets::IconType,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) {
        let entry = if let Some(&entry) = self.icons.get(&icon) {
            entry
        } else {
            let Some(entry) = self.upload_builtin_icon(icon) else {
                return;
            };
            self.icons.insert(icon, entry);
            entry
        };
        let color = if icon == crate::widgets::IconType::Api {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            color
        };
        self.push_quad(
            x,
            y,
            size,
            size,
            entry.u,
            entry.v,
            entry.uw,
            entry.vh,
            color,
            5.0,
        );
    }

    fn upload_file_icon_from_pending_raster(
        &mut self,
        key: &'static str,
        is_folder: bool,
    ) -> Option<IconAtlasEntry> {
        use crate::app::file_tree::RasterizedIconState;

        let mut cache = crate::app::file_tree::RASTERIZED_ICONS.lock().unwrap();

        if let Some(state) = cache.remove(key) {
            match state {
                RasterizedIconState::Ready(data) => {
                    drop(cache);
                    let entry = self.upload_icon_rgba(64, 64, &data)?;
                    self.file_icon_cache.insert(key, entry);
                    Some(entry)
                }
                state @ (RasterizedIconState::Pending | RasterizedIconState::Missing) => {
                    cache.insert(key, state);
                    None
                }
            }
        } else {
            drop(cache);
            crate::app::file_tree::request_rasterized_icon(key, is_folder);
            None
        }
    }

    /// Рисует SVG-иконку из кэша file_icon_cache.
    /// Загружает текстуру при первом обращении (не в draw-цикле — только при промахе кэша).
    pub fn draw_file_icon(
        &mut self,
        key: &'static str,
        is_folder: bool,
        x: f32,
        y: f32,
        size: f32,
    ) {
        let entry = if let Some(&entry) = self.file_icon_cache.get(key) {
            entry
        } else {
            let Some(entry) = self.upload_file_icon_from_pending_raster(key, is_folder) else {
                return;
            };
            entry
        };
        self.push_quad(
            x,
            y,
            size,
            size,
            entry.u,
            entry.v,
            entry.uw,
            entry.vh,
            [1.0, 1.0, 1.0, 1.0],
            5.0,
        );
    }

    // (функции удалены)

    fn push_autocomplete_text_row_bg(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: [f32; 4],
        edges: AutocompleteRowEdges,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if !edges.top && !edges.bottom {
            self.push_rect(x, y, w, h, color);
            return;
        }
        let r = radius.min(h * 0.5).min(w * 0.5);
        self.push_rounded_rect(x, y, w, h, r, color);
        self.push_rect(x, y, r, h, color);
        if !edges.top {
            self.push_rect(x, y, w, r, color);
        }
        if !edges.bottom {
            self.push_rect(x, y + h - r, w, r, color);
        }
    }

    pub fn draw_autocomplete(
        &mut self,
        x: f32,
        mut y: f32,
        options: &[(crate::app::AutocompleteItem, Vec<usize>)],
        mode: crate::app::AutocompleteMode,
        selected_idx: usize,
        anim_progress: f32,
        scroll_y: f32,
        hovered_idx: Option<usize>,
        min_width: f32,
    ) -> (f32, f32, f32, f32) {
        let scale = self.scale_factor;
        if options.is_empty() {
            if mode != crate::app::AutocompleteMode::TyImports {
                return (x, y, 0.0, 0.0);
            }

            let max_w = autocomplete_popup_width(self.width, x, min_width, scale)
                .max(220.0 * scale)
                .min((self.width - x - 8.0 * scale).max(195.0 * scale));
            let target_h = 36.0 * scale;
            let anim_progress = anim_progress.clamp(0.0, 1.0);
            let smooth_progress = anim_progress
                * anim_progress
                * anim_progress
                * (anim_progress * (anim_progress * 6.0 - 15.0) + 10.0);
            let current_h = target_h * smooth_progress;
            if y + target_h > self.height {
                y -= target_h + 10.0 * scale;
            } else {
                y += 10.0 * scale;
            }
            if current_h < 1.0 {
                return (x, y, max_w, current_h);
            }

            let border_width = 2.0 * scale;
            self.push_rounded_rect_border(
                x - border_width,
                y - border_width,
                max_w + border_width * 2.0,
                current_h + border_width * 2.0,
                6.0 * scale,
                border_width,
                [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0],
                [0.15, 0.16, 0.20, 1.0],
            );
            self.draw_string_scaled(
                "Начните набирать",
                x + 12.0 * scale,
                y + 23.0 * scale,
                [0.62, 0.64, 0.70, 1.0],
                0.9,
            );
            return (x, y, max_w, current_h);
        }

        let step = 36.0 * scale;
        let item_h = 28.0 * scale;
        let padding_top = 0.0;
        let padding_bottom = 0.0;

        let border_width = 2.0 * scale;
        let icon_sz = step;
        let icon_gap = 8.0 * scale;
        let right_pad = 18.0 * scale;
        let content_start_offset = icon_sz + icon_gap;
        let module_scale = 1.0;
        let mut max_name_w: f32 = 0.0;
        for (item, _) in options {
            let name_w: f32 = item
                .word
                .chars()
                .filter_map(|c| self.get_glyph(c).map(|glyph| glyph.advance))
                .sum();
            max_name_w = max_name_w.max(name_w);
        }
        let available_w = (self.width - x - 8.0 * scale).max(195.0 * scale);
        let name_min_w = content_start_offset + max_name_w + right_pad + 20.0 * scale;
        let max_w = autocomplete_popup_width(self.width, x, min_width, scale)
            .max(name_min_w)
            .min(available_w);

        let visible_items = options.len().max(1).min(7);

        let target_h = visible_items as f32 * step + padding_top + padding_bottom;
        let total_h = options.len().max(1) as f32 * step + padding_top + padding_bottom;

        let anim_progress = anim_progress.clamp(0.0, 1.0);
        let smooth_progress = anim_progress
            * anim_progress
            * anim_progress
            * (anim_progress * (anim_progress * 6.0 - 15.0) + 10.0);
        let current_h = target_h * smooth_progress;

        if y + target_h > self.height {
            y -= target_h + 10.0 * scale;
        } else {
            y += 10.0 * scale;
        }

        if current_h < 1.0 {
            return (x, y, max_w, current_h);
        }

        for i in 1..=5 {
            let offset = i as f32 * scale;
            let alpha = (0.15 - (i as f32 * 0.03)) * smooth_progress;
            self.push_rounded_rect(
                x - offset,
                y - offset,
                max_w + offset * 2.0,
                current_h + offset * 2.0,
                6.0 * scale,
                [0.0, 0.0, 0.0, alpha],
            );
        }

        let bg_color = [0.15, 0.16, 0.20, 1.0];
        let border_color = [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0];
        self.push_rounded_rect_border(
            x - border_width,
            y - border_width,
            max_w + border_width * 2.0,
            current_h + border_width * 2.0,
            6.0 * scale,
            border_width,
            border_color,
            bg_color,
        );

        self.flush();

        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sx = x.floor().max(0.0) as i32;
            let sy = (self.height - (y + current_h)).max(0.0).floor() as i32;
            self.gl.scissor(
                sx,
                sy,
                max_w.ceil().max(1.0) as i32,
                current_h.ceil().max(1.0) as i32,
            );
        }

        let render_scroll_y = scroll_y.round();
        let mut current_y = y + padding_top - render_scroll_y;

        for (i, (item, matches)) in options.iter().enumerate() {
            if current_y + step < y || current_y > y + current_h {
                current_y += step;
                continue;
            }

            let row_y = current_y.round();
            let row_edges = autocomplete_row_edges(row_y, y, step, current_h);
            let badge = autocomplete_badge_style(item.kind);
            let badge_x = x;
            let badge_w = icon_sz;
            let badge_h = step;
            self.push_rect(badge_x, row_y, badge_w, badge_h, badge.bg);
            let text_bg_x = x + icon_sz;
            let text_bg_w = (max_w - icon_sz).max(0.0);

            if i == selected_idx {
                self.push_autocomplete_text_row_bg(
                    text_bg_x,
                    row_y,
                    text_bg_w,
                    step,
                    4.0 * scale,
                    [0.25, 0.27, 0.35, 1.0],
                    row_edges,
                );
            } else if Some(i) == hovered_idx {
                self.push_autocomplete_text_row_bg(
                    text_bg_x,
                    row_y,
                    text_bg_w,
                    step,
                    4.0 * scale,
                    [0.20, 0.21, 0.28, 1.0],
                    row_edges,
                );
            }

            if let Some(letter) = badge.letter {
                if let Some(g) = self.get_ui_glyph(letter) {
                    let char_scale = 0.82;
                    let actual_w = g.width * char_scale;
                    let actual_h = g.height * char_scale;
                    let char_x = badge_x + (badge_w - actual_w) / 2.0;
                    let char_y = row_y + (step - actual_h) / 2.0;

                    self.push_quad(
                        char_x.round(),
                        char_y.round(),
                        actual_w,
                        actual_h,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        badge.fg,
                        0.0,
                    );
                }
            }
            let mut cx = x + icon_sz + icon_gap;

            let cy = row_y + item_h * 0.72 + (step - item_h) * 0.5;

            let name_w: f32 = item
                .word
                .chars()
                .filter_map(|c| self.get_glyph(c).map(|glyph| glyph.advance))
                .sum();
            let module_gap = 14.0 * scale;
            let module_min_x = cx + name_w + module_gap;
            let right_limit = x + max_w - right_pad;
            let ellipsis_w = self.measure_ui_width("...", module_scale);
            let module_metrics = autocomplete_row_source(item).and_then(|module| {
                autocomplete_module_layout(
                    module,
                    &item.word,
                    module_min_x,
                    right_limit,
                    scale,
                    ellipsis_w,
                    |c, _| {
                        self.get_ui_glyph(c)
                            .map(|glyph| glyph.advance * module_scale)
                            .unwrap_or(0.0)
                    },
                )
            });
            let mut truncated = false;
            for (j, c) in item.word.chars().enumerate() {
                if let Some(g) = self.get_glyph(c) {
                    if cx + g.advance > right_limit {
                        truncated = true;
                        break;
                    }

                    let color = if matches.contains(&j) {
                        [1.0, 0.474, 0.776, 1.0]
                    } else {
                        self.theme.fg
                    };

                    self.push_quad(
                        (cx + g.offset_x).round(),
                        (cy - g.offset_y).round(),
                        g.width,
                        g.height,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        color,
                        g.is_emoji,
                    );
                    cx += g.advance;
                }
            }

            if truncated {
                self.draw_string_scaled("...", cx.round(), cy.round(), [0.5, 0.5, 0.55, 1.0], 1.0);
            }

            if let Some(module) = module_metrics {
                self.draw_string_scaled(
                    &module.text,
                    module.x.round(),
                    (cy - 1.5 * scale).round(),
                    [0.50, 0.72, 0.82, 1.0],
                    module_scale,
                );
            }

            current_y += step;
        }

        if total_h > target_h {
            let max_scroll = (total_h - target_h).max(0.0);
            let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);

            let track_margin = autocomplete_scrollbar_track_margin(scale);
            let track_h = (current_h - track_margin * 2.0).max(1.0);
            let thumb_h = (current_h / total_h * track_h).max(20.0 * scale);
            let thumb_y = y + track_margin + scroll_ratio * (track_h - thumb_h);

            let alpha = (smooth_progress * 1.5).clamp(0.0, 0.8);

            self.push_rounded_rect(
                x + max_w - 10.0 * scale,
                thumb_y,
                6.0 * scale,
                thumb_h,
                3.0 * scale,
                [0.7, 0.33, 0.54, alpha],
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        self.push_rounded_rect_outline(
            x - border_width,
            y - border_width,
            max_w + border_width * 2.0,
            current_h + border_width * 2.0,
            6.0 * scale,
            border_width,
            border_color,
        );

        (x, y, max_w, current_h)
    }

    pub fn draw_dialog_window(&mut self, base_title: &str) -> bool {
        let s = self.scale_factor;
        let box_w = 660.0 * s;
        let box_h = 260.0 * s;
        let box_x = 0.0;
        let box_y = 0.0;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        self.push_vertical_gradient(box_x, box_y, box_w, box_h, top_color, bottom_color);

        let pad_h = 24.0 * s;
        let pad_v = 18.0 * s;
        let btn_h = 44.0 * s;
        let btn_margin = 12.0 * s;
        let content_x = (box_x + pad_h).round();
        let content_y = (box_y + pad_v).round();
        let content_w = (box_w - pad_h * 2.0).round();
        let content_h = (box_h - pad_v - btn_h - btn_margin * 2.0 - pad_v).round();

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            8.0 * s,
            [0.15, 0.16, 0.20, 1.0],
        );

        let msg1 = format!("Документ «{}» был изменен.", base_title);
        let msg2 = "Сохранить или отклонить изменения?";

        let icon_sz = 120.0 * s;
        let gap = 45.0 * s;
        let padding_inner = 20.0 * s;

        let icon_x = content_x + padding_inner;
        let icon_y = content_y + (content_h - icon_sz) / 2.0;

        self.draw_atlas_icon(
            crate::widgets::IconType::Warning,
            icon_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );

        let text_x = icon_x + icon_sz + gap;
        let fg = self.theme.fg;
        let text_scale = 1.05;
        let line_h = 28.0 * s;
        let text_block_h = line_h * 2.0;
        let text_y_start = content_y + (content_h - text_block_h) / 2.0 + line_h * 0.85;

        self.draw_string_scaled(&msg1, text_x, text_y_start, fg, text_scale);
        self.draw_string_scaled(
            msg2,
            text_x,
            text_y_start + line_h,
            [0.75, 0.75, 0.80, 1.0],
            text_scale,
        );

        let (btn_save, btn_discard, btn_cancel) =
            crate::widgets::get_dialog_buttons(box_x, box_y, box_w, box_h, s, self);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        // Регистрируем через UI систему — убирает дублирование хитбоксов в input.rs
        let mut ui_reg = crate::ui_system::UiRegistry::new();
        ui_reg.register_button(
            crate::ui_system::UiId::DialogSave,
            &btn_save,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_reg.register_button(
            crate::ui_system::UiId::DialogDiscard,
            &btn_discard,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_reg.register_button(
            crate::ui_system::UiId::DialogCancel,
            &btn_cancel,
            self,
            mx,
            my,
            s,
            false,
        );

        self.flush();
        ui_reg.wants_pointer()
    }

    pub fn draw_welcome(
        &mut self,
        recent_files: &[std::path::PathBuf],
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> bool {
        let scale = self.scale_factor;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl
                .clear_color(bottom_color[0], bottom_color[1], bottom_color[2], 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.push_vertical_gradient(
            -1.0,
            -1.0,
            self.width + 2.0,
            self.height + 2.0,
            top_color,
            bottom_color,
        );
        self.flush();

        let content_x = 40.0 * scale;
        let content_y = 40.0 * scale;
        let content_w = self.width - 80.0 * scale;
        let content_h = self.height - 80.0 * scale;

        let card_bg = [0.169, 0.176, 0.188, 0.95];
        let card_border = [0.224, 0.231, 0.251, 1.0];

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            10.0 * scale,
            card_border,
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            10.0 * scale,
            card_bg,
        );

        let title_x = content_x + 40.0 * scale;
        let mut y = content_y + 60.0 * scale;

        if let Some(tex) = self.icon_logo {
            let icon_y = y - 40.0 * scale;
            self.draw_icon(&tex, title_x, icon_y, 110.0 * scale, 110.0 * scale);
        }

        self.draw_string_scaled(
            "Добро пожаловать в RRiter",
            title_x + 130.0 * scale,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );
        y += 40.0 * scale;
        self.draw_string_scaled(
            "Молниеносный текстовый редактор с GPU-рендерингом",
            title_x + 130.0 * scale,
            y,
            [0.7, 0.7, 0.75, 1.0],
            1.0,
        );

        y += 60.0 * scale;
        let (btn_new, btn_open, btn_ide) =
            crate::widgets::get_welcome_buttons(content_w, title_x, y, scale, self);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        // Регистрируем кнопки через UI систему
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeNewFile,
            &btn_new,
            self,
            mx,
            my,
            scale,
            false,
        );
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeOpenFile,
            &btn_open,
            self,
            mx,
            my,
            scale,
            false,
        );
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeIdeMode,
            &btn_ide,
            self,
            mx,
            my,
            scale,
            false,
        );

        y += 80.0 * scale;
        self.draw_string_scaled(
            "Недавние файлы",
            title_x,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );

        let line_y = y + 20.0 * scale;
        self.push_rect(
            title_x,
            line_y,
            content_w - 80.0 * scale,
            1.0,
            [1.0, 1.0, 1.0, 0.08],
        );

        y += 35.0 * scale;

        let item_h = 44.0 * scale;
        for (idx, path) in recent_files.iter().enumerate() {
            if y + item_h > content_y + content_h - 60.0 * scale {
                break;
            }

            // Регистрируем кликабельную область для недавнего файла
            ui_registry.register_rect(
                crate::ui_system::UiId::WelcomeRecentFile(idx),
                title_x - 10.0 * scale,
                y,
                content_w - 60.0 * scale,
                item_h,
                mx,
                my,
            );

            let is_hovered = mx >= title_x - 10.0 * scale
                && mx <= title_x + content_w - 70.0 * scale
                && my >= y
                && my < y + item_h;

            if is_hovered {
                self.push_rounded_rect(
                    title_x - 10.0 * scale,
                    y,
                    content_w - 60.0 * scale,
                    item_h,
                    6.0 * scale,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let full_dir = path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy();

            self.draw_string_scaled(&name, title_x, y + 25.0 * scale, [0.9, 0.9, 0.9, 1.0], 1.0);
            let name_w = self.measure_ui_width(&name, 1.0);
            self.draw_string_scaled(
                &full_dir,
                title_x + name_w + 15.0 * scale,
                y + 25.0 * scale,
                [0.5, 0.5, 0.5, 1.0],
                0.95,
            );

            self.push_rect(
                title_x,
                y + item_h - 1.0,
                content_w - 80.0 * scale,
                1.0,
                [1.0, 1.0, 1.0, 0.04],
            );

            y += item_h;
        }

        let hint_str_1 = "F1";
        let hint_str_2 = " — Настройки редактора";
        let scale_hint = 0.9;

        let w1 = self.measure_ui_width(hint_str_1, scale_hint) + 16.0 * scale;
        let w2 = self.measure_ui_width(hint_str_2, scale_hint);
        let hint_total_w = w1 + w2;

        let hint_x = content_x + content_w - hint_total_w - 30.0 * scale;
        let hint_y = content_y + content_h - 30.0 * scale;

        let kbd_bg = [0.224, 0.231, 0.251, 1.0];
        let kbd_border = [0.306, 0.318, 0.341, 1.0];
        let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

        let kbd_h = 22.0 * scale;
        let kbd_draw_y = hint_y - 16.0 * scale;

        self.push_rounded_rect(
            hint_x - 1.0,
            kbd_draw_y - 1.0,
            w1 + 2.0,
            kbd_h + 2.0,
            4.0 * scale,
            kbd_border,
        );
        self.push_rounded_rect(hint_x, kbd_draw_y, w1, kbd_h, 4.0 * scale, kbd_bg);

        self.draw_string_scaled(
            hint_str_1,
            hint_x + 8.0 * scale,
            hint_y,
            kbd_text_color,
            scale_hint,
        );

        self.draw_string_scaled(
            hint_str_2,
            hint_x + w1,
            hint_y,
            [0.5, 0.5, 0.55, 1.0],
            scale_hint,
        );

        self.flush();
        ui_registry.wants_pointer()
    }

    /// Рисует индикаторы ошибок и предупреждений слева от скроллбара
    pub fn draw_diagnostics_ruler(
        &mut self,
        editor: &crate::editor::Editor,
        lsp_diags: &[crate::lsp::Diagnostic],
        track_y: f32,
        track_h: f32,
        scrollbar_w: f32,
    ) {
        if lsp_diags.is_empty() || editor.line_offsets.is_empty() || track_h <= 0.0 {
            return;
        }

        let s = self.scale_factor;
        let minimap_w = self.minimap_width;
        let cursor_phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);

        let total_vis_lines = self.phys_to_visual.last().copied().unwrap_or(0) as f32 + 1.0;
        let bottom_blank_lines = super::editor_bottom_blank_lines(track_h, self.line_height);
        let ruler_lines = total_vis_lines + bottom_blank_lines;
        if total_vis_lines < 1.0 {
            return;
        }

        // Полоса слева от скроллбара
        let bar_w = (4.0 * s).max(2.0);
        let bar_x = self.width - minimap_w - scrollbar_w - bar_w;

        // Группируем, чтобы не рисовать черточки друг на друге
        let mut lines_with_errors = std::collections::HashSet::new();
        let mut lines_with_warnings = std::collections::HashSet::new();

        for diag in lsp_diags {
            if crate::render_view::should_suppress_active_line_useless_expression(
                diag,
                cursor_phys_line,
            ) {
                continue;
            }
            match diag.severity {
                crate::lsp::DiagSeverity::Error => {
                    lines_with_errors.insert(diag.start_line);
                }
                crate::lsp::DiagSeverity::Warning => {
                    lines_with_warnings.insert(diag.start_line);
                }
                _ => {}
            }
        }

        let indicator_h = (2.0 * s).max(1.0);

        // Сначала рисуем предупреждения
        for &line_num in &lines_with_warnings {
            if !lines_with_errors.contains(&line_num) {
                let Some(&vis_line) = self.phys_to_visual.get(line_num as usize) else {
                    continue;
                };
                let y = (track_y + (vis_line as f32 / ruler_lines * track_h)).round();
                self.push_rect(bar_x, y, bar_w, indicator_h, self.theme.diag_warn);
            }
        }

        // Потом ошибки (поверх)
        for &line_num in &lines_with_errors {
            let Some(&vis_line) = self.phys_to_visual.get(line_num as usize) else {
                continue;
            };
            let y = (track_y + (vis_line as f32 / ruler_lines * track_h)).round();
            self.push_rect(bar_x, y, bar_w, indicator_h, self.theme.diag_error);
        }
    }
    /// Рисует весёлый cowsay-экран когда в IDE-режиме нет открытых вкладок.
    /// Сайдбар уже нарисован до вызова, рисуем только зону редактора.
    pub fn draw_empty_ide(&mut self, panel_left_w: f32) {
        let s = self.scale_factor;
        let sb_w = 48.0 * s;
        let editor_x = sb_w + panel_left_w;
        let editor_w = self.width - editor_x;
        let editor_h = self.height;

        // Фон области редактора
        self.push_rect(
            editor_x,
            0.0,
            editor_w,
            editor_h,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );

        let arts: &[&[&str]] = &[
            &[
                " _________________________ ",
                "< Открой файл и погнали!  >",
                " ------------------------- ",
                "        \\   ^__^           ",
                "         \\  (oo)\\_______   ",
                "            (__)\\       )\\/\\",
                "                ||----w |  ",
                "                ||     || ",
            ],
            &[
                " ________________________________ ",
                "< Мяу! Код сам себя не напишет... >",
                " -------------------------------- ",
                "  \\",
                "   \\   /\\_/\\",
                "      ( o.o )",
                "       > ^ <",
            ],
            &[
                " _________________________ ",
                "< Прыгаем в код!           >",
                " ------------------------- ",
                "   \\",
                "    \\   //",
                "       ( ' )",
                "      /  _  \\",
                "     (__)(_)(__)",
            ],
            &[
                " _________________________ ",
                "< Судо, открой файл!       >",
                " ------------------------- ",
                "   \\",
                "    \\    .--.",
                "        |o_o |",
                "        |:_/ |",
                "       //   \\ \\",
                "      (|     | )",
                "     /'\\_   _/`\\",
                "     \\___)=(___/",
            ],
        ];

        if !self.was_empty_ide {
            self.was_empty_ide = true;
            let epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as usize;
            self.empty_ide_art_idx = epoch % arts.len();
        }
        let current_art = arts[self.empty_ide_art_idx];

        let hint_lines = ["Ctrl+O  — открыть файл", "Кликни в дереве файлов слева"];

        // Измеряем ширину для центрирования
        let mono_scale = 0.95_f32;
        let line_h = 22.0 * s;

        let art_total_h = current_art.len() as f32 * line_h;
        let hint_gap = 32.0 * s;
        let hint_total_h = hint_lines.len() as f32 * (line_h + 4.0 * s);
        let total_block_h = art_total_h + hint_gap + hint_total_h;

        let start_y = (editor_h - total_block_h) / 2.0;

        // Рисуем арт
        let art_color = [0.55_f32, 0.50, 0.75, 0.9];
        for (i, line) in current_art.iter().enumerate() {
            let lw = self.measure_ui_width(line, mono_scale);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (start_y + i as f32 * line_h + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, art_color, mono_scale);
        }

        // Разделитель
        let sep_y = start_y + art_total_h + hint_gap / 2.0;
        let sep_w = 200.0 * s;
        let sep_x = editor_x + (editor_w - sep_w) / 2.0;
        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.06]);

        // Подсказки
        let hint_y_start = start_y + art_total_h + hint_gap;
        for (i, line) in hint_lines.iter().enumerate() {
            let lw = self.measure_ui_width(line, 0.9);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (hint_y_start + i as f32 * (line_h + 4.0 * s) + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, [0.45, 0.45, 0.52, 1.0], 0.9);
        }

        self.flush();
    }
}
