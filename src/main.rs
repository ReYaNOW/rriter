#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(windows, allow(linker_messages))]

mod app;
mod editor;
// mod generated;
mod highlighter;
mod languages;
mod lsp;
mod platform;
mod queries;
mod render_view;
mod renderer;
mod scroll;
mod ui_system;
mod widgets;
#[cfg(test)]
mod round2_regression_tests;
#[cfg(test)]
mod round3_regression_tests;

use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::renderer::Theme;
#[cfg(target_os = "linux")]
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

pub struct Config {
    pub window_width: f64,
    pub window_height: f64,
    pub maximized: bool,
    pub ide_workspaces: Vec<std::path::PathBuf>,
    pub ide_ignore_patterns: Vec<String>,
    pub enable_telemetry: bool,
    pub tool_paths: crate::platform::ToolPaths,
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
            tool_paths: crate::platform::ToolPaths::default(),
        }
    }
}

#[cfg(not(test))]
fn rriter_config_dir() -> PathBuf {
    crate::platform::config_dir()
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
    parse_recent_files_checked(content).unwrap_or_default()
}

fn parse_recent_files_checked(content: &str) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for line in content.lines() {
        let line = line.strip_prefix("P\t").unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        let path = crate::platform::decode_persisted_path(line)
            .ok_or_else(|| "recent files contains invalid path record".to_string())?;
        files.push(path);
    }
    Ok(crate::platform::dedup_paths(files))
}

fn format_recent_files(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|path| format!("P\t{}", crate::platform::encode_persisted_path(path)))
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
    match crate::platform::read_text_file(&path) {
        Ok(content) => match parse_recent_files_checked(&content.text) {
            Ok(files) => files,
            Err(error) => {
                eprintln!(
                    "RRiter: {error}{}",
                    crate::platform::corrupt_file_backup_note(&path)
                );
                Vec::new()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            eprintln!("RRiter: recent files not read: {error}");
            Vec::new()
        }
    }
}

#[cfg(test)]
pub fn save_recent_files(_files: &[PathBuf]) {}

