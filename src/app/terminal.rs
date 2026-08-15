use alacritty_terminal::vte::{Params, Parser, Perform};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

pub(crate) const ANSI_16_COLORS: [[f32; 4]; 16] = [
    [0.10, 0.10, 0.10, 1.0],
    [0.95, 0.30, 0.30, 1.0],
    [0.30, 0.85, 0.30, 1.0],
    [0.90, 0.85, 0.20, 1.0],
    [0.30, 0.60, 1.00, 1.0],
    [0.90, 0.35, 0.90, 1.0],
    [0.20, 0.85, 0.85, 1.0],
    [0.90, 0.90, 0.90, 1.0],
    [0.45, 0.45, 0.45, 1.0],
    [1.00, 0.40, 0.40, 1.0],
    [0.40, 1.00, 0.40, 1.0],
    [1.00, 1.00, 0.40, 1.0],
    [0.50, 0.70, 1.00, 1.0],
    [1.00, 0.50, 1.00, 1.0],
    [0.40, 1.00, 1.00, 1.0],
    [1.00, 1.00, 1.00, 1.0],
];

pub(crate) fn apply_ansi_sgr(
    params: &Params,
    fg: &mut Option<u8>,
    bold: &mut bool,
    default_fg: Option<u8>,
    mut bg: Option<&mut u8>,
    default_bg: u8,
) {
    if params.is_empty() {
        *fg = default_fg;
        *bold = false;
        if let Some(bg) = bg.as_deref_mut() {
            *bg = default_bg;
        }
        return;
    }
    let values = params.iter().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < values.len() {
        let Some(value) = values[index].first().copied() else {
            index += 1;
            continue;
        };
        match value {
            0 => {
                *fg = default_fg;
                *bold = false;
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = default_bg;
                }
            }
            1 => {
                *bold = true;
                if let Some(color) = fg.as_mut()
                    && *color < 8
                {
                    *color += 8;
                }
            }
            22 => {
                *bold = false;
                if let Some(color) = fg.as_mut()
                    && (8..16).contains(color)
                {
                    *color -= 8;
                }
            }
            30..=37 => *fg = Some((value - 30) as u8 + if *bold { 8 } else { 0 }),
            40..=47 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = (value - 40) as u8;
                }
            }
            90..=97 => *fg = Some((value - 90 + 8) as u8),
            100..=107 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = (value - 100 + 8) as u8;
                }
            }
            39 => *fg = default_fg,
            49 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = default_bg;
                }
            }
            38 | 48 => {
                if index + 1 < values.len() && !values[index + 1].is_empty() {
                    let mode = values[index + 1][0];
                    if mode == 5 && index + 2 < values.len() && !values[index + 2].is_empty() {
                        let color = values[index + 2][0] as u8;
                        if value == 38 {
                            *fg = Some(color);
                        } else if let Some(bg) = bg.as_deref_mut() {
                            *bg = color;
                        }
                        index += 2;
                    } else if mode == 2 && index + 4 < values.len() {
                        index += 4;
                    } else {
                        index += 1;
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
}

/// Locks the terminal grid while recovering the inner state after a worker panic.
///
/// A poisoned terminal mutex must not cascade into a UI-thread panic: the grid is
/// still structurally valid and can be redrawn, cleared, or shut down safely.
#[inline]
fn recover_terminal_lock<T>(result: std::sync::LockResult<T>) -> T {
    result.unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[inline]
pub(crate) fn lock_terminal_grid(grid: &Mutex<TermGrid>) -> MutexGuard<'_, TermGrid> {
    recover_terminal_lock(grid.lock())
}

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: u8,
    pub bg: u8,
    pub presentation: u8,
}

pub(crate) const CELL_PRESENTATION_AUTO: u8 = 0;
pub(crate) const CELL_PRESENTATION_TEXT: u8 = 1;
pub(crate) const CELL_PRESENTATION_EMOJI: u8 = 2;

impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            fg: 7,
            bg: 0,
            presentation: CELL_PRESENTATION_AUTO,
        }
    }
}

#[inline(always)]
pub(crate) fn terminal_presentation_selector(c: char) -> Option<u8> {
    match c {
        '\u{FE0E}' => Some(CELL_PRESENTATION_TEXT),
        '\u{FE0F}' => Some(CELL_PRESENTATION_EMOJI),
        _ => None,
    }
}

#[inline(always)]
pub(crate) fn is_terminal_zero_width_format(c: char) -> bool {
    let u = c as u32;
    c == '\u{200D}' || (0xFE00..=0xFE0F).contains(&u) || (0xE0100..=0xE01EF).contains(&u)
}

#[inline(always)]
pub(crate) fn normalized_selection_bounds(
    sx: usize,
    sy: usize,
    ex: usize,
    ey: usize,
) -> (usize, usize, usize, usize) {
    let start_y = sy.min(ey);
    let end_y = sy.max(ey);
    let start_x = if sy < ey {
        sx
    } else if sy > ey {
        ex
    } else {
        sx.min(ex)
    };
    let end_x = if sy < ey {
        ex
    } else if sy > ey {
        sx
    } else {
        sx.max(ex)
    };
    (start_x, start_y, end_x, end_y)
}

pub(crate) fn terminal_selection_text(grid: &TermGrid) -> Option<String> {
    let (sx, sy, ex, ey) = grid.selection?;
    let scrollback_len = if grid.is_alt {
        0
    } else {
        grid.scrollback.len()
    };
    let total_lines = scrollback_len + grid.lines.len();
    let (start_x, start_y, end_x, end_y) = normalized_selection_bounds(sx, sy, ex, ey);
    let mut result = String::new();

    for y in start_y..=end_y {
        if y >= total_lines {
            continue;
        }
        let row = if grid.is_alt {
            &grid.lines[y]
        } else if y < grid.scrollback.len() {
            &grid.scrollback[y]
        } else {
            &grid.lines[y - grid.scrollback.len()]
        };
        let line_start = if y == start_y { start_x } else { 0 };
        let line_end = if y == end_y {
            end_x
        } else {
            grid.cols.saturating_sub(1)
        };
        for x in line_start..=line_end {
            if let Some(cell) = row.get(x) {
                result.push(cell.c);
            }
        }
        if y != end_y {
            result.push('\n');
        }
    }

    Some(result.trim_end().to_string())
}

