//! The terminal part of `dump` and `dump row`: the active terminal tab's grid, its visible
//! window, and the frame geometry recomputed from app state. Keys and value shapes follow the
//! Ronsole `dump` (spec §5.1 and §5.2). Nothing here is stored per frame; the renderer's own
//! helpers are called with the same inputs it uses.

use crate::app::mouse::app_panel_scroll_rect;
use crate::app::terminal::lock_terminal_grid;
use crate::app::{App, PanelId};
use crate::render_view::terminal_colors::{rgb_to_rgba, terminal_cell_render_colors, terminal_palette};
use crate::render_view::terminal_ui::{
    TERMINAL_TEXT_SCALE, terminal_body_rect, terminal_max_scroll, terminal_render_scroll_offset,
    terminal_text_padding, terminal_text_viewport_height,
};
use ronsole_term::{
    Cell, CellFlags, MouseTrackingMode, Term, TermPalette, UnderlineStyle,
    normalized_selection_bounds,
};
use serde_json::{Value, json};

/// Frame geometry of the active terminal, as the renderer computes it for the current state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TerminalFrame {
    /// `[x, y, w, h]` of the terminal body.
    body: [f32; 4],
    scale: f32,
    char_w: f32,
    char_h: f32,
    text_x: f32,
    scroll_offset: f32,
    max_scroll: f32,
}

/// Visible area of the active terminal: `rows` rows from absolute row `top`.
struct Window {
    top: usize,
    rows: usize,
    total: usize,
    scrollback: usize,
}

/// Computes the visible area of `grid` for a frame with `frame`, with no scroll before the first frame.
fn window(grid: &Term, frame: Option<&TerminalFrame>) -> Window {
    let total = grid.total_rows();
    let rows = grid.rows();
    Window {
        top: window_top(total, rows, frame),
        rows,
        total,
        scrollback: grid.scrollback_len(),
    }
}

/// Absolute row the frame draws at the top of the text viewport (Q24):
/// `max(0, total − floor((viewport_h + scroll_offset) / char_h))`, never past `total − rows`.
/// `viewport_h` is the text height that `rows` and `max_scroll` come from, so a view scrolled to
/// `max_scroll` reaches row 0 even when that height is not a whole number of rows. Before the
/// first frame the view is at the bottom: `total − rows`.
///
/// `scroll_offset` is the frame's offset, rounded to whole pixels, so it can fall short of
/// `max_scroll` by less than half a pixel. At that offset the frame draws row 0 within the same
/// margin of the text top, and the window is clamped to start at row 0. A zero offset is never
/// clamped, so a view at the bottom keeps its last row.
fn window_top(total: usize, rows: usize, frame: Option<&TerminalFrame>) -> usize {
    let Some(frame) = frame.filter(|frame| frame.char_h > 0.0) else {
        return total.saturating_sub(rows);
    };
    let viewport_h = terminal_text_viewport_height(frame.body[3], frame.scale);
    let reach = ((viewport_h + frame.scroll_offset.max(0.0)) / frame.char_h).floor() as usize;
    if frame.scroll_offset > 0.0 && frame.scroll_offset + 0.5 >= frame.max_scroll {
        return 0;
    }
    total.saturating_sub(reach).min(total.saturating_sub(rows))
}

/// Y of the top edge of absolute row `top`, as the renderer's row loop places it.
fn row_draw_y(frame: &TerminalFrame, total: usize, top: usize) -> f32 {
    let (_, pad_bottom) = terminal_text_padding(frame.scale);
    let [_, body_y, _, body_h] = frame.body;
    let offset_from_bottom = total.saturating_sub(1 + top) as f32;
    body_y + body_h - pad_bottom - frame.char_h - offset_from_bottom * frame.char_h
        + frame.scroll_offset
}

/// Scroll position of the active terminal, or 0 without a terminal.
fn active_scroll(app: &App) -> f32 {
    app.ide_panel
        .terminals
        .get(app.ide_panel.active_terminal)
        .map_or(0.0, |terminal| terminal.scroll_y.current)
}

