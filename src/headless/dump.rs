//! Headless confirmation dialog (centered in the main frame instead of a second window)
//! and the `dump` command: a compact JSON snapshot of the UI state.

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
pub(crate) fn dialog_buttons(layout: DialogLayout, renderer: &mut Renderer) -> [(&'static str, [f32; 4]); 3] {
    let s = renderer.scale_factor;
    let (save, discard, cancel) =
        crate::widgets::get_dialog_buttons(0.0, 0.0, layout.dw as f32, layout.dh as f32, s, renderer);
    let (ox, oy) = (layout.ox as f32, layout.oy_top as f32);
    let rect = |button: &Button| [button.x + ox, button.y + oy, button.w, button.h];
    [("save", rect(&save)), ("discard", rect(&discard)), ("cancel", rect(&cancel))]
}

/// Draws the dialog over the finished main frame of a `w`x`h` buffer: what the window
/// branch does on its dialog surface, inside a viewport over the centered rectangle.
pub(crate) fn draw_dialog(renderer: &mut Renderer, base_title: &str, w: u32, h: u32) {
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
    renderer.draw_dialog_window(base_title);
    renderer.resize(w, h);
}

/// JSON snapshot for `dump`. Reading it clears the recorded external request.
pub(crate) fn dump_json(app: &mut App) -> Value {
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
    let hover_popup = crate::app::mouse::HOVER_STATE.with(|state| state.borrow().popup.is_some());
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
    let selection = app.editor.selection_anchor.filter(|&anchor| anchor != app.editor.cursor).map_or(
        Value::Null,
        |anchor| {
            let start = crate::render_view::line_and_character_at(&app.editor, anchor.min(app.editor.cursor));
            let end = crate::render_view::line_and_character_at(&app.editor, anchor.max(app.editor.cursor));
            json!({"start": [start.0, start.1], "end": [end.0, end.1]})
        },
    );
    json!({
        "size": [w, h],
        "scale": scale,
        "cursor_icon": format!("{:?}", app.current_cursor),
        "mode": mode,
        "tabs": tabs_json(app),
        "editor": {"lines": app.editor.line_offsets.len(), "selection": selection},
        "ide_panel": {"active": active_panel, "open": open_panels, "width": panel.left_width},
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
        PendingAction::CloseFile => "CloseFile",
        PendingAction::CloseTab(_) => "CloseTab",
        PendingAction::CloseAllTabs => "CloseAllTabs",
    };
    let buttons: Vec<Value> = app.renderer.as_mut().map_or_else(Vec::new, |renderer| {
        let layout = dialog_layout(w, h, renderer.scale_factor);
        dialog_buttons(layout, renderer)
            .iter()
            .map(|(name, rect)| json!({"name": name, "rect": rect}))
            .collect()
    });
    json!({"action": action, "title": app.base_title, "buttons": buttons})
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
                    active: false,
                    path: tab.file_path.as_deref(),
                    title: &tab.base_title,
                    modified: app.tab_text_is_dirty(index),
                    editor: &tab.editor,
                    scroll_y: &tab.scroll_y,
                    scroll_x: &tab.scroll_x,
                    markdown: tab.markdown.mode != MarkdownMode::Edit,
                })
            }
        })
        .collect()
}

fn active_tab_json(app: &App, index: usize) -> Value {
    tab_json(TabView {
        index,
        active: true,
        path: app.file_path.as_deref(),
        title: &app.base_title,
        modified: if app.tabs.is_empty() { app.editor.is_dirty() } else { app.tab_text_is_dirty(index) },
        editor: &app.editor,
        scroll_y: &app.scroll_y,
        scroll_x: &app.scroll_x,
        markdown: app.markdown_mode() != MarkdownMode::Edit,
    })
}

struct TabView<'a> {
    index: usize,
    active: bool,
    path: Option<&'a Path>,
    title: &'a str,
    modified: bool,
    editor: &'a Editor,
    scroll_y: &'a ScrollState,
    scroll_x: &'a ScrollState,
    markdown: bool,
}

fn tab_json(tab: TabView<'_>) -> Value {
    let (line, col) = crate::render_view::cursor_line_and_character(tab.editor);
    json!({
        "index": tab.index,
        // Output only, not persisted: `display()` is fine here.
        "path": tab.path.map(|path| path.display().to_string()),
        "title": tab.title,
        "active": tab.active,
        "modified": tab.modified,
        "cursor": {"line": line, "col": col},
        "scroll_y": tab.scroll_y.current,
        "scroll_x": tab.scroll_x.current,
        "markdown": tab.markdown,
    })
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
