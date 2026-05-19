#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod app;
mod editor;
// mod generated;
mod highlighter;
mod languages;
mod lsp;
mod queries;
mod render_view;
mod renderer;
mod scroll;
mod ui_system;
mod widgets;

use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::renderer::Theme;
use arboard::Clipboard;
use std::env;
use std::path::PathBuf;
use std::time::Instant;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

pub struct Config {
    pub window_width: f64,
    pub window_height: f64,
    pub maximized: bool,
    pub ide_workspaces: Vec<std::path::PathBuf>,
    pub ide_ignore_patterns: Vec<String>,
    pub enable_telemetry: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_width: 1000.0,
            window_height: 800.0,
            maximized: false,
            ide_workspaces: Vec::new(),
            ide_ignore_patterns: Vec::new(),
            enable_telemetry: false,
        }
    }
}

#[cfg(not(test))]
fn rriter_config_dir() -> PathBuf {
    let mut path = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    path.push(".config");
    path.push("RRiter");
    path
}

#[cfg(not(test))]
fn recent_files_path() -> PathBuf {
    rriter_config_dir().join("recent.txt")
}

#[cfg(not(test))]
fn open_tabs_path(is_ide: bool) -> PathBuf {
    rriter_config_dir().join(if is_ide { "tabs_ide.txt" } else { "tabs.txt" })
}

#[cfg(not(test))]
fn panel_state_path() -> PathBuf {
    rriter_config_dir().join("panels.txt")
}

#[cfg(not(test))]
fn config_path() -> PathBuf {
    rriter_config_dir().join("config.json")
}

fn parse_recent_files(content: &str) -> Vec<PathBuf> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn format_recent_files(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
pub fn load_recent_files() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(test))]
pub fn load_recent_files() -> Vec<PathBuf> {
    let path = recent_files_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        parse_recent_files(&content)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
pub fn save_recent_files(_files: &[PathBuf]) {}

#[cfg(not(test))]
pub fn save_recent_files(files: &[PathBuf]) {
    let dir = rriter_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("recent.txt"), format_recent_files(files));
}

fn parse_open_tabs_content(content: &str) -> (Vec<Option<PathBuf>>, usize) {
    let mut tabs = Vec::new();
    let mut active = 0;
    let mut lines = content.lines();
    if let Some(first) = lines.next() {
        active = first.parse().unwrap_or(0);
    }
    for line in lines {
        if line.is_empty() {
            tabs.push(None);
        } else {
            tabs.push(Some(PathBuf::from(line)));
        }
    }
    (tabs, active)
}