/// The geometry the renderer uses for the active terminal, or `None` before its first frame
/// drew it (that frame sets `presentation_visible`) or while the terminal panel is closed.
/// The inputs are the calls of the render path: `app_panel_scroll_rect`, `terminal_body_rect`,
/// `line_height * TERMINAL_TEXT_SCALE` and `terminal_render_scroll_offset`.
fn frame_layout(app: &mut App, grid: &Term) -> Option<TerminalFrame> {
    if !app.ide_panel.is_open(PanelId::Terminal) || !grid.presentation_visible() {
        return None;
    }
    let scroll = active_scroll(app);
    let scale = app.renderer.as_ref()?.scale_factor;
    let (panel_x, content_y, panel_w, content_h, _) =
        app_panel_scroll_rect(app, PanelId::Terminal, scale);
    let (body_y, body_h) = terminal_body_rect(content_y, content_h, scale);
    let is_alt = grid.is_alt_screen();
    let total = grid.total_rows();
    let renderer = app.renderer.as_mut()?;
    let char_h = renderer.line_height * TERMINAL_TEXT_SCALE;
    let char_w = renderer.char_advance('A') * TERMINAL_TEXT_SCALE;
    let max_scroll = if is_alt {
        0.0
    } else {
        terminal_max_scroll(total, char_h, body_h, scale)
    };
    Some(TerminalFrame {
        body: [panel_x, body_y, panel_w, body_h],
        scale,
        char_w,
        char_h,
        text_x: panel_x + 10.0 * scale,
        scroll_offset: terminal_render_scroll_offset(scroll, max_scroll, is_alt),
        max_scroll,
    })
}

/// Body rectangle `[x, y, w, h]` the last drawn frame registered for terminal tab `index`.
fn tab_rect(app: &App, index: usize) -> Value {
    let id = format!("TerminalTab({index})");
    app.ui_registry
        .element_hits()
        .find(|(hit, _, _, _)| format!("{hit:?}") == id)
        .and_then(|(_, _, rect, _)| rect)
        .map_or(Value::Null, |rect| json!([rect.x, rect.y, rect.w, rect.h]))
}

/// `terminal_tabs` of `dump`: every terminal tab in strip order, with the active one marked.
pub(crate) fn tabs_json(app: &App) -> Value {
    let active = app.ide_panel.active_terminal;
    let tabs: Vec<Value> = app
        .ide_panel
        .terminals
        .iter()
        .enumerate()
        .map(|(index, terminal)| {
            let mut title = String::new();
            terminal.write_display_title(&mut title);
            json!({
                "index": index,
                "title": title,
                "active": index == active,
                "rect": tab_rect(app, index),
            })
        })
        .collect();
    Value::Array(tabs)
}

