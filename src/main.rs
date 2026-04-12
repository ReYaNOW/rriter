mod app;
mod editor;
// mod generated;
mod highlighter;
mod queries;
mod render_view;
mod renderer;
mod scroll;
mod widgets;

use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::renderer::Theme;
use arboard::Clipboard;
use mimalloc::MiMalloc;
use std::env;
use std::path::PathBuf;
use std::time::Instant;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub struct Config {
    pub window_width: f64,
    pub window_height: f64,
    pub maximized: bool,
    pub ide_workspaces: Vec<std::path::PathBuf>,
    pub ide_ignore_patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_width: 1000.0,
            window_height: 800.0,
            maximized: false,
            ide_workspaces: Vec::new(),
            ide_ignore_patterns: Vec::new(),
        }
    }
}

pub fn load_recent_files() -> Vec<PathBuf> {
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    path.push("recent.txt");
    if let Ok(content) = std::fs::read_to_string(&path) {
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(PathBuf::from)
            .collect()
    } else {
        Vec::new()
    }
}

pub fn save_recent_files(files: &[PathBuf]) {
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    let _ = std::fs::create_dir_all(&path);
    path.push("recent.txt");
    let content = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(&path, content);
}

pub fn save_panel_state(state: &crate::app::IdePanelState) {
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    let _ = std::fs::create_dir_all(&path);
    path.push("panels.txt");
    let mut lines: Vec<String> = Vec::new();
    for slot in &state.slots {
        let id_s = match slot.id {
            crate::app::PanelId::Explorer => "Explorer",
            crate::app::PanelId::Terminal => "Terminal",
            crate::app::PanelId::Problems => "Problems",
        };
        let grp_s = match slot.group {
            crate::app::PanelGroup::Top => "Top",
            crate::app::PanelGroup::Bottom => "Bottom",
        };
        lines.push(format!(
            "{}:{}:{}",
            id_s,
            grp_s,
            if slot.open { "1" } else { "0" }
        ));
    }
    lines.push(format!("left_width:{:.1}", state.left_width));
    lines.push(format!("bottom_height:{:.1}", state.bottom_height));
    let _ = std::fs::write(&path, lines.join("\n"));
}

pub fn load_panel_state() -> crate::app::IdePanelState {
    let mut state = crate::app::IdePanelState::default();
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    path.push("panels.txt");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return state,
    };
    let mut loaded: Vec<crate::app::PanelSlot> = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3 {
            let id = match parts[0] {
                "Explorer" => crate::app::PanelId::Explorer,
                "Terminal" => crate::app::PanelId::Terminal,
                "Problems" => crate::app::PanelId::Problems,
                _ => continue,
            };
            let group = if parts[1] == "Top" {
                crate::app::PanelGroup::Top
            } else {
                crate::app::PanelGroup::Bottom
            };
            loaded.push(crate::app::PanelSlot {
                id,
                group,
                open: parts[2] == "1",
            });
        } else if parts.len() == 2 {
            if parts[0] == "left_width" {
                if let Ok(v) = parts[1].parse::<f32>() {
                    state.left_width = v;
                }
            } else if parts[0] == "bottom_height" {
                if let Ok(v) = parts[1].parse::<f32>() {
                    state.bottom_height = v;
                }
            }
        }
    }
    if !loaded.is_empty() {
        state.slots = loaded;
    }
    state
}

pub fn save_config(config: &Config) {
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    let _ = std::fs::create_dir_all(&path);
    path.push("config.json");
        let paths_str = config
            .ide_workspaces
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("|");
        let ignore_str = config.ide_ignore_patterns.join("|");
        let content = format!(
            "{{\n  \"window_width\": {:.1},\n  \"window_height\": {:.1},\n  \"maximized\": {},\n  \"ide_workspaces\": \"{}\",\n  \"ide_ignore_patterns\": \"{}\"\n}}\n",
            config.window_width, config.window_height, config.maximized, paths_str, ignore_str
        );
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == content {
            return;
        }
    }
    let _ = std::fs::write(&path, content);
}