fn format_open_tabs_content(tabs: &[crate::app::EditorTab], active_tab: usize) -> String {
    let mut lines = Vec::new();
    let active_persist_idx = tabs
        .iter()
        .take(active_tab.saturating_add(1))
        .filter(|tab| matches!(&tab.kind, crate::app::EditorTabKind::Normal))
        .count()
        .saturating_sub(1);
    lines.push(active_persist_idx.to_string());
    for tab in tabs
        .iter()
        .filter(|tab| matches!(&tab.kind, crate::app::EditorTabKind::Normal))
    {
        if let Some(p) = &tab.file_path {
            lines.push(p.to_string_lossy().into_owned());
        } else {
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

#[cfg(test)]
pub fn load_open_tabs(_is_ide: bool) -> (Vec<Option<PathBuf>>, usize) {
    (Vec::new(), 0)
}

#[cfg(not(test))]
pub fn load_open_tabs(is_ide: bool) -> (Vec<Option<PathBuf>>, usize) {
    let path = open_tabs_path(is_ide);
    if let Ok(content) = std::fs::read_to_string(&path) {
        parse_open_tabs_content(&content)
    } else {
        (Vec::new(), 0)
    }
}

#[cfg(test)]
pub fn save_open_tabs(_tabs: &[crate::app::EditorTab], _active_tab: usize, _is_ide: bool) {}

#[cfg(not(test))]
pub fn save_open_tabs(tabs: &[crate::app::EditorTab], active_tab: usize, is_ide: bool) {
    let dir = rriter_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        open_tabs_path(is_ide),
        format_open_tabs_content(tabs, active_tab),
    );
}

fn format_panel_state_content(state: &crate::app::IdePanelState) -> String {
    let mut lines: Vec<String> = Vec::new();
    for slot in &state.slots {
        let id_s = match slot.id {
            crate::app::PanelId::Explorer => "Explorer",
            crate::app::PanelId::Git => "Git",
            crate::app::PanelId::Terminal => "Terminal",
            crate::app::PanelId::Problems => "Problems",
            crate::app::PanelId::LspServers => "LspServers",
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
    lines.join("\n")
}

#[cfg(test)]
pub fn save_panel_state(_state: &crate::app::IdePanelState) {}

#[cfg(not(test))]
pub fn save_panel_state(state: &crate::app::IdePanelState) {
    let dir = rriter_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(panel_state_path(), format_panel_state_content(state));
}

fn parse_panel_state_content(content: &str) -> crate::app::IdePanelState {
    let mut state = crate::app::IdePanelState::default();
    let mut loaded: Vec<crate::app::PanelSlot> = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3 {
            let id = match parts[0] {
                "Explorer" => crate::app::PanelId::Explorer,
                "Git" => crate::app::PanelId::Git,
                "Terminal" => crate::app::PanelId::Terminal,
                "Problems" => crate::app::PanelId::Problems,
                "LspServers" => crate::app::PanelId::LspServers,
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
        for default_slot in state.slots {
            if !loaded.iter().any(|s| s.id == default_slot.id) {
                loaded.push(default_slot);
            }
        }
        state.slots = loaded;
    }
    state
}

#[cfg(test)]
pub fn load_panel_state() -> crate::app::IdePanelState {
    crate::app::IdePanelState::default()
}

#[cfg(not(test))]
pub fn load_panel_state() -> crate::app::IdePanelState {
    let path = panel_state_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => parse_panel_state_content(&content),
        Err(_) => crate::app::IdePanelState::default(),
    }
}

fn format_config_content(config: &Config) -> String {
    let paths_str = config
        .ide_workspaces
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("|");
    let ignore_str = config.ide_ignore_patterns.join("|");
    let content = format!(
        "{{\n  \"window_width\": {:.1},\n  \"window_height\": {:.1},\n  \"maximized\": {},\n  \"ide_workspaces\": \"{}\",\n  \"ide_ignore_patterns\": \"{}\",\n  \"enable_telemetry\": {}\n}}\n",
        config.window_width,
        config.window_height,
        config.maximized,
        paths_str,
        ignore_str,
        config.enable_telemetry
    );
    content
}

#[cfg(test)]
pub fn save_config(_config: &Config) {}

#[cfg(not(test))]
pub fn save_config(config: &Config) {
    let dir = rriter_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = config_path();
    let content = format_config_content(config);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == content {
            return;
        }
    }
    let _ = std::fs::write(&path, content);
}

fn parse_config_content(content: &str, mut config: Config) -> Config {
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
        if line.contains("\"enable_telemetry\"") {
            if let Some(val) = line.split(':').nth(1) {
                if let Ok(v) = val.trim().trim_matches(',').parse::<bool>() {
                    config.enable_telemetry = v;
                }
            }
        }
    }
    config
}

#[cfg(test)]
fn load_config() -> Config {
    Config::default()
}

#[cfg(not(test))]
fn load_config() -> Config {
    let mut config = Config::default();
    let mut path = rriter_config_dir();

    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }

    path.push("config.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            config = parse_config_content(&content, config);
        }
    } else {
        // Первый запуск: засеваем дефолтные паттерны в пользовательский конфиг
        config.ide_ignore_patterns = crate::app::file_tree::DEFAULT_IGNORE_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();
        save_config(&config);
    }

    config
}

