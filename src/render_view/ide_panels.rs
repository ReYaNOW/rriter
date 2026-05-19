use crate::render_view::{
    cursor_line_and_character, diagnostic_error_warning_counts, ide_bottom_panel_y,
    ide_status_bar_height, ide_status_bar_y, language_display_name_for_ext, selected_char_count,
};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton};
use glow::HasContext;

fn clipped_label_prefix_len<F>(
    text: &str,
    max_w: f32,
    ellipsis_w: f32,
    mut char_advance: F,
) -> usize
where
    F: FnMut(char) -> f32,
{
    if max_w <= ellipsis_w {
        return 0;
    }
    let mut used = 0.0;
    let mut prefix_len = 0usize;
    for (idx, ch) in text.char_indices() {
        let adv = char_advance(ch);
        if used + adv + ellipsis_w > max_w {
            return prefix_len;
        }
        used += adv;
        prefix_len = idx + ch.len_utf8();
    }
    text.len()
}

fn centered_dialog_button_positions(x: f32, w: f32, btn_w: f32, gap: f32) -> (f32, f32) {
    let total_w = btn_w * 2.0 + gap;
    let first_x = x + (w - total_w) / 2.0;
    (first_x, first_x + btn_w + gap)
}

fn file_tree_menu_group(action: crate::app::file_tree::FileTreeMenuAction) -> u8 {
    match action {
        crate::app::file_tree::FileTreeMenuAction::CreateFile
        | crate::app::file_tree::FileTreeMenuAction::CreateDirectory
        | crate::app::file_tree::FileTreeMenuAction::Paste => 0,
        crate::app::file_tree::FileTreeMenuAction::Delete
        | crate::app::file_tree::FileTreeMenuAction::Copy
        | crate::app::file_tree::FileTreeMenuAction::Cut
        | crate::app::file_tree::FileTreeMenuAction::Rename => 1,
        crate::app::file_tree::FileTreeMenuAction::OpenContainedFolder
        | crate::app::file_tree::FileTreeMenuAction::CopyAbsolutePath
        | crate::app::file_tree::FileTreeMenuAction::CopyRelativePath => 2,
    }
}

fn file_tree_menu_separator_before(
    entries: &[crate::app::file_tree::FileTreeMenuAction],
    idx: usize,
) -> bool {
    idx > 0 && file_tree_menu_group(entries[idx - 1]) != file_tree_menu_group(entries[idx])
}

fn file_tree_menu_separator_count(entries: &[crate::app::file_tree::FileTreeMenuAction]) -> usize {
    (1..entries.len())
        .filter(|&idx| file_tree_menu_separator_before(entries, idx))
        .count()
}

fn git_row_visual_hovered(
    mx: f32,
    my: f32,
    panel_x: f32,
    row_y: f32,
    panel_w: f32,
    row_h: f32,
    workspace_disabled: bool,
) -> bool {
    !workspace_disabled
        && mx >= panel_x
        && mx <= panel_x + panel_w
        && my >= row_y
        && my <= row_y + row_h
}

fn git_folder_stage_hitbox_enabled(
    folder_stage: Option<crate::app::git_panel::GitFolderStageState>,
) -> bool {
    folder_stage.is_some()
}

fn git_file_row_hitbox_enabled(_controls_disabled: bool) -> bool {
    true
}

fn git_file_tooltip_hovered(row_hovered: bool, mx: f32, check_x: f32, check_size: f32) -> bool {
    row_hovered && mx > check_x + check_size
}

fn git_stage_controls_disabled(workspace_disabled: bool, git_pending: bool) -> bool {
    workspace_disabled || git_pending
}

fn git_checkbox_color(
    staged: bool,
    partial: bool,
    controls_disabled: bool,
) -> ([f32; 4], [f32; 4]) {
    if staged {
        let alpha = if controls_disabled { 0.42 } else { 0.95 };
        ([0.48, 0.82, 0.52, alpha], [0.07, 0.09, 0.12, alpha])
    } else if partial {
        (
            [1.0, 1.0, 1.0, if controls_disabled { 0.10 } else { 0.20 }],
            [
                0.72,
                0.76,
                0.88,
                if controls_disabled { 0.36 } else { 0.86 },
            ],
        )
    } else {
        (
            [1.0, 1.0, 1.0, if controls_disabled { 0.07 } else { 0.12 }],
            [0.0; 4],
        )
    }
}

fn render_git_disabled_button(renderer: &mut Renderer, button: &Button, s: f32) {
    let x = button.x.round();
    let y = button.y.round();
    let w = button.w.round();
    let h = button.h.round();
    let radius = 4.0 * s;
    let border_w = (1.0 * s).round().max(1.0);
    renderer.push_rounded_rect_border(
        x,
        y,
        w,
        h,
        radius,
        border_w,
        [0.60, 0.35, 0.85, 0.34],
        [0.18, 0.19, 0.22, 0.74],
    );
    let text_w = if button.text.is_empty() {
        0.0
    } else {
        renderer.measure_ui_width(&button.text, button.text_scale)
    };
    let icon_size = button.icon.map_or(0.0, |_| button.icon_size);
    let icon_gap = if button.icon.is_some() && !button.text.is_empty() {
        8.0 * s
    } else {
        0.0
    };
    let content_w = icon_size + icon_gap + text_w;
    let mut content_x = x + (w - content_w) / 2.0;
    if let Some(icon) = button.icon {
        renderer.draw_atlas_icon(
            icon,
            content_x,
            y + (h - icon_size) / 2.0,
            icon_size,
            [1.0, 1.0, 1.0, 0.34],
        );
        content_x += icon_size + icon_gap;
    }
    if !button.text.is_empty() {
        renderer.draw_string_scaled(
            &button.text,
            content_x,
            y + h / 2.0 + 5.0 * s,
            [1.0, 1.0, 1.0, 0.34],
            button.text_scale,
        );
    }
}

fn render_git_graph_button(
    renderer: &mut Renderer,
    button: &Button,
    s: f32,
    hovered: bool,
    active: bool,
) {
    let x = button.x.round();
    let y = button.y.round();
    let w = button.w.round();
    let h = button.h.round();
    let radius = 4.0 * s;
    let border_w = (1.0 * s).round().max(1.0);
    let border_color = renderer.theme.sel;
    let bg_color = if hovered {
        [0.28, 0.30, 0.33, 1.0]
    } else {
        [0.22, 0.24, 0.26, 1.0]
    };
    renderer.push_rounded_rect_border(x, y, w, h, radius, border_w, border_color, bg_color);
    if active {
        let bottom_h = radius.ceil().max(border_w);
        renderer.push_rect(x, y + h - bottom_h, w, bottom_h, border_color);
        renderer.push_rect(
            x + border_w,
            y + h - bottom_h,
            (w - border_w * 2.0).max(0.0),
            bottom_h,
            bg_color,
        );
    }

    let icon_size = button.icon_size;
    let text_w = renderer.measure_ui_width(&button.text, button.text_scale);
    let has_icon = button.icon.is_some();
    let icon_gap = if has_icon && !button.text.is_empty() {
        8.0 * s
    } else {
        0.0
    };
    let content_w = if has_icon { icon_size } else { 0.0 } + icon_gap + text_w;
    let icon_x = x + (w - content_w) / 2.0;
    let mut text_x = icon_x;
    if let Some(icon) = button.icon {
        renderer.draw_atlas_icon(
            icon,
            icon_x,
            y + (h - icon_size) / 2.0,
            icon_size,
            [1.0, 1.0, 1.0, 1.0],
        );
        text_x += icon_size + icon_gap;
    }
    if !button.text.is_empty() {
        renderer.draw_string_scaled(
            &button.text,
            text_x,
            y + h / 2.0 + 3.7 * s,
            renderer.theme.fg,
            button.text_scale,
        );
    }
}

fn register_git_locked_button_cursor(
    ui_registry: &mut crate::ui_system::UiRegistry,
    id: crate::ui_system::UiId,
    button: &Button,
    mx: f32,
    my: f32,
) {
    ui_registry.register_rect(id, button.x, button.y, button.w, button.h, mx, my);
}

fn git_disabled_color(mut color: [f32; 4], workspace_disabled: bool, alpha: f32) -> [f32; 4] {
    if workspace_disabled {
        color[3] = alpha;
    }
    color
}

fn git_status_word(status: crate::app::git_panel::GitFileStatus) -> &'static str {
    match status {
        crate::app::git_panel::GitFileStatus::Added => "Добавлен",
        crate::app::git_panel::GitFileStatus::Modified => "Изменен",
        crate::app::git_panel::GitFileStatus::Deleted => "Удален",
        crate::app::git_panel::GitFileStatus::Renamed => "Переименован",
        crate::app::git_panel::GitFileStatus::TypeChange => "Изменен тип",
        crate::app::git_panel::GitFileStatus::Untracked => "Не отслеживается",
    }
}

fn git_file_tooltip_path(file: &crate::app::git_panel::GitFileEntry) -> std::path::PathBuf {
    file.repo_root.join(file.rel_path.as_str())
}

fn compact_home_path(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        let rest = rest.to_string_lossy();
        return if rest.starts_with('/') {
            format!("~{rest}")
        } else {
            format!("~/{rest}")
        };
    }
    path.to_string_lossy().into_owned()
}

