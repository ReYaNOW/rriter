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

fn dialog_button_text_baseline(btn_y: f32, btn_h: f32, scale: f32) -> f32 {
    (btn_y + btn_h * 0.5 + 5.0 * scale).round()
}

fn project_search_help_content_factor(dialog_h: f32, scale: f32) -> f32 {
    if scale <= 0.0 {
        return 1.0;
    }
    let reserved = 134.0 * scale;
    let desired_content_span = 255.0 * scale;
    ((dialog_h - reserved).max(0.0) / desired_content_span).clamp(0.45, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GitSimpleTooltipLayout {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text_w: f32,
}

fn git_simple_tooltip_layout(
    window_w: f32,
    window_h: f32,
    anchor_x: f32,
    anchor_y: f32,
    measured_text_w: f32,
    scale: f32,
) -> Option<GitSimpleTooltipLayout> {
    let margin = 8.0 * scale;
    let pad_x = 12.0 * scale;
    let max_w = (window_w - margin * 2.0).max(0.0);
    let max_h = (window_h - margin * 2.0).max(0.0);
    if max_w <= 1.0 || max_h <= 1.0 {
        return None;
    }
    let h = (30.0 * scale).min(max_h).max(1.0);
    let w = (measured_text_w + pad_x * 2.0).min(max_w).max(1.0);
    let preferred_x = anchor_x + 14.0 * scale;
    let preferred_y = anchor_y + 18.0 * scale;
    let x = if preferred_x + w <= window_w - margin {
        preferred_x
    } else {
        anchor_x - 14.0 * scale - w
    }
    .clamp(margin, (window_w - margin - w).max(margin));
    let y = if preferred_y + h <= window_h - margin {
        preferred_y
    } else {
        anchor_y - 10.0 * scale - h
    }
    .clamp(margin, (window_h - margin - h).max(margin));
    Some(GitSimpleTooltipLayout {
        x: x.round(),
        y: y.round(),
        w: w.round(),
        h: h.round(),
        text_w: (w - pad_x * 2.0).max(0.0).round(),
    })
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
        crate::app::file_tree::FileTreeMenuAction::ShowInExplorer => 2,
        crate::app::file_tree::FileTreeMenuAction::OpenContainedFolder
        | crate::app::file_tree::FileTreeMenuAction::CopyAbsolutePath
        | crate::app::file_tree::FileTreeMenuAction::CopyRelativePath
        | crate::app::file_tree::FileTreeMenuAction::CopyTargetAbsolutePath
        | crate::app::file_tree::FileTreeMenuAction::CopyTargetRelativePath => 3,
    }
}

fn file_tree_menu_separator_before(
    entries: &[crate::app::file_tree::FileTreeMenuAction],
    idx: usize,
) -> bool {
    idx > 0 && file_tree_menu_group(entries[idx - 1]) != file_tree_menu_group(entries[idx])
}


#[derive(Clone, Copy, Debug, PartialEq)]
struct GitCommitControlsLayout {
    commit: crate::ui_system::UiClipRect,
    menu: crate::ui_system::UiClipRect,
    options: crate::ui_system::UiClipRect,
}

fn git_commit_controls_layout(
    panel_x: f32,
    panel_w: f32,
    title_h: f32,
    scale: f32,
) -> GitCommitControlsLayout {
    let pad = (10.0 * scale).min((panel_w * 0.15).max(0.0));
    let inner_w = (panel_w - pad * 2.0).max(1.0);
    let arrow_w = (34.0 * scale).min((inner_w * 0.22).max(22.0 * scale));
    let options_w = (32.0 * scale).min((inner_w * 0.20).max(22.0 * scale));
    let gap = (4.0 * scale).min((inner_w * 0.06).max(0.0));
    let commit_w = (inner_w - arrow_w - options_w - gap * 2.0).max(1.0);
    let y = title_h + 44.0 * scale;
    let h = 28.0 * scale;
    let commit = crate::ui_system::UiClipRect::new(panel_x + pad, y, commit_w, h);
    let menu = crate::ui_system::UiClipRect::new(commit.x + commit.w + gap, y, arrow_w, h);
    let options = crate::ui_system::UiClipRect::new(menu.x + menu.w + gap, y, options_w, h);
    GitCommitControlsLayout {
        commit,
        menu,
        options,
    }
}

fn git_dropdown_anchor(
    button: crate::ui_system::UiClipRect,
    scale: f32,
) -> (f32, f32) {
    (
        button.x.round(),
        (button.y.round() + button.h.round() + 2.0 * scale).round(),
    )
}

fn git_top_panel_visible(ide_panel: &crate::app::IdePanelState) -> bool {
    ide_panel.slots.iter().any(|slot| {
        slot.id == crate::app::PanelId::Git
            && slot.group == crate::app::PanelGroup::Top
            && slot.open
    })
}

pub(crate) fn git_dropdown_overlay_active_for_panel(ide_panel: &crate::app::IdePanelState) -> bool {
    if !git_top_panel_visible(ide_panel) {
        return false;
    }
    let commit_controls_enabled = ide_panel.git.commit_enabled() && !ide_panel.git.pending;
    (commit_controls_enabled
        && (ide_panel.git.commit_menu_opened_at.is_some()
            || ide_panel.git.commit_options_menu_opened_at.is_some()))
        || (!ide_panel.git.pending
            && ide_panel.git.repo_action_menu_workspace_idx.is_some()
            && ide_panel.git.repo_action_menu_opened_at.is_some())
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

fn git_stage_checkbox_color(
    staged: bool,
    partial: bool,
    workspace_disabled: bool,
    git_pending: bool,
) -> ([f32; 4], [f32; 4]) {
    git_checkbox_color(
        staged,
        partial,
        git_stage_controls_disabled(workspace_disabled, git_pending),
    )
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
        let rest = rest.to_string_lossy().replace('\\', "/");
        return if rest.starts_with('/') {
            format!("~{rest}")
        } else {
            format!("~/{rest}")
        };
    }
    path.to_string_lossy().replace('\\', "/")
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

const DATABASE_DIALOG_FIELD_TEXT_SCALE: f32 = 0.82;
const DATABASE_DIALOG_SECONDARY_TEXT_SCALE: f32 = 0.78;
const DATABASE_DIALOG_ROW_H: f32 = 38.0;
const DATABASE_DIALOG_FORM_TOP: f32 = 48.0;
const DATABASE_DIALOG_SCROLLBAR_W: f32 = 8.0;
const DATABASE_DIALOG_SCROLLBAR_MARGIN: f32 = 5.0;
const DATABASE_DIALOG_MIN_THUMB_H: f32 = 24.0;
const DATABASE_DIALOG_EYE_VISUAL_RATIO: f32 = 0.86;
const DATABASE_DIALOG_TOOLTIP_NAMESPACE: u64 = 2u64 << 60;

#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabaseConnectionDialogLayout {
    modal: DatabaseModalGeometry,
    footer: DatabaseDialogFooterLayout,
    form_clip: crate::ui_system::UiClipRect,
    content_height: f32,
    max_scroll: f32,
    scrollbar_track: Option<crate::ui_system::UiClipRect>,
    row_h: f32,
    label_x: f32,
    label_w: f32,
    input_x: f32,
    input_w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabaseDialogFieldLayout {
    row_visible: bool,
    label: crate::ui_system::UiClipRect,
    input: crate::ui_system::UiClipRect,
    remember: Option<crate::ui_system::UiClipRect>,
    eye_hit: Option<crate::ui_system::UiClipRect>,
    eye_visual: Option<crate::ui_system::UiClipRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DatabaseDialogTooltipTarget {
    Field(crate::app::database::DatabaseFormField),
    Tls,
    Color,
    Ssh,
    Jump,
}

impl DatabaseDialogTooltipTarget {
    fn key(self) -> u64 {
        let local = match self {
            Self::Field(field) => crate::app::database::DatabaseFormField::ALL
                .iter()
                .position(|candidate| *candidate == field)
                .unwrap_or(0) as u64,
            Self::Tls => 32,
            Self::Color => 33,
            Self::Ssh => 34,
            Self::Jump => 35,
        };
        DATABASE_DIALOG_TOOLTIP_NAMESPACE | local
    }

    fn text(self) -> &'static str {
        use crate::app::database::DatabaseFormField as Field;
        match self {
            Self::Field(Field::DisplayName) => {
                "Понятное имя подключения, которое будет показано в панели баз данных."
            }
            Self::Field(Field::Host) => {
                "DNS-имя, IPv4/IPv6-адрес или доступный alias PostgreSQL-сервера. По умолчанию: localhost."
            }
            Self::Field(Field::Port) => "TCP-порт PostgreSQL. По умолчанию: 5432.",
            Self::Field(Field::Username) => "Имя пользователя PostgreSQL для аутентификации.",
            Self::Field(Field::PostgresPassword) => {
                "Пароль пользователя PostgreSQL. При включённом «Запомнить» сохраняется только в системном хранилище секретов."
            }
            Self::Field(Field::MaintenanceDatabase) => {
                "База, через которую RRiter загружает список доступных баз. По умолчанию: postgres."
            }
            Self::Field(Field::SshHost) => "DNS-имя или IP SSH-сервера, через который открывается туннель.",
            Self::Field(Field::SshPort) => "TCP-порт SSH-сервера. По умолчанию: 22.",
            Self::Field(Field::SshUsername) => "Имя пользователя для SSH-аутентификации.",
            Self::Field(Field::SshPassword) => {
                "Пароль SSH. Значение скрыто; сохранение выполняется только через системное хранилище секретов."
            }
            Self::Field(Field::SshPrivateKey) => "Путь к приватному ключу для SSH-аутентификации.",
            Self::Field(Field::SshKeyPassphrase) => {
                "Passphrase приватного SSH-ключа. Значение не выводится в tooltip или журнал."
            }
            Self::Field(Field::SshConfigAlias) => {
                "Alias из пользовательского SSH config; может задавать host и другие параметры подключения."
            }
            Self::Field(Field::JumpHost) => "DNS-имя или IP промежуточного Bastion SSH-сервера.",
            Self::Field(Field::JumpPort) => "TCP-порт Bastion SSH-сервера. По умолчанию: 22.",
            Self::Field(Field::JumpUsername) => "Имя пользователя для входа на Bastion SSH-сервер.",
            Self::Field(Field::JumpPassword) => {
                "Пароль Bastion SSH. Значение скрыто и не включается в tooltip."
            }
            Self::Field(Field::JumpPrivateKey) => "Путь к приватному ключу для входа на Bastion SSH-сервер.",
            Self::Field(Field::JumpKeyPassphrase) => {
                "Passphrase приватного ключа Bastion SSH. Значение не включается в tooltip."
            }
            Self::Field(Field::JumpConfigAlias) => "Alias Bastion-сервера из пользовательского SSH config.",
            Self::Tls => "Переключает режим TLS PostgreSQL: Disable, Prefer или Require.",
            Self::Color => "Выбирает цвет подключения в дереве Database Tools.",
            Self::Ssh => "Включает SSH-туннель между RRiter и PostgreSQL-сервером.",
            Self::Jump => "Включает промежуточный Bastion SSH-сервер; SSH будет включён автоматически.",
        }
    }
}

fn database_connection_dialog_layout(
    viewport_w: f32,
    viewport_h: f32,
    base_scale: f32,
    visible_rows: usize,
) -> DatabaseConnectionDialogLayout {
    let modal = database_modal_geometry(
        viewport_w,
        viewport_h,
        base_scale,
        700.0,
        780.0,
        420.0,
        420.0,
    );
    let s = modal.scale;
    let footer = database_dialog_footer_layout(modal.y, modal.h, s);
    let form_top = modal.y + DATABASE_DIALOG_FORM_TOP * s;
    let form_bottom = footer.form_bottom.max(form_top);
    let form_clip = crate::ui_system::UiClipRect::new(
        modal.x,
        form_top,
        modal.w,
        (form_bottom - form_top).max(0.0),
    );
    let row_h = DATABASE_DIALOG_ROW_H * s;
    let content_height = visible_rows as f32 * row_h;
    let max_scroll = (content_height - form_clip.h).max(0.0);
    let scrollbar_track = (max_scroll > 0.0 && form_clip.h > 2.0 * DATABASE_DIALOG_SCROLLBAR_MARGIN * s)
        .then(|| {
            crate::ui_system::UiClipRect::new(
                (modal.x + modal.w - (DATABASE_DIALOG_SCROLLBAR_W + DATABASE_DIALOG_SCROLLBAR_MARGIN) * s).round(),
                (form_clip.y + DATABASE_DIALOG_SCROLLBAR_MARGIN * s).round(),
                (DATABASE_DIALOG_SCROLLBAR_W * s).max(1.0).round(),
                (form_clip.h - 2.0 * DATABASE_DIALOG_SCROLLBAR_MARGIN * s).max(1.0).round(),
            )
        });
    let form_pad = (22.0 * s).min(modal.w * 0.08);
    let content_right = scrollbar_track
        .map(|track| track.x - 7.0 * s)
        .unwrap_or(modal.x + modal.w - form_pad);
    let inner_w = (content_right - (modal.x + form_pad)).max(1.0);
    let label_w = (198.0 * s).min(inner_w * 0.34);
    let label_x = modal.x + form_pad;
    let input_x = label_x + label_w;
    let input_w = (inner_w - label_w).max(1.0);
    DatabaseConnectionDialogLayout {
        modal,
        footer,
        form_clip,
        content_height,
        max_scroll,
        scrollbar_track,
        row_h,
        label_x,
        label_w: (label_w - 6.0 * s).max(1.0),
        input_x,
        input_w,
    }
}

fn database_dialog_field_layout(
    layout: &DatabaseConnectionDialogLayout,
    row: usize,
    scroll_y: f32,
    has_remember: bool,
    has_eye: bool,
) -> DatabaseDialogFieldLayout {
    let s = layout.modal.scale;
    let row_y = layout.form_clip.y + row as f32 * layout.row_h - scroll_y;
    let field_h = (28.0 * s).max(1.0).round();
    let field_y = (row_y + 4.0 * s).round();
    let desired_remember_w = if has_remember { 132.0 * s } else { 0.0 };
    let remember_w = desired_remember_w.min((layout.input_w - 80.0 * s).max(0.0));
    let field_w = (layout.input_w - remember_w).max(1.0).round();
    let input = crate::ui_system::UiClipRect::new(
        layout.input_x.round(),
        field_y,
        field_w,
        field_h,
    );
    let remember = (has_remember && remember_w >= 42.0 * s).then(|| {
        crate::ui_system::UiClipRect::new(
            (layout.input_x + field_w + 6.0 * s).round(),
            field_y,
            (remember_w - 6.0 * s).max(1.0).round(),
            field_h,
        )
    });
    let eye_hit = has_eye.then(|| {
        crate::ui_system::UiClipRect::new(
            (input.x + input.w - field_h).round(),
            field_y,
            field_h,
            field_h,
        )
    });
    let eye_visual = eye_hit.map(|hit| {
        let size = (hit.w * DATABASE_DIALOG_EYE_VISUAL_RATIO).round().clamp(1.0, hit.w);
        crate::ui_system::UiClipRect::new(
            (hit.x + (hit.w - size) * 0.5).round(),
            (hit.y + (hit.h - size) * 0.5).round(),
            size,
            size,
        )
    });
    DatabaseDialogFieldLayout {
        row_visible: row_y + layout.row_h >= layout.form_clip.y
            && row_y <= layout.form_clip.y + layout.form_clip.h,
        label: crate::ui_system::UiClipRect::new(
            layout.label_x.round(),
            field_y,
            layout.label_w,
            field_h,
        ),
        input,
        remember,
        eye_hit,
        eye_visual,
    }
}

fn database_connection_dialog_scrollbar_thumb(
    layout: &DatabaseConnectionDialogLayout,
    current_scroll: f32,
) -> Option<crate::scroll::ScrollbarThumb> {
    let track = layout.scrollbar_track?;
    crate::scroll::scrollbar_thumb(
        track.y,
        track.h,
        layout.form_clip.h,
        layout.content_height,
        current_scroll.clamp(0.0, layout.max_scroll),
        DATABASE_DIALOG_MIN_THUMB_H * layout.modal.scale,
    )
}

fn database_dialog_tooltip_rect(
    window_w: f32,
    window_h: f32,
    anchor_x: f32,
    anchor_y: f32,
    desired_w: f32,
    desired_h: f32,
    scale: f32,
) -> Option<crate::ui_system::UiClipRect> {
    let margin = 8.0 * scale;
    let max_w = (window_w - margin * 2.0).max(0.0);
    let max_h = (window_h - margin * 2.0).max(0.0);
    if max_w <= 1.0 || max_h <= 1.0 {
        return None;
    }
    let w = desired_w.min(max_w).max(1.0).round();
    let h = desired_h.min(max_h).max(1.0).round();
    let preferred_x = anchor_x + 10.0 * scale;
    let preferred_y = anchor_y + 8.0 * scale;
    let x = if preferred_x + w <= window_w - margin {
        preferred_x
    } else {
        anchor_x - 10.0 * scale - w
    }
    .clamp(margin, (window_w - margin - w).max(margin));
    let y = if preferred_y + h <= window_h - margin {
        preferred_y
    } else {
        anchor_y - 8.0 * scale - h
    }
    .clamp(margin, (window_h - margin - h).max(margin));
    Some(crate::ui_system::UiClipRect::new(x.round(), y.round(), w, h))
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn database_connection_dialog_scroll_metrics(
        &self,
        visible_rows: usize,
        current_scroll: f32,
    ) -> (
        crate::ui_system::UiClipRect,
        f32,
        f32,
        Option<(crate::ui_system::UiClipRect, crate::scroll::ScrollbarThumb)>,
    ) {
        let layout = database_connection_dialog_layout(
            self.width,
            self.height,
            self.scale_factor,
            visible_rows,
        );
        let scrollbar = layout.scrollbar_track.and_then(|track| {
            database_connection_dialog_scrollbar_thumb(&layout, current_scroll)
                .map(|thumb| (track, thumb))
        });
        (
            layout.form_clip,
            layout.row_h,
            layout.max_scroll,
            scrollbar,
        )
    }

    pub(crate) fn suppress_database_dialog_tooltip_after_click(&mut self) {
        self.suppress_popups_until_next_mouse_move();
        self.reset_delayed_tooltip_anchor_namespace(DATABASE_DIALOG_TOOLTIP_NAMESPACE);
    }

    fn draw_database_dialog_tooltip(
        &mut self,
        text: &str,
        anchor_x: f32,
        anchor_y: f32,
        s: f32,
    ) {
        let text_scale = crate::render_view::DATABASE_DIALOG_TOOLTIP_TEXT_SCALE;
        let pad_x = (12.0 * s).round();
        let pad_y = (9.0 * s).round();
        let line_h = (20.0 * s).round().max(16.0);
        let max_text_w = (420.0 * s)
            .min((self.width - 2.0 * (8.0 * s + pad_x)).max(1.0));
        let ranges = crate::render_view::core_text::wrapped_text_ranges(
            text,
            max_text_w,
            |ch| {
                self.get_ui_glyph(ch)
                    .map(|glyph| Self::snapped_text_advance(glyph.advance, text_scale))
                    .unwrap_or(8.0 * text_scale)
            },
        );
        let measured_w = ranges
            .iter()
            .map(|(start, end)| self.measure_ui_width(&text[*start..*end], text_scale))
            .fold(1.0f32, f32::max);
        let desired_w = measured_w + 2.0 * pad_x;
        let desired_h = ranges.len() as f32 * line_h + 2.0 * pad_y;
        let Some(rect) = database_dialog_tooltip_rect(
            self.width,
            self.height,
            anchor_x,
            anchor_y,
            desired_w,
            desired_h,
            s,
        ) else {
            return;
        };

        self.push_rounded_rect_border(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            6.0 * s,
            1.0,
            self.theme.sel,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                1.0,
            ],
        );
        let text_layout = crate::render_view::standard_tooltip_text_layout(
            rect.x,
            rect.y,
            pad_x,
            pad_y,
            line_h,
            line_h * 0.5 + 5.5 * s,
        );
        for (line, (start, end)) in ranges.into_iter().enumerate() {
            self.draw_standard_tooltip_text_line(
                &text[start..end],
                text_layout,
                line,
                self.theme.fg,
                text_scale,
            );
        }
    }
}
