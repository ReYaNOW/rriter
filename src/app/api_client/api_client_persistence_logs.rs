fn schema_json_literal(kind: ApiSchemaKind, value: &str) -> String {
    match kind {
        ApiSchemaKind::String
        | ApiSchemaKind::Date
        | ApiSchemaKind::DateTime
        | ApiSchemaKind::Time
        | ApiSchemaKind::Bytes => {
            serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
        }
        ApiSchemaKind::Integer | ApiSchemaKind::Number => {
            if value.parse::<f64>().is_ok() {
                value.to_string()
            } else {
                "0".to_string()
            }
        }
        ApiSchemaKind::Boolean => match value {
            "true" | "false" => value.to_string(),
            _ => "false".to_string(),
        },
        ApiSchemaKind::Object | ApiSchemaKind::Array | ApiSchemaKind::Unknown => {
            serde_json::from_str::<Value>(value)
                .map(|json| json.to_string())
                .unwrap_or_else(|_| "null".to_string())
        }
    }
}

pub(crate) fn api_config_dir() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir().join(format!("rriter_api_client_tests-{}", std::process::id()));
    }
    #[cfg(not(test))]
    {
        crate::platform::config_dir()
    }
}

fn api_specs_path() -> PathBuf {
    api_config_dir().join("api_specs.json")
}

fn api_auth_path() -> PathBuf {
    api_config_dir().join("api_auth.json")
}

fn api_cache_dir() -> PathBuf {
    api_config_dir().join("api_cache")
}

const API_AUTH_SECRET_PURPOSE: &str = "RRiter API authentication";

fn load_api_auth_checked() -> Result<ApiAuthStore, String> {
    load_api_auth_from_checked(&api_auth_path())
}

fn load_api_auth_from_checked(path: &std::path::Path) -> Result<ApiAuthStore, String> {
    let record = match std::fs::read(path) {
        Ok(record) => record,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ApiAuthStore::default());
        }
        Err(error) => return Err(format!("API credentials не прочитаны: {error}")),
    };
    let parse_result = crate::platform::open_user_secret(&record, API_AUTH_SECRET_PURPOSE)
        .map_err(|error| format!("API credentials не расшифрованы: {error}"))
        .and_then(|content| {
            serde_json::from_slice::<ApiAuthStore>(&content)
                .map_err(|error| format!("API credentials повреждены: {error}"))
        });
    if let Err(error) = parse_result {
        let backup_note = crate::platform::corrupt_file_backup_note(path);
        return Err(format!("{error}{backup_note}"));
    }
    parse_result
}

#[cfg(test)]
fn load_api_auth() -> ApiAuthStore {
    load_api_auth_checked().unwrap_or_default()
}

fn save_api_auth(auth: &ApiAuthStore) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(auth).map_err(|err| err.to_string())?;
    let record = crate::platform::seal_user_secret(&content, API_AUTH_SECRET_PURPOSE)
        .map_err(|err| err.to_string())?;
    crate::platform::atomic_write_secret(&api_auth_path(), &record)
        .map_err(|err| err.to_string())
}

fn save_url_cache(id: ApiSpecId, raw: &str) -> Result<(), String> {
    save_url_cache_to(&api_cache_dir().join(format!("{}.json", id.0)), raw)
}

fn save_url_cache_to(path: &std::path::Path, raw: &str) -> Result<(), String> {
    crate::platform::atomic_write(path, raw.as_bytes())
        .map_err(|error| format!("OpenAPI URL cache не сохранён: {error}"))
}

fn read_url_cache(id: ApiSpecId) -> Option<String> {
    std::fs::read_to_string(api_cache_dir().join(format!("{}.json", id.0))).ok()
}

pub(crate) fn api_python_runtime_dialog_layout(
    width: f32,
    height: f32,
    scale: f32,
) -> ApiPythonRuntimeDialogLayout {
    let available_w = (width - 32.0 * scale).max(0.0);
    let available_h = (height - 32.0 * scale).max(0.0);
    let box_w = (crate::app::file_tree::FILE_TREE_DIALOG_W * scale).min(available_w);
    let box_h = (500.0 * scale).min(available_h);
    let pad = (crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * scale)
        .min(box_w * 0.5);
    let box_x = ((width - box_w) / 2.0).round();
    let box_y = ((height - box_h) / 2.0).round();
    ApiPythonRuntimeDialogLayout {
        box_x,
        box_y,
        box_w,
        box_h,
        pad,
        content_w: (box_w - pad * 2.0).max(0.0),
    }
}

pub(crate) fn api_python_version_list_rect(
    layout: ApiPythonRuntimeDialogLayout,
    scale: f32,
) -> (f32, f32, f32, f32) {
    let y = (layout.box_y + 210.0 * scale).min(layout.box_y + layout.box_h);
    let footer_y = (layout.box_y + layout.box_h - 64.0 * scale).max(layout.box_y);
    (
        layout.box_x + layout.pad,
        y,
        layout.content_w.max(0.0),
        (footer_y - y).max(0.0).min(158.0 * scale),
    )
}

