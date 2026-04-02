#![allow(static_mut_refs)]

mod app;
mod editor;
mod highlighter;
mod render_view;
mod renderer;
mod widgets;

use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::renderer::Theme;
use arboard::Clipboard;
use dlmalloc::GlobalDlmalloc;
use std::env;
use std::path::PathBuf;
use std::time::Instant;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

#[global_allocator]
static ALLOCATOR: GlobalDlmalloc = GlobalDlmalloc;

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

    let titlebar_bg = get_kde_color("Colors:Header", "BackgroundNormal")
        .or_else(|| get_kde_color("Colors:Window", "BackgroundNormal"))
        .unwrap_or([0.192, 0.211, 0.231, 1.0]);

    Theme {
        bg: [0.156, 0.164, 0.211, 1.0],
        fg: [0.972, 0.972, 0.949, 1.0],
        sel: sel_color,
        minimap_bg: [0.129, 0.133, 0.172, 1.0],
        line_num: [0.384, 0.447, 0.643, 1.0],
        minimap_cursor: sel_color,
        modified_unsaved: [1.0, 0.474, 0.776, 1.0],
        modified_saved: [0.313, 0.980, 0.482, 1.0],
        titlebar_bg,
    }
}

fn load_config() -> bool {
    let mut show_fps = true;
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");

    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }

    path.push("config.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains("\"show_fps\": false")
                || content.replace(" ", "").contains("\"show_fps\":false")
            {
                show_fps = false;
            }
        }
    } else {
        let _ = std::fs::write(&path, "{\n  \"show_fps\": true\n}\n");
    }

    show_fps
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut initial_text = String::new();
    let mut title = "Безымянный".to_string();
    let mut ext = String::new();
    let mut file_path = None;
    let show_welcome = args.len() <= 1;

    // Сразу загружаем список недавних файлов, чтобы иметь возможность его обновить
    let mut recent_files = load_recent_files();

    if args.len() > 1 {
        let path = &args[1];
        if let Ok(content) = std::fs::read_to_string(path) {
            initial_text = content;
            let f_path = std::path::Path::new(path);

            // Превращаем путь в абсолютный. Если функция упадет, используем оригинальный путь.
            let abs_path = std::fs::canonicalize(f_path).unwrap_or_else(|_| f_path.to_path_buf());

            file_path = Some(abs_path.clone());
            let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
            title = file_name.into_owned();

            if let Some(e) = abs_path.extension() {
                ext = e.to_string_lossy().to_string();
            }

            // Добавляем файл, открытый из консоли, в историю
            recent_files.retain(|p| p != &abs_path);
            recent_files.insert(0, abs_path);
            recent_files.truncate(10);
            save_recent_files(&recent_files);
        }
    }

    let mut editor = Editor::new(initial_text.len() + 8192);
    if !initial_text.is_empty() {
        let _ = editor.insert_str(&initial_text, &[]);
        editor.cursor = 0;
        editor.clear_history();
    }
    editor.set_original_text();

    let faq_text = "# Особенности RRiter
Автоматическая подсветка синтаксиса для Rust, Python, Bash.
Молниеносный рендеринг на GPU, плавная кинетическая прокрутка.

# Работа с файлами
Ctrl + S\tСохранить текущий документ
Ctrl + O\tОткрыть файл
Ctrl + Q\tВыйти из редактора

# Навигация и поиск
Ctrl + F\tПоиск по тексту (Нажмите Esc для выхода)
PgUp / PgDn\tПостраничная прокрутка документа
Home / End\tПереход в начало / конец текущей строки
Ctrl + Home\tПереход в самое начало документа
Ctrl + End\tПереход в самый конец документа

# Редактирование
Ctrl + Z\tОтменить последнее действие
Ctrl + Y\tПовторить отмененное действие
Ctrl + X\tВырезать выделенный текст
Ctrl + C\tСкопировать выделенный текст
Ctrl + V\tВставить текст из буфера обмена
Ctrl + A\tВыделить весь текст в документе
Ctrl + Bksp\tУдалить слово слева от курсора
Ctrl + Del\tУдалить слово справа от курсора

# Управление мышью
Зажатие ЛКМ\tПлавное выделение текста
Двойной клик\tБыстрое выделение одного слова
Тройной клик\tВыделение всей строки целиком
Миникарта\tМолниеносная навигация по коду
";

    let mut faq_editor = Editor::new(faq_text.len() + 100);
    let _ = faq_editor.insert_str(faq_text, &[]);
    faq_editor.cursor = 0;
    faq_editor.selection_anchor = None;

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let show_fps = load_config();
    let highlighter = Highlighter::new();

    let mut app = App {
        gl_config: None,
        gl_context: None,
        gl_surface: None,
        window: None,
        dialog_window: None,
        dialog_surface: None,
        renderer: None,
        editor,
        clipboard: Clipboard::new().unwrap_or_else(|_| Clipboard::new().unwrap()),
        theme: load_dracula(),
        base_title: title,
        file_path,
        file_extension: ext,
        highlighter,
        last_sent_version: u64::MAX,
        target_scroll_y: 0.0,
        scroll_y: 0.0,
        scroll_velocity: 0.0,
        last_frame: Instant::now(),
        last_action: Instant::now(),
        last_blink_state: true,
        modifiers: ModifiersState::empty(),
        is_dragging: false,
        is_dragging_minimap: false,
        minimap_drag_offset_y: 0.0,
        is_focused: true,
        show_fps,
        scroll_anim_speed: 15.0,
        show_quit_dialog: false,
        skip_highlight_update: false,
        last_resize_time: None,

        last_click_time: Instant::now(),
        click_count: 0,
        last_click_pos: (0.0, 0.0),

        pending_action: PendingAction::Quit,
        open_file_rx: None,
        save_file_rx: None,

        show_welcome,
        recent_files, // Инициализируем обновленным списком

        show_search: false,
        search_anim_y: -70.0,
        search_editor: Editor::new(256),
        search_focused: false,
        search_case_sensitive: false,
        search_results: Vec::new(),
        search_current_idx: None,
        is_dragging_search: false,

        faq_editor,
        is_dragging_faq: false,
        faq_scroll_y: 0.0,
        faq_target_scroll_y: 0.0,
        faq_scroll_velocity: 0.0,
        faq_scroll_anim_speed: 15.0,

        is_ready: false,
        is_highlighted_once: false,
    };

    app.highlighter.request_update(
        app.editor.version,
        app.editor.get_full_text(),
        app.file_extension.clone(),
    );
    app.last_sent_version = app.editor.version;

    if app.show_welcome {
        app.base_title = "Добро пожаловать".to_string();
    }

    event_loop.run_app(&mut app).unwrap();
}