fn load_config() -> Config {
    let mut config = Config::default();
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");

    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }

    path.push("config.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                if line.contains("\"window_width\"") {
                    if let Some(val) = line.split(':').nth(1) {
                        if let Ok(v) = val.trim().trim_matches(',').parse::<f64>() {
                            config.window_width = v;
                        }
                    }
                }
                if line.contains("\"window_height\"") {
                    if let Some(val) = line.split(':').nth(1) {
                        if let Ok(v) = val.trim().trim_matches(',').parse::<f64>() {
                            config.window_height = v;
                        }
                    }
                }
                if line.contains("\"maximized\"") {
                    if let Some(val) = line.split(':').nth(1) {
                        if let Ok(v) = val.trim().trim_matches(',').parse::<bool>() {
                            config.maximized = v;
                        }
                    }
                }
                                if line.contains("\"ide_workspaces\"") {
                    if let Some(val) = line.split("\": \"").nth(1) {
                        let paths = val.trim().trim_matches(',').trim_matches('"');
                        if !paths.is_empty() {
                            config.ide_workspaces = paths.split('|').map(PathBuf::from).collect();
                        }
                    }
                }
                if line.contains("\"ide_ignore_patterns\"") {
                    if let Some(val) = line.split("\": \"").nth(1) {
                        let pats = val.trim().trim_matches(',').trim_matches('"');
                        if !pats.is_empty() {
                            config.ide_ignore_patterns = pats.split('|').map(|s| s.to_string()).collect();
                        }
                    }
                }
            }
        }
    } else {
        save_config(&config);
    }

    config
}

fn get_kde_color(target_group: &str, target_key: &str) -> Option<[f32; 4]> {
    let path = PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config/kdeglobals");
    let content = std::fs::read_to_string(path).ok()?;
    let mut current_group = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current_group = line[1..line.len() - 1].to_string();
        } else if current_group == target_group && line.starts_with(&format!("{}=", target_key)) {
            let parts: Vec<&str> = line[target_key.len() + 1..].split(',').collect();
            if parts.len() == 3 {
                let r: f32 = parts[0].parse().unwrap_or(0.0);
                let g: f32 = parts[1].parse().unwrap_or(0.0);
                let b: f32 = parts[2].parse().unwrap_or(0.0);
                return Some([r / 255.0, g / 255.0, b / 255.0, 1.0]);
            }
        }
    }
    None
}

fn load_dracula() -> Theme {
    let sel_color =
        get_kde_color("Colors:Selection", "BackgroundNormal").unwrap_or([0.55, 0.55, 0.55, 1.0]);

    Theme {
        bg: [0.156, 0.164, 0.211, 1.0],
        fg: [0.972, 0.972, 0.949, 1.0],
        sel: sel_color,
        minimap_bg: [0.129, 0.133, 0.172, 1.0],
        line_num: [0.384, 0.447, 0.643, 1.0],
        minimap_cursor: sel_color,
        modified_unsaved: [1.0, 0.474, 0.776, 1.0],
        modified_saved: [0.313, 0.980, 0.482, 1.0],
    }
}

