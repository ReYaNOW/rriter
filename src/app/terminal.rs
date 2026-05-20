use alacritty_terminal::vte::{Params, Parser, Perform};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: u8,
    pub bg: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            fg: 7,
            bg: 0,
        }
    }
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
    pub selection: Option<(usize, usize, usize, usize)>,
    pub reply_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    pub saved_cursor: Option<(usize, usize)>,
    pub scroll_region: (usize, usize),
    pub cursor_visible: bool,
    pub app_cursor_keys: bool,
    pub mouse_tracking: bool,
    pub pool: Vec<Vec<Cell>>,
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
            selection: None,
            reply_tx: None,
            saved_cursor: None,
            scroll_region: (0, visible_rows.saturating_sub(1)),
            cursor_visible: true,
            app_cursor_keys: false,
            mouse_tracking: false,
            pool: Vec::with_capacity(128),
        }
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
            }
        }
        self.cur_x += 1;
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
            b"\x1b[m\x1b[1;34;104mA\x1b[39;49mB\x1b[38;2;1;2;3;48;2;4;5;6mC\x1b[90;107mD\x1b[0mE",
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
                bg: 0
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

        feed(&mut grid, b"\x1b[38;5;123m\x1b[48;2;9;8;7mQ\x1b[999m");
        assert_eq!(grid.lines[1][2].c, 'Q');
        assert_eq!(grid.lines[1][2].fg, 123);
        assert_eq!(grid.lines[1][2].bg, 0);
    }
}

impl Perform for TermGrid {
    fn print(&mut self, c: char) {
        self.put_char(c);
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
                if params.is_empty() {
                    self.cur_fg = 7;
                    self.cur_bg = 0;
                    self.cur_bold = false;
                    return;
                }
                let mut i = 0;
                let iter: Vec<&[u16]> = params.iter().collect();
                while i < iter.len() {
                    if iter[i].is_empty() {
                        i += 1;
                        continue;
                    }
                    let p = iter[i][0];
                    match p {
                        0 => {
                            self.cur_fg = 7;
                            self.cur_bg = 0;
                            self.cur_bold = false;
                        }
                        1 => {
                            self.cur_bold = true;
                            if self.cur_fg < 8 {
                                self.cur_fg += 8;
                            }
                        }
                        22 => {
                            self.cur_bold = false;
                            if self.cur_fg >= 8 && self.cur_fg < 16 {
                                self.cur_fg -= 8;
                            }
                        }
                        30..=37 => self.cur_fg = (p - 30) as u8 + if self.cur_bold { 8 } else { 0 },
                        40..=47 => self.cur_bg = (p - 40) as u8,
                        90..=97 => self.cur_fg = (p - 90 + 8) as u8,
                        100..=107 => self.cur_bg = (p - 100 + 8) as u8,
                        39 => self.cur_fg = 7,
                        49 => self.cur_bg = 0,
                        38 | 48 => {
                            if i + 1 < iter.len() && !iter[i + 1].is_empty() {
                                let mode = iter[i + 1][0];
                                if mode == 5 && i + 2 < iter.len() && !iter[i + 2].is_empty() {
                                    let color = iter[i + 2][0] as u8;
                                    if p == 38 {
                                        self.cur_fg = color;
                                    } else {
                                        self.cur_bg = color;
                                    }
                                    i += 2;
                                } else if mode == 2 && i + 4 < iter.len() {
                                    i += 4;
                                } else {
                                    i += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
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

pub struct Terminal {
    pub grid: Arc<Mutex<TermGrid>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master_pty: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    pub scroll_y: crate::scroll::ScrollState,
    pub title: String,
}

impl Terminal {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn spawn(window: Option<std::sync::Arc<winit::window::Window>>) -> Self {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 60,
                cols: 200,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let shell_name = std::path::Path::new(&shell)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "term".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).unwrap();

        // КРИТИЧНО для Linux: освобождаем дескриптор slave в родительском процессе
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
        let writer_arc = Arc::new(Mutex::new(writer));

        let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer_for_reply = writer_arc.clone();
        std::thread::spawn(move || {
            while let Ok(msg) = reply_rx.recv() {
                if let Ok(mut w) = writer_for_reply.lock() {
                    let _ = w.write_all(&msg);
                    let _ = w.flush();
                }
            }
        });

        let mut grid_obj = TermGrid::new(200, 60);
        grid_obj.reply_tx = Some(reply_tx);
        let grid = Arc::new(Mutex::new(grid_obj));
        let grid_clone = grid.clone();

        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 65536];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });

        std::thread::spawn(move || {
            let mut parser = Parser::new();
            while let Ok(chunk) = rx.recv() {
                let mut chunks = vec![chunk];
                let start = std::time::Instant::now();

                // Умный Nagle-буфер: собираем микро-куски, пока труба не замолчит на 8мс
                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(8)) {
                        Ok(more) => {
                            chunks.push(more);
                            // Предохранитель от зависания при бесконечном потоке (например, команда yes)
                            if start.elapsed().as_millis() >= 32 {
                                break;
                            }
                        }
                        Err(_) => break, // Вывод завершен
                    }
                }

                let mut g = grid_clone.lock().unwrap();
                for c in &chunks {
                    parser.advance(&mut *g, c);
                }
                g.dirty = true;
                drop(g);

                if let Some(w) = window.as_ref() {
                    w.request_redraw();
                }
            }
        });

        let master_pty = Arc::new(Mutex::new(pair.master));

        let scroll_y = crate::scroll::ScrollState::new(7.0);

        Self {
            grid,
            writer: writer_arc,
            master_pty,
            child: Arc::new(Mutex::new(child)),
            scroll_y,
            title: shell_name,
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn resize_pty(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        // Call synchronously: shell must get SIGWINCH immediately after grid resize,
        // otherwise cursor positions diverge (fish/zsh redraw prompt at wrong row).
        if let Ok(master) = self.master_pty.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }
}
