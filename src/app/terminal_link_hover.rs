//! Hyperlink hover state of one terminal tab: resolves the link under a grid cell and
//! records it together with the grid generation it was resolved against. The underline
//! is drawn only while that generation matches the grid. App-level routing (modifier,
//! pointer, cursor icon, Ctrl+click) lives in `app/mouse/terminal_mouse.rs`.

use crate::app::terminal::{Terminal, lock_terminal_grid};
use ronsole_term::LinkTarget;

impl Terminal {
    /// Link under `cell` = `(column, absolute row)` for the hover, with the grid generation it was resolved against.
    /// Cells are collected within one viewport height of `cell` on each side, which contains every
    /// visible row, so the underline is complete on screen without walking all of scrollback.
    pub(crate) fn resolve_link(&self, cell: (usize, usize)) -> (Option<LinkTarget>, u64) {
        let (col, row) = cell;
        let grid = lock_terminal_grid(&self.grid);
        let height = grid.rows();
        let link = grid.link_at(row, col, row.saturating_sub(height)..row + height + 1);
        (link, grid.content_generation())
    }

    /// Whether the last lookup found no link on `cell` in the current grid generation.
    fn hovered_link_missed(&self, cell: (usize, usize)) -> bool {
        let Some((missed, generation)) = self.hovered_link_miss else {
            return false;
        };
        missed == cell && generation == lock_terminal_grid(&self.grid).content_generation()
    }

    /// URI of the link under `cell`, for a click or a tracking decision. The URI does not depend on
    /// the rows searched for cells, so only the cell's own row is walked.
    pub(crate) fn link_uri_at(&self, cell: (usize, usize)) -> Option<String> {
        let (col, row) = cell;
        let grid = lock_terminal_grid(&self.grid);
        let link = grid.link_at(row, col, row..row + 1)?;
        Some(link.uri.to_string())
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
            Some(cell) if self.hovered_link_missed(cell) => None,
            Some(cell) => match self.resolve_link(cell) {
                (Some(link), generation) => Some((link, generation)),
                (None, generation) => {
                    self.hovered_link_miss = Some((cell, generation));
                    None
                }
            },
        };
        self.set_hovered_link(next)
    }
}

#[cfg(test)]
mod tests {
    use crate::app::terminal::{Terminal, lock_terminal_grid};
    use crate::app::terminal_process::RriterTermSink;
    use crate::app::terminal_sync_output::advance_terminal_output_batch;

    /// Feeds `bytes` into the grid of `term` through the output batch path.
    fn feed(term: &Terminal, bytes: &[u8]) {
        let mut sink = RriterTermSink::new(None, None);
        let mut grid = lock_terminal_grid(&term.grid);
        advance_terminal_output_batch(&mut grid, &mut sink, &[bytes.to_vec()]);
    }

    #[test]
    fn repeated_miss_on_same_cell_and_generation_is_remembered() {
        let mut term = Terminal::test_without_process(20, 2);
        assert!(!term.update_hovered_link(Some((15, 1))), "blank cell has no link");
        let generation = lock_terminal_grid(&term.grid).content_generation();
        assert_eq!(term.hovered_link_miss, Some(((15, 1), generation)));

        assert!(!term.update_hovered_link(Some((15, 1))), "repeated miss changes nothing");
        assert!(term.hovered_link.is_none());
    }

    #[test]
    fn miss_is_not_reused_after_grid_generation_changes() {
        let mut term = Terminal::test_without_process(20, 2);
        assert!(!term.update_hovered_link(Some((0, 0))));

        feed(&term, b"\x1b]8;;https://example.test\x07link\x1b]8;;\x07");
        assert!(term.update_hovered_link(Some((0, 0))), "link written after the miss is found");
        assert!(term.hovered_link.is_some());
    }
}