fn git_file_tooltip_text(
    file: &crate::app::git_panel::GitFileEntry,
    home: Option<&std::path::Path>,
) -> String {
    let path = git_file_tooltip_path(file);
    format!(
        "{} • {}",
        compact_home_path(&path, home),
        git_status_word(file.status)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GitTooltipTarget {
    kind: u8,
    workspace_idx: usize,
    item_idx: usize,
}

#[derive(Clone, Copy, Debug)]
struct GitTooltipTimer {
    target: GitTooltipTarget,
    start: std::time::Instant,
    anchor_x: f32,
    anchor_y: f32,
}

const GIT_TOOLTIP_DELAY_SECS: f32 = 0.4;
const GIT_TOOLTIP_FILE: u8 = 0;
const GIT_TOOLTIP_ROLLBACK: u8 = 1;
const GIT_TOOLTIP_STAGE_ALL: u8 = 2;
const GIT_TOOLTIP_UNSTAGE_ALL: u8 = 3;
const GIT_TOOLTIP_GRAPH_COMMIT: u8 = 4;

thread_local! {
    static GIT_TOOLTIP_TIMER: std::cell::RefCell<Option<GitTooltipTimer>> = const { std::cell::RefCell::new(None) };
}

fn git_tooltip_anchor(
    target: GitTooltipTarget,
    mouse_x: f32,
    mouse_y: f32,
    now: std::time::Instant,
) -> Option<(f32, f32)> {
    GIT_TOOLTIP_TIMER.with(|timer| {
        let mut timer = timer.borrow_mut();
        let reset = timer.as_ref().is_none_or(|state| state.target != target);
        if reset {
            *timer = Some(GitTooltipTimer {
                target,
                start: now,
                anchor_x: mouse_x,
                anchor_y: mouse_y,
            });
            return None;
        }

        timer.as_ref().and_then(|state| {
            (now.duration_since(state.start).as_secs_f32() > GIT_TOOLTIP_DELAY_SECS)
                .then_some((state.anchor_x, state.anchor_y))
        })
    })
}

fn git_graph_tooltip_anchor(
    target: GitTooltipTarget,
    anchor_x: f32,
    anchor_y: f32,
    now: std::time::Instant,
) -> Option<(f32, f32)> {
    GIT_TOOLTIP_TIMER.with(|timer| {
        let mut timer = timer.borrow_mut();
        if let Some(state) = timer.as_ref()
            && state.target != target
            && now.duration_since(state.start).as_secs_f32() > GIT_TOOLTIP_DELAY_SECS
        {
            *timer = Some(GitTooltipTimer {
                target,
                start: now - std::time::Duration::from_millis(500),
                anchor_x,
                anchor_y,
            });
            return Some((anchor_x, anchor_y));
        }
        let reset = timer.as_ref().is_none_or(|state| state.target != target);
        if reset {
            *timer = Some(GitTooltipTimer {
                target,
                start: now,
                anchor_x,
                anchor_y,
            });
            return None;
        }

        timer.as_ref().and_then(|state| {
            (now.duration_since(state.start).as_secs_f32() > GIT_TOOLTIP_DELAY_SECS)
                .then_some((state.anchor_x, state.anchor_y))
        })
    })
}

fn git_graph_tooltip_branch_counts(
    commits: &[crate::app::git_panel::GitGraphCommit],
    commit_idx: usize,
) -> (usize, usize) {
    let Some(commit) = commits.get(commit_idx) else {
        return (1, 1);
    };
    let total = commit.branch_total_count.unwrap_or(commits.len());
    (
        commit_idx.saturating_add(1),
        total.saturating_sub(commit_idx).max(1),
    )
}

fn git_tooltip_reset() {
    GIT_TOOLTIP_TIMER.with(|timer| *timer.borrow_mut() = None);
}

fn git_graph_selection_range(renderer: &Renderer) -> Option<(usize, usize)> {
    let start = renderer.git_graph_tooltip_selection_anchor?;
    let end = renderer.git_graph_tooltip_selection_cursor?;
    (start != end).then_some((start.min(end), start.max(end)))
}

const GIT_FOLDER_STAGE_GAP: f32 = 6.0;
const GIT_PROGRESS_CYCLES_PER_SEC: f32 = 0.85;

fn git_progress_thumb_phase(elapsed_secs: f32) -> f32 {
    let t = (elapsed_secs.max(0.0) * GIT_PROGRESS_CYCLES_PER_SEC).fract();
    let p = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
    p * p * (3.0 - 2.0 * p)
}

#[derive(Clone, Copy)]
struct GitFolderRowLayout {
    arrow_x: f32,
    check_x: f32,
    check_y: f32,
    check_size: f32,
    icon_x: f32,
    icon_y: f32,
    icon_size: f32,
}

#[derive(Clone, Copy)]
struct GitFileRowLayout {
    check_x: f32,
    check_y: f32,
    check_size: f32,
    icon_x: f32,
    icon_y: f32,
    icon_size: f32,
    text_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GitGraphTooltipTarget {
    workspace_idx: usize,
    commit_idx: usize,
}

#[derive(Clone, Copy, Debug)]
struct GitGraphRowLayout {
    gutter_w: f32,
    lane_step: f32,
    lane_start_x: f32,
    text_x: f32,
}

fn git_graph_row_layout(
    panel_x: f32,
    pad: f32,
    scale: f32,
    commit_column: usize,
    lanes: &[crate::app::git_panel::GitGraphLane],
) -> GitGraphRowLayout {
    let max_column = lanes
        .iter()
        .flat_map(|lane| [usize::from(lane.column), usize::from(lane.target_column)])
        .chain(std::iter::once(commit_column))
        .max()
        .unwrap_or(0);
    let lane_step = 18.0 * scale;
    let gutter_w = (max_column as f32 * lane_step + 28.0 * scale).max(42.0 * scale);
    let lane_start_x = panel_x + pad + 12.0 * scale;
    let text_x = panel_x + pad + gutter_w + 8.0 * scale;
    GitGraphRowLayout {
        gutter_w,
        lane_step,
        lane_start_x,
        text_x,
    }
}

fn branch_chip_y_from_text_center(text_center_y: f32, chip_h: f32) -> f32 {
    (text_center_y - chip_h * 0.5).round()
}

fn branch_chip_width(text_w: f32, pad_x: f32, max_w: f32) -> f32 {
    (text_w + pad_x * 2.0).min(max_w)
}

fn git_graph_lane_color(color_idx: usize, alpha: f32, main: [f32; 4]) -> [f32; 4] {
    let mut color = match color_idx % 7 {
        0 => main,
        1 => [0.48, 0.74, 1.0, 1.0],
        2 => [0.52, 0.82, 0.58, 1.0],
        3 => [0.97, 0.76, 0.38, 1.0],
        4 => [0.95, 0.42, 0.46, 1.0],
        5 => [0.56, 0.86, 0.88, 1.0],
        _ => [0.82, 0.68, 1.0, 1.0],
    };
    color[3] = alpha;
    color
}

fn git_folder_row_layout(indent_x: f32, row_y: f32, row_h: f32, scale: f32) -> GitFolderRowLayout {
    let arrow_x = indent_x - 2.0 * scale;
    let check_size = 12.0 * scale;
    let icon_size = 20.0 * scale;
    let gap = GIT_FOLDER_STAGE_GAP * scale;
    let check_x = arrow_x + crate::render_view::tree_ui::TREE_DISCLOSURE_SLOT * scale + gap;
    let icon_x = check_x + check_size + gap;

    GitFolderRowLayout {
        arrow_x,
        check_x,
        check_y: (row_y + (row_h - check_size) / 2.0).round(),
        check_size,
        icon_x,
        icon_y: crate::render_view::tree_ui::tree_dir_icon_y(row_y, row_h, scale),
        icon_size,
    }
}

fn git_file_row_layout(indent_x: f32, row_y: f32, row_h: f32, scale: f32) -> GitFileRowLayout {
    let check_size = 12.0 * scale;
    let icon_size = 20.0 * scale;
    let gap = GIT_FOLDER_STAGE_GAP * scale;
    let check_x = indent_x + 2.0 * scale;
    let icon_x = check_x + check_size + gap;
    let icon_y = crate::render_view::tree_ui::tree_dir_icon_y(row_y, row_h, scale);

    GitFileRowLayout {
        check_x,
        check_y: (row_y + (row_h - check_size) / 2.0).round(),
        check_size,
        icon_x,
        icon_y,
        icon_size,
        text_x: icon_x + icon_size + 4.0 * scale,
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_tree_label_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        color: [f32; 4],
        scale: f32,
        scratch: &mut String,
    ) -> f32 {
        if max_w <= 0.0 {
            return 0.0;
        }
        let full_w = self.measure_ui_width(text, scale);
        if full_w <= max_w {
            self.draw_string_scaled(text, x, y, color, scale);
            return full_w;
        }

        let ellipsis = "…";
        let ellipsis_w = self.measure_ui_width(ellipsis, scale);
        if ellipsis_w > max_w {
            return 0.0;
        }
        let prefix_len = clipped_label_prefix_len(text, max_w, ellipsis_w, |ch| {
            self.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        });
        scratch.clear();
        scratch.push_str(&text[..prefix_len]);
        scratch.push_str(ellipsis);
        self.draw_string_scaled(scratch, x, y, color, scale);
        self.measure_ui_width(scratch, scale).min(max_w)
    }

    fn draw_git_graph_row_text(&mut self, text: &str, x: f32, y: f32, color: [f32; 4], scale: f32) {
        let mut draw_x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let glyph_x = draw_x.round();
                let (q_x, q_y, q_w, q_h) = crate::renderer::glyph_quad_rect(glyph_x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
                draw_x += g.advance * scale;
            }
        }
    }

    fn ui_text_visual_mid_y(&mut self, text: &str, scale: f32) -> f32 {
        let mut top = 0.0f32;
        let mut bottom = 0.0f32;
        let mut seen = false;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let glyph_top = -g.offset_y * scale;
                let glyph_bottom = (g.height - g.offset_y) * scale;
                if seen {
                    top = top.min(glyph_top);
                    bottom = bottom.max(glyph_bottom);
                } else {
                    top = glyph_top;
                    bottom = glyph_bottom;
                    seen = true;
                }
            }
        }
        if seen { (top + bottom) * 0.5 } else { 0.0 }
    }

    fn ui_text_center_y(&mut self, text: &str, baseline_y: f32, scale: f32) -> f32 {
        baseline_y.round() + self.ui_text_visual_mid_y(text, scale)
    }

    fn ui_text_baseline_for_center_y(&mut self, text: &str, center_y: f32, scale: f32) -> f32 {
        (center_y - self.ui_text_visual_mid_y(text, scale)).round()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_branch_chip(
        &mut self,
        text: &str,
        chip_x: f32,
        text_center_y: f32,
        chip_w: f32,
        chip_h: f32,
        radius: f32,
        bg_color: [f32; 4],
        text_color: [f32; 4],
        scale: f32,
        pad_x: f32,
        register_tooltip_row: bool,
        scratch: &mut String,
    ) {
        let text_y = self.ui_text_baseline_for_center_y(text, text_center_y, scale);
        let actual_center_y = self.ui_text_center_y(text, text_y, scale);
        let chip_y = branch_chip_y_from_text_center(actual_center_y, chip_h);
        self.push_rounded_rect(chip_x, chip_y, chip_w, chip_h, radius, bg_color);

        let max_text_w = (chip_w - pad_x * 2.0).max(1.0);
        let full_w = self.measure_ui_width(text, scale);
        let (draw_text, draw_w) = if full_w <= max_text_w {
            (text, full_w)
        } else {
            let ellipsis = "…";
            let ellipsis_w = self.measure_ui_width(ellipsis, scale);
            if ellipsis_w > max_text_w {
                ("", 0.0)
            } else {
                let prefix_len = clipped_label_prefix_len(text, max_text_w, ellipsis_w, |ch| {
                    self.get_ui_glyph(ch)
                        .map(|g| g.advance * scale)
                        .unwrap_or(0.0)
                });
                scratch.clear();
                scratch.push_str(&text[..prefix_len]);
                scratch.push_str(ellipsis);
                let draw_w = self.measure_ui_width(scratch, scale).min(max_text_w);
                (scratch.as_str(), draw_w)
            }
        };
        if draw_text.is_empty() {
            return;
        }

        let text_x = (chip_x + (chip_w - draw_w) * 0.5).round();
        if register_tooltip_row {
            let row_start = self
                .push_git_graph_tooltip_text_row(draw_text, text_x, chip_y, chip_h, scale, false);
            self.draw_git_graph_selectable_text(
                draw_text, text_x, text_y, text_color, scale, row_start, chip_y, chip_h, false,
            );
        } else {
            self.draw_git_graph_row_text(draw_text, text_x, text_y, text_color, scale);
        }
    }

    fn draw_git_graph_label_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        color: [f32; 4],
        scale: f32,
        scratch: &mut String,
    ) -> f32 {
        if max_w <= 0.0 {
            return 0.0;
        }
        let full_w = self.measure_ui_width(text, scale);
        if full_w <= max_w {
            self.draw_git_graph_row_text(text, x, y, color, scale);
            return full_w;
        }

        let ellipsis = "…";
        let ellipsis_w = self.measure_ui_width(ellipsis, scale);
        if ellipsis_w > max_w {
            return 0.0;
        }
        let prefix_len = clipped_label_prefix_len(text, max_w, ellipsis_w, |ch| {
            self.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        });
        scratch.clear();
        scratch.push_str(&text[..prefix_len]);
        scratch.push_str(ellipsis);
        self.draw_git_graph_row_text(scratch, x, y, color, scale);
        self.measure_ui_width(scratch, scale).min(max_w)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_side_panels(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        real_height: f32,
        panel_left_w: f32,
        is_ui_disabled: bool,
        blink_alpha: f32,
    ) {
        self.git_file_tooltip = None;

        let sb_w = 48.0 * s;
        let blocking_bottom_y =
            if ide_panel.any_bottom_open() && ide_panel.bottom_panel_blocks_editor_hover() {
                Some(ide_bottom_panel_y(
                    real_height,
                    ide_panel.bottom_height * s,
                    s,
                ))
            } else {
                None
            };
        let mouse_in_blocking_bottom = blocking_bottom_y
            .map(|panel_y| my >= panel_y && my <= panel_y + ide_panel.bottom_height * s)
            .unwrap_or(false);
        let hit_mx = if mouse_in_blocking_bottom { -1.0 } else { mx };
        let hit_my = if mouse_in_blocking_bottom { -1.0 } else { my };

        // Сайдбар рисуется на полную высоту окна (real_height)self.push_rect(0.0, 0.0, sb_w, real_height, sidebar_bg);
        self.push_rect(sb_w - 1.0, 0.0, 1.0, real_height, [1.0, 1.0, 1.0, 0.12]);

        let btn_size = sb_w;
        let btn_gap = 0.0;
        let btn_x = 0.0;
        let top_start_y = 0.0;

        let mut top_idx = 0usize;
        let mut bottom_idx = 0usize;

        let lsp_has_issues = lsp.map_or(false, |l| {
            l.diagnostics.values().any(|diags| {
                diags.iter().any(|d| {
                    d.severity == crate::lsp::DiagSeverity::Error
                        || d.severity == crate::lsp::DiagSeverity::Warning
                })
            })
        });

        for slot in &ide_panel.slots {
            let is_dragging_this = ide_panel
                .drag
                .as_ref()
                .map(|d| d.panel_id == slot.id && d.threshold_passed)
                .unwrap_or(false);
            if is_dragging_this {
                if slot.group == crate::app::PanelGroup::Top {
                    top_idx += 1;
                } else {
                    bottom_idx += 1;
                }
                continue;
            }

            let btn_y = if slot.group == crate::app::PanelGroup::Top {
                let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                top_idx += 1;
                y
            } else {
                // Кнопки нижней группы фиксированы у дна окна, независимо от панели
                let y = real_height - btn_size - bottom_idx as f32 * btn_size;
                bottom_idx += 1;
                y
            };

            let custom_color = if slot.id == crate::app::PanelId::Problems {
                if lsp_has_issues {
                    Some([1.0, 0.8, 0.1, 1.0])
                } else {
                    Some([0.69, 0.745, 0.773, 1.0])
                }
            } else {
                None
            };

            let btn = IconButton {
                x: btn_x,
                y: btn_y,
                size: btn_size,
                icon: Some(slot.id.icon()),
                is_active: slot.open,
                icon_size: Some(36.0 * s),
                active_square_width: Some(sb_w),
                custom_color,
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::SidebarSlot(slot.id),
                &btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }

        // Призрак перетаскиваемой кнопки + разделитель
        if let Some(drag) = &ide_panel.drag {
            if drag.threshold_passed {
                if let Some(slot) = ide_panel.slots.iter().find(|sl| sl.id == drag.panel_id) {
                    let ghost_y =
                        (drag.current_y - btn_size / 2.0).clamp(0.0, real_height - btn_size);
                    let ghost_color = if slot.id == crate::app::PanelId::Problems {
                        if lsp_has_issues {
                            Some([1.0, 0.8, 0.1, 1.0])
                        } else {
                            Some([0.69, 0.745, 0.773, 1.0])
                        }
                    } else {
                        None
                    };
                    let ghost = IconButton {
                        x: btn_x,
                        y: ghost_y,
                        size: btn_size,
                        icon: Some(slot.id.icon()),
                        is_active: false,
                        icon_size: Some(36.0 * s),
                        active_square_width: None,
                        custom_color: ghost_color,
                    };
                    ghost.render(self, -1.0, -1.0, s, false);
                }
                // Горизонтальный разделитель посередине сайдбара
                let sep_y = (real_height / 2.0).round();
                self.push_rect(
                    2.0 * s,
                    sep_y - 1.0,
                    sb_w - 4.0 * s,
                    2.0,
                    [0.60, 0.35, 0.85, 0.9],
                );
            }
        }

        // Левая панель (для групп Top)
        if panel_left_w > 0.0 {
            let panel_x = sb_w;
            let panel_bg = [
                0.129, // #21
                0.133, // #22
                0.173, // #2c
                1.0,
            ];
            self.push_rect(panel_x, 0.0, panel_left_w, real_height, panel_bg);
            self.push_rect(
                panel_x + panel_left_w - 1.0,
                0.0,
                1.0,
                real_height,
                [1.0, 1.0, 1.0, 0.12],
            );
            // Тонкая линия-разделитель между левой панелью и зоной номеров строк (аналог Indent Guide)
            let sep_x = (panel_x + panel_left_w).round();
            self.push_rect(
                sep_x,
                0.0,
                1.0,
                real_height,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
            );

            let title_h = 32.0 * s;
            let title_bg = [
                (self.theme.bg[0] + 0.07).min(1.0),
                (self.theme.bg[1] + 0.07).min(1.0),
                (self.theme.bg[2] + 0.08).min(1.0),
                1.0,
            ];
            self.push_rect(panel_x, 0.0, panel_left_w, title_h, title_bg);

            let open_top_count = ide_panel
                .slots
                .iter()
                .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                .count();

            if open_top_count == 1 {
                let slot = ide_panel
                    .slots
                    .iter()
                    .find(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .unwrap();
                let label = slot.id.label();
                self.draw_string_scaled(
                    label,
                    panel_x + 12.0 * s,
                    title_h / 2.0 + 6.0 * s,
                    self.theme.fg,
                    0.9,
                );
            } else {
                let mut tx = panel_x + 6.0 * s;
                for (i, slot) in ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .enumerate()
                {
                    let label = slot.id.label();
                    let tw = self.measure_ui_width(label, 0.85) + 20.0 * s;
                    if i == 0 {
                        let act_bg = [
                            (self.theme.bg[0] + 0.12).min(1.0),
                            (self.theme.bg[1] + 0.12).min(1.0),
                            (self.theme.bg[2] + 0.13).min(1.0),
                            1.0,
                        ];
                        self.push_rect(tx, 0.0, tw, title_h, act_bg);
                        self.push_rect(tx, title_h - 2.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
                    }
                    self.draw_string_scaled(
                        label,
                        tx + 10.0 * s,
                        title_h / 2.0 + 6.0 * s,
                        self.theme.fg,
                        0.85,
                    );
                    tx += tw;
                }
            }

            // (Ручка ресайза была здесь, перенесена в конец блока левой панели)

            // --- LSP серверы ---
            if ide_panel.is_open(crate::app::PanelId::LspServers) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::LspServers
                        && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    self.draw_lsp_servers_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        real_height - title_h,
                        s,
                        ide_panel,
                        lsp_has_diagnostics,
                        ui_registry,
                    );
                }
            }

            // --- Git ---
            if ide_panel.is_open(crate::app::PanelId::Git) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::Git && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    let panel_bottom_h = if ide_panel.any_bottom_open() {
                        ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let content_bottom = ide_bottom_panel_y(real_height, panel_bottom_h, s);
                    self.draw_git_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        (content_bottom - title_h).max(0.0),
                        s,
                        ide_panel,
                        ui_registry,
                        hit_mx,
                        hit_my,
                        blink_alpha,
                    );
                }
            }

            // --- Дерево файлов проводника ---
            if ide_panel.is_open(crate::app::PanelId::Explorer) {
                let file_tree_overlay_open =
                    crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel);
                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(
                        panel_x as i32,
                        0,
                        panel_left_w as i32,
                        (real_height - title_h) as i32,
                    );
                }

                let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
                let indent_w = crate::render_view::tree_ui::TREE_INDENT_W * s;
                let scroll = ide_panel.explorer_scroll.current.round();
                let content_h = real_height - title_h;
                let total_nodes = ide_panel.file_tree_nodes.len();

                let tree_text_scale = crate::render_view::tree_ui::TREE_TEXT_SCALE;
                if total_nodes == 0 {
                    let hint = "Нет папок в проекте";
                    let tw = self.measure_ui_width(hint, tree_text_scale);
                    let tx = panel_x + (panel_left_w - tw) / 2.0;
                    self.draw_string_scaled(
                        hint,
                        tx,
                        title_h + 30.0 * s,
                        [0.45, 0.45, 0.45, 1.0],
                        tree_text_scale,
                    );
                } else {
                    let first_vis = (scroll / row_h).floor() as usize;
                    let last_vis =
                        (((scroll + content_h) / row_h).ceil() as usize + 1).min(total_nodes);
                    let mut label_scratch = String::new();

                    for i in first_vis..last_vis {
                        let node = &ide_panel.file_tree_nodes[i];
                        let row_y = title_h + i as f32 * row_h - scroll;

                        if !file_tree_overlay_open {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::FileTreeNode(i),
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                hit_mx,
                                hit_my,
                            );
                        }

                        let is_hovered = !file_tree_overlay_open
                            && (ide_panel.file_tree_hovered_idx == Some(i)
                                || ui_registry.hovered()
                                    == Some(crate::ui_system::UiId::FileTreeNode(i)));
                        let is_selected = ide_panel.file_tree_selection.contains(&node.path);

                        if is_selected {
                            self.push_rect(
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                [0.60, 0.35, 0.85, 0.24],
                            );
                        }

                        if is_hovered && !is_ui_disabled {
                            self.push_rect(
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                [1.0, 1.0, 1.0, 0.06],
                            );
                        }

                        let indent_x = panel_x + 8.0 * s + node.depth as f32 * indent_w;
                        let mut has_error = false;
                        let mut has_warn = false;
                        if !node.is_ignored {
                            if let Some(l) = lsp {
                                for (p, diags) in &l.diagnostics {
                                    if !diags.is_empty() && p.starts_with(&node.path) {
                                        for d in diags {
                                            if d.severity == crate::lsp::DiagSeverity::Error {
                                                has_error = true;
                                                break;
                                            } else if d.severity
                                                == crate::lsp::DiagSeverity::Warning
                                            {
                                                has_warn = true;
                                            }
                                        }
                                        if has_error {
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        let color: [f32; 4] = if node.is_ignored {
                            [0.973, 0.584, 0.502, 0.8]
                        } else if node.is_dir {
                            [0.78, 0.68, 1.0, 1.0]
                        } else {
                            [0.651, 0.686, 0.918, 1.0]
                        };

                        let icon_size = 20.0 * s;
                        let icon_y = row_y + (row_h - icon_size) / 2.0;

                        if node.is_dir {
                            let arrow_x = indent_x - 2.0 * s;
                            if !file_tree_overlay_open {
                                ui_registry.register_rect(
                                    crate::ui_system::UiId::FileTreeArrow(i),
                                    arrow_x - 4.0 * s,
                                    row_y,
                                    18.0 * s,
                                    row_h,
                                    hit_mx,
                                    hit_my,
                                );
                            }
                            let arrow_color = if node.is_ignored {
                                [0.973, 0.584, 0.502, 0.6]
                            } else {
                                [0.78, 0.68, 1.0, 0.7]
                            };
                            let label = self.draw_tree_dir_entry(
                                &node.name,
                                node.icon_key,
                                indent_x,
                                row_y,
                                row_h,
                                panel_x + panel_left_w - 10.0 * s,
                                node.is_expanded,
                                color,
                                arrow_color,
                                s,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(label.x, label.y + 2.0 * s, label.w, sq_color);
                            }
                        } else {
                            let file_icon_x = crate::render_view::tree_ui::tree_icon_x(indent_x, s);
                            self.draw_file_icon(
                                node.icon_key,
                                false,
                                file_icon_x,
                                icon_y,
                                icon_size,
                            );
                            let text_x = file_icon_x + icon_size + 4.0 * s;
                            let label = self.draw_tree_leaf_label(
                                &node.name,
                                text_x,
                                row_y,
                                row_h,
                                panel_x + panel_left_w - 10.0 * s,
                                color,
                                s,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(label.x, label.y + 2.0 * s, label.w, sq_color);
                            }
                        }
                    }

                    if let Some(drag) = &ide_panel.file_tree_drag {
                        if drag.threshold_passed {
                            if let Some(target_idx) = drag.target_idx {
                                if target_idx < total_nodes {
                                    let row_y = title_h + target_idx as f32 * row_h - scroll;
                                    self.push_rect(
                                        panel_x,
                                        row_y,
                                        panel_left_w,
                                        row_h,
                                        [0.52, 0.78, 0.58, 0.22],
                                    );
                                    self.push_rect(
                                        panel_x,
                                        row_y + row_h - 2.0,
                                        panel_left_w,
                                        2.0,
                                        [0.52, 0.78, 0.58, 0.85],
                                    );
                                }
                            }
                            let label = if drag.paths.len() == 1 {
                                drag.paths[0]
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("1 элемент")
                                    .to_string()
                            } else {
                                format!("{} элементов", drag.paths.len())
                            };
                            let ghost_w = self.measure_ui_width(&label, tree_text_scale) + 18.0 * s;
                            let ghost_x = drag.current_x + 12.0 * s;
                            let ghost_y = drag.current_y + 10.0 * s;
                            self.push_rounded_rect(
                                ghost_x,
                                ghost_y,
                                ghost_w,
                                26.0 * s,
                                5.0 * s,
                                [0.12, 0.13, 0.18, 0.92],
                            );
                            self.draw_string_scaled(
                                &label,
                                ghost_x + 9.0 * s,
                                ghost_y + 18.0 * s,
                                self.theme.fg,
                                tree_text_scale,
                            );
                        }
                    }

                    // Тонкий скроллбар
                    let total_h = total_nodes as f32 * row_h;
                    if total_h > content_h {
                        let max_s = (total_h - content_h).max(1.0);
                        let ratio = (scroll / max_s).clamp(0.0, 1.0);
                        let thumb_h = (content_h / total_h * (content_h - 8.0 * s)).max(20.0 * s);
                        let thumb_y = title_h + 4.0 * s + ratio * (content_h - 8.0 * s - thumb_h);
                        self.push_rounded_rect(
                            panel_x + panel_left_w - 5.0 * s,
                            thumb_y,
                            3.0 * s,
                            thumb_h,
                            1.5 * s,
                            [1.0, 1.0, 1.0, 0.22],
                        );
                    }
                }

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            // Подсветка ручки ресайза (wants_pointer=false — курсор управляется в events.rs через EwResize)
            // Не подсвечиваем, когда терминал в фокусе
            let resize_x = panel_x + panel_left_w;
            let resize_max_y = blocking_bottom_y.unwrap_or(real_height);
            if !is_ui_disabled
                && mx >= resize_x - 8.0 * s
                && mx <= resize_x + 8.0 * s
                && my >= 0.0
                && my <= resize_max_y
            {
                self.push_rect(
                    resize_x - 2.0,
                    0.0,
                    2.0,
                    resize_max_y,
                    [0.60, 0.35, 0.85, 0.4],
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_file_tooltip(&mut self, text: &str, mouse_x: f32, mouse_y: f32, s: f32) {
        let tooltip_scale = 0.88;
        let pad_x = 12.0 * s;
        let tooltip_h = 30.0 * s;
        let tooltip_w = self.measure_ui_width(text, tooltip_scale) + pad_x * 2.0;
        let tooltip_x = mouse_x + 14.0 * s;
        let tooltip_y = mouse_y + 18.0 * s;

        self.push_rounded_rect(
            tooltip_x,
            tooltip_y,
            tooltip_w,
            tooltip_h,
            6.0 * s,
            self.theme.sel,
        );
        self.push_rounded_rect(
            tooltip_x + 1.0,
            tooltip_y + 1.0,
            tooltip_w - 2.0,
            tooltip_h - 2.0,
            5.0 * s,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                0.98,
            ],
        );
        self.draw_string_scaled(
            text,
            tooltip_x + pad_x,
            tooltip_y + tooltip_h / 2.0 + 5.0 * s,
            self.theme.fg,
            tooltip_scale,
        );
    }

    pub(crate) fn draw_git_file_tooltip_overlay(
        &mut self,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        if ide_panel.is_resizing_left
            || ide_panel.is_resizing_bottom
            || ide_panel.git.graph_resizing
        {
            self.reset_git_file_tooltip_overlay();
            return;
        }
        self.git_tooltip_waiting = false;
        let file_tooltip = self.git_file_tooltip.take().map(
            |(workspace_idx, file_idx, tooltip, mouse_x, mouse_y)| {
                (
                    false,
                    GitTooltipTarget {
                        kind: GIT_TOOLTIP_FILE,
                        workspace_idx,
                        item_idx: file_idx,
                    },
                    tooltip,
                    mouse_x,
                    mouse_y,
                )
            },
        );
        let action_tooltip = self.git_action_tooltip.take().map(
            |(kind, workspace_idx, tooltip, mouse_x, mouse_y)| {
                (
                    true,
                    GitTooltipTarget {
                        kind,
                        workspace_idx,
                        item_idx: 0,
                    },
                    tooltip,
                    mouse_x,
                    mouse_y,
                )
            },
        );
        let graph_tooltip = self.git_graph_tooltip.take();
        let Some((is_action_tooltip, target, tooltip, mouse_x, mouse_y)) =
            action_tooltip.or(file_tooltip)
        else {
            if let Some((workspace_idx, commit_idx, mouse_x, mouse_y)) = graph_tooltip
                && let Some(commit) = ide_panel.git.graph_snapshot.get(commit_idx)
            {
                let target = GitTooltipTarget {
                    kind: GIT_TOOLTIP_GRAPH_COMMIT,
                    workspace_idx,
                    item_idx: commit_idx,
                };
                if self.hide_popups_until_mouse_move {
                    return;
                }
                if let Some((anchor_x, anchor_y)) =
                    git_graph_tooltip_anchor(target, mouse_x, mouse_y, std::time::Instant::now())
                {
                    let mut scratch = std::mem::take(&mut self.scratch_buffer);
                    self.draw_git_graph_tooltip(
                        commit,
                        GitGraphTooltipTarget {
                            workspace_idx,
                            commit_idx,
                        },
                        &ide_panel.git.graph_snapshot,
                        anchor_x,
                        anchor_y,
                        s,
                        ui_registry,
                        mx,
                        my,
                        ide_panel.git.graph_copied_commit,
                        &mut scratch,
                    );
                    self.scratch_buffer = scratch;
                } else {
                    self.git_tooltip_waiting = true;
                }
                return;
            }

            git_tooltip_reset();
            self.git_graph_tooltip_hover = None;
            self.git_graph_tooltip_text.clear();
            self.git_graph_tooltip_text_rows.clear();
            self.git_graph_tooltip_stable_w = 0.0;
            self.clear_git_graph_tooltip_selection();
            return;
        };

        if self.hide_popups_until_mouse_move {
            return;
        }

        if let Some((anchor_x, anchor_y)) =
            git_tooltip_anchor(target, mouse_x, mouse_y, std::time::Instant::now())
        {
            self.draw_git_file_tooltip(&tooltip, anchor_x, anchor_y, s);
        } else {
            self.git_tooltip_waiting = is_action_tooltip;
        }
    }

    pub(crate) fn reset_git_file_tooltip_overlay(&mut self) {
        self.git_file_tooltip = None;
        self.git_action_tooltip = None;
        self.git_graph_tooltip = None;
        self.git_graph_tooltip_hover = None;
        self.git_graph_tooltip_text.clear();
        self.git_graph_tooltip_text_rows.clear();
        self.git_graph_tooltip_stable_w = 0.0;
        self.clear_git_graph_tooltip_selection();
        self.git_tooltip_waiting = false;
        git_tooltip_reset();
        self.reset_delayed_tooltip_anchor();
    }

    fn push_git_graph_vertical_segment(
        &mut self,
        x: f32,
        top: f32,
        bottom: f32,
        s: f32,
        color: [f32; 4],
    ) {
        if bottom - top > 0.5 * s {
            self.push_git_graph_sdf_segment(x, top, x, bottom, 2.0 * s, color);
        }
    }

    fn push_git_graph_soft_vertical_segment(
        &mut self,
        x: f32,
        top: f32,
        bottom: f32,
        width: f32,
        color: [f32; 4],
    ) {
        self.push_git_graph_sdf_segment(x, top, x, bottom, width, color);
    }

    fn push_git_graph_horizontal_segment(
        &mut self,
        x0: f32,
        x1: f32,
        y: f32,
        width: f32,
        color: [f32; 4],
    ) {
        self.push_git_graph_sdf_segment(x0, y, x1, y, width, color);
    }

    fn push_git_graph_parent_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        if w <= 0.5 * s {
            return;
        }
        let line_w = 2.0 * s;
        let start_y = row_y + row_h / 2.0;
        let end_y = row_y + row_h;
        let r = (8.0 * s).min(w * 0.5).max(2.0 * s);

        if to_x > from_x {
            let turn_x = (to_x - r).max(from_x);
            if turn_x - from_x > 0.5 * s {
                self.push_git_graph_horizontal_segment(from_x, turn_x, start_y, line_w, color);
            }
            let turn_y = (start_y + r).min(end_y);
            self.push_git_graph_quadratic_curve(
                turn_x, start_y, to_x, start_y, to_x, turn_y, line_w, color,
            );
            self.push_git_graph_soft_vertical_segment(to_x, turn_y, end_y, line_w, color);
        } else {
            let turn_y = (end_y - r).max(start_y);
            self.push_git_graph_soft_vertical_segment(from_x, start_y, turn_y, line_w, color);
            let turn_x = (from_x - r).max(to_x);
            self.push_git_graph_quadratic_curve(
                from_x, turn_y, from_x, end_y, turn_x, end_y, line_w, color,
            );
            if turn_x - to_x > 0.5 * s {
                self.push_git_graph_horizontal_segment(turn_x, to_x, end_y, line_w, color);
            }
        }
    }

    fn push_git_graph_shift_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        if w <= 0.5 * s {
            self.push_git_graph_vertical_segment(from_x, row_y, row_y + row_h, s, color);
            return;
        }
        let line_w = 2.0 * s;
        let mid_y = row_y + row_h / 2.0;
        let radius = (8.0 * s).min(w * 0.5).max(2.0 * s);
        let turn_in_y = (mid_y - radius).max(row_y);
        let turn_out_y = (mid_y + radius).min(row_y + row_h);
        let dir = if to_x > from_x { 1.0 } else { -1.0 };
        let from_mid_x = from_x + dir * radius;
        let to_mid_x = to_x - dir * radius;
        self.push_git_graph_soft_vertical_segment(from_x, row_y, turn_in_y, line_w, color);
        self.push_git_graph_quadratic_curve(
            from_x, turn_in_y, from_x, mid_y, from_mid_x, mid_y, line_w, color,
        );
        if (to_mid_x - from_mid_x).abs() > 0.5 * s {
            self.push_git_graph_horizontal_segment(from_mid_x, to_mid_x, mid_y, line_w, color);
        }
        self.push_git_graph_quadratic_curve(
            to_mid_x, mid_y, to_x, mid_y, to_x, turn_out_y, line_w, color,
        );
        self.push_git_graph_soft_vertical_segment(to_x, turn_out_y, row_y + row_h, line_w, color);
    }

    fn push_git_graph_shift_to_commit_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        let line_w = 2.0 * s;
        let mid_y = row_y + row_h / 2.0;
        if w <= 0.5 * s {
            self.push_git_graph_vertical_segment(from_x, row_y, mid_y, s, color);
            return;
        }
        let radius = (8.0 * s).min(w * 0.5).max(2.0 * s);
        let turn_in_y = (mid_y - radius).max(row_y);
        let dir = if to_x > from_x { 1.0 } else { -1.0 };
        let mid_x = to_x - dir * radius;
        self.push_git_graph_soft_vertical_segment(from_x, row_y, turn_in_y, line_w, color);
        self.push_git_graph_quadratic_curve(
            from_x,
            turn_in_y,
            from_x,
            mid_y,
            from_x + dir * radius,
            mid_y,
            line_w,
            color,
        );
        if (mid_x - (from_x + dir * radius)).abs() > 0.5 * s {
            self.push_git_graph_horizontal_segment(
                from_x + dir * radius,
                mid_x,
                mid_y,
                line_w,
                color,
            );
        }
        self.push_git_graph_horizontal_segment(mid_x, to_x, mid_y, line_w, color);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_git_graph_quadratic_curve(
        &mut self,
        x0: f32,
        y0: f32,
        cx: f32,
        cy: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: [f32; 4],
    ) {
        let approx_len = ((x1 - x0).abs() + (y1 - y0).abs()).max(width);
        let steps = (approx_len / (width * 0.75)).ceil().clamp(18.0, 64.0) as usize;
        let radius = width * 0.5;
        let extent = radius + 1.25;
        let sdf_params = [approx_len + width * 4.0, radius, 0.0];
        let mut prev_x = x0;
        let mut prev_y = y0;
        let mut prev_left = [x0, y0];
        let mut prev_right = [x0, y0];
        let mut prev_u = 0.0f32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let inv = 1.0 - t;
            let x = inv * inv * x0 + 2.0 * inv * t * cx + t * t * x1;
            let y = inv * inv * y0 + 2.0 * inv * t * cy + t * t * y1;
            let tx = 2.0 * inv * (cx - x0) + 2.0 * t * (x1 - cx);
            let ty = 2.0 * inv * (cy - y0) + 2.0 * t * (y1 - cy);
            let tangent_len = (tx * tx + ty * ty).sqrt();
            if tangent_len <= 0.01 {
                continue;
            }
            let nx = -ty / tangent_len * extent;
            let ny = tx / tangent_len * extent;
            let left = [x + nx, y + ny];
            let right = [x - nx, y - ny];
            if step > 0 {
                let dx = x - prev_x;
                let dy = y - prev_y;
                let u = prev_u + (dx * dx + dy * dy).sqrt();
                let v0 = crate::renderer::Vertex {
                    pos: prev_left,
                    uv: [prev_u, extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v1 = crate::renderer::Vertex {
                    pos: left,
                    uv: [u, extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v2 = crate::renderer::Vertex {
                    pos: right,
                    uv: [u, -extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v3 = crate::renderer::Vertex {
                    pos: prev_right,
                    uv: [prev_u, -extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                self.vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
                prev_u = u;
            }
            prev_x = x;
            prev_y = y;
            prev_left = left;
            prev_right = right;
        }
    }

    fn push_git_graph_sdf_segment(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: [f32; 4],
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.01 {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let radius = width * 0.5;
        let extent = radius + 1.25;
        let nx = -uy * extent;
        let ny = ux * extent;
        let segment_len = len;
        let sdf_params = [segment_len, radius, 0.0];
        let v0 = crate::renderer::Vertex {
            pos: [x0 + nx, y0 + ny],
            uv: [0.0, extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v1 = crate::renderer::Vertex {
            pos: [x1 + nx, y1 + ny],
            uv: [segment_len, extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v2 = crate::renderer::Vertex {
            pos: [x1 - nx, y1 - ny],
            uv: [segment_len, -extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v3 = crate::renderer::Vertex {
            pos: [x0 - nx, y0 - ny],
            uv: [0.0, -extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        self.vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
    }

    fn clear_git_graph_tooltip_selection(&mut self) {
        self.git_graph_tooltip_selection_anchor = None;
        self.git_graph_tooltip_selection_cursor = None;
        self.git_graph_tooltip_selecting = false;
    }

    fn git_graph_tooltip_char_advance(&mut self, c: char, scale: f32, mono: bool) -> f32 {
        if mono {
            self.get_glyph(c).map(|g| g.advance * scale).unwrap_or(0.0)
        } else {
            self.get_ui_glyph(c)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        }
    }

    fn measure_git_graph_tooltip_text_width(&mut self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter(|&c| c != '\n' && c != '\r' && c != '\u{FE0F}' && c != '\u{200D}')
            .map(|c| self.git_graph_tooltip_char_advance(c, scale, false))
            .sum()
    }

    fn measure_git_graph_tooltip_mono_width(&mut self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter(|&c| c != '\n' && c != '\r' && c != '\u{FE0F}' && c != '\u{200D}')
            .map(|c| self.git_graph_tooltip_char_advance(c, scale, true))
            .sum()
    }

    fn git_graph_tooltip_wrap_end(&mut self, text: &str, max_w: f32, scale: f32) -> usize {
        let mut used = 0.0;
        let mut last_break = None;
        let mut end = 0usize;
        for (idx, ch) in text.char_indices() {
            if ch == '\n' || ch == '\r' {
                return idx;
            }
            let adv = self.git_graph_tooltip_char_advance(ch, scale, false);
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

    fn git_graph_tooltip_wrapped_line_count(
        &mut self,
        mut text: &str,
        max_w: f32,
        scale: f32,
    ) -> usize {
        let mut lines = 0usize;
        while !text.is_empty() {
            let end = self.git_graph_tooltip_wrap_end(text, max_w, scale);
            lines += 1;
            if end >= text.len() {
                break;
            }
            text = text[end..].trim_start();
        }
        lines.max(1)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_wrapped_selectable_text(
        &mut self,
        mut text: &str,
        x: f32,
        mut line_top: f32,
        line_h: f32,
        color: [f32; 4],
        scale: f32,
        max_w: f32,
    ) -> f32 {
        if text.is_empty() {
            let _ = self.push_git_graph_tooltip_text_row(text, x, line_top, line_h, scale, false);
            return line_top + line_h;
        }
        while !text.is_empty() {
            let end = self.git_graph_tooltip_wrap_end(text, max_w, scale);
            let row_text = text[..end].trim_end();
            let row_start =
                self.push_git_graph_tooltip_text_row(row_text, x, line_top, line_h, scale, false);
            self.draw_git_graph_selectable_text(
                row_text,
                x,
                line_top + line_h * 0.62,
                color,
                scale,
                row_start,
                line_top,
                line_h,
                false,
            );
            if end >= text.len() {
                break;
            }
            text = text[end..].trim_start();
            line_top += line_h;
        }
        line_top + line_h
    }

    fn push_git_graph_tooltip_text_row(
        &mut self,
        text: &str,
        x: f32,
        top: f32,
        line_h: f32,
        scale: f32,
        mono: bool,
    ) -> usize {
        if !self.git_graph_tooltip_text.is_empty() {
            self.git_graph_tooltip_text.push('\n');
        }
        let start = self.git_graph_tooltip_text.len();
        self.git_graph_tooltip_text.push_str(text);
        let end = self.git_graph_tooltip_text.len();
        self.git_graph_tooltip_text_rows
            .push(crate::renderer::GitGraphTooltipTextRow {
                x,
                top,
                line_h,
                scale,
                mono,
                start,
                end,
            });
        start
    }

    fn draw_git_graph_selectable_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
        byte_start: usize,
        line_top: f32,
        line_h: f32,
        mono: bool,
    ) {
        let selected = git_graph_selection_range(self);
        let mut draw_x = x.round();
        let y = y.round();
        for (idx, c) in text.char_indices() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            let glyph = if mono {
                self.get_glyph(c)
            } else {
                self.get_ui_glyph(c)
            };
            if let Some(g) = glyph {
                let adv = g.advance * scale;
                let glyph_x = draw_x.round();
                if let Some((sel_start, sel_end)) = selected {
                    let offset = byte_start + idx;
                    if offset >= sel_start && offset < sel_end {
                        self.push_rect(
                            glyph_x,
                            line_top.round(),
                            adv.ceil() + 1.0,
                            line_h.ceil(),
                            self.theme.sel,
                        );
                    }
                }
                let (q_x, q_y, q_w, q_h) = crate::renderer::glyph_quad_rect(glyph_x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
                draw_x += adv;
            }
        }
    }

    pub(crate) fn git_graph_tooltip_byte_at(&mut self, mx: f32, my: f32) -> usize {
        let Some(row) = self
            .git_graph_tooltip_text_rows
            .iter()
            .min_by(|a, b| {
                let da = if my < a.top {
                    a.top - my
                } else if my > a.top + a.line_h {
                    my - (a.top + a.line_h)
                } else {
                    0.0
                };
                let db = if my < b.top {
                    b.top - my
                } else if my > b.top + b.line_h {
                    my - (b.top + b.line_h)
                } else {
                    0.0
                };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
        else {
            return 0;
        };
        if mx <= row.x {
            return row.start;
        }
        let line = self
            .git_graph_tooltip_text
            .get(row.start..row.end)
            .unwrap_or("")
            .to_string();
        let mut x = row.x;
        for (idx, ch) in line.char_indices() {
            let adv = self.git_graph_tooltip_char_advance(ch, row.scale, row.mono);
            if mx <= x + adv * 0.5 {
                return row.start + idx;
            }
            x += adv;
        }
        row.end
    }

    pub(crate) fn selected_git_graph_tooltip_text(&self) -> Option<String> {
        let (start, end) = git_graph_selection_range(self)?;
        if end <= self.git_graph_tooltip_text.len()
            && self.git_graph_tooltip_text.is_char_boundary(start)
            && self.git_graph_tooltip_text.is_char_boundary(end)
        {
            Some(self.git_graph_tooltip_text[start..end].to_string())
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_tooltip(
        &mut self,
        commit: &crate::app::git_panel::GitGraphCommit,
        target: GitGraphTooltipTarget,
        commits: &[crate::app::git_panel::GitGraphCommit],
        anchor_x: f32,
        anchor_y: f32,
        s: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        copied_commit: Option<(usize, usize)>,
        scratch: &mut String,
    ) {
        let pad_x = 10.0 * s;
        let pad_y = 6.0 * s;
        let target_key = (target.workspace_idx, target.commit_idx);
        let target_changed = self.git_graph_tooltip_hover.is_none_or(|hover| {
            hover.workspace_idx != target.workspace_idx || hover.commit_idx != target.commit_idx
        });
        if target_changed {
            self.clear_git_graph_tooltip_selection();
            self.git_graph_tooltip_visible_copied = None;
        }
        if self.git_graph_tooltip_seen_copied != copied_commit {
            self.git_graph_tooltip_seen_copied = copied_commit;
            self.git_graph_tooltip_visible_copied =
                (copied_commit == Some(target_key)).then_some(target_key);
        }
        let copied = self.git_graph_tooltip_visible_copied == Some(target_key);
        self.git_graph_tooltip_text.clear();
        self.git_graph_tooltip_text_rows.clear();

        let margin = 6.0 * s;
        let tooltip_w = (440.0 * s).min((self.width - margin * 2.0).max(260.0 * s));
        let inner_w = (tooltip_w - pad_x * 2.0).max(1.0);
        let title_scale = 0.84;
        let title_line_h = 22.0 * s;
        let title_icon_size = 18.0 * s;
        let title_icon_gap = 5.0 * s;
        let title_lines = 2;
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(
            scratch,
            format_args!("{} ({})", commit.relative_time, commit.absolute_time),
        );
        let summary_lines =
            self.git_graph_tooltip_wrapped_line_count(&commit.summary, inner_w, 0.9);
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ =
                std::fmt::Write::write_fmt(scratch, format_args!("{} files", stats.files_changed));
        } else {
            scratch.push_str("stats deferred");
        }
        let files_w = self.measure_git_graph_tooltip_mono_width(scratch, 0.82);
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("+{}", stats.insertions));
        } else {
            scratch.push_str("+?");
        }
        let insertions_w = self.measure_git_graph_tooltip_text_width(scratch, 0.82);
        let branch_chip_w = commit.branch_name.as_ref().map(|branch_name| {
            branch_chip_width(self.measure_ui_width(branch_name, 0.82), 6.0 * s, f32::MAX)
        });
        let title_h = title_lines as f32 * title_line_h;
        let summary_h = summary_lines as f32 * 20.0 * s;
        let branch_section_h = if branch_chip_w.is_some() {
            10.0 * s + 18.0 * s
        } else {
            0.0
        };
        let tooltip_h = pad_y
            + title_h
            + 6.0 * s
            + summary_h
            + 6.0 * s
            + 18.0 * s
            + branch_section_h
            + 6.0 * s
            + 18.0 * s
            + pad_y;
        let mut tooltip_x = anchor_x + 6.0 * s;
        if tooltip_x + tooltip_w > self.width - margin {
            tooltip_x = anchor_x - tooltip_w - 6.0 * s;
        }
        tooltip_x = tooltip_x.clamp(margin, (self.width - tooltip_w - margin).max(margin));
        let content_x = tooltip_x + pad_x;
        let mut tooltip_y = anchor_y - tooltip_h / 2.0;
        tooltip_y = tooltip_y.clamp(margin, (self.height - tooltip_h - margin).max(margin));
        let hover_x = anchor_x.min(tooltip_x);
        let hover_y = anchor_y.min(tooltip_y);
        self.git_graph_tooltip_hover = Some(crate::renderer::GitGraphTooltipHover {
            workspace_idx: target.workspace_idx,
            commit_idx: target.commit_idx,
            anchor_x,
            anchor_y,
            x: hover_x,
            y: hover_y,
            w: (tooltip_x + tooltip_w - hover_x).max(tooltip_w),
            h: (tooltip_y + tooltip_h - hover_y).max(tooltip_h),
        });
        if mx >= tooltip_x
            && mx <= tooltip_x + tooltip_w
            && my >= tooltip_y
            && my <= tooltip_y + tooltip_h
        {
            ui_registry.reset_cursor_state();
            ui_registry.register_blocker(
                crate::ui_system::UiId::GitGraphCommit(target.workspace_idx, target.commit_idx),
                tooltip_x,
                tooltip_y,
                tooltip_w,
                tooltip_h,
                mx,
                my,
            );
        }
        self.push_rounded_rect_border(
            tooltip_x,
            tooltip_y,
            tooltip_w,
            tooltip_h,
            7.0 * s,
            (1.0 * s).round().max(1.0),
            self.theme.sel,
            [0.11, 0.12, 0.16, 0.98],
        );

        let mut line_top = tooltip_y + pad_y;
        let author_row_top = line_top;
        let date_row_top = line_top + title_line_h;
        let author_text_y = (author_row_top + title_line_h * 0.62).round();
        let date_text_y = (date_row_top + title_line_h * 0.62).round();
        let title_icon_raise = 2.0 * s;
        let author_icon_y =
            (author_row_top + (title_line_h - title_icon_size) * 0.38 - title_icon_raise).round();
        let date_icon_extra_drop = 1.0 * s;
        let date_icon_y = (date_row_top + (title_line_h - title_icon_size) * 0.38
            - title_icon_raise
            + date_icon_extra_drop)
            .round();
        let author_x = (content_x + title_icon_size + title_icon_gap).round();
        let title_count_color = self.theme.sel;
        let title_count_text_color = [1.0, 1.0, 1.0, 1.0];
        let (newest_count, oldest_count) =
            git_graph_tooltip_branch_counts(commits, target.commit_idx);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{newest_count}"));
        let newest_w = self.measure_git_graph_tooltip_mono_width(scratch, title_scale);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{oldest_count}"));
        let oldest_w = self.measure_git_graph_tooltip_mono_width(scratch, title_scale);
        let count_text_w = newest_w.max(oldest_w);
        let count_x = (content_x + inner_w - count_text_w).round();
        let count_icon_size = title_icon_size;
        let count_icon_x = (count_x - title_icon_gap - count_icon_size).round();
        self.draw_atlas_icon(
            crate::widgets::IconType::Person,
            content_x.round(),
            author_icon_y,
            title_icon_size,
            self.theme.sel,
        );
        let row_start = self.push_git_graph_tooltip_text_row(
            &commit.author_name,
            author_x,
            author_row_top,
            title_line_h,
            title_scale,
            false,
        );
        self.draw_git_graph_selectable_text(
            &commit.author_name,
            author_x,
            author_text_y,
            [1.0, 1.0, 1.0, 1.0],
            title_scale,
            row_start,
            author_row_top,
            title_line_h,
            false,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::NumberCount,
            count_icon_x,
            author_icon_y,
            count_icon_size,
            title_count_color,
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{newest_count}"));
        self.draw_string_mono_scaled(
            scratch,
            count_x,
            author_text_y,
            title_count_text_color,
            title_scale,
        );

        self.draw_atlas_icon(
            crate::widgets::IconType::Time,
            content_x.round(),
            date_icon_y,
            title_icon_size,
            self.theme.sel,
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(
            scratch,
            format_args!("{} ({})", commit.relative_time, commit.absolute_time),
        );
        let date_x = (content_x + title_icon_size + title_icon_gap).round();
        let row_start = self.push_git_graph_tooltip_text_row(
            scratch,
            date_x,
            date_row_top,
            title_line_h,
            title_scale,
            false,
        );
        self.draw_git_graph_selectable_text(
            scratch,
            date_x,
            date_text_y,
            [1.0, 1.0, 1.0, 1.0],
            title_scale,
            row_start,
            date_row_top,
            title_line_h,
            false,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::NumberCount,
            count_icon_x,
            date_icon_y,
            count_icon_size,
            title_count_color,
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{oldest_count}"));
        self.draw_string_mono_scaled(
            scratch,
            count_x,
            date_text_y,
            title_count_text_color,
            title_scale,
        );
        line_top += title_line_h * 2.0;

        line_top += 6.0 * s;
        line_top = self.draw_git_graph_wrapped_selectable_text(
            &commit.summary,
            content_x,
            line_top,
            20.0 * s,
            [0.86, 0.90, 1.0, 1.0],
            0.9,
            inner_w,
        );

        line_top += 1.0 * s;
        self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, [1.0, 1.0, 1.0, 0.12]);
        line_top += 5.0 * s;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ =
                std::fmt::Write::write_fmt(scratch, format_args!("{} files", stats.files_changed));
        } else {
            scratch.push_str("stats deferred");
        }
        let stats_start = self.push_git_graph_tooltip_text_row(
            scratch,
            content_x,
            line_top,
            18.0 * s,
            0.82,
            true,
        );
        self.draw_git_graph_selectable_text(
            scratch,
            content_x,
            line_top + 18.0 * s * 0.68,
            [0.78, 0.82, 0.92, 1.0],
            0.82,
            stats_start,
            line_top,
            18.0 * s,
            true,
        );
        let mut stat_x = content_x + files_w + 12.0 * s;
        let mut stats_end = stats_start + scratch.len();
        self.git_graph_tooltip_text.push(' ');
        stats_end += 1;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("+{}", stats.insertions));
        } else {
            scratch.push_str("+?");
        }
        self.git_graph_tooltip_text.push_str(scratch);
        if let Some(row) = self.git_graph_tooltip_text_rows.last_mut() {
            row.end = self.git_graph_tooltip_text.len();
        }
        self.draw_git_graph_selectable_text(
            scratch,
            stat_x,
            line_top + 18.0 * s * 0.68,
            [0.52, 0.82, 0.58, 1.0],
            0.82,
            stats_end,
            line_top,
            18.0 * s,
            false,
        );
        stat_x += insertions_w + 10.0 * s;
        let mut stats_end = stats_end + scratch.len();
        self.git_graph_tooltip_text.push(' ');
        stats_end += 1;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("-{}", stats.deletions));
        } else {
            scratch.push_str("-?");
        }
        self.git_graph_tooltip_text.push_str(scratch);
        if let Some(row) = self.git_graph_tooltip_text_rows.last_mut() {
            row.end = self.git_graph_tooltip_text.len();
        }
        self.draw_git_graph_selectable_text(
            scratch,
            stat_x,
            line_top + 18.0 * s * 0.68,
            [0.95, 0.42, 0.46, 1.0],
            0.82,
            stats_end,
            line_top,
            18.0 * s,
            false,
        );
        line_top += 18.0 * s;

        if let Some(branch_name) = &commit.branch_name {
            line_top += 1.0 * s;
            self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, [1.0, 1.0, 1.0, 0.12]);
            line_top += 5.0 * s;
            let pill_h = 18.0 * s;
            let scale = 0.82;
            let pill_w = branch_chip_w.unwrap_or_else(|| {
                branch_chip_width(self.measure_ui_width(branch_name, scale), 6.0 * s, f32::MAX)
            });
            let desired_center_y = line_top + pill_h * 0.5;
            self.draw_git_graph_branch_chip(
                branch_name,
                content_x,
                desired_center_y,
                pill_w,
                pill_h,
                4.0 * s,
                [0.28, 0.24, 0.40, 1.0],
                [0.86, 0.90, 1.0, 1.0],
                scale,
                6.0 * s,
                true,
                scratch,
            );
            line_top += pill_h + 4.0 * s;
        }

        line_top += 1.0 * s;
        self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, [1.0, 1.0, 1.0, 0.12]);
        line_top += 5.0 * s;
        let hash_w = self.measure_git_graph_tooltip_mono_width(&commit.short_oid, 0.86);
        let hash_x = content_x;
        let row_start = self.push_git_graph_tooltip_text_row(
            &commit.short_oid,
            hash_x,
            line_top,
            18.0 * s,
            0.86,
            true,
        );
        self.draw_git_graph_selectable_text(
            &commit.short_oid,
            hash_x,
            line_top + 18.0 * s * 0.62,
            [1.0, 1.0, 1.0, 1.0],
            0.86,
            row_start,
            line_top,
            18.0 * s,
            true,
        );
        let copy_size = 16.0 * s;
        let copy_x = hash_x + hash_w + 7.0 * s;
        let copy_y = line_top + (18.0 * s - copy_size) * 0.5 - 2.0 * s;
        let copy_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphCopyCommit(target.workspace_idx, target.commit_idx),
            copy_x - 3.0 * s,
            copy_y - 3.0 * s,
            copy_size + 6.0 * s,
            copy_size + 6.0 * s,
            mx,
            my,
        );
        self.draw_atlas_icon(
            if copied {
                crate::widgets::IconType::Check
            } else {
                crate::widgets::IconType::Copy
            },
            copy_x,
            copy_y,
            copy_size,
            if copied {
                [0.3, 0.9, 0.4, 1.0]
            } else if copy_hovered {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.38, 0.62, 1.0, 0.86]
            },
        );
        let sep_x = copy_x + copy_size + 12.0 * s;
        self.push_rect(
            sep_x,
            line_top - 1.0 * s,
            1.0,
            18.0 * s,
            [1.0, 1.0, 1.0, 0.28],
        );
        let open_icon_size = 14.0 * s;
        let open_icon_x = sep_x + 14.0 * s;
        let open_icon_y = line_top + (18.0 * s - open_icon_size) * 0.5 - 3.0 * s;
        let open_x = open_icon_x + open_icon_size + 5.0 * s;
        let open_text = "Open on GitHub";
        let open_w = self.measure_ui_width(open_text, 0.86);
        ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphOpenCommit(target.workspace_idx, target.commit_idx),
            open_icon_x,
            line_top,
            open_icon_size + 5.0 * s + open_w,
            18.0 * s,
            mx,
            my,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::GithubDark,
            open_icon_x,
            open_icon_y,
            open_icon_size,
            [0.38, 0.62, 1.0, 1.0],
        );
        self.draw_string_scaled(
            open_text,
            open_x,
            line_top + 18.0 * s * 0.62,
            [0.38, 0.62, 1.0, 1.0],
            0.86,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_panel(
        &mut self,
        panel_x: f32,
        panel_w: f32,
        graph_y: f32,
        graph_h: f32,
        pad: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        scratch: &mut String,
    ) {
        if graph_h <= 20.0 * s {
            return;
        }
        self.push_rect(
            panel_x,
            graph_y,
            panel_w,
            graph_h,
            [
                self.theme.bg[0] + 0.018,
                self.theme.bg[1] + 0.020,
                self.theme.bg[2] + 0.026,
                1.0,
            ],
        );

        let header_h = 34.0 * s;
        self.push_rect(
            panel_x,
            graph_y,
            panel_w,
            header_h,
            [
                self.theme.bg[0] + 0.005,
                self.theme.bg[1] + 0.006,
                self.theme.bg[2] + 0.010,
                1.0,
            ],
        );
        let tab_clip_x = panel_x + pad;
        let tab_clip_w = (panel_w - pad * 2.0).max(0.0);
        let mut tab_x = tab_clip_x - ide_panel.git.graph_workspace_scroll_x.round();
        let tab_y = graph_y + 6.0 * s;
        let tab_h = 23.0 * s;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (graph_y + header_h);
            self.gl.scissor(
                tab_clip_x.round() as i32,
                scissor_y.max(0.0) as i32,
                tab_clip_w.round() as i32,
                header_h.round() as i32,
            );
        }
        for workspace in ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .filter(|workspace| workspace.repo_root.is_some())
        {
            let name = workspace
                .root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace");
            let tab_w = (self.measure_ui_width(name, 0.76) + 18.0 * s).max(48.0 * s);
            let active = ide_panel.git.graph_workspace_idx == Some(workspace.workspace_idx);
            let visible = tab_x + tab_w >= tab_clip_x && tab_x <= tab_clip_x + tab_clip_w;
            if visible {
                let hit_x = tab_x.max(tab_clip_x);
                let hit_w = (tab_x + tab_w).min(tab_clip_x + tab_clip_w) - hit_x;
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::GitGraphWorkspace(workspace.workspace_idx),
                    hit_x,
                    tab_y,
                    hit_w,
                    tab_h,
                    mx,
                    my,
                );
                if active || hovered {
                    self.push_rounded_rect(
                        tab_x,
                        tab_y,
                        tab_w,
                        tab_h,
                        4.0 * s,
                        if active {
                            [0.60, 0.35, 0.85, 0.28]
                        } else {
                            [1.0, 1.0, 1.0, 0.075]
                        },
                    );
                }
                self.draw_string_scaled(
                    name,
                    tab_x + 9.0 * s,
                    tab_y + tab_h / 2.0 + 4.5 * s,
                    if active {
                        self.theme.fg
                    } else {
                        [0.72, 0.76, 0.88, 0.72]
                    },
                    0.76,
                );
            }
            tab_x += tab_w + 6.0 * s;
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let rows_y = graph_y + header_h;
        let rows_h = (graph_h - header_h).max(0.0);
        let commits = &ide_panel.git.graph_snapshot;
        if rows_h <= 0.0 {
            return;
        }
        if commits.is_empty() {
            let hint = if ide_panel.git.graph_pending {
                "Graph scan..."
            } else {
                ide_panel
                    .git
                    .graph_notice
                    .as_deref()
                    .unwrap_or("No commits")
            };
            let tw = self.measure_ui_width(hint, 0.82);
            self.draw_string_scaled(
                hint,
                panel_x + (panel_w - tw) / 2.0,
                rows_y + 28.0 * s,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.48],
                0.82,
            );
            return;
        }

        let row_h = crate::app::git_panel::GIT_GRAPH_ROW_H * s;
        let scroll = ide_panel.git.graph_scroll.current.round();
        let first = (scroll / row_h).floor().max(0.0) as usize;
        let last = (((scroll + rows_h) / row_h).ceil() as usize + 1).min(commits.len());
        let active_workspace = ide_panel.git.graph_workspace_idx.unwrap_or(0);
        let mut row_hover_target = None;

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (rows_y + rows_h);
            self.gl.scissor(
                panel_x as i32,
                scissor_y.max(0.0) as i32,
                panel_w as i32,
                rows_h as i32,
            );
        }

        for idx in first..last {
            let commit = &commits[idx];
            let row_y = rows_y + idx as f32 * row_h - scroll;
            let hovered = ui_registry.register_rect(
                crate::ui_system::UiId::GitGraphCommit(active_workspace, idx),
                panel_x,
                row_y,
                panel_w,
                row_h,
                mx,
                my,
            );
            if hovered {
                row_hover_target = Some((
                    GitGraphTooltipTarget {
                        workspace_idx: active_workspace,
                        commit_idx: idx,
                    },
                    panel_x + panel_w,
                    my,
                ));
                self.push_rect(panel_x, row_y, panel_w, row_h, [1.0, 1.0, 1.0, 0.055]);
            }

            let circle_y = row_y + row_h / 2.0;
            let graph_layout = git_graph_row_layout(panel_x, pad, s, commit.column, &commit.lanes);
            let gutter_w = graph_layout.gutter_w;
            let lane_step = graph_layout.lane_step;
            let lane_start_x = graph_layout.lane_start_x;
            let text_x = graph_layout.text_x;
            let commit_x = lane_start_x + commit.column as f32 * lane_step;
            let graph_clip_right = panel_x + panel_w - 8.0 * s;
            for vertical_pass in [false, true] {
                for lane in &commit.lanes {
                    let is_vertical = matches!(
                        lane.kind,
                        crate::app::git_panel::GitGraphLaneKind::Vertical
                            | crate::app::git_panel::GitGraphLaneKind::VerticalTop
                            | crate::app::git_panel::GitGraphLaneKind::VerticalBottom
                    );
                    if is_vertical != vertical_pass {
                        continue;
                    }
                    let lane_x = lane_start_x + lane.column as f32 * lane_step;
                    let target_x = lane_start_x + lane.target_column as f32 * lane_step;
                    if lane_x > panel_x + pad + gutter_w {
                        continue;
                    }
                    if lane_x > graph_clip_right && target_x > graph_clip_right {
                        continue;
                    }
                    let color =
                        git_graph_lane_color(usize::from(lane.color_idx), 0.62, self.theme.sel);
                    match lane.kind {
                        crate::app::git_panel::GitGraphLaneKind::Vertical => {
                            let mut top = row_y;
                            let mut bottom = row_y + row_h;
                            if usize::from(lane.column) == commit.column {
                                if idx == 0 {
                                    top = circle_y;
                                }
                                if idx + 1 == commits.len() {
                                    bottom = circle_y;
                                }
                            }
                            self.push_git_graph_vertical_segment(lane_x, top, bottom, s, color);
                        }
                        crate::app::git_panel::GitGraphLaneKind::VerticalTop => {
                            let bottom = if usize::from(lane.column) == commit.column {
                                circle_y - 5.0 * s
                            } else {
                                circle_y
                            };
                            self.push_git_graph_vertical_segment(lane_x, row_y, bottom, s, color);
                        }
                        crate::app::git_panel::GitGraphLaneKind::VerticalBottom => {
                            let top = if usize::from(lane.column) == commit.column {
                                circle_y + 5.0 * s
                            } else {
                                circle_y
                            };
                            self.push_git_graph_vertical_segment(
                                lane_x,
                                top,
                                row_y + row_h,
                                s,
                                color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::Shift => {
                            self.push_git_graph_shift_segment(
                                lane_x, target_x, row_y, row_h, s, color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::ShiftToCommit => {
                            self.push_git_graph_shift_to_commit_segment(
                                lane_x, target_x, row_y, row_h, s, color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::Parent => {
                            self.push_git_graph_parent_segment(
                                commit_x, target_x, row_y, row_h, s, color,
                            );
                        }
                    }
                }
            }
            let circle_color = git_graph_lane_color(commit.color_idx, 1.0, self.theme.sel);
            if commit.is_head {
                self.push_rounded_rect(
                    commit_x - 6.0 * s,
                    circle_y - 6.0 * s,
                    12.0 * s,
                    12.0 * s,
                    6.0 * s,
                    circle_color,
                );
                self.push_rounded_rect(
                    commit_x - 3.0 * s,
                    circle_y - 3.0 * s,
                    6.0 * s,
                    6.0 * s,
                    3.0 * s,
                    [
                        self.theme.bg[0] + 0.018,
                        self.theme.bg[1] + 0.020,
                        self.theme.bg[2] + 0.026,
                        1.0,
                    ],
                );
            } else {
                self.push_rounded_rect(
                    commit_x - 5.0 * s,
                    circle_y - 5.0 * s,
                    10.0 * s,
                    10.0 * s,
                    5.0 * s,
                    circle_color,
                );
            }

            let has_last_name = commit.author_name.split_whitespace().nth(1).is_some();
            let author_text_w = self.measure_ui_width(&commit.author_name, 0.78);
            let author_reserve_w = if has_last_name {
                118.0 * s
            } else {
                (author_text_w + 6.0 * s).clamp(48.0 * s, 92.0 * s)
            };
            let author_right_x = panel_x + panel_w - 30.0 * s;
            let author_draw_w = author_text_w.min(author_reserve_w);
            let author_x = (author_right_x - author_draw_w).max(text_x);
            let row_text_y = Self::tree_row_text_y(row_y, row_h, s);
            let local_ref_name = commit
                .local_refs
                .first()
                .map(|git_ref| git_ref.name.as_str());
            let remote_ref_name = commit
                .remote_refs
                .first()
                .map(|git_ref| git_ref.name.as_str());
            let chip_scale = 0.82;
            let chip_pad_x = 5.0 * s;
            let chip_gap = 5.0 * s;
            let chip_max_w = 140.0 * s;
            let local_chip_w = local_ref_name.map(|name| {
                branch_chip_width(
                    self.measure_ui_width(name, chip_scale),
                    chip_pad_x,
                    chip_max_w,
                )
            });
            let remote_chip_w = remote_ref_name.map(|name| {
                branch_chip_width(
                    self.measure_ui_width(name, chip_scale),
                    chip_pad_x,
                    chip_max_w,
                )
            });
            let mut chips_w = local_chip_w.unwrap_or(0.0) + remote_chip_w.unwrap_or(0.0);
            if local_chip_w.is_some() && remote_chip_w.is_some() {
                chips_w += chip_gap;
            }
            let row_available_w = (author_x - text_x - 12.0 * s).max(20.0 * s);
            let chips_visible = chips_w > 0.0 && row_available_w >= chips_w + 36.0 * s;
            let summary_max_w = if chips_visible {
                row_available_w - chips_w - 8.0 * s
            } else {
                row_available_w
            };
            let summary_w = self.draw_git_graph_label_clipped(
                &commit.summary,
                text_x,
                row_text_y,
                summary_max_w,
                self.theme.fg,
                0.82,
                scratch,
            );
            let row_text_center_y = self.ui_text_center_y(&commit.summary, row_text_y, 0.82);
            if chips_visible {
                let chip_h = 18.0 * s;
                let mut chip_x = (text_x + summary_w + 8.0 * s).round();
                if let (Some(name), Some(chip_w)) = (local_ref_name, local_chip_w) {
                    self.draw_git_graph_branch_chip(
                        name,
                        chip_x,
                        row_text_center_y,
                        chip_w,
                        chip_h,
                        4.0 * s,
                        [0.28, 0.24, 0.40, 1.0],
                        [0.86, 0.90, 1.0, 1.0],
                        chip_scale,
                        chip_pad_x,
                        false,
                        scratch,
                    );
                    chip_x += chip_w + chip_gap;
                }
                if let (Some(name), Some(chip_w)) = (remote_ref_name, remote_chip_w) {
                    self.draw_git_graph_branch_chip(
                        name,
                        chip_x,
                        row_text_center_y,
                        chip_w,
                        chip_h,
                        4.0 * s,
                        [0.24, 0.32, 0.42, 1.0],
                        [0.86, 0.90, 1.0, 1.0],
                        chip_scale,
                        chip_pad_x,
                        false,
                        scratch,
                    );
                }
            }
            self.draw_git_graph_label_clipped(
                &commit.author_name,
                author_x,
                row_text_y,
                author_draw_w,
                [0.72, 0.76, 0.88, 0.72],
                0.78,
                scratch,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let max_scroll = crate::app::git_panel::git_graph_max_scroll(commits.len(), rows_h, s);
        if max_scroll > 0.0 {
            let ratio = (scroll / max_scroll).clamp(0.0, 1.0);
            let thumb_h = crate::app::git_panel::git_graph_scroll_thumb_h(commits.len(), rows_h, s);
            let thumb_y = rows_y + 4.0 * s + ratio * (rows_h - 8.0 * s - thumb_h);
            let track_w = 10.0 * s;
            let track_x = panel_x + panel_w - track_w - 9.0 * s;
            ui_registry.register_rect(
                crate::ui_system::UiId::GitGraphScroll,
                track_x,
                rows_y,
                track_w,
                rows_h,
                mx,
                my,
            );
            self.push_rounded_rect(
                track_x + 2.0 * s,
                thumb_y,
                6.0 * s,
                thumb_h,
                3.0 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        let mouse_in_commit_area =
            mx >= panel_x && mx <= panel_x + panel_w && my >= rows_y && my <= rows_y + rows_h;
        if let Some((target, anchor_x, anchor_y)) = row_hover_target {
            self.git_graph_tooltip =
                Some((target.workspace_idx, target.commit_idx, anchor_x, anchor_y));
        } else if let Some(hover) = self.git_graph_tooltip_hover
            && hover.workspace_idx == active_workspace
            && (hover.contains(mx, my) || self.git_graph_tooltip_selecting)
        {
            self.git_graph_tooltip = Some((
                hover.workspace_idx,
                hover.commit_idx,
                hover.anchor_x,
                hover.anchor_y,
            ));
        } else if !mouse_in_commit_area {
            self.git_graph_tooltip = None;
            self.git_graph_tooltip_hover = None;
            self.git_graph_tooltip_stable_w = 0.0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_panel(
        &mut self,
        panel_x: f32,
        title_h: f32,
        panel_w: f32,
        content_h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        let pad = (10.0 * s).min((panel_w * 0.15).max(0.0));
        let inner_w = (panel_w - pad * 2.0).max(1.0);
        let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
        let list_y = title_h + controls_h;
        let full_list_h = (content_h - controls_h).max(40.0 * s);
        let (list_h, graph_divider_h, graph_h) = if ide_panel.git.graph_open {
            crate::app::git_panel::git_graph_split_heights(
                full_list_h,
                ide_panel.git.graph_height_ratio,
                s,
            )
        } else {
            (full_list_h, 0.0, 0.0)
        };
        let graph_divider_y = list_y + list_h;
        let graph_y = graph_divider_y + graph_divider_h;
        let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
        let workspace_h = 30.0 * s;
        let scroll = ide_panel.git.scroll.current.round();
        let mut y = list_y - scroll;
        let text_scale = crate::render_view::tree_ui::TREE_TEXT_SCALE;
        let mut label_scratch = String::new();
        let mut git_file_tooltip: Option<(usize, usize, String, f32, f32)> = None;

        let input_x = panel_x + pad;
        let input_y = title_h + 8.0 * s;
        let input_w = inner_w;
        let input_h = 30.0 * s;
        let input_border = if ide_panel.git.message_focused {
            [0.60, 0.35, 0.85, 0.78]
        } else {
            [1.0, 1.0, 1.0, 0.10]
        };
        self.push_rounded_rect(
            input_x - 1.0,
            input_y - 1.0,
            input_w + 2.0,
            input_h + 2.0,
            4.0 * s,
            input_border,
        );
        self.push_rounded_rect(
            input_x,
            input_y,
            input_w,
            input_h,
            4.0 * s,
            if ide_panel.git.message_focused {
                [0.18, 0.19, 0.25, 1.0]
            } else {
                [0.11, 0.12, 0.16, 1.0]
            },
        );
        ui_registry.register_text_input(
            crate::ui_system::UiId::GitMessageInput,
            input_x,
            input_y,
            input_w,
            input_h,
            mx,
            my,
        );

        self.flush();
        unsafe {
            let text = ide_panel.git.message_editor.get_full_text();
            let text_y = input_y + input_h / 2.0 + 6.0 * s;
            let text_start_x = input_x + 5.0 * s;
            let visible_width = input_w - 10.0 * s;

            let mut cursor_total_x = 0.0;
            let mut total_text_width = 0.0;
            for (byte_idx, c) in text.char_indices() {
                let adv = self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0);
                if byte_idx < ide_panel.git.message_editor.cursor {
                    cursor_total_x += adv;
                }
                total_text_width += adv;
            }

            if ide_panel.git.message_focused {
                if cursor_total_x - self.search_scroll_x > visible_width {
                    self.search_scroll_x = cursor_total_x - visible_width;
                }
                if cursor_total_x - self.search_scroll_x < 0.0 {
                    self.search_scroll_x = cursor_total_x;
                }
                self.search_scroll_x = self
                    .search_scroll_x
                    .min(total_text_width - visible_width)
                    .max(0.0);
            }

            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(
                input_x as i32,
                scissor_y as i32,
                input_w as i32,
                input_h as i32,
            );

            let sel_start = ide_panel
                .git
                .message_editor
                .selection_anchor
                .unwrap_or(ide_panel.git.message_editor.cursor)
                .min(ide_panel.git.message_editor.cursor);
            let sel_end = ide_panel
                .git
                .message_editor
                .selection_anchor
                .unwrap_or(ide_panel.git.message_editor.cursor)
                .max(ide_panel.git.message_editor.cursor);
            let mut cursor_draw_x = text_start_x - self.search_scroll_x;

            if text.is_empty() {
                self.draw_string_scaled(
                    "Message",
                    text_start_x,
                    text_y,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.34],
                    1.0,
                );
            } else {
                let mut current_x = text_start_x - self.search_scroll_x;
                let mut byte_idx = 0usize;

                for c in text.chars() {
                    if byte_idx == ide_panel.git.message_editor.cursor {
                        cursor_draw_x = current_x;
                    }
                    let adv = self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0);
                    if byte_idx >= sel_start && byte_idx < sel_end {
                        self.push_rect(
                            current_x,
                            input_y + 4.0 * s,
                            adv,
                            input_h - 8.0 * s,
                            self.theme.sel,
                        );
                    }
                    if let Some(g) = self.get_ui_glyph(c) {
                        self.push_quad(
                            current_x + g.offset_x,
                            text_y - g.offset_y,
                            g.width,
                            g.height,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                    current_x += adv;
                    byte_idx += c.len_utf8();
                }
                if byte_idx == ide_panel.git.message_editor.cursor {
                    cursor_draw_x = current_x;
                }
            }
            if ide_panel.git.message_focused && sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x,
                    input_y + 4.0 * s,
                    2.0 * s,
                    input_h - 8.0 * s,
                    self.theme.fg,
                );
            }
            self.flush();
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let commit_y = title_h + 44.0 * s;
        let arrow_w = (38.0 * s).min((inner_w * 0.28).max(22.0 * s));
        let commit_gap = (4.0 * s).min((inner_w * 0.06).max(0.0));
        let commit_main_w = (inner_w - arrow_w - commit_gap).max(1.0);
        let commit_btn = Button {
            x: panel_x + pad,
            y: commit_y,
            w: commit_main_w,
            h: 28.0 * s,
            text: "Commit".to_string(),
            icon: Some(crate::widgets::IconType::Check),
            text_scale: 0.92,
            icon_size: 20.0 * s,
        };
        if ide_panel.git.pending {
            render_git_disabled_button(self, &commit_btn, s);
        } else {
            ui_registry.register_button(
                crate::ui_system::UiId::GitCommit,
                &commit_btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }
        let menu_btn = Button {
            x: panel_x + pad + commit_main_w + commit_gap,
            y: commit_y,
            w: arrow_w,
            h: 28.0 * s,
            text: String::new(),
            icon: Some(crate::widgets::IconType::Down),
            text_scale: 0.0,
            icon_size: 24.0 * s,
        };
        if ide_panel.git.pending {
            render_git_disabled_button(self, &menu_btn, s);
        } else {
            ui_registry.register_button(
                crate::ui_system::UiId::GitCommitMenuToggle,
                &menu_btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }

        let graph_btn_y = title_h + 75.0 * s;
        let graph_btn_w = (72.0 * s).min(inner_w.max(1.0));
        let graph_btn = Button {
            x: panel_x + pad,
            y: graph_btn_y,
            w: graph_btn_w,
            h: 22.0 * s,
            text: "Граф".to_string(),
            icon: Some(crate::widgets::IconType::Branch),
            text_scale: 0.78,
            icon_size: 21.0 * s,
        };
        let graph_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphToggle,
            graph_btn.x,
            graph_btn.y,
            graph_btn.w,
            graph_btn.h,
            mx,
            my,
        );
        render_git_graph_button(self, &graph_btn, s, graph_hovered, ide_panel.git.graph_open);
        if ide_panel.git.graph_open {
            self.push_rect(
                graph_btn.x,
                graph_btn.y + graph_btn.h - 2.0,
                graph_btn.w,
                2.0,
                [0.60, 0.35, 0.85, 0.9],
            );
        }

        let refresh_gap = 6.0 * s;
        let refresh_x = graph_btn.x + graph_btn.w + refresh_gap;
        let refresh_available_w = (panel_x + pad + inner_w - refresh_x).max(0.0);
        let refresh_label_w = self.measure_ui_width("Обновить", 0.78);
        let refresh_full_w = refresh_label_w + 22.0 * s + 18.0 * s;
        let mut notice_x = graph_btn.x + graph_btn.w + 8.0 * s;
        if refresh_available_w >= 30.0 * s {
            let refresh_icon_only = refresh_available_w < refresh_full_w;
            let refresh_btn = Button {
                x: refresh_x,
                y: graph_btn_y,
                w: if refresh_icon_only {
                    (34.0 * s).min(refresh_available_w)
                } else {
                    refresh_full_w.min(refresh_available_w)
                },
                h: 22.0 * s,
                text: if refresh_icon_only {
                    String::new()
                } else {
                    "Обновить".to_string()
                },
                icon: Some(crate::widgets::IconType::Reload),
                text_scale: 0.78,
                icon_size: 22.0 * s,
            };
            let refresh_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::GitRefresh,
                refresh_btn.x,
                refresh_btn.y,
                refresh_btn.w,
                refresh_btn.h,
                mx,
                my,
            );
            render_git_graph_button(self, &refresh_btn, s, refresh_hovered, false);
            notice_x = refresh_btn.x + refresh_btn.w + 8.0 * s;
        }

        if let Some(notice) = ide_panel
            .git
            .graph_notice
            .as_ref()
            .or(ide_panel.git.notice.as_ref())
        {
            self.draw_tree_label_clipped(
                notice,
                notice_x,
                graph_btn_y + 16.0 * s,
                (panel_x + pad + inner_w - notice_x).max(0.0),
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.68],
                0.78,
                &mut label_scratch,
            );
        }

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (list_y + list_h);
            self.gl.scissor(
                panel_x as i32,
                scissor_y.max(0.0) as i32,
                panel_w as i32,
                list_h as i32,
            );
        }

        let staged_workspace = ide_panel.git.staged_workspace_lock();
        let mut drew_any = false;

        for workspace in &ide_panel.git.snapshot.workspaces {
            let workspace_disabled =
                staged_workspace.is_some_and(|idx| idx != workspace.workspace_idx);
            let workspace_is_collapsed = ide_panel
                .git
                .collapsed_workspaces
                .contains(&workspace.workspace_idx);

            drew_any = true;
            let row_visible = y + workspace_h >= list_y && y <= list_y + list_h;
            if row_visible {
                let workspace_name_color =
                    git_disabled_color(self.theme.fg, workspace_disabled, 0.38);
                let show_stage_actions = !workspace.files.is_empty();
                let stage_interaction_disabled =
                    git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                let changed_count = workspace.files.len();
                let count_text_scale = 0.78;
                let count_badge_h = 19.0 * s;
                let (count_badge_w, count_text_w) = if changed_count > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("{changed_count}"),
                    );
                    let text_w = self
                        .measure_ui_width(&label_scratch, count_text_scale)
                        .round();
                    ((text_w + 12.0 * s).max(count_badge_h), text_w)
                } else {
                    (0.0, 0.0)
                };
                let count_reserve = if changed_count > 0 {
                    count_badge_w + 6.0 * s
                } else {
                    0.0
                };
                let stage_btn_w = 26.0 * s;
                let stage_btn_gap = 4.0 * s;
                let stage_actions_w = if show_stage_actions {
                    stage_btn_w * 3.0 + stage_btn_gap * 2.0 + 8.0 * s
                } else {
                    0.0
                };
                let (ahead_text_w, push_w) = if workspace.ahead > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("↑{}", workspace.ahead),
                    );
                    (
                        self.measure_ui_width(&label_scratch, 0.78).round(),
                        (46.0 * s).min((panel_w * 0.36).max(18.0 * s)),
                    )
                } else {
                    (0.0, 0.0)
                };
                let push_reserve = if workspace.ahead > 0 {
                    ahead_text_w + 8.0 * s + push_w + 6.0 * s
                } else {
                    0.0
                };
                self.push_rect(
                    panel_x,
                    y,
                    panel_w,
                    workspace_h,
                    [
                        self.theme.bg[0] + 0.035,
                        self.theme.bg[1] + 0.035,
                        self.theme.bg[2] + 0.045,
                        1.0,
                    ],
                );
                let name = workspace
                    .root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("workspace");
                let workspace_has_rows = workspace.has_collapsible_rows();
                let workspace_arrow_x = panel_x + pad;
                let workspace_label_x = if workspace_has_rows {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::GitWorkspaceToggle(workspace.workspace_idx),
                        workspace_arrow_x - 4.0 * s,
                        y + 4.0 * s,
                        18.0 * s,
                        workspace_h - 8.0 * s,
                        mx,
                        my,
                    );
                    self.draw_tree_disclosure_icon(
                        !workspace_is_collapsed,
                        workspace_arrow_x,
                        y + 2.0 * s,
                        workspace_h,
                        git_disabled_color([0.78, 0.80, 0.88, 0.75], workspace_disabled, 0.26),
                    );
                    workspace_arrow_x + 18.0 * s
                } else {
                    workspace_arrow_x
                };
                let right_reserve = 12.0 * s + count_reserve + push_reserve + stage_actions_w;
                let label_w = (panel_x + panel_w - workspace_label_x - right_reserve).max(0.0);
                let workspace_text_y = y + workspace_h / 2.0 + 4.5 * s;
                if let Some(branch_name) = &workspace.branch_name {
                    let branch_scale = 0.82;
                    let chip_pad_x = 6.0 * s;
                    let chip_h = 19.0 * s;
                    let chip_w =
                        self.measure_ui_width(branch_name, branch_scale) + chip_pad_x * 2.0;
                    let gap = 8.0 * s;
                    if label_w > chip_w + gap + 24.0 * s {
                        let name_w = self.measure_ui_width(name, 0.9).min(label_w - chip_w - gap);
                        self.draw_tree_label_clipped(
                            name,
                            workspace_label_x,
                            workspace_text_y,
                            name_w,
                            workspace_name_color,
                            0.9,
                            &mut label_scratch,
                        );
                        let chip_x = (workspace_label_x + name_w + gap).round();
                        let workspace_text_center_y =
                            self.ui_text_center_y(name, workspace_text_y, 0.9);
                        let branch_text_y = self.ui_text_baseline_for_center_y(
                            branch_name,
                            workspace_text_center_y,
                            branch_scale,
                        );
                        let branch_center_y =
                            self.ui_text_center_y(branch_name, branch_text_y, branch_scale);
                        let chip_y = branch_chip_y_from_text_center(branch_center_y, chip_h);
                        let chip_w = chip_w.round();
                        self.push_rounded_rect(
                            chip_x,
                            chip_y,
                            chip_w,
                            chip_h,
                            4.0 * s,
                            [0.20, 0.22, 0.30, 1.0],
                        );
                        self.draw_string_scaled(
                            branch_name,
                            (chip_x + chip_pad_x).round(),
                            branch_text_y,
                            [0.78, 0.82, 0.92, 1.0],
                            branch_scale,
                        );
                    } else {
                        self.draw_tree_label_clipped(
                            name,
                            workspace_label_x,
                            workspace_text_y,
                            label_w,
                            workspace_name_color,
                            0.9,
                            &mut label_scratch,
                        );
                    }
                } else {
                    self.draw_tree_label_clipped(
                        name,
                        workspace_label_x,
                        workspace_text_y,
                        label_w,
                        workspace_name_color,
                        0.9,
                        &mut label_scratch,
                    );
                }

                let right_x = panel_x + panel_w - pad;
                if show_stage_actions {
                    let stage_btn_h = 22.0 * s;
                    let unstage_x = right_x - stage_btn_w;
                    let stage_x = unstage_x - stage_btn_gap - stage_btn_w;
                    let rollback_x = stage_x - stage_btn_gap - stage_btn_w;
                    let push_x = if workspace.ahead > 0 {
                        rollback_x - 6.0 * s - push_w
                    } else {
                        rollback_x
                    };
                    let btn_y = y + ((workspace_h - stage_btn_h) / 2.0).round();
                    if changed_count > 0 {
                        label_scratch.clear();
                        let _ = std::fmt::Write::write_fmt(
                            &mut label_scratch,
                            format_args!("{changed_count}"),
                        );
                        let badge_x = if workspace.ahead > 0 {
                            push_x - 8.0 * s - ahead_text_w - 6.0 * s - count_badge_w
                        } else {
                            rollback_x - stage_btn_gap - count_badge_w
                        };
                        let badge_y = y + ((workspace_h - count_badge_h) / 2.0).round();
                        self.push_rounded_rect(
                            badge_x,
                            badge_y,
                            count_badge_w,
                            count_badge_h,
                            count_badge_h / 2.0,
                            git_disabled_color([0.24, 0.27, 0.34, 1.0], workspace_disabled, 0.34),
                        );
                        self.draw_string_scaled(
                            &label_scratch,
                            (badge_x + (count_badge_w - count_text_w) / 2.0).round(),
                            (badge_y + count_badge_h / 2.0 + 4.0 * s).round(),
                            git_disabled_color([0.86, 0.90, 1.0, 1.0], workspace_disabled, 0.38),
                            count_text_scale,
                        );
                    }
                    if workspace.ahead > 0 {
                        label_scratch.clear();
                        let _ = std::fmt::Write::write_fmt(
                            &mut label_scratch,
                            format_args!("↑{}", workspace.ahead),
                        );
                        self.draw_string_scaled(
                            &label_scratch,
                            (push_x - 8.0 * s - ahead_text_w).max(panel_x + pad),
                            y + workspace_h / 2.0 + 5.0 * s,
                            git_disabled_color([0.48, 0.74, 1.0, 1.0], workspace_disabled, 0.34),
                            0.78,
                        );
                        let push_btn = Button {
                            x: push_x,
                            y: y + 5.0 * s,
                            w: push_w,
                            h: 20.0 * s,
                            text: if push_w < 38.0 * s { "↑" } else { "Push" }.to_string(),
                            icon: None,
                            text_scale: 0.76,
                            icon_size: 0.0,
                        };
                        if workspace_disabled {
                            render_git_disabled_button(self, &push_btn, s);
                            register_git_locked_button_cursor(
                                ui_registry,
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                mx,
                                my,
                            );
                        } else if ide_panel.git.pending {
                            push_btn.render(self, -1.0, -1.0, s, false);
                            register_git_locked_button_cursor(
                                ui_registry,
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                mx,
                                my,
                            );
                        } else {
                            ui_registry.register_button(
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                        }
                    }
                    let rollback_btn = Button {
                        x: rollback_x,
                        y: btn_y,
                        w: stage_btn_w,
                        h: stage_btn_h,
                        text: String::new(),
                        icon: Some(crate::widgets::IconType::Rollback),
                        text_scale: 0.98,
                        icon_size: 21.0 * s,
                    };
                    let rollback_hovered = if workspace_disabled {
                        render_git_disabled_button(self, &rollback_btn, s);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            mx,
                            my,
                        );
                        false
                    } else if stage_interaction_disabled {
                        rollback_btn.render(self, -1.0, -1.0, s, false);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            mx,
                            my,
                        );
                        false
                    } else {
                        ui_registry.register_button(
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            self,
                            mx,
                            my,
                            s,
                            false,
                        )
                    };
                    if rollback_hovered {
                        self.git_action_tooltip = Some((
                            GIT_TOOLTIP_ROLLBACK,
                            workspace.workspace_idx,
                            "Откатить staged".to_string(),
                            mx,
                            my,
                        ));
                    }

                    for (id, icon, bx, tooltip, kind) in [
                        (
                            crate::ui_system::UiId::GitStageAll(workspace.workspace_idx),
                            crate::widgets::IconType::GitPlus,
                            stage_x,
                            "Добавить все",
                            GIT_TOOLTIP_STAGE_ALL,
                        ),
                        (
                            crate::ui_system::UiId::GitUnstageAll(workspace.workspace_idx),
                            crate::widgets::IconType::GitMinus,
                            unstage_x,
                            "Убрать все",
                            GIT_TOOLTIP_UNSTAGE_ALL,
                        ),
                    ] {
                        let btn = Button {
                            x: bx,
                            y: btn_y,
                            w: stage_btn_w,
                            h: stage_btn_h,
                            text: String::new(),
                            icon: Some(icon),
                            text_scale: 0.0,
                            icon_size: 28.0 * s,
                        };
                        let hovered = if workspace_disabled {
                            render_git_disabled_button(self, &btn, s);
                            register_git_locked_button_cursor(ui_registry, id, &btn, mx, my);
                            false
                        } else if stage_interaction_disabled {
                            btn.render(self, -1.0, -1.0, s, false);
                            register_git_locked_button_cursor(ui_registry, id, &btn, mx, my);
                            false
                        } else {
                            ui_registry.register_button(id, &btn, self, mx, my, s, false)
                        };
                        if hovered {
                            self.git_action_tooltip =
                                Some((kind, workspace.workspace_idx, tooltip.to_string(), mx, my));
                        }
                    }
                } else if workspace.ahead > 0 {
                    let push_x = right_x - push_w;
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("↑{}", workspace.ahead),
                    );
                    self.draw_string_scaled(
                        &label_scratch,
                        (push_x - 8.0 * s - ahead_text_w).max(panel_x + pad),
                        y + workspace_h / 2.0 + 5.0 * s,
                        git_disabled_color([0.48, 0.74, 1.0, 1.0], workspace_disabled, 0.34),
                        0.78,
                    );
                    let push_btn = Button {
                        x: push_x,
                        y: y + 5.0 * s,
                        w: push_w,
                        h: 20.0 * s,
                        text: if push_w < 38.0 * s { "↑" } else { "Push" }.to_string(),
                        icon: None,
                        text_scale: 0.76,
                        icon_size: 0.0,
                    };
                    if workspace_disabled {
                        render_git_disabled_button(self, &push_btn, s);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            mx,
                            my,
                        );
                    } else if ide_panel.git.pending {
                        push_btn.render(self, -1.0, -1.0, s, false);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            mx,
                            my,
                        );
                    } else {
                        ui_registry.register_button(
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            self,
                            mx,
                            my,
                            s,
                            false,
                        );
                    }
                } else if changed_count > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("{changed_count}"),
                    );
                    let badge_x = right_x - count_badge_w;
                    let badge_y = y + ((workspace_h - count_badge_h) / 2.0).round();
                    self.push_rounded_rect(
                        badge_x,
                        badge_y,
                        count_badge_w,
                        count_badge_h,
                        count_badge_h / 2.0,
                        git_disabled_color([0.24, 0.27, 0.34, 1.0], workspace_disabled, 0.34),
                    );
                    self.draw_string_scaled(
                        &label_scratch,
                        (badge_x + (count_badge_w - count_text_w) / 2.0).round(),
                        (badge_y + count_badge_h / 2.0 + 4.0 * s).round(),
                        git_disabled_color([0.86, 0.90, 1.0, 1.0], workspace_disabled, 0.38),
                        count_text_scale,
                    );
                }
            }
            y += workspace_h;

            if workspace_is_collapsed {
                continue;
            }

            if let Some(err) = &workspace.error {
                if y + row_h >= list_y && y <= list_y + list_h {
                    self.draw_tree_label_clipped(
                        err,
                        panel_x + pad,
                        y + row_h / 2.0 + 5.0 * s,
                        inner_w,
                        [0.95, 0.42, 0.46, 1.0],
                        0.82,
                        &mut label_scratch,
                    );
                }
                y += row_h;
                continue;
            }

            let mut collapsed_depth = None;
            let workspace_collapsed = ide_panel.git.collapsed_dirs.get(&workspace.workspace_idx);
            for (row_idx, row) in workspace.tree.iter().enumerate() {
                if let Some(depth) = collapsed_depth {
                    if row.depth > depth {
                        continue;
                    }
                    collapsed_depth = None;
                }
                let visible = y + row_h >= list_y && y <= list_y + list_h;
                let row_collapsed = row.file_idx.is_none()
                    && workspace_collapsed.is_some_and(|dirs| dirs.contains(row.path.as_str()));
                if visible {
                    let indent_x = panel_x
                        + pad
                        + row.depth as f32 * crate::render_view::tree_ui::TREE_INDENT_W * s;
                    if let Some(file_idx) = row.file_idx {
                        let Some(file) = workspace.files.get(file_idx) else {
                            y += row_h;
                            continue;
                        };
                        let file_layout = git_file_row_layout(indent_x, y, row_h, s);
                        let check_x = file_layout.check_x;
                        let check_y = file_layout.check_y;
                        let stage_interaction_disabled =
                            git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                        let hovered = git_row_visual_hovered(
                            mx,
                            my,
                            panel_x,
                            y,
                            panel_w,
                            row_h,
                            workspace_disabled,
                        );
                        let selected = ide_panel.git.selected_file
                            == Some((workspace.workspace_idx, file_idx));
                        if git_file_row_hitbox_enabled(stage_interaction_disabled) {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFile(workspace.workspace_idx, file_idx),
                                check_x - 3.0 * s,
                                y + 4.0 * s,
                                file_layout.check_size + 6.0 * s,
                                row_h - 8.0 * s,
                                mx,
                                my,
                            );
                        }
                        if hovered {
                            self.push_rect(panel_x, y, panel_w, row_h, [1.0, 1.0, 1.0, 0.055]);
                        } else if selected {
                            self.push_rect(
                                panel_x,
                                y,
                                panel_w,
                                row_h,
                                [
                                    self.theme.sel[0],
                                    self.theme.sel[1],
                                    self.theme.sel[2],
                                    0.16,
                                ],
                            );
                        }
                        if git_file_tooltip_hovered(hovered, mx, check_x, file_layout.check_size) {
                            let home = std::env::var_os("HOME")
                                .or_else(|| std::env::var_os("USERPROFILE"))
                                .map(std::path::PathBuf::from);
                            git_file_tooltip = Some((
                                workspace.workspace_idx,
                                file_idx,
                                git_file_tooltip_text(file, home.as_deref()),
                                mx,
                                my,
                            ));
                        }

                        let (checkbox_color, check_color) =
                            git_checkbox_color(file.staged, false, workspace_disabled);
                        self.push_rounded_rect(
                            check_x,
                            check_y,
                            file_layout.check_size,
                            file_layout.check_size,
                            2.0 * s,
                            checkbox_color,
                        );
                        if file.staged {
                            self.draw_string_scaled(
                                "✓",
                                check_x + 2.0 * s,
                                y + 18.0 * s,
                                check_color,
                                0.78,
                            );
                        }

                        let status_w = 18.0 * s;
                        let status_x = panel_x + panel_w - pad - status_w;
                        if !workspace_disabled {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFileDiff(
                                    workspace.workspace_idx,
                                    file_idx,
                                ),
                                file_layout.icon_x - 3.0 * s,
                                y,
                                (status_x - file_layout.icon_x - 8.0 * s).max(0.0),
                                row_h,
                                mx,
                                my,
                            );
                        }
                        self.draw_string_scaled(
                            file.status.label(),
                            status_x,
                            y + row_h / 2.0 + 5.0 * s,
                            if workspace_disabled {
                                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.28]
                            } else {
                                file.status.color()
                            },
                            0.82,
                        );

                        self.draw_file_icon(
                            row.icon_key,
                            false,
                            file_layout.icon_x,
                            file_layout.icon_y,
                            file_layout.icon_size,
                        );
                        self.draw_tree_leaf_label(
                            &row.name,
                            file_layout.text_x,
                            y,
                            row_h,
                            status_x - 8.0 * s,
                            if workspace_disabled {
                                [0.72, 0.76, 0.88, 0.38]
                            } else {
                                [0.72, 0.76, 0.88, 1.0]
                            },
                            s,
                            text_scale,
                            &mut label_scratch,
                        );
                    } else {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::GitFolder(workspace.workspace_idx, row_idx),
                            panel_x,
                            y,
                            panel_w,
                            row_h,
                            mx,
                            my,
                        );
                        let folder_stage =
                            crate::app::git_panel::git_folder_stage_state(workspace, row_idx);
                        let folder_layout = git_folder_row_layout(indent_x, y, row_h, s);
                        let stage_interaction_disabled =
                            git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                        let check_size = folder_layout.check_size;
                        let check_x = folder_layout.check_x;
                        let check_y = folder_layout.check_y;
                        let (checkbox_color, check_color) = git_checkbox_color(
                            matches!(
                                folder_stage,
                                Some(crate::app::git_panel::GitFolderStageState::All)
                            ),
                            matches!(
                                folder_stage,
                                Some(crate::app::git_panel::GitFolderStageState::Partial)
                            ),
                            workspace_disabled,
                        );
                        self.push_rounded_rect(
                            check_x,
                            check_y,
                            check_size,
                            check_size,
                            2.0 * s,
                            checkbox_color,
                        );
                        match folder_stage {
                            Some(crate::app::git_panel::GitFolderStageState::All) => {
                                self.draw_string_scaled(
                                    "✓",
                                    check_x + 2.0 * s,
                                    y + 18.0 * s,
                                    check_color,
                                    0.78,
                                );
                            }
                            Some(crate::app::git_panel::GitFolderStageState::Partial) => {
                                let mark_w = 8.0 * s;
                                let mark_h = 2.0 * s;
                                self.push_rect(
                                    check_x + (check_size - mark_w) / 2.0,
                                    check_y + (check_size - mark_h) / 2.0,
                                    mark_w,
                                    mark_h,
                                    check_color,
                                );
                            }
                            _ => {}
                        }
                        if !stage_interaction_disabled
                            && git_folder_stage_hitbox_enabled(folder_stage)
                        {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFolderStage(
                                    workspace.workspace_idx,
                                    row_idx,
                                ),
                                check_x - 3.0 * s,
                                y + 4.0 * s,
                                check_size + 6.0 * s,
                                row_h - 8.0 * s,
                                mx,
                                my,
                            );
                        }

                        let text_y = Self::tree_row_text_y(y, row_h, s);
                        let arrow_color =
                            git_disabled_color([0.78, 0.80, 0.88, 0.75], workspace_disabled, 0.26);
                        self.draw_tree_disclosure_icon(
                            !row_collapsed,
                            folder_layout.arrow_x,
                            y,
                            row_h,
                            arrow_color,
                        );
                        let icon_size = folder_layout.icon_size;
                        let icon_x = folder_layout.icon_x;
                        let icon_y = folder_layout.icon_y;
                        self.draw_file_icon(row.icon_key, true, icon_x, icon_y, icon_size);
                        let text_x = icon_x + icon_size + 4.0 * s;
                        self.draw_tree_label_clipped(
                            &row.name,
                            text_x,
                            text_y,
                            (panel_x + panel_w - pad - text_x).max(0.0),
                            git_disabled_color(self.theme.fg, workspace_disabled, 0.38),
                            text_scale,
                            &mut label_scratch,
                        );
                    }
                }
                y += row_h;
                if row_collapsed {
                    collapsed_depth = Some(row.depth);
                }
            }
        }

        if !drew_any {
            let hint = if ide_panel.git.pending {
                "Git scan..."
            } else {
                "No changes"
            };
            let tw = self.measure_ui_width(hint, text_scale);
            self.draw_string_scaled(
                hint,
                panel_x + (panel_w - tw) / 2.0,
                list_y + 30.0 * s,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.45],
                text_scale,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let total_h = (y + scroll - list_y).max(0.0);
        if total_h > list_h {
            let max_s = (total_h - list_h).max(1.0);
            let ratio = (scroll / max_s).clamp(0.0, 1.0);
            let thumb_h = (list_h / total_h * (list_h - 8.0 * s)).max(20.0 * s);
            let thumb_y = list_y + 4.0 * s + ratio * (list_h - 8.0 * s - thumb_h);
            self.push_rounded_rect(
                panel_x + panel_w - 5.0 * s,
                thumb_y,
                3.0 * s,
                thumb_h,
                1.5 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        if ide_panel.git.graph_open {
            let divider_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::GitGraphResize,
                panel_x,
                graph_divider_y - 3.0 * s,
                panel_w,
                graph_divider_h + 6.0 * s,
                mx,
                my,
            );
            self.push_rect(
                panel_x,
                graph_divider_y,
                panel_w,
                1.0,
                [0.0, 0.0, 0.0, 0.22],
            );
            self.push_rect(
                panel_x,
                graph_divider_y,
                panel_w,
                if divider_hovered {
                    2.0
                } else {
                    graph_divider_h.max(1.0)
                },
                if divider_hovered {
                    [0.60, 0.35, 0.85, 0.4]
                } else {
                    [1.0, 1.0, 1.0, 0.10]
                },
            );
            self.draw_git_graph_panel(
                panel_x,
                panel_w,
                graph_y,
                graph_h,
                pad,
                s,
                ide_panel,
                ui_registry,
                mx,
                my,
                &mut label_scratch,
            );
        }

        if ide_panel.git.commit_menu_open && !ide_panel.git.pending {
            let menu_w = inner_w.min(230.0 * s).max(120.0 * s).min(panel_w);
            let menu_x = (panel_x + pad + inner_w - menu_w).max(panel_x + 2.0 * s);
            let item_h = 32.0 * s;
            let menu_items = ["Commit", "Commit (Amend)", "Commit & Push"];
            let menu_h = item_h * menu_items.len() as f32 + 8.0 * s;
            let menu_y = commit_y + 30.0 * s;
            self.push_rounded_rect(
                menu_x,
                menu_y,
                menu_w,
                menu_h,
                8.0 * s,
                [0.18, 0.19, 0.25, 0.98],
            );
            self.push_rect(
                menu_x,
                menu_y + item_h * 2.0 + 4.0 * s,
                menu_w,
                1.0,
                [1.0, 1.0, 1.0, 0.14],
            );
            for (idx, label) in menu_items.iter().enumerate() {
                let item_y = menu_y + 4.0 * s + idx as f32 * item_h;
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::GitCommitMenuItem(idx),
                    menu_x,
                    item_y,
                    menu_w,
                    item_h,
                    mx,
                    my,
                );
                if hovered {
                    self.push_rounded_rect(
                        menu_x + 5.0 * s,
                        item_y + 3.0 * s,
                        menu_w - 10.0 * s,
                        item_h - 6.0 * s,
                        5.0 * s,
                        [1.0, 1.0, 1.0, 0.07],
                    );
                }
                self.draw_tree_label_clipped(
                    label,
                    menu_x + 16.0 * s,
                    item_y + item_h / 2.0 + 5.5 * s,
                    menu_w - 32.0 * s,
                    self.theme.fg,
                    0.9,
                    &mut label_scratch,
                );
            }
        }

        self.git_file_tooltip = git_file_tooltip;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_bottom_panel(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
        _is_ui_disabled: bool,
    ) {
        let sb_w = 48.0 * s;
        let panel_x = sb_w;
        let panel_y = ide_bottom_panel_y(self.height, panel_bottom_h, s);
        let panel_w = self.width - panel_x;

        let uses_translucent_bg = ide_panel.slots.iter().any(|sl| {
            sl.group == crate::app::PanelGroup::Bottom
                && sl.open
                && (sl.id == crate::app::PanelId::Terminal
                    || sl.id == crate::app::PanelId::Problems)
        });
        // Прозрачность терминала/ляпов (0.0 - полностью прозрачный, 1.0 - непрозрачный)
        let panel_alpha = if uses_translucent_bg { 0.80 } else { 1.0 };

        let panel_bg = [
            0.129, // #21
            0.133, // #22
            0.173, // #2c
            panel_alpha,
        ];
        // Ручка ресайза (1px линия вверху панели)self.push_rect(panel_x, panel_y, panel_w, 1.0,[1.0, 1.0, 1.0, 0.15]);
        self.push_rect(
            panel_x,
            panel_y + 1.0,
            panel_w,
            panel_bottom_h - 1.0,
            panel_bg,
        );

        let blocked = ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            panel_x,
            panel_y,
            panel_w,
            panel_bottom_h,
            mx,
            my,
        );
        if blocked {
            ui_registry.reset_cursor_state();
        }

        let tab_h = 32.0 * s;
        let tab_bar_bg = [
            (self.theme.bg[0] + 0.07).min(1.0),
            (self.theme.bg[1] + 0.07).min(1.0),
            (self.theme.bg[2] + 0.08).min(1.0),
            panel_alpha,
        ];
        self.push_rect(panel_x, panel_y + 1.0, panel_w, tab_h, tab_bar_bg);

        let mut tx = panel_x + 8.0 * s;
        for (i, slot) in ide_panel
            .slots
            .iter()
            .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
            .enumerate()
        {
            let label = slot.id.label();
            let tw = self.measure_ui_width(label, 0.9) + 20.0 * s;
            if i == 0 {
                let act_bg = [
                    (self.theme.bg[0] + 0.12).min(1.0),
                    (self.theme.bg[1] + 0.12).min(1.0),
                    (self.theme.bg[2] + 0.13).min(1.0),
                    1.0,
                ];
                self.push_rect(tx, panel_y + 1.0, tw, tab_h, act_bg);
                self.push_rect(tx, panel_y + tab_h - 1.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
            }
            self.draw_string_scaled(
                label,
                tx + 10.0 * s,
                panel_y + 1.0 + tab_h / 2.0 + 5.5 * s,
                self.theme.fg,
                0.9,
            );
            tx += tw;
        }

        // Подсветка ручки ресайза при наведении (wants_pointer=false — курсор через NsResize)
        if my >= panel_y - 8.0 * s && my <= panel_y + 8.0 * s && mx >= panel_x {
            self.push_rect(panel_x, panel_y, panel_w, 2.0, [0.60, 0.35, 0.85, 0.4]);
        }

        // Плейсхолдер контента
        let content_y = panel_y + 1.0 + tab_h;
        let content_h = panel_bottom_h - 1.0 - tab_h;
        if content_h > 8.0 * s {
            if let Some(slot) = ide_panel
                .slots
                .iter()
                .find(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
            {
                if slot.id == crate::app::PanelId::LspServers {
                    self.draw_lsp_servers_panel(
                        panel_x,
                        content_y,
                        panel_w,
                        content_h,
                        s,
                        ide_panel,
                        lsp_has_diagnostics,
                        ui_registry,
                    );
                } else if slot.id == crate::app::PanelId::Problems {
                    self.draw_problems_panel(
                        panel_x,
                        content_y,
                        panel_w,
                        content_h,
                        s,
                        lsp,
                        ide_panel,
                        ui_registry,
                    );
                } else if slot.id == crate::app::PanelId::Terminal {
                    self.draw_terminal_panel(
                        panel_x,
                        content_y,
                        panel_w,
                        content_h,
                        s,
                        ide_panel,
                        ui_registry,
                        mx,
                        my,
                    );
                } else {
                    let label = slot.id.label();
                    let lw = self.measure_ui_width(label, 0.85);
                    let col = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.18];
                    self.draw_string_scaled(
                        label,
                        panel_x + (panel_w - lw) / 2.0,
                        content_y + content_h / 2.0 + 6.0 * s,
                        col,
                        0.85,
                    );
                }
            }
        }
    }

    pub(crate) fn draw_status_bar(
        &mut self,
        editor: &crate::editor::Editor,
        editor_path: Option<&std::path::PathBuf>,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
        git_progress_label: Option<&str>,
        git_progress_elapsed_secs: Option<f32>,
    ) {
        let bar_h = ide_status_bar_height(s).round();
        let bar_y = ide_status_bar_y(self.height, panel_bottom_h, s).round();
        let bar_x = (48.0 * s).round();
        let bar_w = (self.width - bar_x).max(0.0);
        if bar_w <= 1.0 || bar_h <= 1.0 {
            return;
        }

        self.push_rect(bar_x, bar_y, bar_w, bar_h, [0.118, 0.125, 0.165, 1.0]);
        self.push_rect(
            bar_x,
            bar_y,
            bar_w,
            1.0,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.12],
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::StatusBar,
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            mx,
            my,
        );

        let (error_count, warning_count) = lsp
            .map(|l| diagnostic_error_warning_counts(l.diagnostics.values().map(|v| v.as_slice())))
            .unwrap_or((0, 0));

        let icon_sz = 20.0 * s;
        let text_scale = 0.95;
        let pad_x = 10.0 * s;
        let icon_gap = 5.0 * s;
        let item_gap = 16.0 * s;
        let diag_x = bar_x + pad_x;
        let icon_y = bar_y + (bar_h - icon_sz) / 2.0;
        let text_y = bar_y + bar_h / 2.0 + 5.0 * s;

        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
        let error_w = self.measure_ui_width(&scratch, text_scale).round();
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
        let warning_w = self.measure_ui_width(&scratch, text_scale).round();

        let diagnostics_w =
            icon_sz + icon_gap + error_w + item_gap + icon_sz + icon_gap + warning_w + pad_x;
        let diagnostics_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::StatusDiagnostics,
            diag_x - 4.0 * s,
            bar_y,
            diagnostics_w,
            bar_h,
            mx,
            my,
        );
        if diagnostics_hovered {
            self.push_rect(
                diag_x - 4.0 * s,
                bar_y,
                diagnostics_w,
                bar_h,
                [1.0, 1.0, 1.0, 0.07],
            );
        }

        self.draw_atlas_icon(
            crate::widgets::IconType::Error,
            diag_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
        let error_text_x = diag_x + icon_sz + icon_gap;
        self.draw_string_scaled(&scratch, error_text_x, text_y, self.theme.fg, text_scale);

        let warn_icon_x = error_text_x + error_w + item_gap;
        self.draw_atlas_icon(
            crate::widgets::IconType::Warning,
            warn_icon_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
        self.draw_string_scaled(
            &scratch,
            warn_icon_x + icon_sz + icon_gap,
            text_y,
            self.theme.fg,
            text_scale,
        );

        let ext = editor_path
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let lang = language_display_name_for_ext(ext);
        scratch.clear();
        scratch.push_str(lang);
        let lang_w = self.measure_ui_width(&scratch, text_scale).round();
        let lang_x = (bar_x + bar_w - pad_x - lang_w).max(diag_x);
        self.draw_string_scaled(&scratch, lang_x, text_y, self.theme.fg, text_scale);

        let (line, character) = cursor_line_and_character(editor);
        const ZERO_SAMPLE: &str = "00000000000000000000";
        let item_gap = 14.0 * s;
        let digit_gap = 4.0 * s;
        let line_digits = line.to_string();
        let char_digits = character.to_string();
        let line_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..line_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let char_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..char_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let line_label_w = self.measure_ui_width("Стр", text_scale).round();
        let char_label_w = self.measure_ui_width("Сим", text_scale).round();
        let line_block_w = line_label_w + digit_gap + line_digits_w;
        let char_block_w = char_label_w + digit_gap + char_digits_w;
        let selected_count = selected_char_count(editor);
        let selected_count_digits = selected_count.map(|count| count.to_string());
        let selected_block_w = selected_count_digits
            .as_ref()
            .map(|digits| {
                self.measure_ui_width("(", text_scale).round()
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round()
                    + self.measure_ui_width(" выделено)", text_scale).round()
            })
            .unwrap_or(0.0);
        let pos_color = self.theme.fg;
        let mut group_w = line_block_w + item_gap + char_block_w;
        if selected_block_w > 0.0 {
            group_w += item_gap + selected_block_w;
        }
        let line_x = lang_x - 22.0 * s - group_w;
        if let Some(label) = git_progress_label {
            let label_w = self.measure_ui_width(label, 0.82).round();
            let progress_gap = 8.0 * s;
            let track_w = 74.0 * s;
            let track_h = 5.0 * s;
            let progress_w = label_w + progress_gap + track_w;
            let progress_x = line_x - 18.0 * s - progress_w;
            if progress_x > diag_x + diagnostics_w + 8.0 * s {
                let track_x = progress_x + label_w + progress_gap;
                let track_y = bar_y + (bar_h - track_h) / 2.0;
                self.draw_string_scaled(
                    label,
                    progress_x,
                    text_y,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.72],
                    0.82,
                );
                self.push_rounded_rect(
                    track_x,
                    track_y,
                    track_w,
                    track_h,
                    track_h / 2.0,
                    [1.0, 1.0, 1.0, 0.10],
                );
                let thumb_w = (28.0 * s).min(track_w);
                let phase = git_progress_elapsed_secs
                    .map(git_progress_thumb_phase)
                    .unwrap_or(1.0);
                self.push_rounded_rect(
                    track_x + (track_w - thumb_w) * phase,
                    track_y,
                    thumb_w,
                    track_h,
                    track_h / 2.0,
                    [0.60, 0.35, 0.85, 0.88],
                );
            }
        }
        if line_x > diag_x + diagnostics_w + 8.0 * s {
            self.draw_string_scaled("Стр", line_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &line_digits,
                line_x + line_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            let char_x = line_x + line_block_w + item_gap;
            self.draw_string_scaled("Сим", char_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &char_digits,
                char_x + char_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            if let Some(digits) = selected_count_digits.as_deref() {
                let selected_x = char_x + char_block_w + item_gap;
                self.draw_string_scaled("(", selected_x, text_y, pos_color, text_scale);
                let digit_x = selected_x + self.measure_ui_width("(", text_scale).round();
                self.draw_string_mono_scaled(digits, digit_x, text_y, pos_color, text_scale);
                let suffix_x = digit_x
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round();
                self.draw_string_scaled(" выделено)", suffix_x, text_y, pos_color, text_scale);
            }
        }

        if diagnostics_hovered {
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(
                &mut scratch,
                format_args!(
                    "Ляпы: {} ошибок, {} предупреждений",
                    error_count, warning_count
                ),
            );
            let tip_w = self.measure_ui_width(&scratch, text_scale).round() + 16.0 * s;
            let tip_h = 24.0 * s;
            let tip_x = (diag_x - 4.0 * s)
                .min(self.width - tip_w - 6.0 * s)
                .max(6.0 * s);
            let tip_y = (bar_y - tip_h - 6.0 * s).max(6.0 * s);
            self.push_rounded_rect_border(
                tip_x,
                tip_y,
                tip_w,
                tip_h,
                5.0 * s,
                1.0,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.18],
                [0.08, 0.085, 0.115, 0.96],
            );
            self.draw_string_scaled(
                &scratch,
                tip_x + 8.0 * s,
                tip_y + 18.0 * s,
                self.theme.fg,
                text_scale,
            );
        }

        self.scratch_buffer = scratch;
    }

    fn draw_file_tree_dialog_shell(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let border = 2.0 * s;
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            10.0 * s,
            border,
            self.theme.sel,
            [0.15, 0.16, 0.20, 1.0],
        );
    }

    fn draw_file_tree_dialog_input(
        &mut self,
        editor: &crate::editor::Editor,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        blink_alpha: f32,
    ) {
        let s = self.scale_factor;
        let text_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let pad_x = 8.0 * s;
        let text_y = input_y + 23.0 * s;
        let text_start_x = input_x + pad_x;
        let visible_width = (input_w - pad_x * 2.0).max(0.0);

        self.push_rounded_rect(
            input_x,
            input_y,
            input_w,
            input_h,
            5.0 * s,
            [0.08, 0.09, 0.12, 1.0],
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(
                input_x as i32,
                scissor_y as i32,
                input_w as i32,
                input_h as i32,
            );

            let text = editor.get_full_text();
            let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                &text,
                editor.cursor,
                visible_width,
                |c| {
                    let char_to_render = if c == '\n' { '↵' } else { c };
                    self.get_ui_glyph(char_to_render)
                        .map(|g| g.advance * text_scale)
                        .unwrap_or(10.0 * text_scale)
                },
            );

            let sel_start = editor
                .selection_anchor
                .unwrap_or(editor.cursor)
                .min(editor.cursor);
            let sel_end = editor
                .selection_anchor
                .unwrap_or(editor.cursor)
                .max(editor.cursor);

            let mut current_x = text_start_x - scroll_x;
            let mut byte_idx = 0usize;
            let mut cursor_draw_x = current_x;
            for c in text.chars() {
                if byte_idx == editor.cursor {
                    cursor_draw_x = current_x;
                }

                let char_to_render = if c == '\n' { '↵' } else { c };
                let adv = self
                    .get_ui_glyph(char_to_render)
                    .map(|g| g.advance * text_scale)
                    .unwrap_or(10.0 * text_scale);

                if byte_idx >= sel_start && byte_idx < sel_end {
                    self.push_rect(
                        current_x,
                        input_y + 7.0 * s,
                        adv,
                        input_h - 14.0 * s,
                        self.theme.sel,
                    );
                }

                if current_x + adv >= input_x && current_x <= input_x + input_w {
                    if let Some(g) = self.get_ui_glyph(char_to_render) {
                        self.push_quad(
                            current_x + g.offset_x * text_scale,
                            text_y - g.offset_y * text_scale,
                            g.width * text_scale,
                            g.height * text_scale,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                }

                current_x += adv;
                byte_idx += c.len_utf8();
            }
            if byte_idx == editor.cursor {
                cursor_draw_x = current_x;
            }

            if sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x,
                    input_y + 7.0 * s,
                    2.0 * s,
                    input_h - 14.0 * s,
                    self.theme.fg,
                );
            }

            self.flush();
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    pub(crate) fn draw_file_tree_overlays(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let s = self.scale_factor;
        let mut wants_pointer = false;
        let mut label_scratch = String::new();
        if crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel) {
            ui_registry.mark_overlay_start();
            ui_registry.reset_cursor_state();
        }

        if let Some(menu) = &ide_panel.file_tree_context_menu {
            let row_h = 28.0 * s;
            let pad_x = 12.0 * s;
            let border = 2.0 * s;
            let separator_h = 8.0 * s;
            let mut menu_w = 190.0 * s;
            for action in &menu.entries {
                menu_w = menu_w.max(self.measure_ui_width(action.label(), 0.88) + pad_x * 2.0);
            }
            let menu_h = menu.entries.len() as f32 * row_h
                + file_tree_menu_separator_count(&menu.entries) as f32 * separator_h
                + border * 2.0;
            let x = menu.x.min((self.width - menu_w - 6.0 * s).max(6.0 * s));
            let y = menu.y.min((self.height - menu_h - 6.0 * s).max(6.0 * s));
            let anim_progress = crate::app::file_tree::file_tree_context_menu_anim_progress(
                menu.opened_at,
                std::time::Instant::now(),
            );
            let visible_h = (menu_h * anim_progress).max(border * 2.0);
            self.push_rounded_rect_border(
                x,
                y,
                menu_w,
                visible_h,
                6.0 * s,
                border,
                self.theme.sel,
                [0.09, 0.10, 0.14, 1.0],
            );

            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let sy = (self.height - (y + visible_h)).round() as i32;
                self.gl.scissor(
                    x.round() as i32,
                    sy,
                    menu_w.round() as i32,
                    visible_h.round() as i32,
                );
            }

            let mut row_y = y + border;
            let visible_bottom = y + visible_h;
            for (idx, action) in menu.entries.iter().enumerate() {
                if file_tree_menu_separator_before(&menu.entries, idx) {
                    let line_y = row_y + separator_h / 2.0;
                    self.push_rect(
                        x + border + pad_x,
                        line_y.round(),
                        menu_w - border * 2.0 - pad_x * 2.0,
                        1.0,
                        [1.0, 1.0, 1.0, 0.16],
                    );
                    row_y += separator_h;
                }
                if row_y >= visible_bottom {
                    break;
                }
                let visible_row_h = (visible_bottom - row_y).min(row_h).max(0.0);
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::FileTreeMenuItem(idx),
                    x,
                    row_y,
                    menu_w,
                    visible_row_h,
                    mx,
                    my,
                );
                if hovered {
                    wants_pointer = true;
                    self.push_rect(
                        x + border,
                        row_y,
                        menu_w - border * 2.0,
                        visible_row_h,
                        [1.0, 1.0, 1.0, 0.10],
                    );
                }
                self.draw_string_scaled(
                    action.label(),
                    x + pad_x,
                    row_y + row_h / 2.0 + 5.0 * s,
                    self.theme.fg,
                    0.88,
                );
                row_y += row_h;
            }
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_create_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                dialog.kind.title(),
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) =
                crate::app::file_tree::file_tree_path_input_layout(
                    x,
                    w,
                    s,
                    &dialog.parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                );
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            self.draw_string_scaled(
                &path_prefix,
                x + side_pad,
                input_y + 23.0 * s,
                [0.55, 0.57, 0.64, 1.0],
                path_scale,
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeCreateInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 112.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeCreateConfirm,
                    "Создать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeCreateCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_rename_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Переименовать",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) = if let Some(parent_dir) = dialog.path.parent() {
                crate::app::file_tree::file_tree_path_input_layout(x, w, s, parent_dir, |text| {
                    self.measure_ui_width(text, path_scale)
                })
            } else {
                (String::new(), x + side_pad, w - side_pad * 2.0)
            };
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            if !path_prefix.is_empty() {
                self.draw_string_scaled(
                    &path_prefix,
                    x + side_pad,
                    input_y + 23.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    path_scale,
                );
            }
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeRenameInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 130.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeRenameConfirm,
                    "Переименовать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeRenameCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_move_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Подтвердить перемещение",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_move_dialog_message(
                &dialog.sources,
                &dialog.target_dir,
            );
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeMoveConfirm,
                    "Переместить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeMoveCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить в корзину",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_delete_dialog_message(&dialog.paths);
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeDeleteConfirm,
                    "В корзину",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeDeleteCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.git.confirm_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 40.0) * s).min(self.width - 32.0 * s);
            let visible_files = dialog.files.len().min(7);
            let h = (172.0 * s + visible_files as f32 * 20.0 * s).min(self.height - 32.0 * s);
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);

            let (title, message, confirm_label) = match dialog.action {
                crate::app::git_panel::GitConfirmAction::RollbackStaged => (
                    "Откатить staged файлы",
                    "Отменить staged изменения в выбранных файлах?",
                    "Откатить",
                ),
            };
            self.draw_string_scaled(title, x + side_pad, y + 38.0 * s, self.theme.fg, 1.0);
            self.draw_string_scaled(
                message,
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );

            let list_x = x + side_pad;
            let list_y = y + 92.0 * s;
            let list_w = w - side_pad * 2.0;
            for (idx, file) in dialog.files.iter().take(visible_files).enumerate() {
                self.draw_tree_label_clipped(
                    file.display_path.as_str(),
                    list_x,
                    list_y + idx as f32 * 20.0 * s,
                    list_w,
                    [0.72, 0.76, 0.88, 1.0],
                    0.82,
                    &mut label_scratch,
                );
            }
            if dialog.files.len() > visible_files {
                let more = format!("+{} more", dialog.files.len() - visible_files);
                self.draw_string_scaled(
                    &more,
                    list_x,
                    list_y + visible_files as f32 * 20.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::GitConfirmAction,
                    confirm_label,
                    ok_x,
                ),
                (crate::ui_system::UiId::GitConfirmCancel, "Отмена", cancel_x),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        wants_pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start_idx = source.find(start).expect("start marker exists");
        let tail = &source[start_idx..];
        let end_idx = tail.find(end).expect("end marker exists");
        &tail[..end_idx]
    }

    #[test]
    fn clipped_label_prefix_len_reserves_ellipsis_and_keeps_utf8_boundary() {
        assert_eq!(clipped_label_prefix_len("abcdef", 38.0, 8.0, |_| 10.0), 3);
        assert_eq!(
            clipped_label_prefix_len("абвг", 18.0, 8.0, |_| 5.0),
            "аб".len()
        );
        assert_eq!(clipped_label_prefix_len("abc", 4.0, 8.0, |_| 3.0), 0);
    }

    #[test]
    fn git_graph_row_layout_shifts_text_for_many_lanes() {
        let one_lane = [crate::app::git_panel::GitGraphLane {
            column: 0,
            target_column: 0,
            color_idx: 0,
            kind: crate::app::git_panel::GitGraphLaneKind::VerticalTop,
        }];
        let six_lane = [
            crate::app::git_panel::GitGraphLane {
                column: 0,
                target_column: 0,
                color_idx: 0,
                kind: crate::app::git_panel::GitGraphLaneKind::VerticalTop,
            },
            crate::app::git_panel::GitGraphLane {
                column: 0,
                target_column: 5,
                color_idx: 5,
                kind: crate::app::git_panel::GitGraphLaneKind::Parent,
            },
        ];

        let one = git_graph_row_layout(10.0, 8.0, 1.0, 0, &one_lane);
        let six = git_graph_row_layout(10.0, 8.0, 1.0, 0, &six_lane);
        let last_lane_x = six.lane_start_x + six.lane_step * 5.0;

        let one_commit_far_right = git_graph_row_layout(10.0, 8.0, 1.0, 5, &one_lane);

        assert!((one.lane_step - 18.0).abs() < 0.001);
        assert!((six.lane_step - 18.0).abs() < 0.001);
        assert!(six.text_x > one.text_x);
        assert!(six.text_x > last_lane_x + 6.0);
        assert_eq!(six.text_x, one_commit_far_right.text_x);
    }

    #[test]
    fn branch_chip_y_uses_text_visual_center() {
        assert_eq!(branch_chip_y_from_text_center(34.0, 18.0), 25.0);
        assert_eq!(branch_chip_y_from_text_center(34.5, 19.0), 25.0);
        assert_eq!(branch_chip_width(50.0, 5.0, 140.0), 60.0);
        assert_eq!(branch_chip_width(160.0, 5.0, 140.0), 140.0);
    }

    #[test]
    fn git_graph_render_shift_to_commit_has_no_bottom_tail() {
        let source = include_str!("ide_panels.rs");
        let body = source_between(
            source,
            "fn push_git_graph_shift_to_commit_segment",
            "#[allow(clippy::too_many_arguments)]",
        );

        assert!(body.contains("let mid_x = to_x - dir * radius;"));
        assert!(body.contains("self.push_git_graph_horizontal_segment("));
        assert!(!body.contains("turn_out_y"));
        assert!(!body.contains("row_y + row_h,"));
    }

    #[test]
    fn git_graph_render_soft_vertical_preserves_lane_alpha() {
        let source = include_str!("ide_panels.rs");
        let body = source_between(
            source,
            "fn push_git_graph_soft_vertical_segment",
            "fn push_git_graph_parent_segment",
        );

        assert!(body.contains("self.push_git_graph_sdf_segment(x, top, x, bottom, width, color);"));
        assert!(!body.contains("color[3]"));
    }

    #[test]
    fn centered_dialog_button_positions_keep_pair_centered() {
        let (ok_x, cancel_x) = centered_dialog_button_positions(100.0, 420.0, 112.0, 10.0);

        assert_eq!(ok_x, 193.0);
        assert_eq!(cancel_x, 315.0);
        assert_eq!((ok_x + cancel_x + 112.0) / 2.0, 310.0);
    }

    #[test]
    fn git_row_hover_stays_visual_even_when_stage_click_is_locked() {
        assert!(git_row_visual_hovered(
            84.0, 128.0, 48.0, 112.0, 260.0, 28.0, false
        ));
        assert!(git_row_visual_hovered(
            260.0, 128.0, 48.0, 112.0, 260.0, 28.0, false
        ));
        assert!(!git_row_visual_hovered(
            84.0, 128.0, 48.0, 112.0, 260.0, 28.0, true
        ));

        assert!(git_file_row_hitbox_enabled(false));
        assert!(git_file_row_hitbox_enabled(true));
        assert!(!git_file_tooltip_hovered(true, 100.0, 100.0, 16.0));
        assert!(!git_file_tooltip_hovered(true, 116.0, 100.0, 16.0));
        assert!(git_file_tooltip_hovered(true, 116.1, 100.0, 16.0));
        assert!(!git_file_tooltip_hovered(false, 140.0, 100.0, 16.0));

        assert!(git_folder_stage_hitbox_enabled(Some(
            crate::app::git_panel::GitFolderStageState::All
        )));
        assert!(git_folder_stage_hitbox_enabled(Some(
            crate::app::git_panel::GitFolderStageState::Empty
        )));
        assert!(!git_folder_stage_hitbox_enabled(None));
    }

    #[test]
    fn git_disabled_color_dims_folder_text_alpha_only() {
        assert_eq!(
            git_disabled_color([0.2, 0.3, 0.4, 1.0], true, 0.38),
            [0.2, 0.3, 0.4, 0.38]
        );
        assert_eq!(
            git_disabled_color([0.2, 0.3, 0.4, 1.0], false, 0.38),
            [0.2, 0.3, 0.4, 1.0]
        );
    }

    #[test]
    fn git_progress_thumb_phase_ping_pongs_without_jump() {
        let cycle = 1.0 / GIT_PROGRESS_CYCLES_PER_SEC;
        assert!(git_progress_thumb_phase(0.0).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.25) - 0.5).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.5) - 1.0).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.75) - 0.5).abs() < 0.001);
        assert!(git_progress_thumb_phase(cycle).abs() < 0.001);
    }

    #[test]
    fn git_stage_controls_disable_for_pending_or_inactive_workspace() {
        assert!(git_stage_controls_disabled(true, false));
        assert!(git_stage_controls_disabled(false, true));
        assert!(git_stage_controls_disabled(true, true));
        assert!(!git_stage_controls_disabled(false, false));
    }

    #[test]
    fn git_checkbox_color_keeps_staged_state_visible_when_disabled() {
        let (active_bg, active_mark) = git_checkbox_color(true, false, false);
        let (disabled_bg, disabled_mark) = git_checkbox_color(true, false, true);

        assert_eq!(&active_bg[..3], &[0.48, 0.82, 0.52]);
        assert_eq!(&disabled_bg[..3], &[0.48, 0.82, 0.52]);
        assert!(disabled_bg[3] > 0.0 && disabled_bg[3] < active_bg[3]);
        assert!(disabled_mark[3] > 0.0 && disabled_mark[3] < active_mark[3]);

        let (partial_bg, partial_mark) = git_checkbox_color(false, true, true);
        assert!(partial_bg[3] > 0.0);
        assert!(partial_mark[3] > 0.0);
    }

    #[test]
    fn git_file_tooltip_uses_tilde_path_and_status_word() {
        let file = crate::app::git_panel::GitFileEntry {
            workspace_idx: 0,
            repo_root: std::path::PathBuf::from("/home/reyan/projects/rriter"),
            rel_path: "src/main.rs".to_string(),
            old_rel_path: None,
            display_path: "src/main.rs".to_string(),
            depth: 1,
            staged: false,
            status: crate::app::git_panel::GitFileStatus::Modified,
        };

        assert_eq!(
            git_file_tooltip_text(&file, Some(std::path::Path::new("/home/reyan"))),
            "~/projects/rriter/src/main.rs • Изменен"
        );
        assert_eq!(
            git_status_word(crate::app::git_panel::GitFileStatus::Deleted),
            "Удален"
        );
        assert_eq!(
            git_status_word(crate::app::git_panel::GitFileStatus::Untracked),
            "Не отслеживается"
        );
    }

    #[test]
    fn git_file_tooltip_timer_stays_active_while_mouse_moves_in_same_target() {
        let target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 2,
        };
        let other_target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 3,
        };
        let now = std::time::Instant::now();

        git_tooltip_reset();
        assert_eq!(git_tooltip_anchor(target, 10.0, 20.0, now), None);
        assert_eq!(
            git_tooltip_anchor(
                target,
                30.0,
                40.0,
                now + std::time::Duration::from_millis(200)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                50.0,
                60.0,
                now + std::time::Duration::from_millis(450)
            ),
            Some((10.0, 20.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                70.0,
                80.0,
                now + std::time::Duration::from_millis(500)
            ),
            Some((10.0, 20.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                other_target,
                90.0,
                100.0,
                now + std::time::Duration::from_millis(550)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                other_target,
                110.0,
                120.0,
                now + std::time::Duration::from_millis(1000)
            ),
            Some((90.0, 100.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                130.0,
                140.0,
                now + std::time::Duration::from_millis(1100)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                150.0,
                160.0,
                now + std::time::Duration::from_millis(1550)
            ),
            Some((130.0, 140.0))
        );
        git_tooltip_reset();
    }

    #[test]
    fn git_tooltip_reset_requires_new_dwell_after_scroll() {
        let target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 2,
        };
        let now = std::time::Instant::now();

        git_tooltip_reset();
        assert_eq!(git_tooltip_anchor(target, 10.0, 20.0, now), None);
        assert_eq!(
            git_tooltip_anchor(
                target,
                10.0,
                20.0,
                now + std::time::Duration::from_millis(450)
            ),
            Some((10.0, 20.0))
        );

        git_tooltip_reset();
        assert_eq!(
            git_tooltip_anchor(
                target,
                10.0,
                20.0,
                now + std::time::Duration::from_millis(500)
            ),
            None
        );
        git_tooltip_reset();
    }

    #[test]
    fn git_folder_row_layout_uses_equal_gaps_and_centered_icon() {
        let layout = git_folder_row_layout(80.0, 40.0, 28.0, 1.0);
        let arrow_to_check =
            layout.check_x - (layout.arrow_x + crate::render_view::tree_ui::TREE_DISCLOSURE_SLOT);
        let check_to_icon = layout.icon_x - (layout.check_x + layout.check_size);

        assert_eq!(arrow_to_check, 6.0);
        assert_eq!(check_to_icon, 6.0);
        assert_eq!(layout.check_y, 48.0);
        assert_eq!(layout.icon_y, 44.0);
    }

    #[test]
    fn git_file_row_layout_draws_icon_between_checkbox_and_label() {
        let layout = git_file_row_layout(98.0, 40.0, 28.0, 1.0);
        let parent_folder_layout = git_folder_row_layout(80.0, 40.0, 28.0, 1.0);

        assert_eq!(layout.check_x, 100.0);
        assert_eq!(layout.check_x, parent_folder_layout.check_x);
        assert_eq!(layout.check_y, 48.0);
        assert_eq!(layout.icon_x - (layout.check_x + layout.check_size), 6.0);
        assert_eq!(layout.icon_x, parent_folder_layout.icon_x);
        assert_eq!(layout.icon_y, 44.0);
        assert_eq!(layout.text_x - (layout.icon_x + layout.icon_size), 4.0);
    }

    #[test]
    fn file_tree_context_menu_groups_insert_logical_separators() {
        use crate::app::file_tree::FileTreeMenuAction;

        let entries = [
            FileTreeMenuAction::CreateFile,
            FileTreeMenuAction::CreateDirectory,
            FileTreeMenuAction::Paste,
            FileTreeMenuAction::Delete,
            FileTreeMenuAction::Rename,
            FileTreeMenuAction::OpenContainedFolder,
            FileTreeMenuAction::CopyRelativePath,
        ];

        assert!(!file_tree_menu_separator_before(&entries, 0));
        assert!(!file_tree_menu_separator_before(&entries, 2));
        assert!(file_tree_menu_separator_before(&entries, 3));
        assert!(file_tree_menu_separator_before(&entries, 5));
        assert_eq!(file_tree_menu_separator_count(&entries), 2);
    }
}