pub(crate) fn api_output_schema_menu_scroll_metrics(
    example_count: usize,
    scale: f32,
) -> (f32, f32) {
    let row_h = 30.0 * scale;
    let top_pad = 6.0 * scale;
    let bottom_pad = 4.0 * scale;
    let visible_h = (top_pad + example_count as f32 * row_h + bottom_pad)
        .min(top_pad + row_h * 6.0 + bottom_pad);
    let content_h = top_pad + example_count as f32 * row_h + bottom_pad;
    (visible_h, (content_h - visible_h).max(0.0))
}

pub(crate) fn api_output_schema_menu_scrollbar_drag_target(
    track_rect: (f32, f32, f32, f32),
    example_count: usize,
    current: f32,
    pointer_y: f32,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let geometry = api_output_schema_menu_scrollbar(track_rect, example_count, current, scale)
        .geometry(scale)?;
    if let Some(offset) = drag_offset {
        Some((offset, geometry.drag_target(pointer_y, offset)?))
    } else {
        geometry.press_target(pointer_y)
    }
}

pub(crate) fn api_output_schema_menu_scrollbar(
    lane: (f32, f32, f32, f32),
    example_count: usize,
    current: f32,
    scale: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let (visible_h, max_scroll) = api_output_schema_menu_scroll_metrics(example_count, scale);
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 4.0,
            edge_gap: Some(4.0),
            min_thumb: 22.0,
            thumb_color: [0.70, 0.72, 0.80, 0.88],
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane,
        extent: ScrollbarExtent::new(visible_h, visible_h + max_scroll, current),
    }
}

pub(crate) fn api_python_version_list_max_scroll(
    count: usize,
    visible_h: f32,
    scale: f32,
) -> f32 {
    let row_h = api_python_version_row_height(scale);
    let inner_h = (visible_h - 8.0 * scale).max(0.0);
    (count as f32 * row_h - inner_h).max(0.0)
}

pub(crate) fn api_python_version_row_height(scale: f32) -> f32 {
    28.0 * scale
}

pub(crate) fn api_python_scrollbar_drag_target(
    rect: (f32, f32, f32, f32),
    current: f32,
    max_scroll: f32,
    pointer_y: f32,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let geometry = api_python_scrollbar(rect, current, max_scroll, scale).geometry(scale)?;
    if let Some(offset) = drag_offset {
        Some((offset, geometry.drag_target(pointer_y, offset)?))
    } else {
        geometry.press_target(pointer_y)
    }
}

pub(crate) fn api_python_scrollbar(
    rect: (f32, f32, f32, f32),
    current: f32,
    max_scroll: f32,
    scale: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let track_h = (rect.3 - 12.0 * scale).max(0.0);
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 4.0,
            edge_gap: Some(4.0),
            track_pad: 0.0,
            min_thumb: 18.0,
            thumb_color: [1.0, 1.0, 1.0, 0.36],
            thumb_paint: crate::render_view::scrollbar_widget::ScrollbarPaint::Ink,
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane: (
            rect.0 + rect.2 - 12.0 * scale,
            rect.1 + 6.0 * scale,
            12.0 * scale,
            track_h,
        ),
        extent: ScrollbarExtent::with_max(track_h, max_scroll, current),
    }
}

pub(crate) fn api_python_install_log_visible(api: &ApiClientState) -> bool {
    api.mock_python_install_running || !api.mock_python_install_log.is_empty()
}

pub(crate) fn api_python_install_log_rect(
    layout: ApiPythonRuntimeDialogLayout,
    scale: f32,
) -> (f32, f32, f32, f32) {
    let y = layout.box_y + 286.0 * scale;
    let btn_y = layout.box_y + layout.box_h - 64.0 * scale;
    (
        layout.box_x + layout.pad,
        y,
        layout.content_w.max(0.0),
        (btn_y - y - 12.0 * scale).max(0.0),
    )
}

pub(crate) fn api_python_install_log_max_scroll(count: usize, view_h: f32, scale: f32) -> f32 {
    (count as f32 * api_python_install_log_line_height(scale) - view_h).max(0.0)
}

pub(crate) fn api_python_install_log_line_height(scale: f32) -> f32 {
    18.0 * scale
}


fn parse_uv_python_list(raw: &str) -> Vec<ApiPythonVersionRow> {
    let mut rows = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        let Some(version) = first
            .strip_prefix("cpython-")
            .or_else(|| first.strip_prefix("python-"))
        else {
            continue;
        };
        let version = version
            .split('-')
            .next()
            .unwrap_or(version)
            .trim()
            .to_string();
        if version.is_empty() {
            continue;
        }
        let installed = !line.contains("<download available>") && !line.contains("download only");
        rows.push(ApiPythonVersionRow {
            version,
            installed,
            detail: line.to_string(),
        });
        if rows.len() >= 80 {
            break;
        }
    }
    rows.sort_by(|a, b| b.version.cmp(&a.version));
    rows.dedup_by(|a, b| a.version == b.version);
    rows
}

