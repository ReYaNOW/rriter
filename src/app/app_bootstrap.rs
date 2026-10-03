use super::App;
use crate::app::automation::AutomationOptions;
use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::platform::TextFileFormat;
use std::path::PathBuf;
use std::time::Instant;
use winit::keyboard::ModifiersState;

const FAQ_TEXT: &str = "# Особенности RRiter
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
Ctrl/Cmd + Shift + V\tMarkdown: чтение / редактирование
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
Shift + колесо над Python mock\tПрокрутка всей страницы вместо внутреннего окна кода
Миникарта\tМолниеносная навигация по коду

# IDE-режим и Терминал
Alt + Q\tОткрыть/сфокусировать терминал
Alt + Shift + Q\tОткрыть/закрыть терминал
";

pub(crate) struct AppInitOptions {
    pub(crate) editor: Option<Editor>,
    pub(crate) title: Option<String>,
    pub(crate) ext: Option<String>,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) text_file_format: Option<TextFileFormat>,
    pub(crate) recent_files: Option<Vec<PathBuf>>,
    pub(crate) has_file_arg: bool,
    pub(crate) run_ide_on_startup: bool,
    pub(crate) automation_options: Option<AutomationOptions>,
    pub(crate) scroll_bench_idx: Option<usize>,
    pub(crate) scroll_bench_seconds: Option<f32>,
    pub headless: bool,
    /// Background tasks spawned by the App wake the UI through clones of this handle.
    pub(crate) ui_waker: crate::ui_waker::UiWaker,
    pub(crate) startup_trace: crate::startup_trace::StartupTrace,
}

impl AppInitOptions {
    pub(crate) fn headless(automation: Option<AutomationOptions>) -> Self {
        Self {
            editor: None,
            title: None,
            ext: None,
            file_path: None,
            text_file_format: None,
            recent_files: None,
            has_file_arg: false,
            run_ide_on_startup: false,
            automation_options: automation,
            scroll_bench_idx: None,
            scroll_bench_seconds: None,
            headless: true,
            ui_waker: crate::ui_waker::UiWaker::counting(),
            startup_trace: crate::startup_trace::StartupTrace::disabled(),
        }
    }
}

impl App {
    pub(crate) fn initial_editor(initial_text: &str) -> Editor {
        let mut editor = Editor::new(initial_text.len() + 8192);
        if !initial_text.is_empty() {
            let _ = editor.insert_str(initial_text);
            editor.cursor = 0;
            editor.clear_history();
        }
        editor.set_original_text();
        editor.sync_edits.clear();
        editor
    }
}

