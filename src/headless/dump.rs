//! Headless confirmation dialog (centered in the main frame instead of a second window)
//! and the `dump` command: a compact JSON snapshot of the UI state.

use crate::app::events::host_loop::HeadlessLoopState;
use crate::app::{App, MarkdownMode, PanelGroup, PanelId, PendingAction};
use crate::editor::Editor;
use crate::platform::ExternalRequest;
use crate::renderer::Renderer;
use crate::scroll::ScrollState;
use crate::widgets::Button;
use glow::HasContext;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

/// Logical dialog size, the same as the window branch's second window.
const DIALOG_W: f32 = 660.0;
const DIALOG_H: f32 = 260.0;
/// Dialog background, as the window branch clears its dialog surface.
const DIALOG_CLEAR: [f32; 4] = [0.12, 0.13, 0.22, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DialogLayout {
    pub(crate) ox: u32,
    pub(crate) oy_top: u32,
    pub(crate) dw: u32,
    pub(crate) dh: u32,
}

/// Dialog rectangle centered in a `w`x`h` buffer (top-left origin). A buffer smaller than
/// the dialog puts it at the corner; the viewport clips the rest.
pub(crate) fn dialog_layout(w: u32, h: u32, scale: f32) -> DialogLayout {
    // Same truncation as the window branch's `resize((660.0 * s) as u32, …)`.
    let dw = (DIALOG_W * scale) as u32;
    let dh = (DIALOG_H * scale) as u32;
    DialogLayout { ox: w.saturating_sub(dw) / 2, oy_top: h.saturating_sub(dh) / 2, dw, dh }
}

/// Save/discard/cancel buttons in buffer coordinates, from the same layout the dialog draws.
pub(crate) fn dialog_buttons(layout: DialogLayout, action: PendingAction, renderer: &mut Renderer) -> Vec<(&'static str, [f32; 4])> {
    let s = renderer.scale_factor;
    let (save, mut discard, cancel) =
        crate::widgets::get_dialog_buttons(0.0, 0.0, layout.dw as f32, layout.dh as f32, s, renderer);
    let (ox, oy) = (layout.ox as f32, layout.oy_top as f32);
    let rect = |button: &Button| [button.x + ox, button.y + oy, button.w, button.h];
    if action == PendingAction::ResetKeymap {
        discard.text = "Отмена".to_string();
        vec![("save", rect(&save)), ("cancel", rect(&discard))]
    } else {
        vec![("save", rect(&save)), ("discard", rect(&discard)), ("cancel", rect(&cancel))]
    }
}

/// Draws the dialog over the finished main frame of a `w`x`h` buffer: what the window
/// branch does on its dialog surface, inside a viewport over the centered rectangle.
pub(crate) fn draw_dialog(renderer: &mut Renderer, base_title: &str, action: PendingAction, w: u32, h: u32) {
    let layout = dialog_layout(w, h, renderer.scale_factor);
    if layout.dw == 0 || layout.dh == 0 {
        return;
    }
    let x = layout.ox as i32;
    // GL origin is the bottom-left corner.
    let y_gl = h as i32 - layout.oy_top as i32 - layout.dh as i32;
    renderer.resize_viewport(x, y_gl, layout.dw, layout.dh);
    unsafe {
        let gl = &renderer.gl;
        // A plain clear would wipe the whole main frame: limit it to the dialog.
        let scissor_on = gl.is_enabled(glow::SCISSOR_TEST);
        let mut scissor_box = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(x, y_gl, layout.dw as i32, layout.dh as i32);
        let [r, g, b, a] = DIALOG_CLEAR;
        gl.clear_color(r, g, b, a);
        gl.clear(glow::COLOR_BUFFER_BIT);
        let [sx, sy, sw, sh] = scissor_box;
        gl.scissor(sx, sy, sw, sh);
        if !scissor_on {
            gl.disable(glow::SCISSOR_TEST);
        }
    }
    renderer.draw_dialog_window(base_title, action);
    renderer.resize(w, h);
}

/// The last `about_to_wait` decision of the event loop and what would wake it next.
fn event_loop_json(app: &App, loop_state: &HeadlessLoopState, now: Instant) -> Value {
    let (control_flow, deadline_ms) =
        super::frame::control_flow_label(loop_state.last_control_flow.get(), now);
    let redraw_requested = app
        .window
        .as_ref()
        .and_then(|window| window.headless())
        .is_some_and(|window| window.redraw_requested());
    json!({
        "control_flow": control_flow,
        "deadline_ms": deadline_ms,
        "awaiting_background": loop_state.awaiting_background.get(),
        "wake_pending": app.ui_waker.is_pending(),
        "wake_events": app.ui_waker.queued_events(),
        "redraw_requested": redraw_requested,
    })
}

/// JSON snapshot for `dump`. Reading it clears the recorded external request.
pub(crate) fn dump_json(app: &mut App, loop_state: &HeadlessLoopState) -> Value {
    let (w, h) = app
        .window
        .as_ref()
        .map_or((0, 0), |window| {
            let size = window.inner_size();
            (size.width, size.height)
        });
    let dialog = dialog_json(app, w, h);
    let Some(renderer) = app.renderer.as_ref() else {
        return json!(null);
    };
    let scale = renderer.scale_factor;
    let atlas_stats = (
        renderer.alpha_atlas_resets,
        renderer.color_atlas_resets,
        renderer.atlas_y + renderer.max_row_h,
    );
    let mode = if app.show_welcome {
        "welcome"
    } else if app.is_ide_mode {
        "ide"
    } else {
        "editor"
    };
    let now = Instant::now();
    let panel = &app.ide_panel;
    let file_tree_dialog = if panel.file_tree_create_dialog.is_some() {
        json!("create")
    } else if panel.file_tree_rename_dialog.is_some() {
        json!("rename")
    } else if panel.file_tree_delete_dialog.is_some() {
        json!("delete")
    } else if panel.file_tree_move_dialog.is_some() {
        json!("move")
    } else {
        Value::Null
    };
    let active_panel = panel
        .slots
        .iter()
        .find(|slot| slot.open && slot.group == PanelGroup::Top)
        .map_or("none", |slot| panel_name(slot.id));
    let open_panels: Vec<&str> =
        panel.slots.iter().filter(|slot| slot.open).map(|slot| panel_name(slot.id)).collect();
    let hover_popup = app.hover.popup.is_some();
    let clipboard = app.clipboard.as_ref().map_or_else(
        || json!({"mode": "disabled", "text": null}),
        |clipboard| {
            json!({
                "mode": if clipboard.is_in_memory() { "memory" } else { "system" },
                "text": clipboard.in_memory_text(),
            })
        },
    );
    let ui: Vec<Value> = app
        .ui_registry
        .element_hits()
        .map(|(id, kind, rect, overlay)| {
            let rect = rect.map_or(Value::Null, |r| json!([r.x, r.y, r.w, r.h]));
            json!({"id": format!("{id:?}"), "kind": kind, "rect": rect, "overlay": overlay})
        })
        .collect();
    let blame_inline = app.inline_blame_dwell.text.as_str();
    let blame_rect = ui.iter().find_map(|element| {
        (element.get("id").and_then(Value::as_str) == Some("EditorBlameInline"))
            .then(|| element.get("rect").cloned().unwrap_or(Value::Null))
    });
    let blame_inline = match (blame_inline.is_empty(), blame_rect) {
        (false, Some(Value::Array(rect))) if rect.len() == 4 => json!({
            "text": blame_inline,
            "x": rect[0], "y": rect[1], "w": rect[2], "h": rect[3],
        }),
        _ => Value::Null,
    };
    let blame_status_visible = ui.iter().any(|element| {
        element.get("id").and_then(Value::as_str) == Some("StatusGitBlame")
    });
    let blame_column_labels: Vec<Value> = ui.iter().filter_map(|element| {
        let id = element.get("id")?.as_str()?;
        let line = id.strip_prefix("EditorBlameColumnRow(")?.strip_suffix(')')?.parse::<usize>().ok()?;
        let head_line = crate::editor::head_line_for(&app.editor.git_hunks, line)?;
        let blame = app.editor.git_blame.blame.as_ref()?;
        let commit_idx = *blame.line_commit.get(head_line)? as usize;
        Some(json!({"line": line, "label": blame.column_labels.get(commit_idx)?}))
    }).collect();
    let selection = app.editor.selection_anchor.filter(|&anchor| anchor != app.editor.cursor).map_or(
        Value::Null,
        |anchor| {
            let start = crate::render_view::line_and_character_at(&app.editor, anchor.min(app.editor.cursor));
            let end = crate::render_view::line_and_character_at(&app.editor, anchor.max(app.editor.cursor));
            json!({"start": [start.0, start.1], "end": [end.0, end.1]})
        },
    );
    let media_stats = app.markdown_media.stats();
    json!({
        "size": [w, h],
        "scale": scale,
        "cursor_icon": format!("{:?}", app.current_cursor),
        "mode": mode,
        "themes": {
            "editor": app.editor_theme_id.key(),
            "ui": app.ui_theme_id.key(),
            "linked": app.theme_linked,
        },
        "atlas": {
            "alpha_resets": atlas_stats.0,
            "color_resets": atlas_stats.1,
            "alpha_fill_y": atlas_stats.2,
        },
        "tabs": tabs_json(app),
        "markdown_media_stats": {
            "media_gen": app.markdown_media.media_gen(),
            "loads_started": media_stats.loads_started,
            "texture_bytes": media_stats.texture_bytes,
            "visible_texture_bytes": media_stats.visible_texture_bytes,
        },
        "markdown_toc": {
            "open": app.markdown_toc.open,
            "items": app.markdown_toc.items.iter().map(|item| &item.text).collect::<Vec<_>>(),
            "selected": app.markdown_toc.selected,
        },
        "editor": {
            "lines": app.editor.line_offsets.len(),
            "cursor": app.editor.cursor,
            "selection": selection,
            "extra_cursors": app.editor.extra_cursors(),
            "highlight_version": app.highlighter.current_version,
            "highlight_spans": app.highlighter.spans.iter().map(|span| [span.start, span.end]).collect::<Vec<_>>(),
        },
        "blame_inline": blame_inline,
        "blame_status_visible": blame_status_visible,
        "blame_column": {
            "open": app.editor.git_blame.column_open,
            "visible_labels": blame_column_labels,
        },
        "graph_highlight_oid": app.ide_panel.git.graph_reveal.highlight_oid(),
        "ide_panel": {
            "active": active_panel,
            "open": open_panels,
            "width": panel.left_width,
            "problem_rows": panel.flat_diags.len(),
            "problems_scroll": panel.problems_scroll.current,
        },
        "diagnostics": app.lsp.as_ref().map(|lsp| {
            let (errors, warnings) = lsp.total_diagnostic_counts();
            json!({"errors": errors, "warnings": warnings, "generation": lsp.diagnostic_generation()})
        }),
        "overlays": {
            "settings": app.show_settings,
            "search": app.show_search,
            "welcome": app.show_welcome,
            "file_tree_dialog": file_tree_dialog,
            "context_menu": panel.file_tree_context_menu.is_some(),
            "lsp_actions_menu": app.lsp_actions_menu.is_some(),
            "readonly_notice": app.readonly_notice_until.is_some_and(|until| until > now),
            "inline_git_popup": app.inline_git_popup.is_some(),
        },
        "dialog": dialog,
        "external_request": external_request_json(app.external_requests.take()),
        "clipboard": clipboard,
        "writes_allowed": crate::platform::headless_writes_allowed(),
        "hover": {
            "ui": app.ui_registry.hovered().map(|id| format!("{id:?}")),
            "popup": hover_popup,
        },
        "event_loop": event_loop_json(app, loop_state, now),
        "ui": ui,
    })
}

fn dialog_json(app: &mut App, w: u32, h: u32) -> Value {
    if !app.confirm_dialog.drawn_in_frame() {
        return Value::Null;
    }
    let action = match app.confirm_dialog.action() {
        PendingAction::None => "None",
        PendingAction::Quit => "Quit",
        PendingAction::OpenFile => "OpenFile",
        PendingAction::OpenLinkedFile => "OpenLinkedFile",
        PendingAction::CloseFile => "CloseFile",
        PendingAction::CloseTab(_) => "CloseTab",
        PendingAction::CloseAllTabs => "CloseAllTabs",
        PendingAction::ResetKeymap => "ResetKeymap",
    };
    let buttons: Vec<Value> = app.renderer.as_mut().map_or_else(Vec::new, |renderer| {
        let layout = dialog_layout(w, h, renderer.scale_factor);
        dialog_buttons(layout, app.confirm_dialog.action(), renderer)
            .iter()
            .map(|(name, rect)| json!({"name": name, "rect": rect}))
            .collect()
    });
    let title = if action == "ResetKeymap" { "Сброс сочетаний" } else { &app.base_title };
    json!({"action": action, "title": title, "buttons": buttons})
}

fn tabs_json(app: &App) -> Value {
    if app.tabs.is_empty() {
        // Editor mode keeps its one document in the `App` fields, without a tab.
        if app.show_welcome {
            return json!([]);
        }
        return json!([active_tab_json(app, 0)]);
    }
    app.tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            if index == app.active_tab {
                active_tab_json(app, index)
            } else {
                // Inactive tabs keep their own state; the active one is swapped into `App`.
                tab_json(TabView {
                    index,
                    activity: TabActivity::Inactive,
                    path: tab.file_path.as_deref(),
                    title: &tab.base_title,
                    modified: app.tab_text_is_dirty(index),
                    deleted: tab.deleted,
                    editor: &tab.editor,
                    scroll_y: &tab.scroll_y,
                    scroll_x: &tab.scroll_x,
                    markdown: tab.markdown.mode != MarkdownMode::Edit,
                    markdown_media: tab.markdown.media_dump(
                        &app.markdown_media,
                        tab.editor.version,
                        tab.file_path.as_deref().and_then(Path::parent),
                    ),
                    kind: tab_kind_name(tab),
                    pdf: tab.pdf.as_deref(),
                    image: tab.image.as_deref(),
                    engine: &app.pdf_engine,
                })
            }
        })
        .collect()
}

