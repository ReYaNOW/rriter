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
    pub lines: std::collections::VecDeque<Vec<Cell>>,
    pub cols: usize,
    pub visible_rows: usize,
    pub cur_x: usize,
    pub cur_y: usize,
    pub cur_fg: u8,
    pub cur_bg: u8,
    pub dirty: bool,
    pub selection: Option<(usize, usize, usize, usize)>,
}

impl TermGrid {
    pub fn new(cols: usize, visible_rows: usize) -> Self {
        let mut lines = std::collections::VecDeque::new();
        for _ in 0..visible_rows {
            lines.push_back(vec![Cell::default(); cols]);
        }
        Self {
            lines,
            cols,
            visible_rows,
            cur_x: 0,
            cur_y: 0,
            cur_fg: 7,
            cur_bg: 0,
            dirty: true,
            selection: None,
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        self.cols = new_cols;
        self.visible_rows = new_rows;
        for line in self.lines.iter_mut() {
            line.resize(new_cols, Cell::default());
        }
        while self.lines.len() < new_rows {
            self.lines.push_back(vec![Cell::default(); new_cols]);
        }
    }

    pub fn put_char(&mut self, c: char) {
        if self.cur_y >= self.visible_rows { return; }
        if self.cur_x >= self.cols {
            self.newline();
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
            self.lines.pop_front();
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
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        match action {
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
                for param in params.iter() {
                    let p = param[0];
                    match p {
                        0 => { self.cur_fg = 7; self.cur_bg = 0; }
                        30..=37 => self.cur_fg = (p - 30) as u8,
                        40..=47 => self.cur_bg = (p - 40) as u8,
                        90..=97 => self.cur_fg = (p - 90 + 8) as u8,
                        100..=107 => self.cur_bg = (p - 100 + 8) as u8,
                        39 => self.cur_fg = 7,
                        49 => self.cur_bg = 0,
                        _ => {}
                    }
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
}

impl Terminal {
        pub fn spawn(window: Option<std::sync::Arc<winit::window::Window>>) -> Self {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).unwrap();
        
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd).unwrap();

        // КРИТИЧНО для Linux: освобождаем дескриптор slave в родительском процессе, 
        // иначе fish/bash зависает на 10-20 секунд при инициализации
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();

        let grid = Arc::new(Mutex::new(TermGrid::new(80, 24)));
        let grid_clone = grid.clone();

                std::thread::spawn(move || {
            let mut parser = Parser::new();
            let mut buf =[0u8; 4096];
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
        
        Self {
            grid,
            writer: Arc::new(Mutex::new(writer)),
            master_pty: Arc::new(Mutex::new(pair.master)),
        }
    }

        pub fn resize_pty(&self, cols: u16, rows: u16) {
        let master = self.master_pty.lock().unwrap();
        let _ = master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
}