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
    pub cols: usize,
    pub visible_rows: usize,
    pub cur_x: usize,
    pub cur_y: usize,
    pub cur_fg: u8,
        pub cur_bg: u8,
    pub dirty: bool,
    pub selection: Option<(usize, usize, usize, usize)>,
    pub reply_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    pub saved_cursor: Option<(usize, usize)>,
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
            cols,
            visible_rows,
            cur_x: 0,
            cur_y: 0,
            cur_fg: 7,
                        cur_bg: 0,
            dirty: true,
            selection: None,
            reply_tx: None,
            saved_cursor: None,
        }
    }

            pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
                self.cols = new_cols;
                for line in self.scrollback.iter_mut() {
                    line.resize(new_cols, Cell::default());
                }
                for line in self.lines.iter_mut() {
                    line.resize(new_cols, Cell::default());
                }

                                if new_rows < self.lines.len() {
            let mut lines_to_remove = self.lines.len() - new_rows;
            while lines_to_remove > 0 && self.lines.len() > self.cur_y + 1 {
                self.lines.pop_back();
                lines_to_remove -= 1;
            }
            for _ in 0..lines_to_remove {
                if let Some(top) = self.lines.pop_front() {
                    self.scrollback.push_back(top);
                    if self.scrollback.len() > 10000 {
                        self.scrollback.pop_front();
                    }
                }
            }
            self.cur_y = self.cur_y.saturating_sub(lines_to_remove);
        } else if new_rows > self.lines.len() {
            let diff = new_rows - self.lines.len();
            for _ in 0..diff {
                self.lines.push_back(vec![Cell::default(); new_cols]);
            }
        }

        self.visible_rows = new_rows;
        self.cur_x = self.cur_x.min(self.cols.saturating_sub(1));
        self.cur_y = self.cur_y.min(self.visible_rows.saturating_sub(1));
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
        if self.cur_y + 1 < self.visible_rows {
            self.cur_y += 1;
        } else {
            if let Some(top) = self.lines.pop_front() {
                self.scrollback.push_back(top);
                if self.scrollback.len() > 1000 {
                    self.scrollback.pop_front();
                }
            }
            self.lines.push_back(vec![Cell::default(); self.cols]);
        }
    }

    pub fn get_selection_text(&self) -> String {
        // Selection dragging will populate selection bounds. Flat mock for now.
        String::new()
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
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        match action {
            's' => self.saved_cursor = Some((self.cur_x, self.cur_y)),
            'u' => {
                if let Some((x, y)) = self.saved_cursor {
                    self.cur_x = x;
                    self.cur_y = y;
                }
            }
            'G' | '`' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cur_x = p.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
            'd' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
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
                self.cur_y = y.saturating_sub(1).min(self.visible_rows.saturating_sub(1));
                self.cur_x = x.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
                        'J' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if param == 2 || param == 3 {
                    for line in self.lines.iter_mut() {
                        for cell in line.iter_mut() {
                            *cell = Cell::default();
                        }
                    }
                    if param == 3 {
                        self.scrollback.clear();
                    }
                    self.cur_x = 0;
                    self.cur_y = 0;
                } else if param == 0 {
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
            }
            'K' => {
                let param = params.iter().next().map(|p| p[0]).unwrap_or(0);
                if param == 0 {
                    if let Some(line) = self.lines.get_mut(self.cur_y) {
                        for i in self.cur_x..self.cols {
                            if let Some(c) = line.get_mut(i) { *c = Cell::default(); }
                        }
                    }
                }
            }
                        'm' => {
                if params.is_empty() {
                    self.cur_fg = 7; self.cur_bg = 0;
                    return;
                }
                let mut i = 0;
                let iter: Vec<&[u16]> = params.iter().collect();
                while i < iter.len() {
                    if iter[i].is_empty() { i += 1; continue; }
                    let p = iter[i][0];
                    match p {
                        0 => { self.cur_fg = 7; self.cur_bg = 0; }
                        30..=37 => self.cur_fg = (p - 30) as u8,
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
                self.cur_x = (self.cur_x + p).min(self.cols.saturating_sub(1));
            }
            'D' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cur_x = self.cur_x.saturating_sub(p);
            }
            'A' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cur_y = self.cur_y.saturating_sub(p);
            }
            'B' => {
                let p = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cur_y = (self.cur_y + p).min(self.visible_rows.saturating_sub(1));
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
    pub last_pty_size: Arc<Mutex<(u16, u16)>>,
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

                        println!("[Terminal] Process spawned");
        Self {
            grid,
            writer: writer_arc,
            master_pty: Arc::new(Mutex::new(pair.master)),
            child: Arc::new(Mutex::new(child)),
            last_pty_size: Arc::new(Mutex::new((200, 60))),
        }
    }

                        pub fn resize_pty(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }

        if let Ok(mut last) = self.last_pty_size.lock() {
            if *last == (cols, rows) {
                return; // Дебаунс: пресекаем шторм из SIGWINCH при ресайзе панели
            }
            *last = (cols, rows);
            println!("[Terminal] Resizing PTY to {}x{}", cols, rows);
        }

        // ВНИМАНИЕ: Мы убрали grid.lock() отсюда. 
        // Вызов resize сетки делает функция draw() в render_view.rs, 
        // иначе мы получаем Deadlock.

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