fn active_tab_json(app: &App, index: usize) -> Value {
    tab_json(TabView {
        index,
        activity: TabActivity::Active,
        path: app.file_path.as_deref(),
        title: &app.base_title,
        modified: if app.tabs.is_empty() { app.editor.is_dirty() } else { app.tab_text_is_dirty(index) },
        // `deleted` is not swapped into `App`: read it from the tab itself.
        deleted: app.tabs.get(index).is_some_and(|tab| tab.deleted),
        editor: &app.editor,
        scroll_y: &app.scroll_y,
        scroll_x: &app.scroll_x,
        markdown: app.markdown_mode() != MarkdownMode::Edit,
        markdown_media: if app.active_document_is_markdown() {
            app.markdown.media_dump(
                &app.markdown_media,
                app.editor.version,
                app.file_path.as_deref().and_then(Path::parent),
            )
        } else {
            Vec::new()
        },
        kind: app.tabs.get(index).map(tab_kind_name).unwrap_or("normal"),
        pdf: app.tabs.get(index).and_then(|tab| tab.pdf.as_deref()),
        image: app.tabs.get(index).and_then(|tab| tab.image.as_deref()),
        engine: &app.pdf_engine,
    })
}

struct TabView<'a> {
    index: usize,
    activity: TabActivity,
    path: Option<&'a Path>,
    title: &'a str,
    modified: bool,
    deleted: bool,
    editor: &'a Editor,
    scroll_y: &'a ScrollState,
    scroll_x: &'a ScrollState,
    markdown: bool,
    /// Media elements of a Markdown tab in Read mode (empty otherwise).
    markdown_media: Vec<Value>,
    kind: &'static str,
    pdf: Option<&'a crate::app::pdf_tab::PdfTabState>,
    image: Option<&'a crate::app::image_tab::ImageTabState>,
    engine: &'a crate::app::pdf_tab::PdfEngineState,
}

