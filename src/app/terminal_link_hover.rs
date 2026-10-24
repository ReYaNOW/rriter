//! Hyperlink hover state of one terminal tab: resolves the link under a grid cell and
//! records it together with the grid generation it was resolved against. The underline
//! is drawn only while that generation matches the grid. App-level routing (modifier,
//! pointer, cursor icon, Ctrl+click) lives in `app/mouse/terminal_mouse.rs`.

use crate::app::terminal::{Terminal, lock_terminal_grid};
use ronsole_term::LinkTarget;

impl Terminal {
    /// Link under `cell` = `(column, absolute row)`, with the grid generation it was resolved against.
    /// The search covers scrollback and the visible area, so a link that starts in history is reported whole.
    pub(crate) fn resolve_link(&self, cell: (usize, usize)) -> Option<(LinkTarget, u64)> {
        let (col, row) = cell;
        let grid = lock_terminal_grid(&self.grid);
        let rows = 0..grid.scrollback_len() + grid.rows();
        let link = grid.link_at(row, col, rows)?;
        Some((link, grid.content_generation()))
    }

    /// Whether the stored hovered link is still current for the grid and covers `cell`,
    /// so the lookup can be skipped on a repeated frame or a small pointer move.
    fn hovered_link_covers(&self, cell: (usize, usize)) -> bool {
        let Some((link, generation)) = self.hovered_link.as_ref() else {
            return false;
        };
        let (col, row) = cell;
        if *generation != lock_terminal_grid(&self.grid).content_generation() {
            return false;
        }
        link.cells
            .iter()
            .any(|(link_row, range)| *link_row == row && range.contains(&col))
    }

    /// Stores `link` as the hovered link. Returns `true` when it differs from the stored one,
    /// so the caller knows a redraw is needed.
    pub(crate) fn set_hovered_link(&mut self, link: Option<(LinkTarget, u64)>) -> bool {
        if self.hovered_link == link {
            return false;
        }
        self.hovered_link = link;
        true
    }

    /// Re-resolves the hover for `cell`; `None` clears it (no link modifier, or pointer off the body).
    /// Returns `true` when the stored link changed.
    pub(crate) fn update_hovered_link(&mut self, cell: Option<(usize, usize)>) -> bool {
        let next = match cell {
            None => None,
            Some(cell) if self.hovered_link_covers(cell) => return false,
            Some(cell) => self.resolve_link(cell),
        };
        self.set_hovered_link(next)
    }
}