pub struct TermGrid {
    pub scrollback: std::collections::VecDeque<Vec<Cell>>,
    pub lines: std::collections::VecDeque<Vec<Cell>>,
    pub alt_lines: Option<std::collections::VecDeque<Vec<Cell>>>,
    pub alt_saved_cursor: Option<(usize, usize)>,
    pub is_alt: bool,
    pub cols: usize,
    pub visible_rows: usize,
    pub cur_x: usize,
    pub cur_y: usize,
    pub cur_fg: u8,
    pub cur_bg: u8,
    pub cur_bold: bool,
    pub dirty: bool,
    pub(crate) presentation_ready: bool,
    pub(crate) presentation_layout_ready: bool,
    pub selection: Option<(usize, usize, usize, usize)>,
    pub reply_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    pub saved_cursor: Option<(usize, usize)>,
    pub scroll_region: (usize, usize),
    pub cursor_visible: bool,
    pub app_cursor_keys: bool,
    pub mouse_tracking: bool,
    pub pool: Vec<Vec<Cell>>,
    title_cache: Option<crate::app::terminal_process::TerminalTitleCache>,
}

impl TermGrid {
    pub fn new(cols: usize, visible_rows: usize) -> Self {
        let mut lines = std::collections::VecDeque::new();
        for _ in 0..visible_rows {
            lines.push_back(vec![Cell::default(); cols]);
        }
        Self {
            scrollback: std::collections::VecDeque::new(),
            lines,
            alt_lines: None,
            alt_saved_cursor: None,
            is_alt: false,
            cols,
            visible_rows,
            cur_x: 0,
            cur_y: 0,
            cur_fg: 7,
            cur_bg: 0,
            cur_bold: false,
            dirty: true,
            presentation_ready: false,
            presentation_layout_ready: false,
            selection: None,
            reply_tx: None,
            saved_cursor: None,
            scroll_region: (0, visible_rows.saturating_sub(1)),
            cursor_visible: true,
            app_cursor_keys: false,
            mouse_tracking: false,
            pool: Vec::with_capacity(128),
            title_cache: None,
        }
    }

    pub(crate) fn new_with_title_cache(
        cols: usize,
        visible_rows: usize,
        title_cache: crate::app::terminal_process::TerminalTitleCache,
    ) -> Self {
        let mut grid = Self::new(cols, visible_rows);
        grid.title_cache = Some(title_cache);
        grid
    }

    #[inline]
    pub(crate) fn mark_presentation_ready(&mut self) {
        self.presentation_ready = true;
        self.dirty = true;
    }

    #[inline]
    pub(crate) fn mark_presentation_layout_ready(&mut self) {
        self.presentation_layout_ready = true;
    }

    #[inline]
    pub(crate) fn presentation_visible(&self) -> bool {
        self.presentation_ready && self.presentation_layout_ready
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        if new_cols == self.cols && new_rows == self.visible_rows {
            return;
        }

        if new_cols != self.cols {
            for line in self.lines.iter_mut() {
                line.resize(new_cols, Cell::default());
            }
            if let Some(alt) = &mut self.alt_lines {
                for line in alt.iter_mut() {
                    line.resize(new_cols, Cell::default());
                }
            }
            self.cols = new_cols;
        }

        let current_rows = self.lines.len();
        let was_full_region = self.scroll_region.1 >= current_rows.saturating_sub(1);

        if new_rows < current_rows {
            let diff = current_rows - new_rows;
            let rows_below_cursor = current_rows.saturating_sub(self.cur_y + 1);
            let drop_bottom = rows_below_cursor.min(diff);
            let drop_top = diff - drop_bottom;

            for _ in 0..drop_bottom {
                if let Some(mut line) = self.lines.pop_back() {
                    if self.pool.len() < 128 {
                        line.clear();
                        self.pool.push(line);
                    }
                }
            }
            for _ in 0..drop_top {
                if let Some(top) = self.lines.pop_front() {
                    if !self.is_alt {
                        self.scrollback.push_back(top);
                    }
                }
            }

            self.cur_y = self.cur_y.saturating_sub(drop_top);
            if let Some((_, ref mut sy)) = self.saved_cursor {
                *sy = sy.saturating_sub(drop_top);
            }
        } else if new_rows > current_rows {
            let diff = new_rows - current_rows;

            if !self.is_alt {
                let from_scrollback = diff.min(self.scrollback.len());

                for _ in 0..from_scrollback {
                    if let Some(mut row) = self.scrollback.pop_back() {
                        row.resize(self.cols, Cell::default());
                        self.lines.push_front(row);
                    }
                }

                let blanks = diff - from_scrollback;
                for _ in 0..blanks {
                    let mut line = self
                        .pool
                        .pop()
                        .unwrap_or_else(|| Vec::with_capacity(self.cols));
                    line.resize(self.cols, Cell::default());
                    line.fill(Cell::default());
                    self.lines.push_back(line);
                }

                self.cur_y += from_scrollback;
                if let Some((_, ref mut sy)) = self.saved_cursor {
                    *sy += from_scrollback;
                }
            } else {
                for _ in 0..diff {
                    self.lines.push_back(vec![Cell::default(); self.cols]);
                }
            }
        }

        if let Some(alt) = &mut self.alt_lines {
            let alt_current_rows = alt.len();
            if new_rows < alt_current_rows {
                let diff = alt_current_rows - new_rows;
                for _ in 0..diff {
                    if let Some(mut line) = alt.pop_back() {
                        if self.pool.len() < 128 {
                            line.clear();
                            self.pool.push(line);
                        }
                    }
                }
            } else if new_rows > alt_current_rows {
                let diff = new_rows - alt_current_rows;
                for _ in 0..diff {
                    let mut line = self
                        .pool
                        .pop()
                        .unwrap_or_else(|| Vec::with_capacity(self.cols));
                    line.resize(self.cols, Cell::default());
                    line.fill(Cell::default());
                    alt.push_back(line);
                }
            }
            if let Some((_, ref mut sy)) = self.alt_saved_cursor {
                *sy = (*sy).min(new_rows.saturating_sub(1));
            }
        }

        while self.scrollback.len() > 10000 {
            self.scrollback.pop_front();
        }
        self.visible_rows = new_rows;
        self.cur_x = self.cur_x.min(self.cols.saturating_sub(1));
        self.cur_y = self.cur_y.min(self.visible_rows.saturating_sub(1));

        if was_full_region {
            self.scroll_region = (0, new_rows.saturating_sub(1));
        } else {
            let (sr_top, sr_bot) = self.scroll_region;
            self.scroll_region = (
                sr_top.min(new_rows.saturating_sub(1)),
                sr_bot.min(new_rows.saturating_sub(1)),
            );
        }
        self.dirty = true;
    }

