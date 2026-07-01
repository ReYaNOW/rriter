use crate::render_view::{
    cursor_line_and_character, ide_bottom_panel_y, ide_status_bar_height, ide_status_bar_y,
    language_display_name_for_ext, selected_char_count,
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

fn draw_git_checkmark(
    renderer: &mut Renderer,
    check_x: f32,
    check_y: f32,
    check_size: f32,
    color: [f32; 4],
) {
    let icon_size = (check_size * 0.86).round().max(1.0);
    renderer.draw_atlas_icon(
        crate::widgets::IconType::Check,
        (check_x + (check_size - icon_size) * 0.5).round(),
        (check_y + (check_size - icon_size) * 0.5).round(),
        icon_size,
        color,
    );
}

fn render_git_action_button(
    renderer: &mut Renderer,
    ui_registry: &mut crate::ui_system::UiRegistry,
    id: crate::ui_system::UiId,
    button: &Button,
    workspace_disabled: bool,
    action_disabled: bool,
    mx: f32,
    my: f32,
    s: f32,
) -> bool {
    if workspace_disabled {
        render_git_disabled_button(renderer, button, s);
        register_git_locked_button_cursor(ui_registry, id, button, mx, my);
        false
    } else if action_disabled {
        button.render(renderer, -1.0, -1.0, s, false);
        register_git_locked_button_cursor(ui_registry, id, button, mx, my);
        false
    } else {
        ui_registry.register_button(id, button, renderer, mx, my, s, false)
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

fn git_file_tooltip_path(
    repo_root: &std::path::Path,
    file: &crate::app::git_panel::GitFileEntry,
) -> std::path::PathBuf {
    repo_root.join(file.rel_path.as_ref())
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
    repo_root: &std::path::Path,
    file: &crate::app::git_panel::GitFileEntry,
    home: Option<&std::path::Path>,
) -> String {
    let path = git_file_tooltip_path(repo_root, file);
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