#[derive(Clone, Copy)]
enum TabActivity {
    Active,
    Inactive,
}

fn tab_json(tab: TabView<'_>) -> Value {
    let (line, col) = crate::render_view::cursor_line_and_character(tab.editor);
    let scroll_y = tab.pdf.map_or(tab.scroll_y.current, |pdf| pdf.scroll.current);
    let pdf = tab.pdf.map(|pdf| {
        let (current_page, _) = pdf.anchor();
        let phase = match &pdf.phase {
            crate::app::pdf_tab::PdfPhase::EngineMissing { .. } => "engine_missing",
            crate::app::pdf_tab::PdfPhase::EngineStarting => "engine_starting",
            crate::app::pdf_tab::PdfPhase::Loading => "loading",
            crate::app::pdf_tab::PdfPhase::Ready => "ready",
            crate::app::pdf_tab::PdfPhase::Error(_) => "error",
            crate::app::pdf_tab::PdfPhase::PasswordRequired => "password_required",
        };
        let (engine_name, engine_message) = match tab.engine {
            crate::app::pdf_tab::PdfEngineState::NotStarted => ("not_started", ""),
            crate::app::pdf_tab::PdfEngineState::Starting => ("starting", ""),
            crate::app::pdf_tab::PdfEngineState::Ready => ("ready", ""),
            crate::app::pdf_tab::PdfEngineState::NotInstalled => ("missing", crate::pdf::library::NOT_FOUND_MESSAGE),
            crate::app::pdf_tab::PdfEngineState::Missing { message, .. } => ("missing", message.as_str()),
            crate::app::pdf_tab::PdfEngineState::Failed(message) => ("failed", message.as_str()),
            crate::app::pdf_tab::PdfEngineState::Installing { .. } => ("installing", ""),
        };
        serde_json::json!({
            "phase": phase, "page_count": pdf.page_count(), "current_page": current_page,
            "scroll": pdf.scroll.current, "search_matches": pdf.search.matches.len(),
            "textures": pdf.textures.len(),
            "search_done": pdf.search.done, "selection_chars": pdf.selection_chars(),
            "search_current": pdf.search.current,
            "engine": engine_name, "engine_message": engine_message,
        })
    });
    let image = tab.image.map(|image| {
        let phase = match &image.phase {
            crate::app::image_tab::ImagePhase::Loading => "loading",
            crate::app::image_tab::ImagePhase::Ready => "ready",
            crate::app::image_tab::ImagePhase::Failed(_) => "failed",
        };
        serde_json::json!({"phase": phase, "natural_w": image.natural.0, "natural_h": image.natural.1,
            "zoom": image.zoom, "texture": image.texture.is_some()})
    });
    let mut value = json!({
        "index": tab.index,
        // Output only, not persisted: `display()` is fine here.
        "path": tab.path.map(|path| path.display().to_string()),
        "title": tab.title,
        "active": matches!(tab.activity, TabActivity::Active),
        "modified": tab.modified,
        "deleted": tab.deleted,
        "cursor": {"line": line, "col": col},
        "scroll_y": scroll_y,
        "scroll_x": tab.scroll_x.current,
        "markdown": tab.markdown,
        "markdown_media": tab.markdown_media,
        "kind": tab.kind,
    });
    if let Some(pdf) = pdf { value["pdf"] = pdf; }
    if let Some(image) = image { value["image"] = image; }
    value
}