fn push_api_python_install_log(api: &mut ApiClientState, line: ApiPythonInstallLogLine) {
    api.mock_python_install_log.push(line);
    if api.mock_python_install_log.len() > 24 {
        api.mock_python_install_log.remove(0);
    }
    api.mock_python_install_log_scroll.current = 10_000.0;
    api.mock_python_install_log_scroll.target = 10_000.0;
}

fn push_api_mock_server_log(api: &mut ApiClientState, text: String) {
    let stamp = format_api_mock_log_time(now_epoch_secs());
    api.mock_server_logs.push(ApiMockServerLogLine {
        text: format!("[{stamp}] {text}"),
    });
    if api.mock_server_logs.len() > 80 {
        api.mock_server_logs.remove(0);
    }
    api.mock_server_log_scroll.current = 1_000_000.0;
    api.mock_server_log_scroll.target = 1_000_000.0;
}

pub(crate) fn api_mock_server_log_max_scroll(line_count: usize, visible_h: f32, s: f32) -> f32 {
    let line_h = 20.0 * s;
    (line_count as f32 * line_h + 12.0 * s - visible_h).max(0.0)
}

pub(crate) fn api_mock_server_log_scrollbar_drag_target(
    rect: (f32, f32, f32, f32),
    line_count: usize,
    current: f32,
    pointer_y: f32,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let geometry = api_mock_server_log_scrollbar(rect, line_count, current, scale).geometry(scale)?;
    if let Some(offset) = drag_offset {
        Some((offset, geometry.drag_target(pointer_y, offset)?))
    } else {
        geometry.press_target(pointer_y)
    }
}

pub(crate) fn api_mock_server_log_scrollbar(
    rect: (f32, f32, f32, f32),
    line_count: usize,
    current: f32,
    scale: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let content_h = line_count as f32 * 20.0 * scale + 12.0 * scale;
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 4.0,
            edge_gap: Some(4.0),
            track_pad: 7.0,
            min_thumb: 24.0,
            thumb_color: [1.0, 1.0, 1.0, 0.24],
            thumb_paint: crate::render_view::scrollbar_widget::ScrollbarPaint::Ink,
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane: (
            rect.0 + rect.2 - 14.0 * scale,
            rect.1,
            14.0 * scale,
            rect.3,
        ),
        extent: ScrollbarExtent::new(rect.3, content_h, current),
    }
}

pub(crate) fn api_mock_guide_max_scroll(visible_h: f32, s: f32) -> f32 {
    (740.0 * s - visible_h).max(0.0)
}

pub(crate) fn api_mock_guide_scrollbar(
    lane: (f32, f32, f32, f32),
    current: f32,
    scale: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let viewport = lane.3;
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 4.0,
            edge_gap: Some(6.0),
            track_pad: 7.0,
            min_thumb: 28.0,
            thumb_color: [1.0, 1.0, 1.0, 0.28],
            thumb_paint: crate::render_view::scrollbar_widget::ScrollbarPaint::Ink,
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane,
        extent: ScrollbarExtent::with_max(viewport, api_mock_guide_max_scroll(viewport, scale), current),
    }
}

pub(crate) fn api_mock_combined_editor_scrollbar(
    lane: (f32, f32, f32, f32),
    viewport_h: f32,
    content_h: f32,
    current: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 3.0,
            min_thumb: 22.0,
            track_color: Some([0.52, 0.54, 0.60, 0.22]),
            thumb_color: [0.64, 0.66, 0.72, 0.70],
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane,
        extent: ScrollbarExtent::new(viewport_h, content_h, current),
    }
}

fn api_mock_server_event_text(event: &ApiMockServerEvent) -> String {
    match event {
        ApiMockServerEvent::Running { url } => format!("server ready: {url}"),
        ApiMockServerEvent::Log { text } => text.clone(),
        ApiMockServerEvent::Stopped => "server stopped".to_string(),
        ApiMockServerEvent::Failed(err) => format!("server error: {err}"),
        ApiMockServerEvent::Request {
            method,
            path,
            status,
            action,
        } => format!("{method} {path} -> {status} · {action}"),
    }
}

fn format_api_mock_log_time(epoch_secs: u64) -> String {
    let secs = epoch_secs % 86_400;
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn clear_legacy_api_python_runtime_message(api: &mut ApiClientState) {
    let message = api.mock.uv.last_error.as_str();
    if message.contains("uv run --python")
        || message.contains("загрузит версию")
        || message.contains("download python")
        || message.contains("download Python")
    {
        api.mock.uv.last_error.clear();
    }
}