/// The `terminal` section of `dump` for the active tab, or `null` without a terminal tab.
pub(crate) fn terminal_json(app: &mut App) -> Value {
    let index = app.ide_panel.active_terminal;
    // The title lock is taken and released before the grid lock, so the two never nest.
    let Some((grid_arc, title)) = app.ide_panel.terminals.get(index).map(|terminal| {
        let mut title = String::new();
        terminal.write_display_title(&mut title);
        (terminal.grid.clone(), title)
    }) else {
        return Value::Null;
    };
    let grid = lock_terminal_grid(&grid_arc);
    let frame = frame_layout(app, &grid);
    let window = window(&grid, frame.as_ref());
    let text: Vec<String> = (0..window.rows)
        .map(|row| row_text(&grid, window.top + row))
        .collect();
    let cursor = {
        let (cursor_col, cursor_row) = grid.cursor();
        let absolute = grid.scrollback_len() + cursor_row;
        if absolute >= window.top && absolute - window.top < window.rows {
            json!({"row": absolute - window.top, "col": cursor_col})
        } else {
            Value::Null
        }
    };
    let top = window.top as i64;
    let selection = grid.selection().map_or(Value::Null, |(sx, sy, ex, ey)| {
        let (start_x, start_y, end_x, end_y) = normalized_selection_bounds(sx, sy, ex, ey);
        json!({
            "start": [start_y as i64 - top, start_x],
            "last": [end_y as i64 - top, end_x],
        })
    });
    // A link found before the latest content change is stale, as the frame treats it.
    let generation = grid.content_generation();
    let (hovered_link, sync_pending) = app
        .ide_panel
        .terminals
        .get(index)
        .map_or((Value::Null, false), |terminal| {
            let hovered = terminal
                .hovered_link
                .as_ref()
                .filter(|(_, link_generation)| *link_generation == generation)
                .map_or(Value::Null, |(link, _)| {
                    let ranges: Vec<Value> = link
                        .cells
                        .iter()
                        .map(|(row, range)| json!([*row as i64 - top, range.start, range.end]))
                        .collect();
                    json!({"uri": link.uri.to_string(), "ranges": ranges})
                });
            (hovered, terminal.sync_deadline().is_some())
        });
    let layout = match (frame.as_ref(), app.renderer.as_ref()) {
        (Some(frame), Some(renderer)) => json!({
            "body": frame.body,
            "cell0": [frame.text_x, row_draw_y(frame, window.total, window.top)],
            "char_w": frame.char_w,
            "char_h": frame.char_h,
            "baseline": renderer.baseline_offset * TERMINAL_TEXT_SCALE,
            "x_height": renderer.terminal_regular_x_height_at_size(
                renderer.final_text_pixel_size(TERMINAL_TEXT_SCALE),
            ),
        }),
        _ => Value::Null,
    };
    let tracking = match grid.mouse_tracking() {
        MouseTrackingMode::None => "none",
        MouseTrackingMode::Press => "press",
        MouseTrackingMode::ButtonMotion => "button_motion",
        MouseTrackingMode::AnyMotion => "any_motion",
    };
    json!({
        "cols": grid.cols(),
        "rows": window.rows,
        "viewport": {"top": window.top, "scrollback": window.scrollback, "total": window.total},
        "text": text,
        "cursor": cursor,
        "title": title,
        "modes": {
            "app_cursor_keys": grid.app_cursor_keys(),
            "bracketed_paste": grid.bracketed_paste(),
            "mouse_tracking": tracking,
            "mouse_sgr": grid.mouse_sgr(),
            "focus_reporting": grid.focus_reporting(),
            "kitty_flags": grid.kitty_flags(),
            "sync_pending": sync_pending,
        },
        "selection": selection,
        "hovered_link": hovered_link,
        "layout": layout,
    })
}

/// Text of absolute row `index`, with trailing spaces removed.
fn row_text(grid: &Term, index: usize) -> String {
    let mut text = String::new();
    if let Some(cells) = grid.row(index) {
        for cell in cells.iter().take(grid.cols()) {
            cell.append_text_to(&mut text);
        }
    }
    text.truncate(text.trim_end_matches(' ').len());
    text
}

/// `dump row <row>` (spec §5.2) for row `row` of the visible area of the active tab.
pub(crate) fn dump_row_json(app: &mut App, row: usize) -> Result<Value, String> {
    let index = app.ide_panel.active_terminal;
    let grid_arc = app
        .ide_panel
        .terminals
        .get(index)
        .map(|terminal| terminal.grid.clone())
        .ok_or_else(|| "no terminal tab".to_string())?;
    let palette = app
        .renderer
        .as_ref()
        .map(|renderer| terminal_palette(&renderer.theme))
        .ok_or_else(|| "no renderer".to_string())?;
    let grid = lock_terminal_grid(&grid_arc);
    let frame = frame_layout(app, &grid);
    let window = window(&grid, frame.as_ref());
    if row >= window.rows {
        return Err("row out of range".to_string());
    }
    let cells: Vec<Value> = grid
        .row(window.top + row)
        .unwrap_or(&[])
        .iter()
        .take(grid.cols())
        .enumerate()
        .filter(|(_, cell)| !is_plain(cell))
        .map(|(col, cell)| cell_json(col, cell, &palette))
        .collect();
    Ok(json!({"row": row, "cells": cells}))
}