fn tab_kind_name(tab: &crate::app::EditorTab) -> &'static str {
    match &tab.kind {
        crate::app::EditorTabKind::Normal => "normal",
        crate::app::EditorTabKind::GitDiff(_, _) => "git_diff",
        crate::app::EditorTabKind::ApiClient(_, _) => "api_client",
        crate::app::EditorTabKind::DatabaseTable(_, _) => "database_table",
        crate::app::EditorTabKind::DatabaseQuery(_, _) => "database_query",
        crate::app::EditorTabKind::Pdf => "pdf",
        crate::app::EditorTabKind::Image => "image",
    }
}

fn panel_name(id: PanelId) -> &'static str {
    match id {
        PanelId::Explorer => "explorer",
        PanelId::Search => "search",
        PanelId::Git => "git",
        PanelId::ApiClient => "api",
        PanelId::Database => "database",
        PanelId::Terminal => "terminal",
        PanelId::Problems => "problems",
        PanelId::LspServers => "lsp",
    }
}

fn external_request_json(request: Option<ExternalRequest>) -> Value {
    let Some(request) = request else {
        return Value::Null;
    };
    let (kind, arg) = match request {
        ExternalRequest::PickFile => ("pick_file", None),
        ExternalRequest::PickFiles => ("pick_files", None),
        ExternalRequest::PickFolder => ("pick_folder", None),
        ExternalRequest::SaveFile => ("save_file", None),
        ExternalRequest::OpenUrl(url) => ("open_url", Some(url)),
        ExternalRequest::RevealPath(path) => ("reveal_path", Some(path.display().to_string())),
    };
    json!({"kind": kind, "arg": arg})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_dialog_layout_centers_at_scale() {
        assert_eq!(dialog_layout(1920, 1080, 1.0), DialogLayout { ox: 630, oy_top: 410, dw: 660, dh: 260 });
        assert_eq!(dialog_layout(1920, 1080, 2.0), DialogLayout { ox: 300, oy_top: 280, dw: 1320, dh: 520 });
        // Same truncation as the window branch: 660 * 1.25 = 825, 260 * 1.25 = 325.
        assert_eq!(dialog_layout(1000, 500, 1.25), DialogLayout { ox: 87, oy_top: 87, dw: 825, dh: 325 });
    }

    #[test]
    fn headless_dialog_layout_smaller_buffer_starts_at_corner() {
        assert_eq!(dialog_layout(320, 200, 1.0), DialogLayout { ox: 0, oy_top: 0, dw: 660, dh: 260 });
    }

    #[test]
    fn headless_dump_external_request_kinds() {
        assert_eq!(external_request_json(None), Value::Null);
        assert_eq!(external_request_json(Some(ExternalRequest::PickFolder)), json!({"kind": "pick_folder", "arg": null}));
        assert_eq!(
            external_request_json(Some(ExternalRequest::OpenUrl("https://x.test".into()))),
            json!({"kind": "open_url", "arg": "https://x.test"})
        );
        assert_eq!(
            external_request_json(Some(ExternalRequest::RevealPath("/tmp/a b".into()))),
            json!({"kind": "reveal_path", "arg": "/tmp/a b"})
        );
    }
}