fn parse_kde_color(content: &str, target_group: &str, target_key: &str) -> Option<[f32; 4]> {
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

fn get_kde_color(target_group: &str, target_key: &str) -> Option<[f32; 4]> {
    let path = PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config/kdeglobals");
    let content = std::fs::read_to_string(path).ok()?;
    parse_kde_color(&content, target_group, target_key)
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
        diag_warn: [0.945, 0.980, 0.549, 1.0],
        diag_error: [1.0, 0.333, 0.333, 1.0],
        unused: [0.48, 0.48, 0.48, 0.6],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(path: Option<&str>) -> crate::app::EditorTab {
        crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: path.map(PathBuf::from),
            base_title: path.unwrap_or("Безымянный").to_string(),
            file_extension: String::new(),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            last_sent_version: 0,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: false,
            icon_key: "default_file",
            syntax_errors: Vec::new(),
            kind: crate::app::EditorTabKind::Normal,
        }
    }

    #[test]
    fn recent_and_open_tabs_text_roundtrip() {
        let recent = parse_recent_files("/tmp/a.py\n\n  \nrel.rs\n");
        assert_eq!(
            recent,
            vec![PathBuf::from("/tmp/a.py"), PathBuf::from("rel.rs")]
        );
        assert_eq!(
            format_recent_files(&recent),
            format!("{}{}{}", "/tmp/a.py", "\n", "rel.rs")
        );

        let (tabs, active) = parse_open_tabs_content("2\n/tmp/a.py\n\nrel.rs\n");
        assert_eq!(active, 2);
        assert_eq!(
            tabs,
            vec![
                Some(PathBuf::from("/tmp/a.py")),
                None,
                Some(PathBuf::from("rel.rs"))
            ]
        );

        let formatted = format_open_tabs_content(&[tab(Some("/tmp/a.py")), tab(None)], 1);
        assert_eq!(formatted, "1\n/tmp/a.py\n");
    }

    #[test]
    fn panel_state_text_preserves_slots_and_sizes() {
        let mut state = crate::app::IdePanelState::default();
        state.left_width = 321.25;
        state.bottom_height = 222.75;
        state.toggle(crate::app::PanelId::Terminal);

        let formatted = format_panel_state_content(&state);
        assert!(formatted.contains("Terminal:Bottom:1"));
        assert!(formatted.contains("left_width:321.2"));
        assert!(formatted.contains("bottom_height:222.8"));

        let parsed = parse_panel_state_content(
            "Explorer:Top:1\nTerminal:Bottom:0\nleft_width:444.4\nbottom_height:155.5\n",
        );
        assert!(parsed.is_open(crate::app::PanelId::Explorer));
        assert!(!parsed.is_open(crate::app::PanelId::Terminal));
        assert_eq!(parsed.left_width, 444.4);
        assert_eq!(parsed.bottom_height, 155.5);
        assert!(
            parsed
                .slots
                .iter()
                .any(|slot| slot.id == crate::app::PanelId::Problems)
        );
    }

    #[test]
    fn config_parser_handles_missing_invalid_and_empty_values() {
        let mut defaults = Config::default();
        defaults.window_width = 900.0;
        defaults.window_height = 700.0;
        defaults.maximized = true;
        defaults.ide_workspaces = vec![PathBuf::from("/keep")];
        defaults.ide_ignore_patterns = vec!["old".to_string()];
        defaults.enable_telemetry = true;

        let parsed = parse_config_content(
            r#"{
  "window_width": "wide",
  "window_height": 640,
  "maximized": false,
  "ide_workspaces": "",
  "ide_ignore_patterns": "target||.git",
  "enable_telemetry": false
}"#,
            defaults,
        );

        assert_eq!(parsed.window_width, 900.0);
        assert_eq!(parsed.window_height, 640.0);
        assert!(!parsed.maximized);
        assert_eq!(parsed.ide_workspaces, vec![PathBuf::from("/keep")]);
        assert_eq!(parsed.ide_ignore_patterns, vec!["target", "", ".git"]);
        assert!(!parsed.enable_telemetry);

        let invalid_json = parse_config_content("not json", Config::default());
        assert_eq!(invalid_json.window_width, Config::default().window_width);
    }

    #[test]
    fn panel_state_parser_keeps_defaults_for_unknown_and_missing_slots() {
        let parsed = parse_panel_state_content(
            "Unknown:Top:1\nExplorer:Top:0\nleft_width:nope\nbottom_height:333.3\n",
        );

        assert!(!parsed.is_open(crate::app::PanelId::Explorer));
        assert_eq!(
            parsed.left_width,
            crate::app::IdePanelState::default().left_width
        );
        assert_eq!(parsed.bottom_height, 333.3);
        assert!(
            parsed
                .slots
                .iter()
                .any(|slot| slot.id == crate::app::PanelId::Terminal)
        );
        assert!(
            parsed
                .slots
                .iter()
                .any(|slot| slot.id == crate::app::PanelId::LspServers)
        );
    }

    #[test]
    fn tab_and_recent_parsers_handle_empty_or_invalid_headers() {
        assert!(parse_recent_files("\n\t\n").is_empty());
        assert_eq!(format_recent_files(&[]), "");

        let (tabs, active) = parse_open_tabs_content("not-a-number\n\n/tmp/a.py\n");
        assert_eq!(active, 0);
        assert_eq!(tabs, vec![None, Some(PathBuf::from("/tmp/a.py"))]);

        let (empty_tabs, empty_active) = parse_open_tabs_content("");
        assert!(empty_tabs.is_empty());
        assert_eq!(empty_active, 0);
    }

    #[test]
    fn kde_color_parser_respects_group_boundaries_and_rgb_shape() {
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nOther=9,9,9\n[Colors:Window]\nBackgroundNormal=5,10,15\n",
                "Colors:Window",
                "BackgroundNormal",
            ),
            Some([5.0 / 255.0, 10.0 / 255.0, 15.0 / 255.0, 1.0])
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=1,2,3,4\n",
                "Colors:Selection",
                "BackgroundNormal",
            ),
            None
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=a,b,c\n",
                "Colors:Selection",
                "BackgroundNormal",
            ),
            Some([0.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=1,2,3\n[Other]\nBackgroundNormal=4,5,6\n",
                "Other",
                "Missing",
            ),
            None
        );
    }

    #[test]
    fn config_text_parse_format_and_kde_color_parse() {
        let content = r#"{
  "window_width": 1280.5,
  "window_height": 720.25,
  "maximized": true,
  "ide_workspaces": "/tmp/a|rel",
  "ide_ignore_patterns": "target|.git",
  "enable_telemetry": true
}"#;
        let config = parse_config_content(content, Config::default());
        assert_eq!(config.window_width, 1280.5);
        assert_eq!(config.window_height, 720.25);
        assert!(config.maximized);
        assert_eq!(
            config.ide_workspaces,
            vec![PathBuf::from("/tmp/a"), PathBuf::from("rel")]
        );
        assert_eq!(config.ide_ignore_patterns, vec!["target", ".git"]);
        assert!(config.enable_telemetry);

        let formatted = format_config_content(&config);
        assert!(formatted.contains("\"window_width\": 1280.5"));
        assert!(formatted.contains("\"ide_workspaces\": \"/tmp/a|rel\""));

        let color = parse_kde_color(
            "[Colors:Window]\nBackgroundNormal=1,2,3\n[Colors:Selection]\nBackgroundNormal=128,64,255\n",
            "Colors:Selection",
            "BackgroundNormal",
        );
        assert_eq!(color, Some([128.0 / 255.0, 64.0 / 255.0, 1.0, 1.0]));
        assert_eq!(parse_kde_color("[Bad]\nColor=1,2\n", "Bad", "Color"), None);
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn main() {
    #[cfg(target_os = "linux")]
    unsafe {
        // Константа M_ARENA_MAX = -8. Настраиваем glibc напрямую,
        // так как переменные окружения читать уже поздно.
        unsafe extern "C" {
            fn mallopt(param: i32, val: i32) -> i32;
        }
        mallopt(-8, 2);
    }

    let args: Vec<String> = env::args().collect();
    if let Some(idx) = args.iter().position(|arg| arg == "--probe-git-graph") {
        let Some(repo) = args.get(idx + 1) else {
            eprintln!("usage: rriter --probe-git-graph <repo-path> [iterations]");
            return;
        };
        let iterations = args
            .get(idx + 2)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3);
        match crate::app::git_panel::run_git_graph_probe(std::path::Path::new(repo), iterations) {
            Ok(report) => print!("{report}"),
            Err(err) => eprintln!("{err}"),
        }
        return;
    }
    let run_ide_on_startup = args.iter().any(|a| a == "--ide" || a == "ide");
    let has_file_arg = args.iter().skip(1).any(|a| *a != "--ide" && *a != "ide");
    let mut initial_text = String::new();
    let mut title = "Безымянный".to_string();
    let mut ext = String::new();
    let mut file_path = None;

    let mut recent_files = load_recent_files();

    if has_file_arg {
        let path = args
            .iter()
            .skip(1)
            .find(|a| *a != "--ide" && *a != "ide")
            .unwrap();
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

# IDE-режим и Терминал
Alt + Q\tОткрыть/сфокусировать терминал
Alt + Shift + Q\tОткрыть/закрыть терминал
";

    let mut faq_editor = Editor::new(faq_text.len() + 100);
    let _ = faq_editor.insert_str(faq_text);
    faq_editor.cursor = 0;
    faq_editor.selection_anchor = None;

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let config = load_config();
    crate::render_view::TELEMETRY_ENABLED.store(
        config.enable_telemetry,
        std::sync::atomic::Ordering::Relaxed,
    );
    let highlighter = Highlighter::new();

    let show_welcome = !has_file_arg && !run_ide_on_startup;

    let mut app = App {
        pending_key_log: None,
        gl_config: None,
        gl_context: None,
        gl_surface: None,
        window: None,
        dialog_window: None,
        dialog_gl_surface: None,
        settings_scroll: crate::scroll::ScrollState::new(15.0),
        tab_scroll: crate::scroll::ScrollState::new(15.0),
        renderer: None,
        editor,
        clipboard: Clipboard::new().ok(),
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
        render_suspended: false,
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

        is_ide_mode: false,
        ide_workspaces: config.ide_workspaces.clone(),
        ide_ignore_patterns: config.ide_ignore_patterns.clone(),
        settings_ignore_editor: Editor::new(128),
        settings_ignore_focused: false,
        settings_ignore_scroll_x: 0.0,
        is_dragging_settings_ignore: false,
        open_folder_rx: None,

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
        tried_maximize: false,
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

        ide_panel: crate::app::IdePanelState::default(),
        file_tree_rx: None,
        file_tree_notify_rx: None,
        external_changes_rx: None,
        git_diff_rx: Vec::new(),
        readonly_notice_until: None,
        lsp: None,
        lsp_actions_menu: None,
        pending_fix_all_id: None,
        ctrl_definition: crate::app::CtrlDefinitionState::default(),
        ui_registry: crate::ui_system::UiRegistry::new(),
        tabs: Vec::new(),
        active_tab: 0,
        run_ide_on_startup,
    };

    app.highlighter.reset(
        app.editor.version,
        app.editor.get_full_text(),
        app.file_extension.clone(),
        app.editor.cursor,
    );
    app.last_sent_version = app.editor.version;

    if show_welcome {
        app.base_title = "Добро пожаловать".to_string();
    }

    event_loop.run_app(&mut app).unwrap();
}