    pub fn put_char(&mut self, c: char) {
        if self.cur_x >= self.cols {
            self.newline();
            self.cur_x = 0;
        }
        if self.cur_y >= self.visible_rows {
            self.cur_y = self.visible_rows.saturating_sub(1);
        }
        let fg = self.cur_fg;
        let bg = self.cur_bg;
        if let Some(line) = self.lines.get_mut(self.cur_y) {
            if let Some(cell) = line.get_mut(self.cur_x) {
                cell.c = c;
                cell.fg = fg;
                cell.bg = bg;
                cell.presentation = CELL_PRESENTATION_AUTO;
            }
        }
        self.cur_x += 1;
    }

    pub fn apply_presentation_selector(&mut self, presentation: u8) {
        if self.cur_x == 0 {
            return;
        }
        let Some(line) = self.lines.get_mut(self.cur_y) else {
            return;
        };
        let cell_x = self.cur_x - 1;
        if let Some(cell) = line.get_mut(cell_x) {
            if cell.c != ' ' {
                cell.presentation = if presentation == CELL_PRESENTATION_EMOJI
                    && crate::renderer::terminal_force_text_presentation(cell.c)
                {
                    CELL_PRESENTATION_TEXT
                } else {
                    presentation
                };
            }
        }
    }

    pub fn newline(&mut self) {
        if self.cur_y == self.scroll_region.1 {
            self.scroll_region_up(1);
        } else if self.cur_y + 1 < self.visible_rows {
            self.cur_y += 1;
        } else {
            self.scroll_region_up(1);
            self.cur_y = self.visible_rows.saturating_sub(1);
        }
    }

