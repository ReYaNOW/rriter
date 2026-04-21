use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::sync::{Arc, Mutex};
use std::io::{Read, Write};
use alacritty_terminal::vte::{Parser, Perform, Params};

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: u8,
    pub bg: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { c: ' ', fg: 7, bg: 0 }
    }
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
        }
    }

                        pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        if new_cols == self.cols && new_rows == self.visible_rows { return; }

        if new_cols != self.cols {
            for line in self.lines.iter_mut() { line.resize(new_cols, Cell::default()); }
            for line in self.scrollback.iter_mut() { line.resize(new_cols, Cell::default()); }
            if let Some(alt) = &mut self.alt_lines {
                for line in alt.iter_mut() { line.resize(new_cols, Cell::default()); }
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
                self.lines.pop_back();
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
                    if let Some(row) = self.scrollback.pop_back() {
                        self.lines.push_front(row);
                    }
                }

                let blanks = diff - from_scrollback;
                for _ in 0..blanks {
                    self.lines.push_back(vec![Cell::default(); self.cols]);
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
                for _ in 0..diff { alt.pop_back(); }
            } else if new_rows > alt_current_rows {
                let diff = new_rows - alt_current_rows;
                for _ in 0..diff { alt.push_back(vec![Cell::default(); self.cols]); }
            }
            if let Some((_, ref mut sy)) = self.alt_saved_cursor {
                *sy = (*sy).min(new_rows.saturating_sub(1));
            }
        }

        while self.scrollback.len() > 10000 { self.scrollback.pop_front(); }
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
        if bottom >= self.lines.len() || top >= bottom { return; }
        for _ in 0..rows {
            let removed = self.lines.remove(top).unwrap_or_else(|| vec![Cell::default(); self.cols]);
            if top == 0 && bottom == self.visible_rows.saturating_sub(1) {
                if !self.is_alt {
                    self.scrollback.push_back(removed);
                    if self.scrollback.len() > 10000 { self.scrollback.pop_front(); }
                }
            }
            self.lines.insert(bottom, vec![Cell::default(); self.cols]);
        }
        self.dirty = true;
    }

    pub fn scroll_region_down(&mut self, rows: usize) {
        let (top, bottom) = self.scroll_region;
        if bottom >= self.lines.len() || top >= bottom { return; }
        for _ in 0..rows {
            self.lines.remove(bottom);
            self.lines.insert(top, vec![Cell::default(); self.cols]);
        }
        self.dirty = true;
    }

        pub fn get_selection_text(&self) -> String {
        if let Some((sx, sy, ex, ey)) = self.selection {
            let mut res = String::new();
            let total_lines = self.scrollback.len() + self.lines.len();
            let start_y = sy.min(ey);
            let end_y = sy.max(ey);
            let start_x = if sy < ey { sx } else if sy > ey { ex } else { sx.min(ex) };
            let end_x = if sy < ey { ex } else if sy > ey { sx } else { sx.max(ex) };

            for y in start_y..=end_y {
                if y >= total_lines { continue; }
                let row = if y < self.scrollback.len() { &self.scrollback[y] } else { &self.lines[y - self.scrollback.len()] };
                let line_start = if y == start_y { start_x } else { 0 };
                let line_end = if y == end_y { end_x } else { self.cols.saturating_sub(1) };

                for x in line_start..=line_end {
                    if x < row.len() {
                        res.push(row[x].c);
                    }
                }
                if y != end_y { res.push('\n'); }
            }
            res.trim_end().to_string()
        } else {
            String::new()
        }
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
                for _ in 0..spaces { self.put_char(' '); }
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
                                    alt.push_back(vec![Cell::default(); self.cols]);
                                }
                                self.alt_lines = Some(std::mem::replace(&mut self.lines, alt));
                                self.cur_x = 0;
                                self.cur_y = 0;
                                self.dirty = true;
                            } else if !enable && self.is_alt {
                                self.is_alt = false;
                                if let Some(alt) = self.alt_lines.take() {
                                    self.lines = alt;
                                }
                                if let Some((x, y)) = self.alt_saved_cursor.take() {
                                    self.cur_x = x;
                                    self.cur_y = y;
                                }
                                self.cur_x = self.cur_x.min(self.cols.saturating_sub(1));
                                self.cur_y = self.cur_y.min(self.visible_rows.saturating_sub(1));
                                self.dirty = true;
                            }
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
                            for i in self.cur_x..self.cols {
                                if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                            }
                        }
                        for i in (self.cur_y + 1)..self.visible_rows {
                            if let Some(line) = self.lines.get_mut(i) {
                                for cell in line.iter_mut() { *cell = Cell::default(); }
                            }
                        }
                    }
                    1 => {
                        for i in 0..self.cur_y {
                            if let Some(line) = self.lines.get_mut(i) {
                                for cell in line.iter_mut() { *cell = Cell::default(); }
                            }
                        }
                        if let Some(line) = self.lines.get_mut(self.cur_y) {
                            for i in 0..=self.cur_x.min(self.cols - 1) {
                                if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                            }
                        }
                    }
                                        2 | 3 => {
                        for line in self.lines.iter_mut() {
                            for cell in line.iter_mut() { *cell = Cell::default(); }
                        }
                        if param == 3 { self.scrollback.clear(); }
                    }
                    _ => {}
                }
            }
            'K' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if let Some(line) = self.lines.get_mut(self.cur_y) {
                    match param {
                        0 => {
                            for i in self.cur_x..self.cols {
                                if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                            }
                        }
                        1 => {
                            for i in 0..=self.cur_x.min(self.cols - 1) {
                                if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                            }
                        }
                        2 => {
                            for cell in line.iter_mut() { *cell = Cell::default(); }
                        }
                        _ => {}
                    }
                }
            }
                                                'r' => {
                let mut iter = params.iter();
                let top = iter.next().map(|p| p[0]).unwrap_or(1) as usize;
                let bottom = iter.next().map(|p| p[0]).unwrap_or(self.visible_rows as u16) as usize;
                let top = if top == 0 { 1 } else { top };
                let bottom = if bottom == 0 { self.visible_rows } else { bottom };
                let top_idx = top.saturating_sub(1).min(self.visible_rows.saturating_sub(1));
                let bottom_idx = bottom.saturating_sub(1).min(self.visible_rows.saturating_sub(1));
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
                        self.lines.remove(bottom);
                        self.lines.insert(self.cur_y, vec![Cell::default(); self.cols]);
                    }
                }
            }
            'M' => {
                let count = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                let count = if count == 0 { 1 } else { count };
                let bottom = self.scroll_region.1;
                for _ in 0..count {
                    if self.cur_y <= bottom && bottom < self.lines.len() {
                        self.lines.remove(self.cur_y);
                        self.lines.insert(bottom, vec![Cell::default(); self.cols]);
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
                    for i in self.cur_x..(self.cur_x + count).min(self.cols) {
                        if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                    }
                }
            }
                                                'm' => {
                if params.is_empty() {
                    self.cur_fg = 7; self.cur_bg = 0; self.cur_bold = false;
                    return;
                }
                let mut i = 0;
                let iter: Vec<&[u16]> = params.iter().collect();
                while i < iter.len() {
                    if iter[i].is_empty() { i += 1; continue; }
                    let p = iter[i][0];
                    match p {
                        0 => { self.cur_fg = 7; self.cur_bg = 0; self.cur_bold = false; }
                        1 => {
                            self.cur_bold = true;
                            if self.cur_fg < 8 { self.cur_fg += 8; }
                        }
                        22 => {
                            self.cur_bold = false;
                            if self.cur_fg >= 8 && self.cur_fg < 16 { self.cur_fg -= 8; }
                        }
                        30..=37 => self.cur_fg = (p - 30) as u8 + if self.cur_bold { 8 } else { 0 },
                        40..=47 => self.cur_bg = (p - 40) as u8,
                        90..=97 => self.cur_fg = (p - 90 + 8) as u8,
                        100..=107 => self.cur_bg = (p - 100 + 8) as u8,
                        39 => self.cur_fg = 7,
                        49 => self.cur_bg = 0,
                        38 | 48 => {
                            if i + 1 < iter.len() && !iter[i+1].is_empty() {
                                let mode = iter[i+1][0];
                                if mode == 5 && i + 2 < iter.len() && !iter[i+2].is_empty() {
                                    let color = iter[i+2][0] as u8;
                                    if p == 38 { self.cur_fg = color; } else { self.cur_bg = color; }
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
}

impl Terminal {
                        pub fn spawn(window: Option<std::sync::Arc<winit::window::Window>>) -> Self {
        let pty_system = NativePtySystem::default();
                let pair = pty_system.openpty(PtySize {
            rows: 60,
            cols: 200,
            pixel_width: 0,
            pixel_height: 0,
        }).unwrap();

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).unwrap();

        // КРИТИЧНО для Linux: освобождаем дескриптор slave в родительском процессе
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().unwrap();
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

        std::thread::spawn(move || {
            let mut parser = Parser::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 { break; }
                let mut g = grid_clone.lock().unwrap();
                parser.advance(&mut *g, &buf[..n]);
                g.dirty = true;
                drop(g);
                if let Some(w) = window.as_ref() {
                    w.request_redraw();
                }
            }
        });

                                        let master_pty = Arc::new(Mutex::new(pair.master));

        println!("[Terminal] Process spawned");
        
        let scroll_y = crate::scroll::ScrollState::new(7.0);

        Self {
            grid,
            writer: writer_arc,
            master_pty,
            child: Arc::new(Mutex::new(child)),
            scroll_y,
        }
    }

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