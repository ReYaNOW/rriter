use crate::editor::Editor;
use std::path::{Path, PathBuf};

pub(crate) const GIT_BLAME_DELAY_DEFAULT: u32 = 400;

pub(crate) fn normalize_git_blame_delay_ms(value: u32) -> u32 {
    (value.clamp(0, 2000) / 100) * 100
}

pub(crate) const CTRL_WHEEL_MULTIPLIER_DEFAULT: f32 = 2.0;
pub(crate) const CTRL_WHEEL_MULTIPLIER_MIN: f32 = 1.25;
pub(crate) const CTRL_WHEEL_MULTIPLIER_MAX: f32 = 5.0;
pub(crate) const CTRL_WHEEL_MULTIPLIER_STEP: f32 = 0.25;

pub(crate) fn normalize_ctrl_wheel_multiplier(value: f32) -> f32 {
    if !value.is_finite() {
        return CTRL_WHEEL_MULTIPLIER_DEFAULT;
    }
    let clamped = value.clamp(CTRL_WHEEL_MULTIPLIER_MIN, CTRL_WHEEL_MULTIPLIER_MAX);
    let steps = ((clamped - CTRL_WHEEL_MULTIPLIER_MIN) / CTRL_WHEEL_MULTIPLIER_STEP).round();
    (CTRL_WHEEL_MULTIPLIER_MIN + steps * CTRL_WHEEL_MULTIPLIER_STEP)
        .clamp(CTRL_WHEEL_MULTIPLIER_MIN, CTRL_WHEEL_MULTIPLIER_MAX)
}

pub struct Config {
    pub window_width: f64,
    pub window_height: f64,
    pub maximized: bool,
    pub ide_workspaces: Vec<std::path::PathBuf>,
    pub ide_ignore_patterns: Vec<String>,
    pub enable_telemetry: bool,
    pub pdf_dark_pages: bool,
    pub theme: crate::theme::ThemeSelection,
    pub ctrl_wheel_multiplier: f32,
    pub git_blame_inline: bool,
    pub git_blame_delay_ms: u32,
    pub tool_paths: crate::platform::ToolPaths,
    pub dart_settings: crate::app::DartSettings,
    pub rust_settings: crate::app::RustSettings,
    pub keymap_overrides: crate::keymap::KeymapOverrides,
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
            pdf_dark_pages: true,
            theme: crate::theme::ThemeSelection::default(),
            ctrl_wheel_multiplier: CTRL_WHEEL_MULTIPLIER_DEFAULT,
            git_blame_inline: false,
            git_blame_delay_ms: GIT_BLAME_DELAY_DEFAULT,
            tool_paths: crate::platform::ToolPaths::default(),
            dart_settings: crate::app::DartSettings::default(),
            rust_settings: crate::app::RustSettings::default(),
            keymap_overrides: crate::keymap::KeymapOverrides::default(),
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

#[cfg(test)]
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

#[derive(Clone, Debug, PartialEq)]
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
    Pdf { path: PathBuf, page: usize, frac: f32 },
}

#[cfg(test)]
fn parse_open_tabs_content(content: &str) -> (Vec<OpenTabSnapshot>, usize) {
    parse_open_tabs_content_checked(content).unwrap_or_default()
}