#[cfg(not(test))]
pub fn save_recent_files(files: &[PathBuf]) {
    let dir = rriter_config_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("RRiter: failed to create config directory for recent files: {error}");
        return;
    }
    if let Err(error) = crate::platform::atomic_write(
        &dir.join("recent.txt"),
        format_recent_files(files).as_bytes(),
    ) {
        eprintln!("RRiter: failed to persist recent files: {error}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTabSnapshot {
    Empty,
    File(PathBuf),
    Api {
        spec_id: crate::app::api_client::ApiSpecId,
        route_idx: Option<usize>,
        auth_view: bool,
    },
    DatabaseTable {
        connection_id: crate::app::database::DatabaseConnectionId,
        database_name: String,
        table_name: String,
    },
    DatabaseQuery {
        connection_id: crate::app::database::DatabaseConnectionId,
        database_name: String,
        console_id: crate::app::database::SqlConsoleId,
    },
}

fn parse_open_tabs_content(content: &str) -> (Vec<OpenTabSnapshot>, usize) {
    parse_open_tabs_content_checked(content).unwrap_or_default()
}

fn parse_open_tabs_content_checked(
    content: &str,
) -> Result<(Vec<OpenTabSnapshot>, usize), String> {
    let mut tabs = Vec::new();
    let mut active = 0;
    let mut lines = content.lines();
    if let Some(first) = lines.next() {
        active = first
            .parse()
            .map_err(|_| "open tabs contains invalid active index".to_string())?;
    }
    for line in lines {
        if line == "EMPTY" || line.is_empty() {
            tabs.push(OpenTabSnapshot::Empty);
        } else if let Some(record) = line.strip_prefix("FILE\t") {
            if let Some(path) = crate::platform::decode_persisted_path(record) {
                tabs.push(OpenTabSnapshot::File(path));
            }
        } else if let Some(rest) = line.strip_prefix("API\t") {
            let mut parts = rest.splitn(2, '\t');
            let spec_id = parts
                .next()
                .and_then(|raw| raw.parse::<u64>().ok())
                .map(crate::app::api_client::ApiSpecId);
            let tail = parts.next().unwrap_or("");
            let auth_view = tail == "auth";
            let route_idx = (!auth_view)
                .then_some(tail)
                .and_then(|raw| (!raw.is_empty()).then_some(raw))
                .and_then(|raw| raw.parse::<usize>().ok());
            if !auth_view && !tail.is_empty() && route_idx.is_none() {
                return Err("open tabs contains invalid API route index".to_string());
            }
            if let Some(spec_id) = spec_id {
                tabs.push(OpenTabSnapshot::Api {
                    spec_id,
                    route_idx,
                    auth_view,
                });
            } else {
                return Err("open tabs contains invalid API specification id".to_string());
            }
        } else if let Some(rest) = line.strip_prefix("DBTABLE\t") {
            let (connection_id, database_name, table_name) =
                serde_json::from_str::<(u64, String, String)>(rest)
                    .map_err(|_| "open tabs contains invalid database table record".to_string())?;
            tabs.push(OpenTabSnapshot::DatabaseTable {
                connection_id: crate::app::database::DatabaseConnectionId(connection_id),
                database_name,
                table_name,
            });
        } else if let Some(rest) = line.strip_prefix("DBQUERY\t") {
            let (connection_id, database_name, console_id) =
                serde_json::from_str::<(u64, String, u64)>(rest)
                    .map_err(|_| "open tabs contains invalid database query record".to_string())?;
            tabs.push(OpenTabSnapshot::DatabaseQuery {
                connection_id: crate::app::database::DatabaseConnectionId(connection_id),
                database_name,
                console_id: crate::app::database::SqlConsoleId(console_id),
            });
        } else {
            let path = crate::platform::decode_persisted_path(line)
                .ok_or_else(|| "open tabs contains invalid file path record".to_string())?;
            tabs.push(OpenTabSnapshot::File(path));
        }
    }
    if tabs.is_empty() {
        active = 0;
    } else {
        active = active.min(tabs.len() - 1);
    }
    Ok((tabs, active))
}

fn format_open_tabs_content(tabs: &[crate::app::EditorTab], active_tab: usize) -> String {
    let mut lines = Vec::new();
    let mut active_persist_idx = 0usize;
    let mut persisted_seen = 0usize;
    for (idx, tab) in tabs.iter().enumerate() {
        if open_tab_line(tab).is_some() {
            if idx <= active_tab {
                active_persist_idx = persisted_seen;
            }
            persisted_seen = persisted_seen.saturating_add(1);
        }
    }
    lines.push(active_persist_idx.to_string());
    for tab in tabs {
        if let Some(line) = open_tab_line(tab) {
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn open_tab_line(tab: &crate::app::EditorTab) -> Option<String> {
    match &tab.kind {
        crate::app::EditorTabKind::Normal => Some(
            tab.file_path
                .as_ref()
                .map(|path| {
                    format!(
                        "FILE\t{}",
                        crate::platform::encode_persisted_path(path)
                    )
                })
                .unwrap_or_else(|| "EMPTY".to_string()),
        ),
        crate::app::EditorTabKind::ApiClient(meta, state) => {
            if matches!(
                meta.route_identity,
                Some(crate::app::api_client::ApiClientRouteIdentity::Manual { .. })
            ) {
                return None;
            }
            Some(format!(
                "API\t{}\t{}",
                meta.spec_id.0,
                if state.auth_view {
                    "auth".to_string()
                } else {
                    state
                        .route_idx
                        .map(|idx| idx.to_string())
                        .unwrap_or_default()
                }
            ))
        }
        crate::app::EditorTabKind::DatabaseTable(meta, _) => serde_json::to_string(&(
            meta.connection_id.0,
            &meta.database_name,
            &meta.table_name,
        ))
        .ok()
        .map(|payload| format!("DBTABLE\t{payload}")),
        crate::app::EditorTabKind::DatabaseQuery(meta, _) => serde_json::to_string(&(
            meta.connection_id.0,
            &meta.database_name,
            meta.console_id.0,
        ))
        .ok()
        .map(|payload| format!("DBQUERY\t{payload}")),
        crate::app::EditorTabKind::GitDiff(_, _) => None,
    }
}

#[cfg(test)]
pub fn load_open_tabs(_is_ide: bool) -> (Vec<OpenTabSnapshot>, usize) {
    (Vec::new(), 0)
}

#[cfg(not(test))]
pub fn load_open_tabs(is_ide: bool) -> (Vec<OpenTabSnapshot>, usize) {
    let path = open_tabs_path(is_ide);
    match crate::platform::read_text_file(&path) {
        Ok(content) => match parse_open_tabs_content_checked(&content.text) {
            Ok(tabs) => tabs,
            Err(error) => {
                eprintln!("RRiter: {error}{}", crate::platform::corrupt_file_backup_note(&path));
                (Vec::new(), 0)
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), 0),
        Err(error) => {
            eprintln!("RRiter: open tabs not read: {error}");
            (Vec::new(), 0)
        }
    }
}

#[cfg(test)]
pub fn save_open_tabs(_tabs: &[crate::app::EditorTab], _active_tab: usize, _is_ide: bool) {}

#[cfg(not(test))]
pub fn save_open_tabs(tabs: &[crate::app::EditorTab], active_tab: usize, is_ide: bool) {
    let dir = rriter_config_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        eprintln!("RRiter: failed to create config directory for open tabs: {err}");
        return;
    }
    if let Err(err) = crate::platform::atomic_write(
        &open_tabs_path(is_ide),
        format_open_tabs_content(tabs, active_tab).as_bytes(),
    ) {
        eprintln!("RRiter: failed to persist open tabs: {err}");
    }
}

fn format_panel_state_content(state: &crate::app::IdePanelState) -> String {
    let mut lines: Vec<String> = Vec::new();
    for slot in &state.slots {
        let id_s = match slot.id {
            crate::app::PanelId::Explorer => "Explorer",
            crate::app::PanelId::Search => "Search",
            crate::app::PanelId::Git => "Git",
            crate::app::PanelId::ApiClient => "ApiClient",
            crate::app::PanelId::Database => "Database",
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
    lines.push(format!(
        "project_search_include:{}",
        escape_panel_field(&state.project_search.include_editor.get_full_text())
    ));
    lines.push(format!(
        "project_search_exclude:{}",
        escape_panel_field(&state.project_search.exclude_editor.get_full_text())
    ));
    lines.join("\n")
}

fn escape_panel_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_panel_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn set_panel_text_editor(editor: &mut Editor, text: &str) {
    *editor = Editor::new(text.len() + 64);
    editor.insert_str(text);
    editor.cursor = text.len();
    editor.selection_anchor = None;
}

#[cfg(test)]
pub fn save_panel_state(_state: &crate::app::IdePanelState) {}

#[cfg(not(test))]
pub fn save_panel_state(state: &crate::app::IdePanelState) {
    let dir = rriter_config_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        eprintln!("RRiter: failed to create config directory for panel state: {err}");
        return;
    }
    if let Err(err) = crate::platform::atomic_write(
        &panel_state_path(),
        format_panel_state_content(state).as_bytes(),
    ) {
        eprintln!("RRiter: failed to persist panel state: {err}");
    }
}

fn parse_panel_state_content(content: &str) -> crate::app::IdePanelState {
    parse_panel_state_content_checked(content).unwrap_or_default()
}

fn parse_panel_state_content_checked(
    content: &str,
) -> Result<crate::app::IdePanelState, String> {
    let mut state = crate::app::IdePanelState::default();
    let mut loaded: Vec<crate::app::PanelSlot> = Vec::new();
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("left_width:") {
            let v = value
                .parse::<f32>()
                .map_err(|_| "panel state contains invalid left width".to_string())?;
            if !v.is_finite() || v < 0.0 {
                return Err("panel state contains non-finite left width".to_string());
            }
            state.left_width = v;
            continue;
        }
        if let Some(value) = line.strip_prefix("bottom_height:") {
            let v = value
                .parse::<f32>()
                .map_err(|_| "panel state contains invalid bottom height".to_string())?;
            if !v.is_finite() || v < 0.0 {
                return Err("panel state contains non-finite bottom height".to_string());
            }
            state.bottom_height = v;
            continue;
        }
        if let Some(value) = line.strip_prefix("project_search_include:") {
            set_panel_text_editor(
                &mut state.project_search.include_editor,
                &unescape_panel_field(value),
            );
            continue;
        }
        if let Some(value) = line.strip_prefix("project_search_exclude:") {
            set_panel_text_editor(
                &mut state.project_search.exclude_editor,
                &unescape_panel_field(value),
            );
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3 {
            let id = match parts[0] {
                "Explorer" => crate::app::PanelId::Explorer,
                "Search" => crate::app::PanelId::Search,
                "Git" => crate::app::PanelId::Git,
                "ApiClient" => crate::app::PanelId::ApiClient,
                "Database" => crate::app::PanelId::Database,
                "Terminal" => crate::app::PanelId::Terminal,
                "Problems" => crate::app::PanelId::Problems,
                "LspServers" => crate::app::PanelId::LspServers,
                _ => continue,
            };
            if loaded.iter().any(|slot| slot.id == id) {
                return Err("panel state contains duplicate panel record".to_string());
            }
            let group = match parts[1] {
                "Top" => crate::app::PanelGroup::Top,
                "Bottom" => crate::app::PanelGroup::Bottom,
                _ => return Err("panel state contains invalid panel group".to_string()),
            };
            let open = match parts[2] {
                "0" => false,
                "1" => true,
                _ => return Err("panel state contains invalid open flag".to_string()),
            };
            loaded.push(crate::app::PanelSlot {
                id,
                group,
                open,
            });
        } else if !line.trim().is_empty() {
            return Err("panel state contains malformed record".to_string());
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
    Ok(state)
}

#[cfg(test)]
pub fn load_panel_state() -> crate::app::IdePanelState {
    crate::app::IdePanelState::default()
}

#[cfg(not(test))]
pub fn load_panel_state() -> crate::app::IdePanelState {
    let path = panel_state_path();
    match crate::platform::read_text_file(&path) {
        Ok(content) => match parse_panel_state_content_checked(&content.text) {
            Ok(state) => state,
            Err(error) => {
                eprintln!("RRiter: {error}{}", crate::platform::corrupt_file_backup_note(&path));
                crate::app::IdePanelState::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::app::IdePanelState::default()
        }
        Err(error) => {
            eprintln!("RRiter: panel state not read: {error}");
            crate::app::IdePanelState::default()
        }
    }
}

fn format_config_content(config: &Config) -> String {
    let workspaces = config
        .ide_workspaces
        .iter()
        .map(|path| crate::platform::encode_persisted_path(path))
        .collect::<Vec<_>>();
    let tool_paths = config
        .tool_paths
        .iter()
        .filter_map(|(kind, path)| {
            path.map(|path| {
                (
                    kind.config_key().to_string(),
                    serde_json::Value::String(crate::platform::encode_persisted_path(path)),
                )
            })
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    let value = serde_json::json!({
        "schema_version": 3,
        "window_width": config.window_width,
        "window_height": config.window_height,
        "maximized": config.maximized,
        "ide_workspaces": workspaces,
        "ide_ignore_patterns": config.ide_ignore_patterns,
        "enable_telemetry": config.enable_telemetry,
        "tool_paths": tool_paths,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("config value is serializable")
    )
}

#[cfg(test)]
pub fn save_config(_config: &Config) {}

#[cfg(not(test))]
pub fn save_config(config: &Config) {
    let dir = rriter_config_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("RRiter: failed to create config directory: {error}");
        return;
    }
    let path = config_path();
    let content = format_config_content(config);
    if let Ok(existing) = crate::platform::read_text_file(&path) {
        if existing.text == content {
            return;
        }
    }
    if let Err(error) = crate::platform::atomic_write(&path, content.as_bytes()) {
        eprintln!("RRiter: failed to persist config: {error}");
    }
}

fn parse_config_content(content: &str, mut config: Config) -> Config {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return config;
    };
    if let Some(value) = value.get("window_width").and_then(serde_json::Value::as_f64) {
        config.window_width = value;
    }
    if let Some(value) = value.get("window_height").and_then(serde_json::Value::as_f64) {
        config.window_height = value;
    }
    if let Some(value) = value.get("maximized").and_then(serde_json::Value::as_bool) {
        config.maximized = value;
    }
    if let Some(value) = value.get("ide_workspaces") {
        if let Some(values) = value.as_array() {
            config.ide_workspaces = crate::platform::dedup_paths(
                values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(crate::platform::decode_persisted_path)
                .collect::<Vec<_>>(),
            );
        } else if let Some(legacy) = value.as_str().filter(|value| !value.is_empty()) {
            config.ide_workspaces =
                crate::platform::dedup_paths(legacy.split('|').map(PathBuf::from));
        }
    }
    if let Some(value) = value.get("ide_ignore_patterns") {
        if let Some(values) = value.as_array() {
            config.ide_ignore_patterns = values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
        } else if let Some(legacy) = value.as_str().filter(|value| !value.is_empty()) {
            config.ide_ignore_patterns = legacy.split('|').map(str::to_string).collect();
        }
    }
    if let Some(value) = value
        .get("enable_telemetry")
        .and_then(serde_json::Value::as_bool)
    {
        config.enable_telemetry = value;
    }
    if let Some(values) = value.get("tool_paths").and_then(serde_json::Value::as_object) {
        for kind in crate::platform::ToolKind::ALL {
            let path = values
                .get(kind.config_key())
                .and_then(serde_json::Value::as_str)
                .and_then(crate::platform::decode_persisted_path);
            config.tool_paths.set(kind, path);
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
        if let Err(error) = std::fs::create_dir_all(&path) {
            eprintln!("RRiter: failed to create config directory: {error}");
            return config;
        }
    }

    path.push("config.json");
    if path.exists() {
        match crate::platform::read_text_file(&path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content.text) {
                Ok(_) => config = parse_config_content(&content.text, config),
                Err(error) => eprintln!(
                    "RRiter: config is corrupted: {error}{}",
                    crate::platform::corrupt_file_backup_note(&path)
                ),
            },
            Err(error) => eprintln!("RRiter: config not read: {error}"),
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
                let [r, g, b] = [parts[0], parts[1], parts[2]].map(|part| {
                    part.trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite() && (0.0..=255.0).contains(value))
                });
                let (Some(r), Some(g), Some(b)) = (r, g, b) else {
                    return None;
                };
                return Some([r / 255.0, g / 255.0, b / 255.0, 1.0]);
            }
        }
    }
    None
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_project_search_probe(args: &[String], idx: usize) {
    let Some(query) = args.get(idx + 1).cloned() else {
        eprintln!("usage: rriter --probe-project-search <query> [iterations]");
        return;
    };
    let iterations = args
        .get(idx + 2)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let config = load_config();
    let panel = load_panel_state();
    let include = panel.project_search.include_editor.get_full_text();
    let exclude = panel.project_search.exclude_editor.get_full_text();
    for run_idx in 0..iterations {
        let started = Instant::now();
        let rx = crate::app::project_search::start_project_search_worker(
            crate::app::project_search::ProjectSearchRequest {
                generation: run_idx as u64 + 1,
                query: query.clone(),
                include: include.clone(),
                exclude: exclude.clone(),
                case_sensitive: false,
                workspaces: config.ide_workspaces.clone(),
                ignore_patterns: config.ide_ignore_patterns.clone(),
            },
        );
        let mut result_files = 0usize;
        let mut matches = 0usize;
        let mut worker_ms = 0u128;
        let mut capped = false;
        let mut error = None;
        while let Ok(message) = rx.recv() {
            match message {
                crate::app::project_search::ProjectSearchWorkerMessage::File { file, .. } => {
                    result_files += 1;
                    matches = matches.saturating_add(file.matches.len());
                }
                crate::app::project_search::ProjectSearchWorkerMessage::Done {
                    elapsed_ms,
                    capped: done_capped,
                    error: done_error,
                    ..
                } => {
                    worker_ms = elapsed_ms;
                    capped = done_capped;
                    error = done_error;
                    break;
                }
            }
        }
        println!(
            "[PROJECT SEARCH PROBE] run={} wall={}ms worker={}ms result_files={} matches={} capped={} error={}",
            run_idx + 1,
            started.elapsed().as_millis(),
            worker_ms,
            result_files,
            matches,
            capped,
            error.unwrap_or_default()
        );
    }
}

#[cfg(target_os = "linux")]
fn get_kde_color(target_group: &str, target_key: &str) -> Option<[f32; 4]> {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("kdeglobals");
    let content = std::fs::read_to_string(path).ok()?;
    parse_kde_color(&content, target_group, target_key)
}

#[cfg(not(target_os = "linux"))]
fn get_kde_color(_target_group: &str, _target_key: &str) -> Option<[f32; 4]> {
    None
}

fn rayon_thread_cap(available: usize) -> usize {
    available.clamp(1, 4)
}

fn init_rayon_global_pool() {
    let threads = std::thread::available_parallelism()
        .map(|threads| rayon_thread_cap(threads.get()))
        .unwrap_or(1);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
}

fn selection_color(
    desktop_color: Option<[f32; 4]>,
    system_color: Option<[f32; 4]>,
) -> [f32; 4] {
    desktop_color
        .or(system_color)
        .unwrap_or(crate::platform::DEFAULT_ACCENT_COLOR)
}

fn load_dracula() -> Theme {
    let sel_color = selection_color(
        get_kde_color("Colors:Selection", "BackgroundNormal"),
        crate::platform::system_accent_color(),
    );

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

#[cfg(target_os = "linux")]
const EGL_VENDOR_ENV: &str = "__EGL_VENDOR_LIBRARY_FILENAMES";
#[cfg(target_os = "linux")]
const RRITER_EGL_VENDOR_ENV: &str = "RRITER_EGL_VENDOR";
#[cfg(target_os = "linux")]
const NVIDIA_EGL_VENDOR: &str = "/usr/share/glvnd/egl_vendor.d/10_nvidia.json";
#[cfg(target_os = "linux")]
const MESA_EGL_VENDOR: &str = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EglVendorPreference {
    Auto,
    System,
    Nvidia,
    Mesa,
}

fn parse_egl_vendor_preference(raw: Option<&str>) -> EglVendorPreference {
    let Some(value) = raw.map(str::trim) else {
        return EglVendorPreference::Auto;
    };
    if value.eq_ignore_ascii_case("auto") {
        EglVendorPreference::Auto
    } else if value.eq_ignore_ascii_case("system") {
        EglVendorPreference::System
    } else if value.eq_ignore_ascii_case("nvidia") {
        EglVendorPreference::Nvidia
    } else if value.eq_ignore_ascii_case("mesa") {
        EglVendorPreference::Mesa
    } else {
        EglVendorPreference::System
    }
}

#[cfg(target_os = "linux")]
fn prefer_egl_vendor() {
    if env::var_os(EGL_VENDOR_ENV).is_some() {
        return;
    }

    let preference = env::var_os(RRITER_EGL_VENDOR_ENV)
        .and_then(|value| value.into_string().ok())
        .map(|value| parse_egl_vendor_preference(Some(&value)))
        .unwrap_or(EglVendorPreference::Auto);
    let vendor_path = match preference {
        EglVendorPreference::System => return,
        EglVendorPreference::Nvidia => NVIDIA_EGL_VENDOR,
        EglVendorPreference::Mesa => MESA_EGL_VENDOR,
        EglVendorPreference::Auto => {
            if nvidia_gpu_present() {
                NVIDIA_EGL_VENDOR
            } else {
                return;
            }
        }
    };

    if !std::path::Path::new(vendor_path).exists() {
        return;
    }

    // Must run before EGL/GLVND loads; main is still single-threaded here.
    unsafe {
        env::set_var(EGL_VENDOR_ENV, vendor_path);
    }
}

#[cfg(not(target_os = "linux"))]
fn prefer_egl_vendor() {}

#[cfg(target_os = "linux")]
fn nvidia_gpu_present() -> bool {
    if std::path::Path::new("/dev/nvidiactl").exists()
        || std::path::Path::new("/proc/driver/nvidia/version").exists()
    {
        return true;
    }

    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };
    for entry in entries.flatten() {
        let vendor_path = entry.path().join("device/vendor");
        let Ok(vendor) = std::fs::read_to_string(vendor_path) else {
            continue;
        };
        if vendor.trim().eq_ignore_ascii_case("0x10de") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_color_prefers_desktop_then_system_and_uses_requested_purple_fallback() {
        let desktop = [0.1, 0.2, 0.3, 1.0];
        let system = [0.4, 0.5, 0.6, 1.0];
        assert_eq!(selection_color(Some(desktop), Some(system)), desktop);
        assert_eq!(selection_color(None, Some(system)), system);
        assert_eq!(
            selection_color(None, None),
            [114.0 / 255.0, 89.0 / 255.0, 175.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn initial_file_argument_skips_ide_mode_and_honors_benchmark_position() {
        let normal = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--ide"),
            std::ffi::OsString::from(r"C:\Work Tree\пример.py"),
        ];
        assert_eq!(
            initial_file_argument(&normal, None),
            Some(std::ffi::OsStr::new(r"C:\Work Tree\пример.py"))
        );

        let benchmark = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--bench-scroll-render"),
            std::ffi::OsString::from("sample.py"),
            std::ffi::OsString::from("2"),
        ];
        assert_eq!(
            initial_file_argument(&benchmark, Some(1)),
            Some(std::ffi::OsStr::new("sample.py"))
        );

        let automation = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--pgo-train"),
            std::ffi::OsString::from("--pgo-workspace"),
            std::ffi::OsString::from("fixture"),
            std::ffi::OsString::from("--pgo-report"),
            std::ffi::OsString::from("report.json"),
            std::ffi::OsString::from("fixture/src/main.rs"),
        ];
        assert_eq!(
            initial_file_argument(&automation, None),
            Some(std::ffi::OsStr::new("fixture/src/main.rs"))
        );
    }

    #[test]
    fn pgo_automation_arguments_are_validated_without_becoming_file_paths() {
        let args = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-workspace".to_string(),
            "fixture".to_string(),
            "--pgo-report".to_string(),
            "report.json".to_string(),
            "--pgo-timeout-seconds".to_string(),
            "90".to_string(),
        ];
        let options = automation_options(&args, None).unwrap().unwrap();
        assert_eq!(options.workspace, std::path::PathBuf::from("fixture"));
        assert_eq!(options.report_path, std::path::PathBuf::from("report.json"));
        assert_eq!(options.timeout, Duration::from_secs(90));

        let missing = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-workspace".to_string(),
        ];
        assert!(automation_options(&missing, None).is_err());

        let too_short = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-timeout-seconds".to_string(),
            "5".to_string(),
        ];
        assert!(automation_options(&too_short, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn initial_file_argument_preserves_non_utf8_native_paths() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let path = std::ffi::OsString::from_vec(b"source-\xff.py".to_vec());
        let arguments = vec![std::ffi::OsString::from("rriter"), path.clone()];
        assert_eq!(
            initial_file_argument(&arguments, None)
                .expect("native path")
                .as_bytes(),
            path.as_os_str().as_bytes()
        );
    }

    fn tab(path: Option<&str>) -> crate::app::EditorTab {
        crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: path.map(PathBuf::from),
            file_key: path.map(PathBuf::from).as_deref().map(crate::platform::PathKey::new),
            text_file_format: crate::platform::TextFileFormat::default(),
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
            is_highlight_complete: false,
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
        let formatted_recent = format_recent_files(&recent);
        assert!(formatted_recent.lines().all(|line| line.starts_with("P\t")));
        assert_eq!(parse_recent_files(&formatted_recent), recent);

        let (tabs, active) = parse_open_tabs_content("2\n/tmp/a.py\n\nrel.rs\n");
        assert_eq!(active, 2);
        assert_eq!(
            tabs,
            vec![
                OpenTabSnapshot::File(PathBuf::from("/tmp/a.py")),
                OpenTabSnapshot::Empty,
                OpenTabSnapshot::File(PathBuf::from("rel.rs"))
            ]
        );

        let formatted = format_open_tabs_content(&[tab(Some("/tmp/a.py")), tab(None)], 1);
        let (tabs, active) = parse_open_tabs_content(&formatted);
        assert_eq!(active, 1);
        assert_eq!(
            tabs,
            vec![
                OpenTabSnapshot::File(PathBuf::from("/tmp/a.py")),
                OpenTabSnapshot::Empty,
            ]
        );
        assert!(formatted.lines().nth(1).unwrap().starts_with("FILE\t"));
    }

    #[test]
    fn recent_open_tabs_and_config_preserve_delimiters_and_empty_arrays() {
        let paths = vec![
            PathBuf::from(r"C:\Work|spaces\tab\tname.py"),
            PathBuf::from("relative\nname.rs"),
        ];
        let recent = format_recent_files(&paths);
        assert_eq!(parse_recent_files(&recent), paths);

        let formatted_tabs = format_open_tabs_content(
            &[tab(Some(r"C:\Work|spaces\tab\tname.py")), tab(Some("relative\nname.rs"))],
            1,
        );
        let (tabs, active) = parse_open_tabs_content(&formatted_tabs);
        assert_eq!(active, 1);
        assert_eq!(
            tabs,
            vec![
                OpenTabSnapshot::File(PathBuf::from(r"C:\Work|spaces\tab\tname.py")),
                OpenTabSnapshot::File(PathBuf::from("relative\nname.rs")),
            ]
        );

        let mut defaults = Config::default();
        defaults.ide_workspaces = vec![PathBuf::from("/keep")];
        defaults.ide_ignore_patterns = vec!["keep".to_string()];
        let parsed = parse_config_content(
            r#"{
  "ide_workspaces": [],
  "ide_ignore_patterns": []
}"#,
            defaults,
        );
        assert!(parsed.ide_workspaces.is_empty());
        assert!(parsed.ide_ignore_patterns.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn persisted_path_records_roundtrip_non_utf8_names() {
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', b'n', b'a', b'm', b'e', 0xff,
        ]));
        let recent = format_recent_files(&[path.clone()]);
        assert_eq!(parse_recent_files(&recent), vec![path.clone()]);

        let formatted = format_open_tabs_content(
            &[crate::app::EditorTab {
                file_path: Some(path.clone()),
                file_key: Some(crate::platform::PathKey::new(&path)),
                ..tab(None)
            }],
            0,
        );
        assert_eq!(
            parse_open_tabs_content(&formatted).0,
            vec![OpenTabSnapshot::File(path)]
        );
    }

    #[test]
    fn open_tabs_text_preserves_api_tabs_and_ignores_bad_api_lines() {
        let mut api_tab = tab(None);
        api_tab.kind = crate::app::EditorTabKind::ApiClient(
            crate::app::api_client::ApiClientTabMeta {
                spec_id: crate::app::api_client::ApiSpecId(42),
                title: "Pets".to_string(),
                route_identity: Some(crate::app::api_client::ApiClientRouteIdentity::OpenApi {
                    spec_id: crate::app::api_client::ApiSpecId(42),
                    route_idx: 7,
                }),
                route_method: None,
                route_path: String::new(),
            },
            crate::app::api_client::ApiClientTabState {
                route_idx: Some(7),
                ..Default::default()
            },
        );

        let formatted = format_open_tabs_content(&[tab(Some("/tmp/a.py")), api_tab], 1);
        let (formatted_tabs, formatted_active) = parse_open_tabs_content(&formatted);
        assert_eq!(formatted_active, 1);
        assert_eq!(
            formatted_tabs,
            vec![
                OpenTabSnapshot::File(PathBuf::from("/tmp/a.py")),
                OpenTabSnapshot::Api {
                    spec_id: crate::app::api_client::ApiSpecId(42),
                    route_idx: Some(7),
                    auth_view: false,
                },
            ]
        );

        let (tabs, active) = parse_open_tabs_content("1\nAPI\t42\t7\nAPI\tbad\t1\napi:/tmp/file\n");
        assert_eq!(active, 0);
        assert!(tabs.is_empty());
        assert!(parse_open_tabs_content_checked(
            "1\nAPI\t42\t7\nAPI\tbad\t1\napi:/tmp/file\n"
        )
        .is_err());

        let mut auth_tab = tab(None);
        auth_tab.kind = crate::app::EditorTabKind::ApiClient(
            crate::app::api_client::ApiClientTabMeta {
                spec_id: crate::app::api_client::ApiSpecId(42),
                title: "Auth".to_string(),
                route_identity: None,
                route_method: None,
                route_path: String::new(),
            },
            crate::app::api_client::ApiClientTabState {
                auth_view: true,
                ..Default::default()
            },
        );
        assert_eq!(format_open_tabs_content(&[auth_tab], 0), "0\nAPI\t42\tauth");
        let (tabs, _) = parse_open_tabs_content("0\nAPI\t42\tauth\n");
        assert_eq!(
            tabs,
            vec![OpenTabSnapshot::Api {
                spec_id: crate::app::api_client::ApiSpecId(42),
                route_idx: None,
                auth_view: true,
            }]
        );
    }

    #[test]
    fn open_tabs_text_preserves_database_table_and_query_tabs() {
        let mut table_tab = tab(None);
        table_tab.kind = crate::app::EditorTabKind::DatabaseTable(
            crate::app::database::DatabaseTableTabMeta {
                tab_id: crate::app::database::DatabaseTabId(9),
                connection_id: crate::app::database::DatabaseConnectionId(42),
                database_name: "analytics db".to_string(),
                table_name: "events".to_string(),
            },
            crate::app::database::DatabaseTableTabState::default(),
        );

        let mut query_tab = tab(None);
        query_tab.kind = crate::app::EditorTabKind::DatabaseQuery(
            crate::app::database::DatabaseQueryTabMeta {
                console_id: crate::app::database::SqlConsoleId(17),
                connection_id: crate::app::database::DatabaseConnectionId(42),
                database_name: "analytics db".to_string(),
                title: "analytics db — SQL 2".to_string(),
            },
            crate::app::database::DatabaseQueryTabState::default(),
        );

        let formatted = format_open_tabs_content(&[table_tab, query_tab], 1);
        let (tabs, active) = parse_open_tabs_content(&formatted);

        assert_eq!(active, 1);
        assert_eq!(
            tabs,
            vec![
                OpenTabSnapshot::DatabaseTable {
                    connection_id: crate::app::database::DatabaseConnectionId(42),
                    database_name: "analytics db".to_string(),
                    table_name: "events".to_string(),
                },
                OpenTabSnapshot::DatabaseQuery {
                    connection_id: crate::app::database::DatabaseConnectionId(42),
                    database_name: "analytics db".to_string(),
                    console_id: crate::app::database::SqlConsoleId(17),
                },
            ]
        );
    }

    #[test]
    fn panel_state_text_preserves_slots_and_sizes() {
        let mut state = crate::app::IdePanelState::default();
        state.left_width = 321.25;
        state.bottom_height = 222.75;
        state.toggle(crate::app::PanelId::Terminal);
        set_panel_text_editor(&mut state.project_search.include_editor, "./core, lib");
        set_panel_text_editor(&mut state.project_search.exclude_editor, "target\\cache");

        let formatted = format_panel_state_content(&state);
        assert!(formatted.contains("Terminal:Bottom:1"));
        assert!(formatted.contains("left_width:321.2"));
        assert!(formatted.contains("bottom_height:222.8"));
        assert!(formatted.contains("project_search_include:./core, lib"));
        assert!(formatted.contains("project_search_exclude:target\\\\cache"));

        let parsed = parse_panel_state_content(
            "Explorer:Top:1\nTerminal:Bottom:0\nleft_width:444.4\nbottom_height:155.5\nproject_search_include:./src, **/*.rs\nproject_search_exclude:target\\\\cache\n",
        );
        assert!(parsed.is_open(crate::app::PanelId::Explorer));
        assert!(!parsed.is_open(crate::app::PanelId::Terminal));
        assert_eq!(parsed.left_width, 444.4);
        assert_eq!(parsed.bottom_height, 155.5);
        assert_eq!(
            parsed.project_search.include_editor.get_full_text(),
            "./src, **/*.rs"
        );
        assert_eq!(
            parsed.project_search.exclude_editor.get_full_text(),
            "target\\cache"
        );
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
        assert_eq!(
            parsed.bottom_height,
            crate::app::IdePanelState::default().bottom_height
        );
        assert!(parse_panel_state_content_checked(
            "Unknown:Top:1\nExplorer:Top:0\nleft_width:nope\nbottom_height:333.3\n"
        )
        .is_err());
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
        assert!(tabs.is_empty());
        assert!(parse_open_tabs_content_checked("not-a-number\n\n/tmp/a.py\n").is_err());

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
            None
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
        let mut config = parse_config_content(content, Config::default());
        assert_eq!(config.window_width, 1280.5);
        assert_eq!(config.window_height, 720.25);
        assert!(config.maximized);
        assert_eq!(
            config.ide_workspaces,
            vec![PathBuf::from("/tmp/a"), PathBuf::from("rel")]
        );
        assert_eq!(config.ide_ignore_patterns, vec!["target", ".git"]);
        assert!(config.enable_telemetry);
        config.tool_paths.set(
            crate::platform::ToolKind::Git,
            Some(PathBuf::from(r"C:\Program Files\Git\cmd\git.exe")),
        );
        config.tool_paths.set(
            crate::platform::ToolKind::Shell,
            Some(PathBuf::from("/opt/Оболочка/bin/zsh")),
        );

        let formatted = format_config_content(&config);
        assert!(formatted.contains("\"window_width\": 1280.5"));
        let value: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(value["schema_version"], 3);
        assert_eq!(value["ide_workspaces"].as_array().unwrap().len(), 2);
        let reparsed = parse_config_content(&formatted, Config::default());
        assert_eq!(reparsed.ide_workspaces, config.ide_workspaces);
        assert_eq!(reparsed.ide_ignore_patterns, config.ide_ignore_patterns);
        assert_eq!(
            reparsed.tool_paths.get(crate::platform::ToolKind::Git),
            config.tool_paths.get(crate::platform::ToolKind::Git)
        );
        assert_eq!(
            reparsed.tool_paths.get(crate::platform::ToolKind::Shell),
            config.tool_paths.get(crate::platform::ToolKind::Shell)
        );

        let color = parse_kde_color(
            "[Colors:Window]\nBackgroundNormal=1,2,3\n[Colors:Selection]\nBackgroundNormal=128,64,255\n",
            "Colors:Selection",
            "BackgroundNormal",
        );
        assert_eq!(color, Some([128.0 / 255.0, 64.0 / 255.0, 1.0, 1.0]));
        assert_eq!(parse_kde_color("[Bad]\nColor=1,2\n", "Bad", "Color"), None);
    }

    #[test]
    fn rayon_thread_cap_stays_small_and_nonzero() {
        assert_eq!(rayon_thread_cap(0), 1);
        assert_eq!(rayon_thread_cap(1), 1);
        assert_eq!(rayon_thread_cap(4), 4);
        assert_eq!(rayon_thread_cap(128), 4);
    }

    #[test]
    fn egl_vendor_preference_parser_is_portable_and_conservative() {
        assert_eq!(parse_egl_vendor_preference(None), EglVendorPreference::Auto);
        assert_eq!(
            parse_egl_vendor_preference(Some("nvidia")),
            EglVendorPreference::Nvidia
        );
        assert_eq!(
            parse_egl_vendor_preference(Some(" mesa ")),
            EglVendorPreference::Mesa
        );
        assert_eq!(
            parse_egl_vendor_preference(Some("SYSTEM")),
            EglVendorPreference::System
        );
        assert_eq!(
            parse_egl_vendor_preference(Some("bad")),
            EglVendorPreference::System
        );
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn default_headless_ty_mem_files() -> Vec<String> {
    vec![
        "tests/perf_diagnostics_stress_12000.py".to_string(),
        "tests/perf_identical_words_20000.py".to_string(),
        "tests/perf_large_realistic_15000.py".to_string(),
    ]
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_smaps_field_kb(pid: u32, field: &str) -> Option<u64> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            return rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok());
        }
    }
    None
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_pid_ppid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let rest = stat.get(end + 2..)?;
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse::<u32>().ok()
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_process_name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|value| value.trim().to_string())
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_lsp_child_rss_kb(parent_pid: u32) -> (u64, u64, u64) {
    let mut total = 0;
    let mut ty = 0;
    let mut ruff = 0;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return (0, 0, 0);
    };

    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        if headless_pid_ppid(pid) != Some(parent_pid) {
            continue;
        }
        let rss = headless_smaps_field_kb(pid, "Rss:").unwrap_or(0);
        total += rss;
        if let Some(name) = headless_process_name(pid) {
            if name.starts_with("ty") {
                ty += rss;
            } else if name.starts_with("ruff") {
                ruff += rss;
            }
        }
    }

    (total, ty, ruff)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn print_headless_ty_mem_sample(
    label: &str,
    started_at: Instant,
    manager: Option<&crate::lsp::LspManager>,
    opened_files: usize,
    opened_bytes: usize,
) {
    let pid = std::process::id();
    let parent_rss = headless_smaps_field_kb(pid, "Rss:").unwrap_or(0);
    let parent_private_dirty = headless_smaps_field_kb(pid, "Private_Dirty:").unwrap_or(0);
    let (child_rss, ty_rss, ruff_rss) = headless_lsp_child_rss_kb(pid);
    let (
        diag_paths,
        diag_count,
        ty_diag_paths,
        ty_diag_count,
        ruff_diag_paths,
        ruff_diag_count,
        logs,
        log_bytes,
    ) = if let Some(manager) = manager {
        let paths = manager.diagnostic_paths();
        let diag_count = paths
            .iter()
            .map(|path| manager.diagnostic_count(path.as_path()))
            .sum();
        let ty_diag_count = manager
            .ty_instant_diagnostics
            .values()
            .map(|(_, items)| items.len())
            .sum();
        let (ruff_diag_paths, ruff_diag_count) = manager.ruff_diagnostic_storage_counts();
        let mut logs = 0usize;
        let mut log_bytes = 0usize;
        for entries in manager.server_logs.values() {
            logs += entries.len();
            log_bytes += entries.iter().map(|entry| entry.text.len()).sum::<usize>();
        }
        (
            paths.len(),
            diag_count,
            manager.ty_instant_diagnostics.len(),
            ty_diag_count,
            ruff_diag_paths,
            ruff_diag_count,
            logs,
            log_bytes,
        )
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0)
    };

    println!(
        "HEADLESS_TY_MEM stage={label} elapsed_ms={} parent_rss_kb={parent_rss} parent_private_dirty_kb={parent_private_dirty} child_rss_kb={child_rss} ty_rss_kb={ty_rss} ruff_rss_kb={ruff_rss} opened_files={opened_files} opened_bytes={opened_bytes} diag_paths={diag_paths} diag_count={diag_count} ty_diag_paths={ty_diag_paths} ty_diag_count={ty_diag_count} ruff_diag_paths={ruff_diag_paths} ruff_diag_count={ruff_diag_count} logs={logs} log_bytes={log_bytes}",
        started_at.elapsed().as_millis(),
    );
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_headless_ty_mem_probe(args: &[String], flag_idx: usize) {
    let raw_files: Vec<String> = args
        .iter()
        .skip(flag_idx + 1)
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .collect();
    let raw_files = if raw_files.is_empty() {
        default_headless_ty_mem_files()
    } else {
        raw_files
    };
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let workspace = std::fs::canonicalize(&workspace).unwrap_or(workspace);
    let started_at = Instant::now();
    let mut manager = crate::lsp::LspManager::new(vec![workspace.clone()]);
    let mut editors = Vec::with_capacity(raw_files.len());
    let mut opened_bytes = 0usize;

    print_headless_ty_mem_sample("start", started_at, Some(&manager), 0, 0);

    for (idx, raw) in raw_files.iter().enumerate() {
        let input_path = PathBuf::from(raw);
        let path = if input_path.is_absolute() {
            input_path
        } else {
            workspace.join(input_path)
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!(
                    "HEADLESS_TY_MEM read_error path={} err={err}",
                    path.display()
                );
                continue;
            }
        };
        opened_bytes += text.len();

        let mut editor = Editor::new(text.len().saturating_add(8192));
        editor.set_clean_text(&text);
        editors.push(editor);

        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        manager.notify_open(&path, ext, &text, idx as i32 + 1);
        let label = format!("after_open_{}", idx + 1);
        print_headless_ty_mem_sample(
            &label,
            started_at,
            Some(&manager),
            editors.len(),
            opened_bytes,
        );
    }

    let mut workspace_done = false;
    let wait_started = Instant::now();
    while wait_started.elapsed() < Duration::from_secs(45) {
        let events = manager.poll();
        if events
            .iter()
            .any(|event| matches!(event, crate::lsp::LspEvent::WorkspaceDiagnosticsDone { .. }))
        {
            workspace_done = true;
            print_headless_ty_mem_sample(
                "workspace_diagnostics_done",
                started_at,
                Some(&manager),
                editors.len(),
                opened_bytes,
            );
            break;
        }
        if !events.is_empty() {
            print_headless_ty_mem_sample(
                "poll_events",
                started_at,
                Some(&manager),
                editors.len(),
                opened_bytes,
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    for _ in 0..10 {
        let _ = manager.poll();
        std::thread::sleep(Duration::from_millis(25));
    }
    print_headless_ty_mem_sample(
        if workspace_done {
            "final_done"
        } else {
            "final_timeout"
        },
        started_at,
        Some(&manager),
        editors.len(),
        opened_bytes,
    );

    manager.shutdown();
    std::thread::sleep(Duration::from_millis(250));
    print_headless_ty_mem_sample(
        "after_shutdown",
        started_at,
        None,
        editors.len(),
        opened_bytes,
    );
    std::hint::black_box(editors);
}

fn initial_file_argument(
    args: &[std::ffi::OsString],
    scroll_bench_idx: Option<usize>,
) -> Option<&std::ffi::OsStr> {
    if let Some(index) = scroll_bench_idx {
        return args.get(index + 1).map(std::ffi::OsString::as_os_str);
    }

    let mut index = 1;
    while let Some(argument) = args.get(index) {
        let value = argument.as_os_str();
        if value == std::ffi::OsStr::new("--ide")
            || value == std::ffi::OsStr::new("ide")
            || value == std::ffi::OsStr::new("--pgo-train")
        {
            index += 1;
            continue;
        }
        if value == std::ffi::OsStr::new("--pgo-workspace")
            || value == std::ffi::OsStr::new("--pgo-report")
            || value == std::ffi::OsStr::new("--pgo-timeout-seconds")
        {
            index += 2;
            continue;
        }
        if value.to_string_lossy().starts_with("--") {
            index += 1;
            continue;
        }
        return Some(value);
    }
    None
}

fn argument_value<'a>(args: &'a [String], flag: &str) -> Result<Option<&'a str>, String> {
    let Some(index) = args.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    let Some(value) = args.get(index + 1) else {
        return Err(format!("{flag} requires a value"));
    };
    if value.starts_with("--") {
        return Err(format!("{flag} requires a value"));
    }
    Ok(Some(value.as_str()))
}

fn automation_options(
    args: &[String],
    initial_file: Option<&std::ffi::OsStr>,
) -> Result<Option<crate::app::automation::AutomationOptions>, String> {
    if !args.iter().any(|argument| argument == "--pgo-train") {
        return Ok(None);
    }

    let workspace = argument_value(args, "--pgo-workspace")?
        .map(std::path::PathBuf::from)
        .or_else(|| {
            initial_file
                .map(std::path::PathBuf::from)
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        })
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "unable to determine PGO automation workspace".to_string())?;
    let report_path = argument_value(args, "--pgo-report")?
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| workspace.join("rriter-pgo-automation-report.json"));
    let timeout_seconds = argument_value(args, "--pgo-timeout-seconds")?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("invalid --pgo-timeout-seconds value: {value}"))
        })
        .transpose()?
        .unwrap_or(240);
    if timeout_seconds < 30 {
        return Err("--pgo-timeout-seconds must be at least 30".to_string());
    }

    Ok(Some(crate::app::automation::AutomationOptions {
        workspace,
        report_path,
        timeout: Duration::from_secs(timeout_seconds),
    }))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn event_loop_error_message(stage: &str, error: &impl std::fmt::Display) -> String {
    format!("RRiter: {stage}: {error}")
}

