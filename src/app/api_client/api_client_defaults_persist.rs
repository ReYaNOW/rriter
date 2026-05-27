fn fill_api_tab_inputs(state: &mut ApiClientTabState, route: &ApiRouteRow, model: &ApiSpecModel) {
    state.path_values = route
        .path_params
        .iter()
        .map(|param| ApiInputValue {
            name: param.name.clone(),
            value: param
                .default_value
                .clone()
                .or_else(|| param.example.clone())
                .unwrap_or_default(),
        })
        .collect();
    state.query_values = route
        .query_params
        .iter()
        .map(|param| ApiInputValue {
            name: param.name.clone(),
            value: param
                .default_value
                .clone()
                .or_else(|| param.example.clone())
                .unwrap_or_default(),
        })
        .collect();
    state.body_values = default_body_values_for_route(route, model);
    state.body_json = default_body_for_route(route, model);
}

fn default_body_values_for_route(route: &ApiRouteRow, model: &ApiSpecModel) -> Vec<ApiInputValue> {
    let Some(body) = route
        .request_body
        .as_ref()
        .filter(|body| body.is_multipart || body.is_form_urlencoded)
    else {
        return Vec::new();
    };
    body.schema
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
        .map(|schema| {
            schema
                .properties
                .iter()
                .filter_map(|prop| {
                    let prop_schema = model.schema_arena.get(prop.schema.0)?;
                    Some(ApiInputValue {
                        name: prop.name.clone(),
                        value: prop_schema
                            .default_value
                            .clone()
                            .or_else(|| prop_schema.examples.first().cloned())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn api_multipart_parts_for_route(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    values: &[ApiInputValue],
) -> Vec<ApiMultipartPart> {
    let Some(body) = route.request_body.as_ref().filter(|body| body.is_multipart) else {
        return Vec::new();
    };
    let Some(schema) = body
        .schema
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for prop in &schema.properties {
        let Some(prop_schema) = model.schema_arena.get(prop.schema.0) else {
            continue;
        };
        let value = values
            .iter()
            .find(|item| item.name == prop.name)
            .map(|item| item.value.as_str())
            .unwrap_or("");
        if api_schema_is_file_input(prop_schema, model) {
            for path in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
                out.push(ApiMultipartPart::File {
                    name: prop.name.clone(),
                    path: PathBuf::from(path),
                });
            }
        } else if api_schema_is_array_input(prop_schema) {
            for item in split_api_array_values(value) {
                out.push(ApiMultipartPart::Text {
                    name: prop.name.clone(),
                    value: item,
                });
            }
        } else {
            out.push(ApiMultipartPart::Text {
                name: prop.name.clone(),
                value: value.to_string(),
            });
        }
    }
    out
}

fn default_body_for_route(route: &ApiRouteRow, model: &ApiSpecModel) -> String {
    let Some(body) = &route.request_body else {
        return String::new();
    };
    if body.is_form_urlencoded {
        return String::new();
    }
    let Some(schema_ref) = body.schema else {
        return "{\n  \n}".to_string();
    };
    schema_example_json(schema_ref, model, 0)
}

pub(crate) fn api_generated_response_for_route(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
) -> (u16, &'static str, String) {
    let response = route
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .or_else(|| {
            route
                .responses
                .iter()
                .find(|response| response.status == "default")
        })
        .or_else(|| route.responses.first());
    let status = response
        .and_then(|response| response.status.parse::<u16>().ok())
        .unwrap_or(200);
    let content_type = response
        .map(|response| response.content_type.as_str())
        .unwrap_or("application/json");
    let is_json = content_type.is_empty() || content_type.contains("json");
    if let Some(example) = response.and_then(|response| response.example.as_ref()) {
        if is_json && serde_json::from_str::<Value>(example).is_err() {
            return (status, "application/json", "{}".to_string());
        }
        return (
            status,
            if is_json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            },
            example.clone(),
        );
    }
    if let Some(schema_ref) = response.and_then(|response| response.schema) {
        return (
            status,
            if is_json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            },
            schema_example_json(schema_ref, model, 0),
        );
    }
    if is_json {
        (status, "application/json", "{}".to_string())
    } else {
        (status, "text/plain; charset=utf-8", String::new())
    }
}

pub(crate) fn api_mock_lan_url(mock: &ApiMockState) -> String {
    match &mock.server_status {
        crate::app::api_mock::types::ApiMockServerStatus::Running { url } => url.clone(),
        _ => format!("http://0.0.0.0:{}", mock.port),
    }
}

pub(crate) fn api_manual_route_title(method: ApiMethod, path: &str) -> String {
    format!("Mock · {} {}", method.as_str(), path)
}

fn schema_example_json(schema_ref: ApiSchemaRef, model: &ApiSpecModel, depth: usize) -> String {
    if depth > 6 {
        return "null".to_string();
    }
    let Some(schema) = model.schema_arena.get(schema_ref.0) else {
        return "null".to_string();
    };
    if let Some(value) = schema.examples.first() {
        return match schema.kind {
            ApiSchemaKind::Object | ApiSchemaKind::Array | ApiSchemaKind::Unknown => {
                if serde_json::from_str::<Value>(value).is_ok() {
                    value.clone()
                } else {
                    "null".to_string()
                }
            }
            _ => schema_json_literal(schema.kind, value),
        };
    }
    if let Some(value) = schema
        .default_value
        .as_ref()
        .or_else(|| schema.enum_values.first())
    {
        return schema_json_literal(schema.kind, value);
    }
    match schema.kind {
        ApiSchemaKind::Object => {
            let mut lines = Vec::new();
            for prop in schema.properties.iter().take(24) {
                let value = schema_example_json(prop.schema, model, depth + 1);
                lines.push(format!("  \"{}\": {}", prop.name, value));
            }
            if lines.is_empty() {
                "{\n  \n}".to_string()
            } else {
                format!("{{\n{}\n}}", lines.join(",\n"))
            }
        }
        ApiSchemaKind::Array => {
            let item = schema
                .item
                .map(|item| schema_example_json(item, model, depth + 1))
                .unwrap_or_else(|| "null".to_string());
            format!("[{}]", item)
        }
        ApiSchemaKind::String | ApiSchemaKind::Date | ApiSchemaKind::DateTime => "\"\"".to_string(),
        ApiSchemaKind::Integer | ApiSchemaKind::Number => "0".to_string(),
        ApiSchemaKind::Boolean => "false".to_string(),
        ApiSchemaKind::Bytes => "\"\"".to_string(),
        ApiSchemaKind::Unknown => "null".to_string(),
    }
}

fn schema_json_literal(kind: ApiSchemaKind, value: &str) -> String {
    match kind {
        ApiSchemaKind::String
        | ApiSchemaKind::Date
        | ApiSchemaKind::DateTime
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

fn api_config_dir() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir().join("rriter_api_client_tests");
    }
    #[cfg(not(test))]
    {
        let mut path = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        path.push(".config");
        path.push("RRiter");
        path
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

fn load_api_auth() -> ApiAuthStore {
    std::fs::read_to_string(api_auth_path())
        .ok()
        .and_then(|content| serde_json::from_str::<ApiAuthStore>(&content).ok())
        .unwrap_or_default()
}

fn save_api_auth(auth: &ApiAuthStore) {
    let Ok(content) = serde_json::to_string_pretty(auth) else {
        return;
    };
    if let Some(dir) = api_auth_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    write_secret_file(&api_auth_path(), content.as_bytes());
}

fn write_secret_file(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
        {
            let _ = file.write_all(bytes);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(path, bytes);
    }
}

fn save_url_cache(id: ApiSpecId, raw: &str) {
    let dir = api_cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}.json", id.0)), raw);
}

fn read_url_cache(id: ApiSpecId) -> Option<String> {
    std::fs::read_to_string(api_cache_dir().join(format!("{}.json", id.0))).ok()
}

pub(crate) fn api_python_runtime_dialog_layout(
    width: f32,
    height: f32,
    scale: f32,
) -> ApiPythonRuntimeDialogLayout {
    let pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * scale;
    let box_w = (crate::app::file_tree::FILE_TREE_DIALOG_W * scale).min(width - 32.0 * scale);
    let box_h = (500.0 * scale).min(height - 32.0 * scale);
    let box_x = ((width - box_w) / 2.0).round();
    let box_y = ((height - box_h) / 2.0).round();
    ApiPythonRuntimeDialogLayout {
        box_x,
        box_y,
        box_w,
        box_h,
        pad,
        content_w: box_w - pad * 2.0,
    }
}

pub(crate) fn api_python_version_list_rect(
    layout: ApiPythonRuntimeDialogLayout,
    scale: f32,
) -> (f32, f32, f32, f32) {
    (
        layout.box_x + layout.pad,
        layout.box_y + 210.0 * scale,
        layout.content_w,
        158.0 * scale,
    )
}

pub(crate) fn api_python_version_list_max_scroll(count: usize, scale: f32) -> f32 {
    let row_h = api_python_version_row_height(scale);
    let inner_h = (158.0 * scale - 8.0 * scale).max(row_h);
    (count as f32 * row_h - inner_h).max(0.0)
}

pub(crate) fn api_python_version_row_height(scale: f32) -> f32 {
    28.0 * scale
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
        layout.content_w,
        (btn_y - y - 12.0 * scale).max(44.0 * scale),
    )
}

pub(crate) fn api_python_install_log_max_scroll(count: usize, view_h: f32, scale: f32) -> f32 {
    (count as f32 * api_python_install_log_line_height(scale) - view_h).max(0.0)
}

pub(crate) fn api_python_install_log_line_height(scale: f32) -> f32 {
    18.0 * scale
}

fn api_point_in_rect(mx: f32, my: f32, rect: (f32, f32, f32, f32)) -> bool {
    mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3
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

fn spawn_api_python_log_reader<R>(
    stream: R,
    tx: mpsc::Sender<ApiPythonInstallEvent>,
    kind: ApiPythonInstallLogKind,
) where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let _ = tx.send(ApiPythonInstallEvent::Line(ApiPythonInstallLogLine {
                text: line,
                kind,
            }));
        }
    });
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

pub(crate) fn api_mock_guide_max_scroll(visible_h: f32, s: f32) -> f32 {
    (720.0 * s - visible_h).max(0.0)
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
