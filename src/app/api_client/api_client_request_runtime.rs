pub struct ApiJobRequest {
    pub request_id: u64,
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub method: ApiMethod,
    pub url: String,
    pub mock_target: ApiJobMockTarget,
    pub auth_parts: Vec<ApiPreparedAuthPart>,
    pub body_json: Option<String>,
    pub body_form: Option<Vec<ApiInputValue>>,
    pub body_multipart: Option<Vec<ApiMultipartPart>>,
    pub resolved_host: Option<ApiResolvedHost>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApiJobMockTarget {
    #[default]
    None,
    Mock,
    Proxy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiPreparedAuthPart {
    Header { name: String, value: String },
    Query { name: String, value: String },
    Cookie { name: String, value: String },
    Basic { username: String, password: String },
    Bearer { token: String },
    Digest { value: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiMultipartPart {
    Text { name: String, value: String },
    File { name: String, path: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiJobResponse {
    pub request_id: u64,
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub status: Option<u16>,
    pub elapsed_ms: u128,
    pub server_reach_ms: Option<u128>,
    pub timing_text: String,
    pub headers: Vec<(String, String)>,
    pub headers_text: String,
    pub body: String,
    pub truncated: bool,
    pub error: Option<ApiLoadError>,
    pub resolved_host: Option<ApiResolvedHost>,
}

pub fn api_response_text(response: &ApiJobResponse, view: ApiResponseView) -> &str {
    match view {
        ApiResponseView::Body => &response.body,
        ApiResponseView::Headers => &response.headers_text,
    }
}

pub fn spawn_api_request(job: ApiJobRequest) -> Receiver<ApiJobResponse> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let response = run_api_request(job);
        let _ = tx.send(response);
    });
    rx
}

fn send_api_request_body(
    request: reqwest::blocking::RequestBuilder,
    body_json: Option<&str>,
    body_form: Option<&[ApiInputValue]>,
    multipart_body: Option<(String, Vec<u8>)>,
) -> Result<reqwest::blocking::Response, reqwest::Error> {
    if let Some((content_type, body)) = multipart_body {
        request
            .header("Content-Type", content_type)
            .body(body)
            .send()
    } else if let Some(fields) = body_form {
        request.form(&api_form_pairs(fields)).send()
    } else {
        request
            .header("Content-Type", "application/json")
            .body(body_json.unwrap_or_default().to_string())
            .send()
    }
}

fn api_form_pairs(fields: &[ApiInputValue]) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    for field in fields {
        if field.value.contains('\n') {
            for value in field
                .value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                out.push((field.name.as_str(), value));
            }
        } else if !field.value.is_empty() {
            out.push((field.name.as_str(), field.value.as_str()));
        }
    }
    out
}

fn apply_auth_to_builder(
    mut request: reqwest::blocking::RequestBuilder,
    auth_parts: &[ApiPreparedAuthPart],
) -> reqwest::blocking::RequestBuilder {
    let mut cookie_header = String::new();
    for part in auth_parts {
        match part {
            ApiPreparedAuthPart::Header { name, value } => {
                request = request.header(name, value);
            }
            ApiPreparedAuthPart::Basic { username, password } => {
                request = request.basic_auth(username, Some(password));
            }
            ApiPreparedAuthPart::Bearer { token } => {
                request = request.bearer_auth(token);
            }
            ApiPreparedAuthPart::Digest { value } => {
                request = request.header("Authorization", format!("Digest {value}"));
            }
            ApiPreparedAuthPart::Cookie { name, value } => {
                if !cookie_header.is_empty() {
                    cookie_header.push_str("; ");
                }
                cookie_header.push_str(name);
                cookie_header.push('=');
                cookie_header.push_str(value);
            }
            ApiPreparedAuthPart::Query { .. } => {}
        }
    }
    if !cookie_header.is_empty() {
        request = request.header("Cookie", cookie_header);
    }
    request
}

fn build_multipart_body(
    parts: &[ApiMultipartPart],
    request_id: u64,
) -> Result<(String, Vec<u8>), ApiLoadError> {
    let boundary = format!("rriter-api-{}-{}", request_id, now_epoch_secs());
    let mut body = Vec::new();
    for part in parts {
        match part {
            ApiMultipartPart::Text { name, value } => {
                push_multipart_field(&mut body, &boundary, name, None, value.as_bytes());
            }
            ApiMultipartPart::File { name, path } => {
                let size = std::fs::metadata(path)
                    .ok()
                    .and_then(|meta| usize::try_from(meta.len()).ok())
                    .unwrap_or(0);
                if body.len().saturating_add(size) > API_MAX_MULTIPART_BODY_BYTES {
                    return Err(ApiLoadError::new(
                        ApiLoadErrorKind::TooLarge,
                        "multipart body больше лимита",
                    ));
                }
                let bytes = std::fs::read(path).map_err(|err| {
                    ApiLoadError::new(ApiLoadErrorKind::Io, format!("файл не прочитан: {}", err))
                })?;
                let file_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file");
                push_multipart_field(&mut body, &boundary, name, Some(file_name), &bytes);
            }
        }
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

fn push_multipart_field(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    file_name: Option<&str>,
    bytes: &[u8],
) {
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
    push_multipart_quoted(body, name);
    body.extend_from_slice(b"\"");
    if let Some(file_name) = file_name {
        body.extend_from_slice(b"; filename=\"");
        push_multipart_quoted(body, file_name);
        body.extend_from_slice(b"\"\r\nContent-Type: application/octet-stream");
    }
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

fn push_multipart_quoted(out: &mut Vec<u8>, value: &str) {
    for b in value.bytes() {
        if matches!(b, b'"' | b'\\' | b'\r' | b'\n') {
            out.push(b'_');
        } else {
            out.push(b);
        }
    }
}

fn run_api_request(job: ApiJobRequest) -> ApiJobResponse {
    let server_reach_ms = if job.mock_target == ApiJobMockTarget::Mock {
        None
    } else {
        measure_api_server_reach_ms(job.resolved_host.as_ref())
    };
    let mut response = ApiJobResponse {
        request_id: job.request_id,
        spec_id: job.spec_id,
        route_idx: job.route_idx,
        status: None,
        elapsed_ms: 0,
        server_reach_ms,
        timing_text: String::new(),
        headers: Vec::new(),
        headers_text: String::new(),
        body: String::new(),
        truncated: false,
        error: None,
        resolved_host: job.resolved_host.clone(),
    };
    let started = Instant::now();
    let client = api_http_client(job.resolved_host.as_ref());
    let multipart_body = job
        .body_multipart
        .as_ref()
        .map(|parts| build_multipart_body(parts, job.request_id))
        .transpose();
    let result = match multipart_body {
        Ok(multipart_body) => match job.method {
            ApiMethod::Get => apply_auth_to_builder(client.get(&job.url), &job.auth_parts).send(),
            ApiMethod::Delete => {
                apply_auth_to_builder(client.delete(&job.url), &job.auth_parts).send()
            }
            ApiMethod::Head => apply_auth_to_builder(client.head(&job.url), &job.auth_parts).send(),
            ApiMethod::Options => apply_auth_to_builder(
                client.request(reqwest::Method::OPTIONS, &job.url),
                &job.auth_parts,
            )
            .send(),
            ApiMethod::Trace => apply_auth_to_builder(
                client.request(reqwest::Method::TRACE, &job.url),
                &job.auth_parts,
            )
            .send(),
            ApiMethod::Post => {
                let req = apply_auth_to_builder(client.post(&job.url), &job.auth_parts);
                send_api_request_body(
                    req,
                    job.body_json.as_deref(),
                    job.body_form.as_deref(),
                    multipart_body,
                )
            }
            ApiMethod::Put => {
                let req = apply_auth_to_builder(client.put(&job.url), &job.auth_parts);
                send_api_request_body(
                    req,
                    job.body_json.as_deref(),
                    job.body_form.as_deref(),
                    multipart_body,
                )
            }
            ApiMethod::Patch => {
                let req = apply_auth_to_builder(client.patch(&job.url), &job.auth_parts);
                send_api_request_body(
                    req,
                    job.body_json.as_deref(),
                    job.body_form.as_deref(),
                    multipart_body,
                )
            }
        },
        Err(err) => {
            response.elapsed_ms = started.elapsed().as_millis();
            response.timing_text = format_api_timing_text(
                response.elapsed_ms,
                response.server_reach_ms,
                job.mock_target,
            );
            response.error = Some(err);
            return response;
        }
    };
    response.elapsed_ms = started.elapsed().as_millis();
    response.timing_text =
        format_api_timing_text(response.elapsed_ms, response.server_reach_ms, job.mock_target);
    match result {
        Ok(mut res) => {
            response.status = Some(res.status().as_u16());
            for (name, value) in res.headers().iter() {
                if let Ok(v) = value.to_str() {
                    response
                        .headers
                        .push((name.as_str().to_string(), v.to_string()));
                }
            }
            response.headers_text = format_api_response_headers(&response.headers);
            match read_limited_text(&mut res, API_MAX_RESPONSE_BYTES) {
                Ok(body) => response.body = format_api_response_body(body),
                Err(err) if err.kind == ApiLoadErrorKind::TooLarge => {
                    response.truncated = true;
                    response.body = "Ответ больше лимита".to_string();
                }
                Err(err) => response.error = Some(err),
            }
        }
        Err(err) => response.error = Some(classify_reqwest_error(err)),
    }
    response
}

fn format_api_response_headers(headers: &[(String, String)]) -> String {
    if headers.is_empty() {
        return "No headers".to_string();
    }
    let capacity = headers
        .iter()
        .map(|(name, value)| name.len() + value.len() + 3)
        .sum();
    let mut out = String::with_capacity(capacity);
    for (idx, (name, value)) in headers.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
    }
    out
}

#[cfg(test)]
fn capture_response_auth(
    auth: &mut ApiAuthStore,
    spec_id: ApiSpecId,
    schemes: &[ApiSecurityScheme],
    response: &ApiJobResponse,
) -> bool {
    let mut changed = false;
    if let Ok(json) = serde_json::from_str::<Value>(&response.body) {
        changed |= capture_token_json(auth, spec_id, schemes, &json);
    }
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("set-cookie") {
            changed |= capture_set_cookie(auth, spec_id, schemes, value);
        }
    }
    changed
}

#[cfg(test)]
fn capture_token_json(
    auth: &mut ApiAuthStore,
    spec_id: ApiSpecId,
    schemes: &[ApiSecurityScheme],
    json: &Value,
) -> bool {
    let access_token = json.get("access_token").and_then(Value::as_str);
    let refresh_token = json.get("refresh_token").and_then(Value::as_str);
    if access_token.is_none() && refresh_token.is_none() {
        return false;
    }
    let token_type = json
        .get("token_type")
        .and_then(Value::as_str)
        .unwrap_or("Bearer");
    let expires_at = json
        .get("expires_in")
        .and_then(Value::as_u64)
        .map(|secs| now_epoch_secs().saturating_add(secs));
    let scopes = json
        .get("scope")
        .and_then(Value::as_str)
        .map(|scope| {
            scope
                .split_whitespace()
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .or_else(|| {
            json.get("scopes").and_then(Value::as_array).map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();
    let mut changed = false;
    for scheme in schemes.iter().filter(|scheme| scheme.token_capable()) {
        let entry = auth.entry_mut(spec_id, &scheme.name);
        if let Some(token) = access_token
            && entry.access_token != token
        {
            entry.access_token = token.to_string();
            entry.value = token.to_string();
            changed = true;
        }
        if let Some(token) = refresh_token
            && entry.refresh_token != token
        {
            entry.refresh_token = token.to_string();
            changed = true;
        }
        if entry.token_type != token_type {
            entry.token_type = token_type.to_string();
            changed = true;
        }
        if entry.expires_at != expires_at {
            entry.expires_at = expires_at;
            changed = true;
        }
        if !scopes.is_empty() && entry.scopes != scopes {
            entry.scopes = scopes.clone();
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
fn capture_set_cookie(
    auth: &mut ApiAuthStore,
    spec_id: ApiSpecId,
    schemes: &[ApiSecurityScheme],
    header: &str,
) -> bool {
    let Some((cookie_name, rest)) = header.split_once('=') else {
        return false;
    };
    let cookie_name = cookie_name.trim();
    if cookie_name.is_empty() {
        return false;
    }
    let cookie_value = rest.split(';').next().unwrap_or("").trim();
    let mut changed = false;
    for scheme in schemes {
        if let ApiSecuritySchemeKind::ApiKey {
            name,
            location: ApiSecurityApiKeyLocation::Cookie,
        } = &scheme.kind
            && name == cookie_name
        {
            let entry = auth.entry_mut(spec_id, &scheme.name);
            if entry.value != cookie_value {
                entry.value = cookie_value.to_string();
                changed = true;
            }
        }
    }
    changed
}

fn measure_api_server_reach_ms(resolved: Option<&ApiResolvedHost>) -> Option<u128> {
    let resolved = resolved?;
    measure_api_icmp_reach_ms(resolved).or_else(|| measure_api_tcp_reach_ms(resolved))
}

fn measure_api_icmp_reach_ms(resolved: &ApiResolvedHost) -> Option<u128> {
    let ip = resolved.ip.to_string();
    let output = Command::new("ping")
        .args(["-n", "-c", "1", "-W", "1", ip.as_str()])
        .output()
        .ok()?;
    parse_api_ping_rtt_ms(&output.stdout)
        .or_else(|| parse_api_ping_rtt_ms(&output.stderr))
        .map(|rtt_ms| rtt_ms.saturating_add(1) / 2)
}

fn measure_api_tcp_reach_ms(resolved: &ApiResolvedHost) -> Option<u128> {
    let addr = SocketAddr::new(resolved.ip, resolved.port);
    let started = Instant::now();
    TcpStream::connect_timeout(&addr, API_REACH_TIMEOUT)
        .ok()
        .map(|_| started.elapsed().as_millis().max(1).saturating_add(1) / 2)
}

fn parse_api_ping_rtt_ms(bytes: &[u8]) -> Option<u128> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text.contains("time<1") {
        return Some(1);
    }
    let rest = text.split_once("time=")?.1;
    let mut end = 0usize;
    for (idx, ch) in rest.char_indices() {
        if ch.is_ascii_digit() || ch == '.' || ch == ',' {
            end = idx + ch.len_utf8();
        } else if end > 0 {
            break;
        } else {
            return None;
        }
    }
    let value = rest.get(..end)?.replace(',', ".");
    let millis = value.parse::<f64>().ok()?;
    Some((millis.round().max(1.0)) as u128)
}

fn format_api_timing_text(
    elapsed_ms: u128,
    server_reach_ms: Option<u128>,
    mock_target: ApiJobMockTarget,
) -> String {
    if mock_target == ApiJobMockTarget::Mock {
        return format!("{elapsed_ms} ms (мок-сервер)");
    }
    if mock_target == ApiJobMockTarget::Proxy {
        return match server_reach_ms {
            Some(server_reach_ms) => format!("{server_reach_ms} ms до сервера"),
            None => "n/a до сервера".to_string(),
        };
    }
    match server_reach_ms {
        Some(server_reach_ms) => format!("{elapsed_ms} ms (~{server_reach_ms} ms до сервера)"),
        None => format!("{elapsed_ms} ms (n/a до сервера)"),
    }
}

fn format_api_response_body(body: String) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return body;
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or(body)
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn format_last_loaded_at(last_loaded: Option<u64>, now: u64) -> String {
    let Some(loaded) = last_loaded else {
        return "не загружено".to_string();
    };
    let age = now.saturating_sub(loaded);
    if age < 60 {
        "только что".to_string()
    } else if age < 3600 {
        format!("{} мин назад", age / 60)
    } else if age < 86_400 {
        format!("{} ч назад", age / 3600)
    } else {
        format!("{} д назад", age / 86_400)
    }
}

pub fn format_api_secs(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.3}с", v.max(0.0)),
        None => "-".to_string(),
    }
}

#[cfg(test)]
pub fn format_api_path_display(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    write_api_path_display(path, &mut out);
    out
}

pub fn write_api_path_display(path: &str, out: &mut String) {
    out.clear();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
            out.push(ch);
            for inner in chars.by_ref() {
                out.push(inner);
                if inner == '}' {
                    break;
                }
            }
            if chars.peek() == Some(&'/') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
}

pub fn api_timing_visible_at(last_loaded: Option<u64>, now: u64) -> bool {
    last_loaded
        .map(|loaded| now.saturating_sub(loaded) < 10)
        .unwrap_or(false)
}

fn line_end_without_newline(editor: &Editor, line_idx: usize) -> usize {
    editor
        .line_offsets
        .get(line_idx + 1)
        .map(|&offset| offset.saturating_sub(1))
        .unwrap_or(editor.len())
}

fn non_empty_path(text: &str) -> Option<PathBuf> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn move_api_input_vertical(editor: &mut Editor, down: bool, shift: bool) {
    if shift {
        if editor.selection_anchor.is_none() {
            editor.selection_anchor = Some(editor.cursor);
        }
    } else {
        editor.selection_anchor = None;
    }
    let line_idx = editor
        .line_offsets
        .partition_point(|&offset| offset <= editor.cursor)
        .saturating_sub(1);
    let Some(&line_start) = editor.line_offsets.get(line_idx) else {
        editor.cursor = editor.len();
        return;
    };
    let col = editor.cursor.saturating_sub(line_start);
    let target_line = if down {
        (line_idx + 1).min(editor.line_offsets.len().saturating_sub(1))
    } else {
        line_idx.saturating_sub(1)
    };
    let Some(&target_start) = editor.line_offsets.get(target_line) else {
        return;
    };
    let target_end = line_end_without_newline(editor, target_line);
    editor.cursor = target_start.saturating_add(col).min(target_end);
}

fn api_line_byte_at_x(
    renderer: &mut crate::renderer::Renderer,
    line: &str,
    target_x: f32,
    text_scale: f32,
) -> usize {
    let mut x = 0.0;
    for (byte_idx, ch) in line.char_indices() {
        if ch == '\n' || ch == '\r' || ch == '\u{FE0F}' || ch == '\u{200D}' {
            continue;
        }
        let adv = renderer
            .get_ui_glyph(ch)
            .map(|g| crate::renderer::Renderer::snapped_text_advance(g.advance, text_scale))
            .unwrap_or(8.0);
        if target_x <= x + adv * 0.5 {
            return byte_idx;
        }
        x += adv;
    }
    line.len()
}

pub(crate) fn api_multiline_ui_byte_at_pointer(
    editor: &Editor,
    renderer: &mut crate::renderer::Renderer,
    cursor_left_x: f32,
    cursor_top_y: f32,
    mx: f32,
    my: f32,
    scale: f32,
    scroll_y: f32,
    scroll_x: f32,
    text_scale: f32,
) -> usize {
    let line_h = api_text_area_line_height(scale);
    let content_y = (my - cursor_top_y + scroll_y - line_h * 0.25).max(0.0);
    let target_line = (content_y / line_h).floor() as usize;
    let line_idx = target_line.min(editor.line_offsets.len().saturating_sub(1));
    let Some(&line_start) = editor.line_offsets.get(line_idx) else {
        return editor.len();
    };
    let line_end = line_end_without_newline(editor, line_idx);
    if line_start >= line_end {
        return line_start;
    }
    let text = editor.line_text_owned(line_idx);
    let line = text.trim_end_matches(['\r', '\n']);
    let target_x = (mx - cursor_left_x + scroll_x).max(0.0);
    line_start + api_line_byte_at_x(renderer, line, target_x, text_scale)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ApiMockTyDiagLayout {
    pub x_start: f32,
    pub squiggle_y: f32,
    pub squiggle_w: f32,
    pub line_h: f32,
    pub line_top: f32,
    pub hit_top: f32,
    pub byte_offset: usize,
}

pub(crate) fn api_byte_offset_for_char_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(idx, _)| idx)
        .unwrap_or(line.len())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn api_mock_ty_diag_layout<F>(
    text: &str,
    diag: &ApiMockTyDiagnostic,
    part: ApiMockSourcePart,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    scroll_y: f32,
    scroll_x: f32,
    mut measure: F,
) -> Option<ApiMockTyDiagLayout>
where
    F: FnMut(&str) -> f32,
{
    if diag.part != part {
        return None;
    }
    let line_h = api_text_area_line_height(s);
    let first_line = (scroll_y / line_h).floor() as usize;
    if diag.line < first_line {
        return None;
    }
    let line_offset = scroll_y - first_line as f32 * line_h;
    let visible_idx = diag.line - first_line;
    let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
    if visible_idx >= max_lines {
        return None;
    }
    let mut line_start = 0usize;
    let mut line = None;
    for (idx, candidate) in text.split('\n').enumerate() {
        if idx == diag.line {
            line = Some(candidate);
            break;
        }
        line_start = line_start.saturating_add(candidate.len()).saturating_add(1);
    }
    let line = line?;
    let start_byte = api_byte_offset_for_char_col(line, diag.start_col);
    let end_byte = api_byte_offset_for_char_col(line, diag.end_col);
    let x_start = x + measure(&line[..start_byte]) - scroll_x;
    let x_end = x + measure(&line[..end_byte]) - scroll_x;
    let base_y = y - line_offset + visible_idx as f32 * line_h;
    let squiggle_w = (x_end - x_start).max(8.0 * s).min(w);
    Some(ApiMockTyDiagLayout {
        x_start: x_start.round(),
        squiggle_y: (base_y + 3.0 * s).round(),
        squiggle_w,
        line_h,
        line_top: base_y - api_text_area_baseline_offset(s),
        hit_top: base_y - 14.0 * s,
        byte_offset: line_start.saturating_add(start_byte),
    })
}

pub(crate) fn api_mock_body_editor_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for (idx, line) in text.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        if line.is_empty() || line.starts_with(char::is_whitespace) {
            out.push_str(line);
        } else {
            out.push_str("    ");
            out.push_str(line);
        }
    }
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn backspace_api_mock_body_editor(editor: &mut Editor) -> Option<(usize, usize)> {
    if let Some(deleted) = editor.backspace() {
        return Some(deleted);
    }
    if editor.cursor == 0 && editor.len() > 0 && editor.byte_at(0) == b'\n' {
        let deleted = editor.delete_forward();
        if deleted.is_some() {
            editor.cursor = editor.len();
            editor.selection_anchor = Some(editor.cursor);
        }
        return deleted;
    }
    None
}

fn set_api_multiline_cursor_at_pointer(
    editor: &mut Editor,
    renderer: &mut crate::renderer::Renderer,
    cursor_left_x: f32,
    cursor_top_y: f32,
    mx: f32,
    my: f32,
    scale: f32,
    scroll_y: f32,
    scroll_x: f32,
    is_click: bool,
) {
    let old_line_height = renderer.line_height;
    let old_left_padding = renderer.left_padding;
    let old_last_scroll_x = renderer.last_scroll_x;
    let old_inlay_hints = std::mem::take(&mut renderer.current_python_inlay_hints);

    renderer.line_height = api_text_area_line_height(scale);
    renderer.left_padding = cursor_left_x;
    renderer.last_scroll_x = scroll_x;
    editor.set_cursor_at_pos(
        mx,
        my - cursor_top_y + scroll_y + renderer.line_height * 0.25,
        renderer,
        is_click,
    );
    renderer.line_height = old_line_height;
    renderer.left_padding = old_left_padding;
    renderer.last_scroll_x = old_last_scroll_x;
    renderer.current_python_inlay_hints = old_inlay_hints;
}