    pub fn scroll_region_up(&mut self, rows: usize) {
        let (top, bottom) = self.scroll_region;
        if bottom >= self.lines.len() || top >= bottom {
            return;
        }
        for _ in 0..rows {
            let mut removed = self
                .lines
                .remove(top)
                .unwrap_or_else(|| vec![Cell::default(); self.cols]);
            if top == 0 && bottom == self.visible_rows.saturating_sub(1) {
                if !self.is_alt {
                    self.scrollback.push_back(removed);
                    if self.scrollback.len() > 10000 {
                        if let Some(mut old) = self.scrollback.pop_front() {
                            if self.pool.len() < 128 {
                                old.clear();
                                self.pool.push(old);
                            }
                        }
                    }
                } else {
                    if self.pool.len() < 128 {
                        removed.clear();
                        self.pool.push(removed);
                    }
                }
            } else {
                if self.pool.len() < 128 {
                    removed.clear();
                    self.pool.push(removed);
                }
            }
            let mut new_line = self
                .pool
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(self.cols));
            new_line.resize(self.cols, Cell::default());
            self.lines.insert(bottom, new_line);
        }
        self.dirty = true;
    }

    pub fn scroll_region_down(&mut self, rows: usize) {
        let (top, bottom) = self.scroll_region;
        if bottom >= self.lines.len() || top >= bottom {
            return;
        }
        for _ in 0..rows {
            if let Some(mut removed) = self.lines.remove(bottom) {
                if self.pool.len() < 128 {
                    removed.clear();
                    self.pool.push(removed);
                }
            }
            let mut new_line = self
                .pool
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(self.cols));
            new_line.resize(self.cols, Cell::default());
            self.lines.insert(top, new_line);
        }
        self.dirty = true;
    }

    pub fn get_selection_text(&self) -> String {
        if let Some((sx, sy, ex, ey)) = self.selection {
            let mut res = String::new();
            let total_lines = self.scrollback.len() + self.lines.len();
            let (start_x, start_y, end_x, end_y) = normalized_selection_bounds(sx, sy, ex, ey);

            for y in start_y..=end_y {
                if y >= total_lines {
                    continue;
                }
                let row = if y < self.scrollback.len() {
                    &self.scrollback[y]
                } else {
                    &self.lines[y - self.scrollback.len()]
                };
                let line_start = if y == start_y { start_x } else { 0 };
                let line_end = if y == end_y {
                    end_x
                } else {
                    self.cols.saturating_sub(1)
                };

                for x in line_start..=line_end {
                    if x < row.len() {
                        res.push(row[x].c);
                    }
                }
                if y != end_y {
                    res.push('\n');
                }
            }
            res.trim_end().to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(grid: &mut TermGrid, bytes: &[u8]) {
        let mut parser = Parser::new();
        parser.advance(grid, bytes);
    }

    fn set_line(grid: &mut TermGrid, row: usize, text: &str) {
        for (x, ch) in text.chars().enumerate() {
            grid.lines[row][x].c = ch;
        }
    }

    #[test]
    fn terminal_spawn_error_is_ready_and_keeps_error_text_visible() {
        let mut grid = TermGrid::new(48, 3);
        write_terminal_spawn_error(&mut grid, &io::Error::other("spawn failed"));

        assert!(grid.presentation_ready);
        assert!(!grid.presentation_visible());
        grid.mark_presentation_layout_ready();
        assert!(grid.presentation_visible());
        let text = grid
            .lines
            .iter()
            .flat_map(|line| line.iter().map(|cell| cell.c))
            .collect::<String>();
        assert!(text.contains("RRiter terminal error: spawn failed"));
    }

    #[test]
    fn terminal_grid_print_scroll_resize_and_selection_end_to_end() {
        let mut grid = TermGrid::new(4, 2);
        for ch in "abcd".chars() {
            grid.put_char(ch);
        }
        grid.put_char('e');

        assert_eq!(grid.lines[0][0].c, 'a');
        assert_eq!(grid.lines[0][3].c, 'd');
        assert_eq!(grid.lines[1][0].c, 'e');

        grid.newline();
        grid.execute(b'\r');
        grid.put_char('z');
        assert_eq!(grid.scrollback.len(), 1);
        assert_eq!(grid.lines[1][0].c, 'z');

        grid.selection = Some((0, 2, 0, 2));
        assert_eq!(grid.get_selection_text(), "z");

        grid.resize(6, 3);
        assert_eq!(grid.cols, 6);
        assert_eq!(grid.visible_rows, 3);
        assert!(grid.lines.iter().all(|line| line.len() == 6));
    }

    #[test]
    fn terminal_print_applies_text_selector_but_keeps_text_default_symbols_text() {
        let mut grid = TermGrid::new(8, 2);

        feed(&mut grid, "✔\u{FE0F}X✅\u{FE0E}Y".as_bytes());

        assert_eq!(
            grid.lines[0][0..4]
                .iter()
                .map(|cell| cell.c)
                .collect::<String>(),
            "✔X✅Y"
        );
        assert_eq!(grid.cur_x, 4);
        assert_eq!(grid.lines[0][0].presentation, CELL_PRESENTATION_TEXT);
        assert_eq!(grid.lines[0][1].presentation, CELL_PRESENTATION_AUTO);
        assert_eq!(grid.lines[0][2].presentation, CELL_PRESENTATION_TEXT);
        assert_eq!(grid.lines[0][3].presentation, CELL_PRESENTATION_AUTO);
        assert_eq!(
            terminal_presentation_selector('\u{FE0F}'),
            Some(CELL_PRESENTATION_EMOJI)
        );
        assert_eq!(
            terminal_presentation_selector('\u{FE0E}'),
            Some(CELL_PRESENTATION_TEXT)
        );
        assert!(is_terminal_zero_width_format('\u{FE0F}'));
        assert!(is_terminal_zero_width_format('\u{FE0E}'));
        assert!(is_terminal_zero_width_format('\u{200D}'));
        assert!(!is_terminal_zero_width_format('✔'));
    }

    #[test]
    fn terminal_cell_presentation_flag_keeps_cell_size_tight() {
        assert_eq!(std::mem::size_of::<Cell>(), 8);
    }

    #[test]
    fn terminal_csi_cursor_modes_colors_and_replies_end_to_end() {
        let mut grid = TermGrid::new(8, 3);
        let (tx, rx) = std::sync::mpsc::channel();
        grid.reply_tx = Some(tx);

        feed(
            &mut grid,
            b"abc\x1b[2D!\x1b[s\x1b[3;8H?\x1b[u\x1b[31;44;1mX\x1b[22mY\x1b[38;5;200;48;5;17mZ",
        );

        assert_eq!(grid.lines[0][0].c, 'a');
        assert_eq!(grid.lines[0][1].c, '!');
        assert_eq!(grid.lines[2][7].c, '?');
        assert_eq!(
            (grid.lines[0][2].c, grid.lines[0][2].fg, grid.lines[0][2].bg),
            ('X', 9, 4)
        );
        assert_eq!(
            (grid.lines[0][3].c, grid.lines[0][3].fg, grid.lines[0][3].bg),
            ('Y', 1, 4)
        );
        assert_eq!(
            (grid.lines[0][4].c, grid.lines[0][4].fg, grid.lines[0][4].bg),
            ('Z', 200, 17)
        );

        feed(&mut grid, b"\x1b[?25l\x1b[?1h\x1b[?1000h");
        assert!(!grid.cursor_visible);
        assert!(grid.app_cursor_keys);
        assert!(grid.mouse_tracking);

        feed(&mut grid, b"\x1b[6n\x1b[c\x1b]10;?\x1b\\");
        let reply_pos = rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        let reply_device = rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        let reply_color = rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        assert!(
            String::from_utf8(reply_pos)
                .unwrap()
                .starts_with("\x1B[1;6R")
        );
        assert_eq!(reply_device, b"\x1B[?62c");
        assert_eq!(reply_color, b"\x1B]10;rgb:ffff/ffff/ffff\x1B\\");

        feed(&mut grid, b"\x1b[?1049hALT\x1b[?1049l");
        assert!(!grid.is_alt);
        assert!(grid.alt_lines.is_none());
        assert_eq!(grid.lines[0][0].c, 'a');
    }

    #[test]
    fn terminal_csi_erases_scroll_region_and_line_mutation_end_to_end() {
        let mut grid = TermGrid::new(5, 4);
        set_line(&mut grid, 0, "abcde");
        set_line(&mut grid, 1, "fghij");
        set_line(&mut grid, 2, "klmno");
        set_line(&mut grid, 3, "pqrst");

        feed(&mut grid, b"\x1b[2;2H\x1b[K");
        assert_eq!(grid.lines[1][0].c, 'f');
        assert_eq!(grid.lines[1][1].c, ' ');

        feed(&mut grid, b"\x1b[1;3H\x1b[1K");
        assert_eq!(grid.lines[0][0].c, ' ');
        assert_eq!(grid.lines[0][3].c, 'd');

        feed(&mut grid, b"\x1b[2J");
        assert!(
            grid.lines
                .iter()
                .flat_map(|line| line.iter())
                .all(|cell| cell.c == ' ')
        );

        set_line(&mut grid, 0, "11111");
        set_line(&mut grid, 1, "22222");
        set_line(&mut grid, 2, "33333");
        set_line(&mut grid, 3, "44444");
        feed(&mut grid, b"\x1b[2;3r\x1b[2;1H\x1b[LAAAAA\x1b[2;1H\x1b[M");
        assert_eq!(grid.scroll_region, (1, 2));
        assert_eq!(grid.cur_y, 1);

        feed(&mut grid, b"\x1b[1;1H12345\x1b[1;3H\x1b[2P");
        let top: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        assert_eq!(top, "125  ");

        feed(&mut grid, b"\x1b[1;1Habcde\x1b[1;2H\x1b[3X");
        let top: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        assert_eq!(top, "a   e");

        feed(&mut grid, b"\x1b[3J");
        assert!(grid.scrollback.is_empty());
    }

    #[test]
    fn terminal_resize_preserves_scrollback_saved_cursor_and_alt_buffer() {
        let mut grid = TermGrid::new(4, 4);
        set_line(&mut grid, 0, "aaaa");
        set_line(&mut grid, 1, "bbbb");
        set_line(&mut grid, 2, "cccc");
        set_line(&mut grid, 3, "dddd");
        grid.cur_y = 3;
        grid.saved_cursor = Some((1, 3));

        grid.resize(4, 2);

        assert_eq!(grid.scrollback.len(), 2);
        assert_eq!(grid.cur_y, 1);
        assert_eq!(grid.saved_cursor, Some((1, 1)));
        let row0: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        let row1: String = grid.lines[1].iter().map(|cell| cell.c).collect();
        assert_eq!(row0, "cccc");
        assert_eq!(row1, "dddd");

        grid.resize(4, 4);

        assert!(grid.scrollback.is_empty());
        assert_eq!(grid.cur_y, 3);
        assert_eq!(grid.saved_cursor, Some((1, 3)));
        let row0: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        let row3: String = grid.lines[3].iter().map(|cell| cell.c).collect();
        assert_eq!(row0, "aaaa");
        assert_eq!(row3, "dddd");

        feed(&mut grid, b"\x1b[?1049h");
        assert!(grid.is_alt);
        set_line(&mut grid, 0, "1111");
        set_line(&mut grid, 1, "2222");
        set_line(&mut grid, 2, "3333");
        set_line(&mut grid, 3, "4444");
        grid.cur_y = 3;

        grid.resize(6, 2);

        assert_eq!(grid.cols, 6);
        assert_eq!(grid.visible_rows, 2);
        assert!(grid.scrollback.is_empty());
        assert!(grid.lines.iter().all(|line| line.len() == 6));
        let row0: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        let row1: String = grid.lines[1].iter().map(|cell| cell.c).collect();
        assert_eq!(row0, "3333  ");
        assert_eq!(row1, "4444  ");

        feed(&mut grid, b"\x1b[?1049l");

        assert!(!grid.is_alt);
        assert!(grid.alt_lines.is_none());
        let row0: String = grid.lines[0].iter().map(|cell| cell.c).collect();
        let row1: String = grid.lines[1].iter().map(|cell| cell.c).collect();
        assert_eq!(row0, "aaaa  ");
        assert_eq!(row1, "bbbb  ");
        assert_eq!(grid.cur_y, 1);
    }

    #[test]
    fn terminal_csi_defaults_scroll_modes_and_sgr_edges_end_to_end() {
        let mut grid = TermGrid::new(6, 3);

        feed(&mut grid, b"\t\x08\x08Z");
        assert_eq!((grid.cur_x, grid.cur_y), (1, 1));
        assert_eq!(grid.lines[1][0].c, 'Z');

        feed(&mut grid, b"\x1b[0G");
        assert_eq!(grid.cur_x, 0);
        feed(&mut grid, b"\x1b[99G");
        assert_eq!(grid.cur_x, 5);
        feed(&mut grid, b"\x1b[0d");
        assert_eq!(grid.cur_y, 0);
        feed(&mut grid, b"\x1b[99d");
        assert_eq!(grid.cur_y, 2);
        feed(&mut grid, b"\x1b[0;0f");
        assert_eq!((grid.cur_x, grid.cur_y), (0, 0));

        feed(
            &mut grid,
            b"\x1b[m\x1b[1;34;104mA\x1b[39;49mB\x1b[38;2;1;2;3;48;2;4;5;6mC\x1b[90;107mD\x1b[0mE\x1b[38;5;13;48;5;6mF",
        );
        assert_eq!(
            (grid.lines[0][0].c, grid.lines[0][0].fg, grid.lines[0][0].bg),
            ('A', 12, 12)
        );
        assert_eq!(
            (grid.lines[0][1].c, grid.lines[0][1].fg, grid.lines[0][1].bg),
            ('B', 7, 0)
        );
        assert_eq!(
            (grid.lines[0][2].c, grid.lines[0][2].fg, grid.lines[0][2].bg),
            ('C', 7, 0)
        );
        assert_eq!(
            (grid.lines[0][3].c, grid.lines[0][3].fg, grid.lines[0][3].bg),
            ('D', 8, 15)
        );
        assert_eq!(
            (grid.lines[0][4].c, grid.lines[0][4].fg, grid.lines[0][4].bg),
            ('E', 7, 0)
        );
        assert_eq!(
            (grid.lines[0][5].c, grid.lines[0][5].fg, grid.lines[0][5].bg),
            ('F', 13, 6)
        );
        assert!(!grid.cur_bold);

        feed(&mut grid, b"\x1b[?25l\x1b[?1h\x1b[?1002h");
        assert!(!grid.cursor_visible);
        assert!(grid.app_cursor_keys);
        assert!(grid.mouse_tracking);
        feed(&mut grid, b"\x1b[?25h\x1b[?1l\x1b[?1006l");
        assert!(grid.cursor_visible);
        assert!(!grid.app_cursor_keys);
        assert!(!grid.mouse_tracking);

        let mut scroll = TermGrid::new(4, 3);
        set_line(&mut scroll, 0, "aaaa");
        set_line(&mut scroll, 1, "bbbb");
        set_line(&mut scroll, 2, "cccc");

        feed(&mut scroll, b"\x1b[2;3r\x1b[S");
        let row0: String = scroll.lines[0].iter().map(|cell| cell.c).collect();
        let row1: String = scroll.lines[1].iter().map(|cell| cell.c).collect();
        let row2: String = scroll.lines[2].iter().map(|cell| cell.c).collect();
        assert_eq!(row0, "aaaa");
        assert_eq!(row1, "cccc");
        assert_eq!(row2, "    ");

        feed(&mut scroll, b"\x1b[T");
        let row1: String = scroll.lines[1].iter().map(|cell| cell.c).collect();
        let row2: String = scroll.lines[2].iter().map(|cell| cell.c).collect();
        assert_eq!(row1, "    ");
        assert_eq!(row2, "cccc");
    }

    #[test]
    fn terminal_osc_insert_delete_and_truecolor_edges_end_to_end() {
        let mut grid = TermGrid::new(8, 4);
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        grid.reply_tx = Some(reply_tx);

        feed(&mut grid, b"\x1b]10;?\x1b\\");
        assert_eq!(
            reply_rx.try_recv().unwrap(),
            b"\x1B]10;rgb:ffff/ffff/ffff\x1B\\".to_vec()
        );

        feed(&mut grid, b"\x1b[2;1Habcdefgh\x1b[2;3H\x1b[2P");
        let row1: String = grid.lines[1].iter().map(|cell| cell.c).collect();
        assert_eq!(row1, "abefgh  ");

        feed(&mut grid, b"\x1b[2;2H\x1b[3X");
        let row1: String = grid.lines[1].iter().map(|cell| cell.c).collect();
        assert_eq!(row1, "a   gh  ");

        set_line(&mut grid, 1, "11111111");
        set_line(&mut grid, 2, "22222222");
        set_line(&mut grid, 3, "33333333");
        feed(&mut grid, b"\x1b[2;4r\x1b[3;1H\x1b[L");
        assert_eq!(
            grid.lines[2].iter().map(|cell| cell.c).collect::<String>(),
            "        "
        );
        assert_eq!(
            grid.lines[3].iter().map(|cell| cell.c).collect::<String>(),
            "22222222"
        );

        feed(&mut grid, b"\x1b[2;1H\x1b[M");
        assert_eq!(
            grid.lines[1].iter().map(|cell| cell.c).collect::<String>(),
            "        "
        );

        feed(&mut grid, b"\x1b[38;2;1;2;3m\x1b[48;5;42mX\x1b[39;49m");
        assert_eq!(grid.cur_bg, 0);
        assert_eq!(grid.cur_fg, 7);
        assert_eq!(grid.lines[1][0].bg, 42);

        let before = (grid.cols, grid.visible_rows, grid.lines.len());
        grid.resize(before.0, before.1);
        assert_eq!((grid.cols, grid.visible_rows, grid.lines.len()), before);

        grid.scroll_region = (2, 2);
        grid.scroll_region_up(1);
        grid.scroll_region_down(1);
        assert_eq!(grid.scroll_region, (2, 2));
    }

    #[test]
    fn terminal_resize_selection_and_alt_growth_edges() {
        let mut grid = TermGrid::new(3, 2);
        set_line(&mut grid, 0, "abc");
        set_line(&mut grid, 1, "def");
        grid.scrollback.push_back(vec![
            Cell {
                c: 's',
                fg: 7,
                bg: 0,
                presentation: CELL_PRESENTATION_AUTO,
            };
            3
        ]);
        grid.saved_cursor = Some((2, 1));

        grid.resize(3, 4);
        assert_eq!(grid.visible_rows, 4);
        assert_eq!(grid.cur_y, 1);
        assert_eq!(grid.saved_cursor, Some((2, 2)));
        assert!(grid.scrollback.is_empty());
        assert_eq!(
            grid.lines[0].iter().map(|cell| cell.c).collect::<String>(),
            "sss"
        );

        grid.selection = Some((2, 2, 1, 0));
        assert_eq!(grid.get_selection_text(), "ss\nabc\ndef");
        grid.selection = Some((0, 99, 2, 100));
        assert_eq!(grid.get_selection_text(), "");
        grid.selection = None;
        assert_eq!(grid.get_selection_text(), "");

        feed(&mut grid, b"\x1b[?1049h");
        assert!(grid.is_alt);
        grid.alt_saved_cursor = Some((9, 9));
        grid.resize(5, 5);
        assert_eq!(grid.alt_saved_cursor, Some((9, 4)));
        assert_eq!(grid.lines.len(), 5);
        assert!(grid.lines.iter().all(|line| line.len() == 5));
        grid.resize(5, 3);
        assert_eq!(grid.lines.len(), 3);
        assert_eq!(grid.visible_rows, 3);
    }

    #[test]
    fn terminal_csi_more_erase_cursor_and_reply_edges() {
        let mut grid = TermGrid::new(5, 3);
        set_line(&mut grid, 0, "abcde");
        set_line(&mut grid, 1, "fghij");
        set_line(&mut grid, 2, "klmno");

        feed(&mut grid, b"\x1b[2;3H\x1b[J");
        assert_eq!(
            grid.lines[1].iter().map(|cell| cell.c).collect::<String>(),
            "fg   "
        );
        assert_eq!(
            grid.lines[2].iter().map(|cell| cell.c).collect::<String>(),
            "     "
        );

        set_line(&mut grid, 0, "abcde");
        set_line(&mut grid, 1, "fghij");
        set_line(&mut grid, 2, "klmno");
        feed(&mut grid, b"\x1b[2;3H\x1b[1J");
        assert_eq!(
            grid.lines[0].iter().map(|cell| cell.c).collect::<String>(),
            "     "
        );
        assert_eq!(
            grid.lines[1].iter().map(|cell| cell.c).collect::<String>(),
            "   ij"
        );

        set_line(&mut grid, 1, "fghij");
        feed(&mut grid, b"\x1b[2;3H\x1b[2K");
        assert_eq!(
            grid.lines[1].iter().map(|cell| cell.c).collect::<String>(),
            "     "
        );

        feed(&mut grid, b"\x1b[1;1H\x1b[10C\x1b[2D\x1b[2B\x1b[A");
        assert_eq!((grid.cur_x, grid.cur_y), (2, 1));

        feed(&mut grid, b"\x1b7\x1b[3;5H\x1b8");
        assert_eq!((grid.cur_x, grid.cur_y), (2, 1));

        let (tx, rx) = std::sync::mpsc::channel();
        grid.reply_tx = Some(tx);
        feed(&mut grid, b"\x1b]11;?\x1b\\");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_millis(50))
                .unwrap(),
            b"\x1B]11;rgb:ffff/ffff/ffff\x1B\\".to_vec()
        );

        feed(&mut grid, b"\x1b]10;?\x07");
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_millis(50))
                .unwrap(),
            b"\x1B]10;rgb:ffff/ffff/ffff\x1B\\".to_vec()
        );

        feed(&mut grid, b"\x1b[38;5;123m\x1b[48;2;9;8;7mQ\x1b[999m");
        assert_eq!(grid.lines[1][2].c, 'Q');
        assert_eq!(grid.lines[1][2].fg, 123);
        assert_eq!(grid.lines[1][2].bg, 0);
    }

    #[test]
    fn bug_70_poisoned_terminal_mutex_recovers_without_ui_thread_panic() {
        let grid = Mutex::new(TermGrid::new(4, 2));
        let mut guard = lock_terminal_grid(&grid);
        guard.put_char('R');
        assert_eq!(guard.lines[0][0].c, 'R');
        drop(guard);

        let source = include_str!("terminal.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("unwrap_or_else(std::sync::PoisonError::into_inner)"));
        assert!(!production.contains("grid.lock().unwrap()"));
    }
}