fn parse_open_tabs_content_checked(content: &str) -> Result<(Vec<OpenTabSnapshot>, usize), String> {
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
        } else if let Some(rest) = line.strip_prefix("PDF\t") {
            let record: serde_json::Value = serde_json::from_str(rest)
                .map_err(|_| "open tabs contains invalid PDF record".to_string())?;
            let path = record.get("path").and_then(serde_json::Value::as_str)
                .and_then(crate::platform::decode_persisted_path)
                .ok_or_else(|| "open tabs contains invalid PDF path".to_string())?;
            let page = record.get("page").and_then(serde_json::Value::as_u64)
                .and_then(|page| usize::try_from(page).ok())
                .ok_or_else(|| "open tabs contains invalid PDF page".to_string())?;
            let frac = record.get("frac").and_then(serde_json::Value::as_f64)
                .map(|frac| frac as f32).filter(|frac| frac.is_finite())
                .ok_or_else(|| "open tabs contains invalid PDF fraction".to_string())?;
            tabs.push(OpenTabSnapshot::Pdf { path, page, frac });
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
                .map(|path| format!("FILE\t{}", crate::platform::encode_persisted_path(path)))
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
        crate::app::EditorTabKind::DatabaseTable(meta, _) => {
            serde_json::to_string(&(meta.connection_id.0, &meta.database_name, &meta.table_name))
                .ok()
                .map(|payload| format!("DBTABLE\t{payload}"))
        }
        crate::app::EditorTabKind::DatabaseQuery(meta, _) => {
            serde_json::to_string(&(meta.connection_id.0, &meta.database_name, meta.console_id.0))
                .ok()
                .map(|payload| format!("DBQUERY\t{payload}"))
        }
        crate::app::EditorTabKind::GitDiff(_, _) => None,
        crate::app::EditorTabKind::Pdf => tab.pdf.as_ref().and_then(|pdf| {
            let (page, frac) = pdf.session_position();
            let record = serde_json::json!({
                "path": crate::platform::encode_persisted_path(&pdf.path),
                "page": page,
                "frac": frac,
            });
            serde_json::to_string(&record).ok().map(|payload| format!("PDF\t{payload}"))
        }),
        crate::app::EditorTabKind::Image => None,
    }
}

/// Tests never read the user's `tabs_ide.txt`; a test that needs a saved session hands it to
/// `App::preload_ide_session` directly.
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
                eprintln!(
                    "RRiter: {error}{}",
                    crate::platform::corrupt_file_backup_note(&path)
                );
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

#[cfg(test)]
fn parse_panel_state_content(content: &str) -> crate::app::IdePanelState {
    parse_panel_state_content_checked(content).unwrap_or_default()
}

fn parse_panel_state_content_checked(content: &str) -> Result<crate::app::IdePanelState, String> {
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
            loaded.push(crate::app::PanelSlot { id, group, open });
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
                eprintln!(
                    "RRiter: {error}{}",
                    crate::platform::corrupt_file_backup_note(&path)
                );
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
        "pdf_dark_pages": config.pdf_dark_pages,
        "theme_linked": config.theme.linked,
        "editor_theme": config.theme.editor.key(),
        "ui_theme": config.theme.ui.key(),
        "ctrl_wheel_multiplier": normalize_ctrl_wheel_multiplier(config.ctrl_wheel_multiplier),
        "git_blame_inline": config.git_blame_inline,
        "git_blame_delay_ms": normalize_git_blame_delay_ms(config.git_blame_delay_ms),
        "tool_paths": tool_paths,
        "dart": {
            "enabled": config.dart_settings.enabled,
            "workspace_analysis": config.dart_settings.workspace_analysis,
            "closing_labels": config.dart_settings.closing_labels.config_value(),
            "minimum_nesting_depth": config.dart_settings.minimum_nesting_depth,
            "minimum_block_lines": config.dart_settings.minimum_block_lines,
        },
        "rust": config.rust_settings.config_value(),
        "keymap": config.keymap_overrides.to_value(),
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("config value is serializable")
    )
}

/// Tests opt into real config I/O by seeding `config.json` in the per-process test
/// profile (`app_root_override`); otherwise config stays in memory and the user's
/// real config is never touched.
#[cfg(test)]
fn test_config_path() -> Option<PathBuf> {
    let root = crate::platform::app_root_override()?;
    let path = crate::platform::app_paths_for_root(root).config.join("config.json");
    path.is_file().then_some(path)
}

#[cfg(test)]
pub fn save_config(config: &Config) {
    if let Some(path) = test_config_path() {
        write_config_file(&path, config);
    }
}

#[cfg(not(test))]
pub fn save_config(config: &Config) {
    let dir = rriter_config_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("RRiter: failed to create config directory: {error}");
        return;
    }
    write_config_file(&config_path(), config);
}

fn write_config_file(path: &Path, config: &Config) {
    let content = format_config_content(config);
    if let Ok(existing) = crate::platform::read_text_file(path) {
        if existing.text == content {
            return;
        }
    }
    if let Err(error) = crate::platform::atomic_write(path, content.as_bytes()) {
        eprintln!("RRiter: failed to persist config: {error}");
    }
}