fn main() {
    let startup_args = std::env::args_os().collect::<Vec<_>>();
    if let Some(exit_code) = crate::platform::handle_startup_helper(&startup_args) {
        std::process::exit(exit_code);
    }
    crate::platform::initialize_gui_application();
    prefer_egl_vendor();

    #[cfg(target_os = "linux")]
    unsafe {
        // Константа M_ARENA_MAX = -8. Настраиваем glibc напрямую,
        // так как переменные окружения читать уже поздно.
        unsafe extern "C" {
            fn mallopt(param: i32, val: i32) -> i32;
        }
        mallopt(-8, 2);
    }
    init_rayon_global_pool();

    let args = startup_args
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
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
    if let Some(idx) = args.iter().position(|arg| arg == "--probe-project-search") {
        run_project_search_probe(&args, idx);
        return;
    }
    if let Some(idx) = args
        .iter()
        .position(|arg| arg == "--headless-ty-mem" || arg == "--probe-ty-mem")
    {
        run_headless_ty_mem_probe(&args, idx);
        return;
    }
    let scroll_bench_idx = args.iter().position(|arg| arg == "--bench-scroll-render");
    let scroll_bench_seconds = scroll_bench_idx
        .and_then(|idx| args.get(idx + 2))
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(22.0);
    let pgo_train = args.iter().any(|argument| argument == "--pgo-train");
    let run_ide_on_startup = scroll_bench_idx.is_some()
        || pgo_train
        || args.iter().any(|a| a == "--ide" || a == "ide");
    let initial_file_arg = initial_file_argument(&startup_args, scroll_bench_idx);
    let automation_options = match automation_options(&args, initial_file_arg) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("PGO_AUTOMATION_ARGUMENT_ERROR {error}");
            return;
        }
    };
    let has_file_arg = initial_file_arg.is_some();
    let mut initial_text = String::new();
    let mut title = "Безымянный".to_string();
    let mut ext = String::new();
    let mut file_path = None;
    let mut text_file_format = crate::platform::TextFileFormat::default();

    let mut recent_files = load_recent_files();

    if has_file_arg {
        let path = initial_file_arg.expect("has_file_arg is derived from initial_file_arg");
        if let Ok(decoded) = crate::platform::read_text_file(std::path::Path::new(path)) {
            initial_text = decoded.text;
            text_file_format = decoded.format;
            let f_path = std::path::Path::new(path);

            let abs_path = std::fs::canonicalize(f_path).unwrap_or_else(|_| f_path.to_path_buf());

            file_path = Some(abs_path.clone());
            let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
            title = file_name.into_owned();

            if let Some(e) = abs_path.extension() {
                ext = e.to_string_lossy().to_string();
            }

            if scroll_bench_idx.is_none() {
                recent_files.retain(|path| !crate::platform::paths_equal(path, &abs_path));
                recent_files.insert(0, abs_path);
                recent_files.truncate(10);
                save_recent_files(&recent_files);
            }
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
Shift + колесо над Python mock\tПрокрутка всей страницы вместо внутреннего окна кода
Миникарта\tМолниеносная навигация по коду

# IDE-режим и Терминал
Alt + Q\tОткрыть/сфокусировать терминал
Alt + Shift + Q\tОткрыть/закрыть терминал
";

    let mut faq_editor = Editor::new(faq_text.len() + 100);
    let _ = faq_editor.insert_str(faq_text);
    faq_editor.cursor = 0;
    faq_editor.selection_anchor = None;

    let mut event_loop_builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        event_loop_builder
            .with_activation_policy(ActivationPolicy::Regular)
            .with_default_menu(true)
            .with_activate_ignoring_other_apps(true);
    }
    let event_loop = match event_loop_builder.build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("{}", event_loop_error_message("не удалось создать event loop", &error));
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);

    let config = load_config();
    crate::platform::configure_tool_paths(config.tool_paths.clone());
    crate::render_view::TELEMETRY_ENABLED.store(
        config.enable_telemetry || scroll_bench_idx.is_some() || pgo_train,
        std::sync::atomic::Ordering::Relaxed,
    );
    let highlighter = Highlighter::new();

    let show_welcome = !has_file_arg && !run_ide_on_startup;

    let file_key = file_path
        .as_deref()
        .map(crate::platform::PathKey::new);
    let mut app = App {
        automation: automation_options.map(crate::app::automation::AutomationController::new),
        scroll_render_bench: scroll_bench_idx
            .map(|_| crate::app::ScrollRenderBench::new(scroll_bench_seconds)),
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
        clipboard: crate::platform::Clipboard::new().ok(),
        theme: load_dracula(),
        base_title: title,
        file_path,
        file_key,
        text_file_format,
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

        pending_action: PendingAction::None,
        pending_action_waiting_for_save_as: false,
        pending_action_ready: false,
        pending_save_tabs: Vec::new(),
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
        settings_tool_picker_rx: None,
        tool_installer: crate::app::tool_installer::ToolInstaller::default(),

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

    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("{}", event_loop_error_message("event loop завершился с ошибкой", &error));
    }
}