/// Centre pixel `(x, y)` of visible cell `(col, row)` of the active terminal, for `mouse_cell`.
/// `None` when the cell is outside the visible area, before the first frame drew the terminal,
/// or when its centre falls outside the terminal body. Rounding follows Ronsole's `cell_center`.
pub(crate) fn cell_center_at(app: &mut App, col: usize, row: usize) -> Option<(i32, i32)> {
    let index = app.ide_panel.active_terminal;
    let grid_arc = app.ide_panel.terminals.get(index).map(|terminal| terminal.grid.clone())?;
    let grid = lock_terminal_grid(&grid_arc);
    let frame = frame_layout(app, &grid)?;
    let window = window(&grid, Some(&frame));
    if row >= window.rows || col >= grid.cols() {
        return None;
    }
    // Cell edges are rounded to whole pixels; the centre is the floor of the middle of the edges.
    let x0 = (frame.text_x + col as f32 * frame.char_w).round();
    let x1 = (frame.text_x + (col + 1) as f32 * frame.char_w).round();
    let y0 = row_draw_y(&frame, window.total, window.top) + row as f32 * frame.char_h;
    let x = ((x0 + x1) / 2.0).floor();
    let y = (y0 + frame.char_h / 2.0).floor();
    let [body_x, body_y, body_w, body_h] = frame.body;
    let inside = x >= body_x && x < body_x + body_w && y >= body_y && y < body_y + body_h;
    inside.then_some((x as i32, y as i32))
}

/// Cell flags that `dump row` reports by name, in output order. `HOST_HIDDEN` is not listed,
/// so it is not an attribute for `is_plain` either.
const REPORTED_FLAGS: [(CellFlags, &str); 8] = [
    (CellFlags::BOLD, "bold"),
    (CellFlags::DIM, "dim"),
    (CellFlags::ITALIC, "italic"),
    (CellFlags::INVERSE, "inverse"),
    (CellFlags::STRIKE, "strike"),
    (CellFlags::CONCEAL, "conceal"),
    (CellFlags::WIDE, "wide"),
    (CellFlags::WIDE_SPACER, "wide_spacer"),
];

/// True for a blank cell with default colours and no other attribute; `dump row` leaves it out.
fn is_plain(cell: &Cell) -> bool {
    cell.ch() == ' '
        && cell.zero_width().is_empty()
        && !REPORTED_FLAGS
            .iter()
            .any(|(flag, _)| cell.flags().contains(*flag))
        && cell.fg().is_default_foreground()
        && cell.bg().is_default_background()
        && cell.underline() == UnderlineStyle::None
        && cell.underline_color().is_none()
        && cell.hyperlink().is_none()
}

/// One cell of `dump row`. Colours are resolved as the frame resolves them, including the
/// underline colour, which the frame takes from the cell's SGR 58 colour and not from `fg`.
fn cell_json(col: usize, cell: &Cell, palette: &TermPalette) -> Value {
    let (foreground, background) = terminal_cell_render_colors(cell, palette);
    let mut ch = String::new();
    cell.append_text_to(&mut ch);
    let flags = cell.flags();
    let names: Vec<&str> = REPORTED_FLAGS
        .iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .map(|(_, name)| *name)
        .collect();
    json!({
        "col": col,
        "ch": ch,
        "fg": hex(foreground),
        "bg": background.map(hex),
        "flags": names,
        "underline": underline_name(cell.underline()),
        "underline_color": cell
            .underline_color()
            .map(|color| hex(rgb_to_rgba(palette.rgb(color)))),
        "link": cell.hyperlink().map(|link| link.uri.to_string()),
    })
}