impl App {
    pub(crate) fn new_from_config(
        config: crate::Config,
        options: AppInitOptions,
    ) -> Self {
        let editor = options
            .editor
            .unwrap_or_else(|| Self::initial_editor(""));
        let title = options
            .title
            .unwrap_or_else(|| "Безымянный".to_string());
        let ext = options.ext.unwrap_or_default();

        let mut faq_editor = Editor::new(FAQ_TEXT.len() + 100);
        let _ = faq_editor.insert_str(FAQ_TEXT);
        faq_editor.cursor = 0;
        faq_editor.selection_anchor = None;

        let highlighter = Highlighter::new();
        highlighter.bind_ui_waker(&options.ui_waker);
        let show_welcome = !options.has_file_arg && !options.run_ide_on_startup;
        let file_key = options
            .file_path
            .as_deref()
            .map(crate::platform::PathKey::new);
        let recent_files = options.recent_files.unwrap_or_default();
        let mut app = App {
            automation: options
                .automation_options
                .map(crate::app::automation::AutomationController::new),
            scroll_render_bench: options.scroll_bench_idx.map(|_| {
                crate::app::ScrollRenderBench::new(
                    options.scroll_bench_seconds.unwrap_or(22.0),
                )
            }),
            pending_key_log: None,
            gl_config: None,
            gl_context: None,
            gl_surface: None,
            window: None,
            confirm_dialog: crate::app::ConfirmDialog::default(),
            protected_saves: crate::app::ProtectedSaves::new(options.headless),
            external_requests: crate::platform::ExternalRequestLog::new(options.headless),
            settings_scroll: crate::scroll::ScrollState::new(15.0),
            tab_scroll: crate::scroll::ScrollState::new(15.0),
            renderer: None,
            editor,
            clipboard: if options.headless {
                Some(crate::platform::Clipboard::in_memory())
            } else {
                Some(crate::platform::Clipboard::deferred_system())
            },
            theme: crate::load_dracula(),
            base_title: title,
            file_path: options.file_path,
            file_key,
            text_file_format: options.text_file_format.unwrap_or_default(),
            file_extension: ext,
            markdown: Default::default(),
            markdown_toc: Default::default(),
            markdown_media: crate::markdown_media::MarkdownMedia::from_platform(),
            highlighter,
            closing_hint_state: Default::default(),
            closing_hint_settings: config.dart_settings.closing_hint_settings(),
            last_sent_version: u64::MAX,
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            last_frame: Instant::now(),
            last_action: Instant::now(),
            last_blink_state: true,
            modifiers: ModifiersState::empty(),
            left_shift_down: false,
            ctrl_wheel_multiplier: config.ctrl_wheel_multiplier,
            is_dragging: false,
            is_editor_drag_pending: false,
            is_focused: true,
            render_suspended: false,
            current_cursor: winit::window::CursorIcon::Default,

            show_fps: false,
            window_width: config.window_width,
            window_height: config.window_height,

            last_resize_time: None,

            last_click_time: Instant::now(),
            click_count: 0,
            last_click_pos: (0.0, 0.0),
            last_click_ui_id: None,

            open_file_rx: None,
            save_file_rx: None,
            api_import_file_rx: None,
            api_body_file_rx: None,
            api_openapi_export_rx: None,
            api_load_rx: Vec::new(),
            api_request_rx: Vec::new(),
            api_mock_ty_rx: None,

            show_welcome,
            recent_files,

            is_ide_mode: false,
            ide_workspaces: config.ide_workspaces.clone(),
            ide_ignore_patterns: config.ide_ignore_patterns.clone(),
            settings_ignore_editor: Editor::new(128),
            settings_ignore_focused: false,
            settings_ignore_scroll_x: 0.0,
            is_dragging_settings_ignore: false,
            open_folder_rx: None,
            tool_paths: config.tool_paths.clone(),
            dart_settings: config.dart_settings.clone(),
            rust_settings: config.rust_settings.clone(),
            settings_tool_picker_rx: None,
            tool_installer: crate::app::tool_installer::ToolInstaller::default(),
            dart_tool_state: crate::app::tool_installer::DartToolState::default(),

            show_search: false,
            search_anim_y: -120.0,
            search_editor: Editor::new(256),
            search_focused: false,
            search_case_sensitive: false,
            search_results: Vec::new(),
            search_current_idx: None,
            is_dragging_search: false,

            is_dragging_lsp_log: false,

            faq_editor,

            is_ready: false,
            is_highlighted_once: false,
            is_highlight_complete: false,
            should_maximize: config.maximized,

            autocomplete_active: false,
            autocomplete_options: Vec::new(),
            autocomplete_selected_idx: 0,
            autocomplete_anim_progress: 0.0,
            autocomplete_scroll: crate::scroll::ScrollState::new(15.0),
            autocomplete_hovered_idx: None,
            autocomplete_rect: None,
            autocomplete_anchor: None,
            autocomplete_mode: crate::app::AutocompleteMode::TreeSitter,
            autocomplete_pending_request_id: None,
            autocomplete_pending_request_mode: None,
            autocomplete_pending_request_path: None,
            autocomplete_pending_context_key: None,
            autocomplete_signature_request_id: None,
            autocomplete_signature_items: Vec::new(),
            autocomplete_detail_request_id: None,
            autocomplete_detail_word: None,
            autocomplete_detail_request_path: None,
            autocomplete_detail_context_key: None,
            autocomplete_detail_popup: None,
            autocomplete_detail_rect: None,
            autocomplete_detail_placement: None,
            autocomplete_detail_max_scroll: 0.0,
            autocomplete_min_width: 0.0,
            autocomplete_detail_min_width: 0.0,
            autocomplete_detail_min_height: 0.0,
            autocomplete_detail_selection_anchor: None,
            autocomplete_detail_selection_cursor: None,
            autocomplete_detail_selecting: false,
            autocomplete_apply_pending_response: false,
            autocomplete_cache: None,
            autocomplete_detail_cache: None,

            current_sticky_lines: Vec::new(),
            target_sticky_lines: Vec::new(),
            sticky_anim_progress: 1.0,
            sticky_anim_is_adding: false,

            show_settings: false,
            settings_anim_progress: 0.0,
            settings_y: 10000.0,
            settings_tab: 0,
            settings_ide_scroll: crate::scroll::ScrollState::new(7.0),
            settings_general_scroll: crate::scroll::ScrollState::new(7.0),
            settings_database_scroll: crate::scroll::ScrollState::new(7.0),
            settings_general_max_scroll: 0.0,
            settings_database_max_scroll: 0.0,

            ide_panel: crate::app::IdePanelState::default(),
            database_runtime: None,
            file_tree_rx: None,
            file_tree_notify_rx: None,
            file_tree_watcher_stop_tx: None,
            file_tree_watched_dirs: Vec::new(),
            external_changes_rx: None,
            git_diff_rx: Vec::new(),
            inline_git_diff_rx: None,
            inline_git_popup: None,
            readonly_notice_until: None,
            lsp: None,
            lsp_actions_menu: None,
            pending_fix_all_id: None,
            ctrl_definition: crate::app::CtrlDefinitionState::default(),
            python_inlay_hints: Vec::new(),
            python_inlay_hint_path: None,
            python_inlay_hint_range: None,
            python_inlay_hint_version: 0,
            python_inlay_hint_pending_request_id: None,
            python_inlay_hint_pending_path: None,
            python_inlay_hint_pending_range: None,
            python_inlay_hint_pending_version: 0,
            python_inlay_hint_cache: rustc_hash::FxHashMap::default(),
            ui_registry: crate::ui_system::UiRegistry::new(),
            hover: crate::app::mouse::HoverState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            pdf_engine: crate::app::pdf_tab::PdfEngineState::NotStarted,
            pdf_library_source: crate::app::pdf_tab::PdfLibrarySource::ProcessEnv,
            pdf_worker: None,
            next_doc_id: 1,
            pdf_textures_to_free: Vec::new(),
            pdf_dark_pages: config.pdf_dark_pages,
            run_ide_on_startup: options.run_ide_on_startup,
            headless_mode: options.headless,
            ui_waker: options.ui_waker,
            startup_trace: options.startup_trace,
            startup_deferred_pending: !options.headless,
            ide_preload: None,
            ide_deferred: crate::app::IdeDeferred::None,
            startup_editor_pending: None,
            startup_editor_reveal_at: None,
        };

        app.highlighter.reset(
            app.editor.version,
            app.editor.get_full_text(),
            app.file_extension.clone(),
            app.editor.cursor,
        );
        app.last_sent_version = app.editor.version;
        // After the empty reset above: the worker keeps only the last queued `Reset`.
        app.preload_ide_startup();

        if show_welcome {
            app.base_title = "Добро пожаловать".to_string();
        }

        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bootstrap_headless_init() {
        let app = App::new_from_config(crate::Config::default(), AppInitOptions::headless(None));

        assert!(app.headless_mode);
        assert!(app.clipboard.as_ref().is_some_and(crate::platform::Clipboard::is_in_memory));
        assert!(app.show_welcome);
        assert_eq!(app.base_title, "Добро пожаловать");
        assert!(!app.editor.original_hashes.is_empty());
    }

    #[test]
    fn app_bootstrap_initial_editor_with_text_is_unmodified() {
        let editor = App::initial_editor("a\nb\n");

        assert!(!editor.is_dirty());
        assert_eq!(editor.cursor, 0);
        assert!(editor.text_equals("a\nb\n"));
    }

    #[test]
    fn app_bootstrap_headless_cursor_does_not_blink() {
        let mut app = crate::app::app_behavior_tests::test_app().unwrap();
        app.headless_mode = true;
        app.is_focused = true;
        app.last_blink_state = true;
        let now = app.last_action + std::time::Duration::from_millis(501);
        let mut needs_redraw = false;

        crate::app::events::about::update_cursor_blink(&mut app, now, &mut needs_redraw);

        assert!(app.last_blink_state);
        assert!(!needs_redraw);
        let idle_blink = crate::app::events::about::idle_blink_enabled(&app);
        assert!(!idle_blink);
        assert_eq!(
            crate::app::events::about::compute_about_wait_plan(
                now,
                app.last_action,
                false,
                false,
                false,
                idle_blink,
                None,
            ),
            crate::app::events::about::AboutWaitPlan::Wait,
        );
    }
}