fn main() {
    std::env::set_var("MIMALLOC_PURGE_DELAY", "100");
    std::env::set_var("MIMALLOC_ARENA_CAPACITY", "4");

    let args: Vec<String> = env::args().collect();
    let is_ide_cli = args.iter().any(|a| a == "--ide" || a == "ide");
    let mut initial_text = String::new();
    let mut title = "Безымянный".to_string();
    let mut ext = String::new();
    let mut file_path = None;
    let show_welcome = args.len() <= 1 && !is_ide_cli;

    let mut recent_files = load_recent_files();

    if args.len() > 1 {
        let path = &args[1];
        if let Ok(content) = std::fs::read_to_string(path) {
            initial_text = content;
            let f_path = std::path::Path::new(path);

            let abs_path = std::fs::canonicalize(f_path).unwrap_or_else(|_| f_path.to_path_buf());

            file_path = Some(abs_path.clone());
            let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
            title = file_name.into_owned();

            if let Some(e) = abs_path.extension() {
                ext = e.to_string_lossy().to_string();
            }

            recent_files.retain(|p| p != &abs_path);
            recent_files.insert(0, abs_path);
            recent_files.truncate(10);
            save_recent_files(&recent_files);
        }
    }

    let mut editor = Editor::new(initial_text.len() + 8192);
    if !initial_text.is_empty() {
        let _ = editor.insert_str(&initial_text);
        editor.cursor = 0;
        editor.clear_history();
    }
    editor.set_original_text();
    editor.sync_edits.clear();

    let faq_text = "# Особенности RRiter
Автоматическая подсветка синтаксиса для Rust, Python, Bash.
Молниеносный рендеринг на GPU, плавная кинетическая прокрутка.

# Работа с файлами
Ctrl + S\tСохранить текущий документ
Ctrl + O\tОткрыть файл
Ctrl + Q\tВыйти из редактора (закрыть документ)

# Навигация и поиск
Ctrl + F\tПоиск по тексту (Нажмите Esc для выхода)
Ctrl + ← / →\tБыстрый переход по словам
PgUp / PgDn\tПостраничная прокрутка документа
Home / End\tПереход в начало / конец текущей строки
Ctrl + Home\tПереход в самое начало документа
Ctrl + End\tПереход в самый конец документа

# Редактирование
Ctrl + W\tУмное выделение (Expand Selection)
Ctrl + Z\tОтменить последнее действие
Ctrl + Y\tПовторить отмененное действие
Ctrl + X\tВырезать выделенный текст
Ctrl + C\tСкопировать выделенный текст
Ctrl + V\tВставить текст из буфера обмена
Ctrl + A\tВыделить весь текст в документе
Ctrl + Bksp\tУдалить слово слева от курсора
Ctrl + Del\tУдалить слово справа от курсора

# Прочее
F1\tОткрыть настройки редактора
F8\tПоказать/скрыть счетчик FPS

# Управление мышью
Зажатие ЛКМ\tПлавное выделение текста
Двойной клик\tБыстрое выделение одного слова
Тройной клик\tВыделение всей строки целиком
Миникарта\tМолниеносная навигация по коду
";

    let mut faq_editor = Editor::new(faq_text.len() + 100);
    let _ = faq_editor.insert_str(faq_text);
    faq_editor.cursor = 0;
    faq_editor.selection_anchor = None;

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let config = load_config();
    let highlighter = Highlighter::new();

    let mut app = App {
        gl_config: None,
        gl_context: None,
        gl_surface: None,
        window: None,
        dialog_window: None,
        dialog_gl_surface: None,
        settings_scroll: crate::scroll::ScrollState::new(15.0),
        renderer: None,
        editor,
        clipboard: Clipboard::new().unwrap_or_else(|_| Clipboard::new().unwrap()),
        theme: load_dracula(),
        base_title: title,
        file_path,
        file_extension: ext,
        highlighter,
        last_sent_version: u64::MAX,
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        last_frame: Instant::now(),
        last_action: Instant::now(),
        last_blink_state: true,
        modifiers: ModifiersState::empty(),
        is_dragging: false,
        is_focused: true,
        current_cursor: winit::window::CursorIcon::Default,

        show_fps: false,
        window_width: config.window_width,
        window_height: config.window_height,

        last_resize_time: None,

        last_click_time: Instant::now(),
        click_count: 0,
        last_click_pos: (0.0, 0.0),

        pending_action: PendingAction::Quit,
        open_file_rx: None,
        save_file_rx: None,

        show_welcome,
        recent_files,

                is_ide_mode: is_ide_cli,
        ide_workspaces: config.ide_workspaces.clone(),
        ide_ignore_patterns: config.ide_ignore_patterns.clone(),
        settings_ignore_input: String::new(),
        settings_ignore_focused: false,
        open_folder_rx: None,

        show_search: false,
        search_anim_y: -120.0,
        search_editor: Editor::new(256),
        search_focused: false,
        search_case_sensitive: false,
        search_results: Vec::new(),
        search_current_idx: None,
        is_dragging_search: false,

        faq_editor,

        is_ready: false,
        is_highlighted_once: false,
        tried_maximize: false,
        should_maximize: config.maximized,

        autocomplete_active: false,
        autocomplete_options: Vec::new(),
        autocomplete_selected_idx: 0,
        autocomplete_anim_progress: 0.0,
        autocomplete_scroll: crate::scroll::ScrollState::new(15.0),
        autocomplete_hovered_idx: None,
        autocomplete_rect: None,

        current_sticky_lines: Vec::new(),
        target_sticky_lines: Vec::new(),
        sticky_anim_progress: 1.0,
        sticky_anim_is_adding: false,

                show_settings: false,
        settings_anim_progress: 0.0,
        settings_y: 10000.0,
        settings_tab: 0,
        settings_ide_scroll: crate::scroll::ScrollState::new(7.0),

        ide_panel: crate::load_panel_state(),
        file_tree_rx: None,
        file_tree_notify_rx: None,
    };

    app.highlighter.reset(
        app.editor.version,
        app.editor.get_full_text(),
        app.file_extension.clone(),
    );
    app.last_sent_version = app.editor.version;

    if app.show_welcome {
        app.base_title = "Добро пожаловать".to_string();
    }

    if app.is_ide_mode && app.ide_panel.is_open(crate::app::PanelId::Explorer) {
        app.refresh_file_tree();
        app.start_file_watcher();
    }

    event_loop.run_app(&mut app).unwrap();
}