impl Perform for TermGrid {
    fn print(&mut self, c: char) {
        if let Some(presentation) = terminal_presentation_selector(c) {
            self.apply_presentation_selector(presentation);
            return;
        }
        if is_terminal_zero_width_format(c) {
            return;
        }
        self.put_char(c);
        if !self.presentation_ready && !c.is_whitespace() {
            self.mark_presentation_ready();
        }
    }
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | b'\x0B' | b'\x0C' => self.newline(),
            b'\r' => self.cur_x = 0,
            b'\x08' => self.cur_x = self.cur_x.saturating_sub(1),
            b'\t' => {
                let spaces = 8 - (self.cur_x % 8);
                for _ in 0..spaces {
                    self.put_char(' ');
                }
            }
            _ => {}
        }
    }
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params
            .first()
            .is_some_and(|selector| *selector == b"0" || *selector == b"2")
            && let Some(title) =
                crate::app::terminal_process::terminal_programmed_title(&params[1..])
            && let Some(cache) = &self.title_cache
        {
            crate::platform::lock_recover(cache).set_programmed(title);
        }

        if params.len() >= 2 && params[1] == b"?" {
            if params[0] == b"10" || params[0] == b"11" {
                if let Some(tx) = &self.reply_tx {
                    let prefix = std::str::from_utf8(params[0]).unwrap_or("10");
                    let msg = format!("\x1B]{};rgb:ffff/ffff/ffff\x1B\\", prefix);
                    let _ = tx.send(msg.into_bytes());
                }
            }
        }
    }
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.saved_cursor = Some((self.cur_x, self.cur_y)),
            b'8' => {
                if let Some((x, y)) = self.saved_cursor {
                    self.cur_x = x;
                    self.cur_y = y;
                }
            }
            _ => {}
        }
    }
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        match action {
            'h' | 'l' => {
                let enable = action == 'h';
                let is_private = intermediates.contains(&b'?');
                if is_private {
                    for param in params.iter() {
                        if param[0] == 1049 || param[0] == 47 || param[0] == 1047 {
                            if enable && !self.is_alt {
                                self.is_alt = true;
                                self.alt_saved_cursor = Some((self.cur_x, self.cur_y));
                                let mut alt = std::collections::VecDeque::new();
                                for _ in 0..self.visible_rows {
                                    let mut line = self
                                        .pool
                                        .pop()
                                        .unwrap_or_else(|| Vec::with_capacity(self.cols));
                                    line.resize(self.cols, Cell::default());
                                    line.fill(Cell::default());
                                    alt.push_back(line);
                                }
                                self.alt_lines = Some(std::mem::replace(&mut self.lines, alt));
                                self.cur_x = 0;
                                self.cur_y = 0;
                                self.dirty = true;
                            } else if !enable && self.is_alt {
                                self.is_alt = false;
                                if let Some(alt) = self.alt_lines.take() {
                                    let old_lines = std::mem::replace(&mut self.lines, alt);
                                    for mut line in old_lines {
                                        if self.pool.len() < 128 {
                                            line.clear();
                                            self.pool.push(line);
                                        }
                                    }
                                }
                                if let Some((x, y)) = self.alt_saved_cursor.take() {
                                    self.cur_x = x;
                                    self.cur_y = y;
                                }
                                self.cur_x = self.cur_x.min(self.cols.saturating_sub(1));
                                self.cur_y = self.cur_y.min(self.visible_rows.saturating_sub(1));
                                self.dirty = true;
                            }
                        } else if param[0] == 25 {
                            self.cursor_visible = enable;
                        } else if param[0] == 1 {
                            self.app_cursor_keys = enable;
                        } else if param[0] == 1000 || param[0] == 1002 || param[0] == 1006 {
                            self.mouse_tracking = enable;
                        }
                    }
                }
            }
            's' => self.saved_cursor = Some((self.cur_x, self.cur_y)),
            'u' => {
                if let Some((x, y)) = self.saved_cursor {
                    self.cur_x = x;
                    self.cur_y = y;
                }
            }
            'G' | '`' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_x = p.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
            'd' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_y = p.saturating_sub(1).min(self.visible_rows.saturating_sub(1));
            }
            'c' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if param == 0 {
                    if let Some(tx) = &self.reply_tx {
                        let _ = tx.send(b"\x1B[?62c".to_vec());
                    }
                }
            }
            'n' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if param == 6 {
                    if let Some(tx) = &self.reply_tx {
                        let msg = format!("\x1B[{};{}R", self.cur_y + 1, self.cur_x + 1);
                        let _ = tx.send(msg.into_bytes());
                    }
                }
            }
            'H' | 'f' => {
                let mut iter = params.iter();
                let y = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
                let x = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
                let y = if y == 0 { 1 } else { y };
                let x = if x == 0 { 1 } else { x };
                self.cur_y = y.saturating_sub(1).min(self.visible_rows.saturating_sub(1));
                self.cur_x = x.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
            'J' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                match param {
                    0 => {
                        if let Some(line) = self.lines.get_mut(self.cur_y) {
                            if self.cur_x < line.len() {
                                line[self.cur_x..].fill(Cell::default());
                            }
                        }
                        for i in (self.cur_y + 1)..self.visible_rows {
                            if let Some(line) = self.lines.get_mut(i) {
                                line.fill(Cell::default());
                            }
                        }
                    }
                    1 => {
                        for i in 0..self.cur_y {
                            if let Some(line) = self.lines.get_mut(i) {
                                line.fill(Cell::default());
                            }
                        }
                        if let Some(line) = self.lines.get_mut(self.cur_y) {
                            let end = (self.cur_x + 1).min(line.len());
                            line[..end].fill(Cell::default());
                        }
                    }
                    2 | 3 => {
                        for line in self.lines.iter_mut() {
                            line.fill(Cell::default());
                        }
                        if param == 3 {
                            self.scrollback.clear();
                        }
                    }
                    _ => {}
                }
            }
            'K' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if let Some(line) = self.lines.get_mut(self.cur_y) {
                    match param {
                        0 => {
                            if self.cur_x < line.len() {
                                line[self.cur_x..].fill(Cell::default());
                            }
                        }
                        1 => {
                            let end = (self.cur_x + 1).min(line.len());
                            line[..end].fill(Cell::default());
                        }
                        2 => {
                            line.fill(Cell::default());
                        }
                        _ => {}
                    }
                }
            }
            'r' => {
                let mut iter = params.iter();
                let top = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
                let bottom = iter
                    .next()
                    .map(|p| p[0])
                    .unwrap_or(self.visible_rows as u16) as usize;
                let top = if top == 0 { 1 } else { top };
                let bottom = if bottom == 0 {
                    self.visible_rows
                } else {
                    bottom
                };
                let top_idx = top
                    .saturating_sub(1)
                    .min(self.visible_rows.saturating_sub(1));
                let bottom_idx = bottom
                    .saturating_sub(1)
                    .min(self.visible_rows.saturating_sub(1));
                if bottom_idx >= top_idx {
                    self.scroll_region = (top_idx, bottom_idx);
                }
                self.cur_x = 0;
                self.cur_y = 0;
            }
            'L' => {
                let count = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let count = if count == 0 { 1 } else { count };
                let bottom = self.scroll_region.1;
                for _ in 0..count {
                    if self.cur_y <= bottom && bottom < self.lines.len() {
                        if let Some(mut line) = self.lines.remove(bottom) {
                            if self.pool.len() < 128 {
                                line.clear();
                                self.pool.push(line);
                            }
                        }
                        let mut new_line = self
                            .pool
                            .pop()
                            .unwrap_or_else(|| Vec::with_capacity(self.cols));
                        new_line.resize(self.cols, Cell::default());
                        new_line.fill(Cell::default());
                        self.lines.insert(self.cur_y, new_line);
                    }
                }
            }
            'M' => {
                let count = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let count = if count == 0 { 1 } else { count };
                let bottom = self.scroll_region.1;
                for _ in 0..count {
                    if self.cur_y <= bottom && bottom < self.lines.len() {
                        if let Some(mut line) = self.lines.remove(self.cur_y) {
                            if self.pool.len() < 128 {
                                line.clear();
                                self.pool.push(line);
                            }
                        }
                        let mut new_line = self
                            .pool
                            .pop()
                            .unwrap_or_else(|| Vec::with_capacity(self.cols));
                        new_line.resize(self.cols, Cell::default());
                        new_line.fill(Cell::default());
                        self.lines.insert(bottom, new_line);
                    }
                }
            }
            'P' => {
                let count = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let count = if count == 0 { 1 } else { count };
                if let Some(line) = self.lines.get_mut(self.cur_y) {
                    for _ in 0..count {
                        if self.cur_x < line.len() {
                            line.remove(self.cur_x);
                            line.push(Cell::default());
                        }
                    }
                }
            }
            'X' => {
                let count = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let count = if count == 0 { 1 } else { count };
                if let Some(line) = self.lines.get_mut(self.cur_y) {
                    let start = self.cur_x.min(line.len());
                    let end = (self.cur_x + count).min(line.len());
                    if start < end {
                        line[start..end].fill(Cell::default());
                    }
                }
            }
            'm' => {
                let mut fg = Some(self.cur_fg);
                apply_ansi_sgr(
                    params,
                    &mut fg,
                    &mut self.cur_bold,
                    Some(7),
                    Some(&mut self.cur_bg),
                    0,
                );
                self.cur_fg = fg.unwrap_or(7);
            }
            'C' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_x = (self.cur_x + p).min(self.cols.saturating_sub(1));
            }
            'D' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_x = self.cur_x.saturating_sub(p);
            }
            'A' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_y = self.cur_y.saturating_sub(p);
            }
            'B' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.cur_y = (self.cur_y + p).min(self.visible_rows.saturating_sub(1));
            }
            'S' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.scroll_region_up(p);
            }
            'T' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let p = if p == 0 { 1 } else { p };
                self.scroll_region_down(p);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalPresentationIntent {
    None,
    ActivateWhenReady,
    OpenPanelWhenReady,
}