/// `#rrggbb` of a rendered colour; channels are `round(c · 255)`.
fn hex(color: [f32; 4]) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color[0]),
        channel(color[1]),
        channel(color[2])
    )
}

fn underline_name(style: UnderlineStyle) -> &'static str {
    match style {
        UnderlineStyle::None => "none",
        UnderlineStyle::Single => "single",
        UnderlineStyle::Double => "double",
        UnderlineStyle::Curly => "curly",
        UnderlineStyle::Dotted => "dotted",
        UnderlineStyle::Dashed => "dashed",
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalFrame, window_top};

    /// The geometry of Ronsole's fractional-viewport test: body 584 px, text viewport 568 px,
    /// char_h 20, 28 rows and 40 lines, so `max_scroll` is 232, not a whole number of rows.
    fn fractional_frame(scroll_offset: f32) -> TerminalFrame {
        TerminalFrame {
            body: [0.0, 0.0, 800.0, 584.0],
            scale: 1.0,
            char_h: 20.0,
            max_scroll: 232.0,
            scroll_offset,
            ..TerminalFrame::default()
        }
    }

    #[test]
    fn window_top_reaches_row_zero_at_fractional_max_scroll() {
        // At max_scroll the frame draws row 0 at the text top, so the window starts there.
        assert_eq!(window_top(40, 28, Some(&fractional_frame(232.0))), 0);
        // Whole-row offsets: floor(568 / 20) = 28 rows reach from the bottom, then one more.
        assert_eq!(window_top(40, 28, Some(&fractional_frame(0.0))), 12);
        assert_eq!(window_top(40, 28, Some(&fractional_frame(20.0))), 11);
    }

    #[test]
    fn window_top_without_frame_or_cell_height_is_the_bottom_view() {
        assert_eq!(window_top(40, 28, None), 12);
        let mut frame = fractional_frame(232.0);
        frame.char_h = 0.0;
        assert_eq!(window_top(40, 28, Some(&frame)), 12);
        // Fewer lines than rows: the window starts at row 0.
        assert_eq!(window_top(10, 28, None), 0);
        assert_eq!(window_top(10, 28, Some(&fractional_frame(0.0))), 0);
    }

    #[test]
    fn window_top_clamps_to_row_zero_within_half_a_pixel_of_max_scroll() {
        // Body 562 px, text viewport 546 px, char_h 24.15, 47 lines, rows 22. The rounded offset
        // 589 is 0.05 px short of max_scroll 589.05, so the window starts at row 0. Unclamped,
        // the formula gives floor((546 + 589) / 24.15) = 46 and top 1.
        let frame = TerminalFrame {
            body: [0.0, 0.0, 800.0, 562.0],
            char_h: 24.15,
            max_scroll: 589.05,
            scroll_offset: 589.0,
            ..fractional_frame(589.0)
        };
        assert_eq!(window_top(47, 22, Some(&frame)), 0);
    }

    #[test]
    fn window_top_zero_offset_keeps_bottom_view_even_with_tiny_max_scroll() {
        // Offset 0 is never clamped to row 0, so a view at the bottom keeps its last row.
        let frame = TerminalFrame {
            max_scroll: 0.3,
            ..fractional_frame(0.0)
        };
        assert_eq!(window_top(40, 28, Some(&frame)), 12);
    }

    #[test]
    fn window_top_short_body_starts_at_bottom_view_not_past_it() {
        // Body 40 px gives a text viewport of 24 px: reach is 1 row, below the 28 visible rows.
        // The window must still stay inside the 40 lines, so it starts at total - rows.
        let frame = TerminalFrame {
            body: [0.0, 0.0, 800.0, 40.0],
            max_scroll: 0.0,
            ..fractional_frame(0.0)
        };
        assert_eq!(window_top(40, 28, Some(&frame)), 12);
    }
}