fn parse_config_content(content: &str, mut config: Config) -> Config {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return config;
    };
    if let Some(keymap) = value.get("keymap") {
        if keymap.is_object() {
            config.keymap_overrides = crate::keymap::KeymapOverrides::from_value(keymap.clone());
        } else {
            eprintln!("RRiter: config keymap must be an object");
        }
    }
    if let Some(value) = value
        .get("window_width")
        .and_then(serde_json::Value::as_f64)
    {
        config.window_width = value;
    }
    if let Some(value) = value.get("git_blame_inline").and_then(serde_json::Value::as_bool) {
        config.git_blame_inline = value;
    }
    if let Some(value) = value.get("git_blame_delay_ms").and_then(serde_json::Value::as_u64) {
        config.git_blame_delay_ms = normalize_git_blame_delay_ms(value.min(u32::MAX as u64) as u32);
    }
    if let Some(value) = value
        .get("window_height")
        .and_then(serde_json::Value::as_f64)
    {
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
    if let Some(value) = value.get("pdf_dark_pages").and_then(serde_json::Value::as_bool) {
        config.pdf_dark_pages = value;
    }
    if let Some(value) = value.get("theme_linked").and_then(serde_json::Value::as_bool) {
        config.theme.linked = value;
    }
    if let Some(value) = value.get("editor_theme").and_then(serde_json::Value::as_str) {
        config.theme.editor = crate::theme::ThemeId::from_key(value);
    }
    if let Some(value) = value.get("ui_theme").and_then(serde_json::Value::as_str) {
        config.theme.ui = crate::theme::ThemeId::from_key(value);
    }
    if config.theme.linked {
        config.theme.ui = config.theme.editor;
    }
    if let Some(value) = value
        .get("ctrl_wheel_multiplier")
        .and_then(serde_json::Value::as_f64)
    {
        config.ctrl_wheel_multiplier =
            normalize_ctrl_wheel_multiplier(value.clamp(f32::MIN as f64, f32::MAX as f64) as f32);
    }
    if let Some(values) = value
        .get("tool_paths")
        .and_then(serde_json::Value::as_object)
    {
        for kind in crate::platform::ToolKind::ALL {
            let path = values
                .get(kind.config_key())
                .and_then(serde_json::Value::as_str)
                .and_then(crate::platform::decode_persisted_path);
            config.tool_paths.set(kind, path);
        }
    }
    if let Some(dart) = value.get("dart").and_then(serde_json::Value::as_object) {
        if let Some(enabled) = dart.get("enabled").and_then(serde_json::Value::as_bool) {
            config.dart_settings.enabled = enabled;
        }
        if let Some(enabled) = dart
            .get("workspace_analysis")
            .and_then(serde_json::Value::as_bool)
        {
            config.dart_settings.workspace_analysis = enabled;
        }
        if let Some(mode) = dart
            .get("closing_labels")
            .and_then(serde_json::Value::as_str)
        {
            config.dart_settings.closing_labels =
                crate::app::DartClosingLabelsMode::from_config_value(mode);
        }
        if let Some(value) = dart
            .get("minimum_nesting_depth")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
        {
            config.dart_settings.minimum_nesting_depth = value;
        }
        if let Some(value) = dart
            .get("minimum_block_lines")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
        {
            config.dart_settings.minimum_block_lines = value;
        }
        config.dart_settings.normalize();
    }
    config.rust_settings = value
        .get("rust")
        .map(crate::app::RustSettings::from_config_value)
        .unwrap_or_default();
    config
}

#[cfg(test)]
pub(crate) fn load_config() -> Config {
    test_config_path()
        .and_then(|path| crate::platform::read_text_file(&path).ok())
        .map_or_else(Config::default, |content| {
            parse_config_content(&content.text, Config::default())
        })
}