pub struct Terminal {
    pub grid: Arc<Mutex<TermGrid>>,
    process: Option<crate::app::terminal_process::TerminalProcess>,
    pub scroll_y: crate::scroll::ScrollState,
    pub(crate) presentation_intent: TerminalPresentationIntent,
    pub(crate) reveal_right_tail_when_presented: bool,
    title_cache: crate::app::terminal_process::TerminalTitleCache,
}

fn write_terminal_spawn_error(grid: &mut TermGrid, error: &io::Error) {
    let message = format!("RRiter terminal error: {error}\r\n");
    let mut parser = Parser::new();
    parser.advance(grid, message.as_bytes());
    grid.mark_presentation_ready();
}

impl Terminal {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn spawn(
        window: Option<std::sync::Arc<winit::window::Window>>,
        cwd: Option<&std::path::Path>,
        display_number: u64,
    ) -> Self {
        let title_cache = Arc::new(Mutex::new(
            crate::app::terminal_process::TerminalTitleState::new_numbered(
                "terminal".to_string(),
                display_number,
            ),
        ));
        let grid = Arc::new(Mutex::new(TermGrid::new_with_title_cache(
            200,
            60,
            title_cache.clone(),
        )));
        let result = crate::app::terminal_process::TerminalProcess::spawn(
            grid.clone(),
            title_cache.clone(),
            window,
            cwd,
        );
        let process = match result {
            Ok((process, _shell)) => Some(process),
            Err(error) => {
                crate::platform::lock_recover(&title_cache)
                    .set_fallback("terminal error".to_string());
                let mut grid = crate::platform::lock_recover(&grid);
                write_terminal_spawn_error(&mut grid, &error);
                None
            }
        };

        Self {
            grid,
            process,
            scroll_y: crate::scroll::ScrollState::new(7.0),
            presentation_intent: TerminalPresentationIntent::None,
            reveal_right_tail_when_presented: false,
            title_cache,
        }
    }

    pub(crate) fn write_display_title(&self, output: &mut String) {
        crate::platform::lock_recover(&self.title_cache).write_resolved(output);
    }

    pub fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        self.process
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "terminal is not running"))?
            .write_input(bytes)
    }

    pub fn is_closed(&mut self) -> bool {
        self.process
            .as_mut()
            .is_some_and(|process| process.try_wait().unwrap_or(true))
    }

    pub fn shutdown(&mut self) {
        if let Some(process) = self.process.as_mut() {
            process.shutdown();
        }
        self.process = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn resize_pty(&self, cols: u16, rows: u16) {
        if let Some(process) = self.process.as_ref() {
            let _ = process.resize(cols, rows);
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}