#[cfg(not(test))]
pub(crate) fn load_config() -> Config {
    let mut config = Config::default();
    let mut path = rriter_config_dir();

    if !path.exists()
        && let Err(error) = std::fs::create_dir_all(&path)
    {
        eprintln!("RRiter: failed to create config directory: {error}");
        return config;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_kde_color;

    #[test]
    fn open_tabs_content_with_garbage_bytes_is_an_error_not_a_panic() {
        let garbage = String::from_utf8_lossy(&[0xff, 0xfe, 0x00, 0x80, b'\n', 0xc3, 0x28]).into_owned();
        assert!(parse_open_tabs_content_checked(&garbage).is_err());
        assert!(parse_open_tabs_content_checked("not-a-number\nFILE\t/tmp/a.py\n").is_err());
        assert_eq!(parse_open_tabs_content(&garbage), (Vec::new(), 0));
    }

    fn tab(path: Option<&str>) -> crate::app::EditorTab {
        crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: path.map(PathBuf::from),
            file_key: path
                .map(PathBuf::from)
                .as_deref()
                .map(crate::platform::PathKey::new),
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: path.unwrap_or("Безымянный").to_string(),
            file_extension: String::new(),
            markdown: Default::default(),
            pdf: None,
            image: None,
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
            closing_hints: Default::default(),
            deleted: false,
            load: crate::app::TabLoad::Loaded,
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
            &[
                tab(Some(r"C:\Work|spaces\tab\tname.py")),
                tab(Some("relative\nname.rs")),
            ],
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
        assert!(
            parse_open_tabs_content_checked("1\nAPI\t42\t7\nAPI\tbad\t1\napi:/tmp/file\n").is_err()
        );

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
    fn pdf_session_record_roundtrips_and_rejects_missing_page_or_bad_json() {
        let path = PathBuf::from("/tmp/session.pdf");
        let mut tab = tab(Some("/tmp/session.pdf"));
        tab.kind = crate::app::EditorTabKind::Pdf;
        let mut pdf = crate::app::pdf_tab::PdfTabState::new(
            path.clone(),
            std::sync::Arc::new(crate::pdf::DocGens::new()),
            crate::app::pdf_tab::PdfPhase::Ready,
        );
        pdf.apply_event(&crate::pdf::PdfEvent::Opened {
            id: crate::pdf::DocId(7),
            pages: vec![crate::pdf::PageGeom { width_pt: 612.0, height_pt: 792.0 }],
        });
        pdf.set_viewport(800, 400, 1.0);
        pdf.scroll.current = pdf.layout.rows[0].0 as f32 + 200.0;
        tab.pdf = Some(Box::new(pdf));
        let line = open_tab_line(&tab).expect("PDF session line");
        assert!(line.starts_with("PDF\t"));
        let (parsed, _) = parse_open_tabs_content_checked(&format!("0\n{line}\n")).expect("parse PDF session");
        assert_eq!(parsed, vec![OpenTabSnapshot::Pdf { path, page: 0, frac: 200.0 / 994.0 }]);
        assert!(parse_open_tabs_content_checked("0\nPDF\t{\"path\":\"/tmp/a.pdf\"}\n").is_err());
        assert!(parse_open_tabs_content_checked("0\nPDF\t{\n").is_err());
    }

    #[test]
    fn pdf_session_record_keeps_pending_restore_before_opened() {
        let path = PathBuf::from("/tmp/session.pdf");
        let mut tab = tab(Some("/tmp/session.pdf"));
        tab.kind = crate::app::EditorTabKind::Pdf;
        let mut pdf = crate::app::pdf_tab::PdfTabState::new(
            path.clone(),
            std::sync::Arc::new(crate::pdf::DocGens::new()),
            crate::app::pdf_tab::PdfPhase::EngineMissing { error: None },
        );
        pdf.restore = Some((4, 0.5));
        pdf.set_viewport(800, 400, 1.0);
        tab.pdf = Some(Box::new(pdf));
        let line = open_tab_line(&tab).expect("PDF session line");
        let (parsed, _) = parse_open_tabs_content_checked(&format!("0\n{line}\n")).expect("parse PDF session");
        assert_eq!(parsed, vec![OpenTabSnapshot::Pdf { path, page: 4, frac: 0.5 }]);
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
        defaults.ctrl_wheel_multiplier = 3.25;

        let parsed = parse_config_content(
            r#"{
  "window_width": "wide",
  "window_height": 640,
  "maximized": false,
  "ide_workspaces": "",
  "ide_ignore_patterns": "target||.git",
  "enable_telemetry": false,
  "ctrl_wheel_multiplier": "fast"
}"#,
            defaults,
        );

        assert_eq!(parsed.window_width, 900.0);
        assert_eq!(parsed.window_height, 640.0);
        assert!(!parsed.maximized);
        assert_eq!(parsed.ide_workspaces, vec![PathBuf::from("/keep")]);
        assert_eq!(parsed.ide_ignore_patterns, vec!["target", "", ".git"]);
        assert!(!parsed.enable_telemetry);
        assert_eq!(parsed.ctrl_wheel_multiplier, 3.25);

        let invalid_json = parse_config_content("not json", Config::default());
        assert_eq!(invalid_json.window_width, Config::default().window_width);
    }

    #[test]
    fn theme_config_roundtrips_and_invalid_external_values_keep_defaults() {
        let config = Config {
            theme: crate::theme::ThemeSelection {
                linked: false,
                editor: crate::theme::ThemeId::OneDark,
                ui: crate::theme::ThemeId::Sepia,
            },
            ..Config::default()
        };
        let parsed = parse_config_content(&format_config_content(&config), Config::default());
        assert!(!parsed.theme.linked);
        assert_eq!(parsed.theme.editor, crate::theme::ThemeId::OneDark);
        assert_eq!(parsed.theme.ui, crate::theme::ThemeId::Sepia);

        let linked = parse_config_content(
            r#"{"theme_linked":true,"editor_theme":"one_dark","ui_theme":"one_light"}"#,
            Config::default(),
        );
        assert!(linked.theme.linked);
        assert_eq!(linked.theme.editor, crate::theme::ThemeId::OneDark);
        assert_eq!(linked.theme.ui, crate::theme::ThemeId::OneDark);

        let invalid = parse_config_content(
            r#"{"window_width": 1234, "theme_linked": "garbage", "editor_theme": "unknown", "ui_theme": 42}"#,
            Config::default(),
        );
        assert!(invalid.theme.linked);
        assert_eq!(invalid.theme.editor, crate::theme::ThemeId::Dracula);
        assert_eq!(invalid.theme.ui, crate::theme::ThemeId::Dracula);
        assert_eq!(invalid.window_width, 1234.0);

        let old = parse_config_content(r#"{"window_width": 900}"#, Config::default());
        assert!(old.theme.linked);
        assert_eq!(old.theme.editor, crate::theme::ThemeId::Dracula);
        assert_eq!(old.theme.ui, crate::theme::ThemeId::Dracula);
    }

    #[test]
    fn config_keymap_roundtrip_preserves_skipped_and_unknown_entries() {
        let config = parse_config_content(
            r#"{"keymap":{"file.save":["mod+s",7],"future.command":["bad chord"]}}"#,
            Config::default(),
        );
        let reparsed = parse_config_content(&format_config_content(&config), Config::default());
        assert_eq!(
            reparsed.keymap_overrides.to_value(),
            config.keymap_overrides.to_value()
        );
        assert!(reparsed.keymap_overrides.has(crate::keymap::Command::FileSave));
    }

    #[test]
    fn config_keymap_non_object_is_ignored() {
        let config = parse_config_content(r#"{"keymap":[]}"#, Config::default());
        assert_eq!(config.keymap_overrides, crate::keymap::KeymapOverrides::default());
    }

    #[test]
    fn ctrl_wheel_multiplier_config_defaults_normalizes_and_roundtrips() {
        assert_eq!(Config::default().ctrl_wheel_multiplier, 2.0);
        assert_eq!(
            parse_config_content(r#"{"window_width": 900}"#, Config::default())
                .ctrl_wheel_multiplier,
            2.0
        );
        assert_eq!(
            parse_config_content(r#"{"ctrl_wheel_multiplier": 0.5}"#, Config::default())
                .ctrl_wheel_multiplier,
            1.25
        );
        assert_eq!(
            parse_config_content(r#"{"ctrl_wheel_multiplier": 9.0}"#, Config::default())
                .ctrl_wheel_multiplier,
            5.0
        );
        assert_eq!(normalize_ctrl_wheel_multiplier(2.13), 2.25);

        let mut config = Config::default();
        config.ctrl_wheel_multiplier = 3.25;
        let formatted = format_config_content(&config);
        let reparsed = parse_config_content(&formatted, Config::default());
        assert_eq!(reparsed.ctrl_wheel_multiplier, 3.25);
        assert!(formatted.contains("\"ctrl_wheel_multiplier\": 3.25"));
    }

    #[test]
    fn git_blame_config_defaults_normalizes_and_roundtrips() {
        assert_eq!(Config::default().git_blame_inline, false);
        assert_eq!(Config::default().git_blame_delay_ms, 400);
        assert_eq!(normalize_git_blame_delay_ms(u32::MAX), 2000);
        assert_eq!(normalize_git_blame_delay_ms(199), 100);
        assert_eq!(normalize_git_blame_delay_ms(99), 0);
        let negative = parse_config_content(
            r#"{"git_blame_delay_ms":-1}"#,
            Config { git_blame_delay_ms: 700, ..Config::default() },
        );
        assert_eq!(negative.git_blame_delay_ms, 700);
        let parsed = parse_config_content(
            r#"{"git_blame_inline":true,"git_blame_delay_ms":987654321}"#,
            Config::default(),
        );
        assert!(parsed.git_blame_inline);
        assert_eq!(parsed.git_blame_delay_ms, 2000);
        let config = Config { git_blame_inline: true, git_blame_delay_ms: 700, ..Config::default() };
        let roundtrip = parse_config_content(&format_config_content(&config), Config::default());
        assert!(roundtrip.git_blame_inline);
        assert_eq!(roundtrip.git_blame_delay_ms, 700);
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
        assert!(
            parse_panel_state_content_checked(
                "Unknown:Top:1\nExplorer:Top:0\nleft_width:nope\nbottom_height:333.3\n"
            )
            .is_err()
        );
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
        config.tool_paths.set(
            crate::platform::ToolKind::Dart,
            Some(PathBuf::from(r"C:\Program Files\Dart\dart-sdk")),
        );
        config.tool_paths.set(
            crate::platform::ToolKind::RustAnalyzer,
            Some(PathBuf::from("/opt/rust-analyzer")),
        );
        config.dart_settings.enabled = false;
        config.dart_settings.workspace_analysis = false;
        config.dart_settings.closing_labels = crate::app::DartClosingLabelsMode::DartServer;
        config.dart_settings.minimum_nesting_depth = 5;
        config.dart_settings.minimum_block_lines = 9;
        config.rust_settings.enabled = false;
        config.rust_settings.check_command = crate::app::RustCheckCommand::Check;

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
        assert_eq!(
            reparsed.tool_paths.get(crate::platform::ToolKind::Dart),
            config.tool_paths.get(crate::platform::ToolKind::Dart)
        );
        assert_eq!(
            reparsed.tool_paths.get(crate::platform::ToolKind::RustAnalyzer),
            config.tool_paths.get(crate::platform::ToolKind::RustAnalyzer)
        );
        assert_eq!(reparsed.dart_settings, config.dart_settings);
        assert_eq!(reparsed.rust_settings, config.rust_settings);

        let color = parse_kde_color(
            "[Colors:Window]\nBackgroundNormal=1,2,3\n[Colors:Selection]\nBackgroundNormal=128,64,255\n",
            "Colors:Selection",
            "BackgroundNormal",
        );
        assert_eq!(color, Some([128.0 / 255.0, 64.0 / 255.0, 1.0, 1.0]));
        assert_eq!(parse_kde_color("[Bad]\nColor=1,2\n", "Bad", "Color"), None);
    }

    #[test]
    fn old_config_keeps_language_defaults_and_invalid_rust_settings_are_safe() {
        let old = parse_config_content(
            r#"{"window_width": 900, "tool_paths": {}}"#,
            Config::default(),
        );
        assert_eq!(old.dart_settings, crate::app::DartSettings::default());
        assert_eq!(old.rust_settings, crate::app::RustSettings::default());

        for invalid in [
            "{\"rust\": null}",
            "{\"rust\": \"invalid\"}",
            "{\"rust\": []}",
            "{\"rust\": {\"enabled\": \"yes\", \"check_command\": 1}}",
        ] {
            let parsed = parse_config_content(
                invalid,
                Config::default(),
            );
            assert_eq!(parsed.rust_settings, crate::app::RustSettings::default());
        }

        let parsed = parse_config_content(
            r#"{
  "dart": {
    "enabled": false,
    "workspace_analysis": false,
    "closing_labels": "future-mode",
    "minimum_nesting_depth": 0,
    "minimum_block_lines": 50000
  }
}"#,
            Config::default(),
        );
        assert!(!parsed.dart_settings.enabled);
        assert!(!parsed.dart_settings.workspace_analysis);
        assert_eq!(
            parsed.dart_settings.closing_labels,
            crate::app::DartClosingLabelsMode::DartServerAndBlocks
        );
        assert_eq!(parsed.dart_settings.minimum_nesting_depth, 1);
        assert_eq!(parsed.dart_settings.minimum_block_lines, 1000);
    }

}
