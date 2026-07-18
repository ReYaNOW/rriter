use super::merge::{api_mock_path_params, resolve_api_mock_route};
use super::python_worker::{PythonMockRequest, call_python_route, stop_python_worker};
use super::types::{
    ApiMockContractField, ApiMockContractFieldKind, ApiMockRouteDecision, ApiMockRuntimeRoute,
    ApiMockServerEvent, ApiMockServerSnapshot, ApiMockServerStatus,
};
use crate::app::api_client::ApiMethod;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::Response;
use axum::routing::any;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::oneshot;

static SERVER: LazyLock<Mutex<Option<ApiMockServerHandle>>> = LazyLock::new(|| Mutex::new(None));
static EVENTS: LazyLock<Mutex<Vec<ApiMockServerEvent>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static SERVER_STOPPING: AtomicBool = AtomicBool::new(false);

struct ApiMockServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
    snapshot: Arc<Mutex<ApiMockServerSnapshot>>,
    finished: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct ApiMockAxumState {
    snapshot: Arc<Mutex<ApiMockServerSnapshot>>,
    proxy_client: reqwest::Client,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MultipartValue {
    Text(String),
    File {
        filename: String,
        content_type: Option<String>,
        content_base64: String,
        size: usize,
    },
}

pub fn drain_api_mock_server_events() -> Vec<ApiMockServerEvent> {
    crate::platform::lock_recover(&EVENTS).drain(..).collect()
}

pub fn start_api_mock_server(snapshot: ApiMockServerSnapshot) -> Result<(), String> {
    if SERVER_STOPPING.load(Ordering::Acquire) {
        return Err("Mock server is still stopping".to_string());
    }
    let mut server = crate::platform::lock_recover(&SERVER);
    reap_finished_server(&mut server);
    if server.is_some() {
        return Ok(());
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let snapshot = Arc::new(Mutex::new(snapshot));
    let thread_snapshot = Arc::clone(&snapshot);
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("rriter-api-mock".to_string())
        .spawn(move || {
            run_server_thread(thread_snapshot, shutdown_rx);
            let _ = finished_tx.send(());
        })
        .map_err(|err| err.to_string())?;
    *server = Some(ApiMockServerHandle {
        shutdown: Some(shutdown_tx),
        snapshot,
        finished: finished_rx,
        thread: Some(thread),
    });
    Ok(())
}

pub fn update_api_mock_server_snapshot(snapshot: ApiMockServerSnapshot) -> Result<bool, String> {
    let mut server = crate::platform::lock_recover(&SERVER);
    reap_finished_server(&mut server);
    let Some(handle) = server.as_ref() else {
        return Ok(false);
    };
    let mut current = crate::platform::lock_recover(&handle.snapshot);
    if *current == snapshot {
        return Ok(false);
    }
    *current = snapshot;
    push_log_event("server config hot-updated");
    Ok(true)
}

pub fn stop_api_mock_server() {
    if SERVER_STOPPING.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut handle = crate::platform::lock_recover(&SERVER).take();
    if let Some(handle) = handle.as_mut() {
        if let Some(shutdown) = handle.shutdown.take() {
            let _ = shutdown.send(());
        }
        if handle.finished.recv_timeout(Duration::from_secs(2)).is_ok()
            && let Some(thread) = handle.thread.take()
        {
            let _ = thread.join();
        }
    }
    SERVER_STOPPING.store(false, Ordering::Release);
    stop_python_worker();
}

fn reap_finished_server(server: &mut Option<ApiMockServerHandle>) -> bool {
    let finished = server.as_ref().is_some_and(|handle| {
        !matches!(
            handle.finished.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        )
    });
    if !finished {
        return false;
    }
    if let Some(mut handle) = server.take()
        && let Some(thread) = handle.thread.take()
    {
        let _ = thread.join();
    }
    true
}

pub fn apply_api_mock_server_event(status: &mut ApiMockServerStatus, event: ApiMockServerEvent) {
    match event {
        ApiMockServerEvent::Running { url } => *status = ApiMockServerStatus::Running { url },
        ApiMockServerEvent::Stopped => *status = ApiMockServerStatus::Stopped,
        ApiMockServerEvent::Failed(err) => *status = ApiMockServerStatus::Failed(err),
        ApiMockServerEvent::Log { .. } => {}
        ApiMockServerEvent::Request { .. } => {}
    }
}

fn run_server_thread(
    snapshot: Arc<Mutex<ApiMockServerSnapshot>>,
    shutdown_rx: oneshot::Receiver<()>,
) {
    push_log_event("tokio runtime: creating multi-thread runtime");
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            push_event(ApiMockServerEvent::Failed(err.to_string()));
            return;
        }
    };

    runtime.block_on(async move {
        push_log_event("bind address: resolving");
        let bind_snapshot = crate::platform::lock_recover(&snapshot).clone();
        let addr = match socket_addr(&bind_snapshot.bind_host, bind_snapshot.port) {
            Ok(addr) => addr,
            Err(err) => {
                push_event(ApiMockServerEvent::Failed(err));
                    return;
            }
        };
        push_log_event(&format!("tcp bind: {addr}"));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(err) => {
                push_event(ApiMockServerEvent::Failed(format_api_mock_bind_error(
                    addr, &err,
                )));
                    return;
            }
        };
        let local = listener.local_addr().ok();
        if let Some(local) = local {
            push_log_event(&format!("listener ready: http://{local}"));
            push_event(ApiMockServerEvent::Running {
                url: format!("http://{}", local),
            });
        }
        push_log_event("axum router: building fallback router");
        let proxy_client = match crate::app::api_client::api_async_client_builder()
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                push_event(ApiMockServerEvent::Failed(format!(
                    "Proxy HTTP client initialization failed: {error}"
                )));
                    return;
            }
        };
        let state = ApiMockAxumState {
            snapshot,
            proxy_client,
        };
        let app = Router::new()
            .fallback(any(handle_mock_request))
            .with_state(state);
        push_log_event("axum serve: started");
        let result = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
        if let Err(err) = result {
            push_event(ApiMockServerEvent::Failed(err.to_string()));
        } else {
            push_event(ApiMockServerEvent::Stopped);
        }
    });
}

async fn handle_mock_request(
    State(state): State<ApiMockAxumState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let snapshot = crate::platform::lock_recover(&state.snapshot).clone();
    let Some(api_method) = api_method_from_http(&method) else {
        let response = response_text(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain",
            "method not allowed",
        );
        push_request_event(
            method.as_str(),
            uri.path(),
            response.status().as_u16(),
            "method_not_allowed",
        );
        return response;
    };
    let path = uri.path();
    match resolve_api_mock_route(&snapshot.routes, snapshot.mode, api_method, path) {
        ApiMockRouteDecision::Mock(route) => {
            if let Some(script) = route.python.as_ref().filter(|script| script.enabled) {
                let request = python_request(&method, &uri, &headers, &body, route);
                let response = match call_python_route(&snapshot.python_runtime, script, request) {
                    Ok(output) => {
                        let status = StatusCode::from_u16(output.status).unwrap_or(StatusCode::OK);
                        let mut builder = Response::builder()
                            .status(status)
                            .header("content-type", output.content_type);
                        for (name, value) in output.headers {
                            if let Ok(name) = HeaderName::from_bytes(name.as_bytes())
                                && let Ok(value) = HeaderValue::from_str(&value)
                            {
                                builder = builder.header(name, value);
                            }
                        }
                        builder
                            .body(Body::from(output.body))
                            .unwrap_or_else(|_| Response::new(Body::empty()))
                    }
                    Err(err) => response_text(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", err),
                };
                push_request_event(method.as_str(), path, response.status().as_u16(), "python");
                return response;
            }
            let (status, content_type, text) = route.static_response_text();
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
            let response = response_text(status, content_type, text);
            push_request_event(method.as_str(), path, response.status().as_u16(), "mock");
            response
        }
        ApiMockRouteDecision::Proxy => {
            let method_label = method.as_str().to_string();
            let path_label = path.to_string();
            let response = proxy_request(
                state.proxy_client.clone(),
                snapshot.proxy_base_url,
                method,
                uri,
                headers,
                body,
            )
            .await;
            push_request_event(
                &method_label,
                &path_label,
                response.status().as_u16(),
                "proxy",
            );
            response
        }
        ApiMockRouteDecision::NotFound => {
            if should_proxy_unmatched(&snapshot) {
                let method_label = method.as_str().to_string();
                let path_label = path.to_string();
                let response = proxy_request(
                    state.proxy_client.clone(),
                    snapshot.proxy_base_url,
                    method,
                    uri,
                    headers,
                    body,
                )
                .await;
                push_request_event(
                    &method_label,
                    &path_label,
                    response.status().as_u16(),
                    "proxy",
                );
                return response;
            }
            let response =
                response_text(StatusCode::NOT_FOUND, "text/plain", "mock route not found");
            push_request_event(
                method.as_str(),
                path,
                response.status().as_u16(),
                "not_found",
            );
            response
        }
    }
}

fn python_request(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &Bytes,
    route: &ApiMockRuntimeRoute,
) -> PythonMockRequest {
    let mut header_map = BTreeMap::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            header_map.insert(name.as_str().to_string(), value.to_string());
        }
    }
    let mut raw_query = BTreeMap::<String, Vec<String>>::new();
    if let Some(raw_query_text) = uri.query() {
        for (key, value) in url::form_urlencoded::parse(raw_query_text.as_bytes()) {
            raw_query
                .entry(key.into_owned())
                .or_default()
                .push(value.into_owned());
        }
    }
    let query = values_from_pairs(&raw_query, &route.contract.query.fields);
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let (body, fields) = request_body_values(body, content_type, route);
    let params = api_mock_path_params(&route.path, uri.path())
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| (name, Value::String(value)))
        .collect();
    PythonMockRequest {
        method: method.as_str().to_string(),
        path: uri.path().to_string(),
        headers: header_map,
        params,
        query,
        body,
        fields,
    }
}

fn request_body_values(
    body: &Bytes,
    content_type: &str,
    route: &ApiMockRuntimeRoute,
) -> (Value, Value) {
    if !route.contract.body.enabled {
        return (Value::Null, Value::Object(Map::new()));
    }
    let lower_content_type = content_type.to_ascii_lowercase();
    if lower_content_type.contains("application/x-www-form-urlencoded") {
        let pairs = form_pairs(body);
        let fields = values_from_pairs(&pairs, &route.contract.body.fields);
        let fields_value = map_to_value(&fields);
        return (fields_value.clone(), fields_value);
    }
    if lower_content_type.contains("multipart/form-data")
        && let Some(parts) = multipart_values(body, content_type)
    {
        let fields = values_from_multipart(&parts, &route.contract.body.fields);
        let fields_value = map_to_value(&fields);
        return (fields_value.clone(), fields_value);
    }
    let parsed = std::str::from_utf8(body)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| {
            std::str::from_utf8(body)
                .map(|text| Value::String(text.to_string()))
                .unwrap_or(Value::Null)
        });
    if let Value::Object(map) = &parsed {
        let body_map = values_from_json(map, &route.contract.body.fields);
        (map_to_value(&body_map), Value::Object(Map::new()))
    } else {
        (parsed, Value::Object(Map::new()))
    }
}

fn form_pairs(body: &Bytes) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for (key, value) in url::form_urlencoded::parse(body.as_ref()) {
        out.entry(key.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    out
}

fn multipart_values(
    body: &Bytes,
    content_type: &str,
) -> Option<BTreeMap<String, Vec<MultipartValue>>> {
    let boundary = content_type
        .split(';')
        .filter_map(|part| part.trim().strip_prefix("boundary="))
        .next()?
        .trim_matches('"');
    if boundary.is_empty() {
        return None;
    }
    let marker = format!("--{boundary}").into_bytes();
    let bytes = body.as_ref();
    let mut cursor = find_bytes(bytes, &marker)?;
    let mut out = BTreeMap::<String, Vec<MultipartValue>>::new();
    loop {
        cursor = cursor.saturating_add(marker.len());
        if bytes
            .get(cursor..cursor.saturating_add(2))
            .is_some_and(|tail| tail == b"--")
        {
            break;
        }
        if bytes
            .get(cursor..cursor.saturating_add(2))
            .is_some_and(|tail| tail == b"\r\n")
        {
            cursor = cursor.saturating_add(2);
        }
        let Some(header_len) = find_bytes(&bytes[cursor..], b"\r\n\r\n") else {
            break;
        };
        let header_end = cursor.saturating_add(header_len);
        let value_start = header_end.saturating_add(4);
        let Some(next_marker) = find_bytes(&bytes[value_start..], &marker) else {
            break;
        };
        let mut value_end = value_start.saturating_add(next_marker);
        if value_end >= 2
            && bytes
                .get(value_end - 2..value_end)
                .is_some_and(|tail| tail == b"\r\n")
        {
            value_end -= 2;
        }
        let header_text = String::from_utf8_lossy(&bytes[cursor..header_end]);
        let Some(name) = multipart_field_name(&header_text) else {
            cursor = value_start.saturating_add(next_marker);
            continue;
        };
        let value_bytes = &bytes[value_start..value_end];
        let value = if let Some(filename) = multipart_file_name(&header_text) {
            MultipartValue::File {
                filename,
                content_type: multipart_content_type(&header_text),
                content_base64: base64_encode(value_bytes),
                size: value_bytes.len(),
            }
        } else {
            MultipartValue::Text(String::from_utf8_lossy(value_bytes).into_owned())
        };
        out.entry(name).or_default().push(value);
        cursor = value_start.saturating_add(next_marker);
    }
    Some(out)
}

fn multipart_field_name(headers: &str) -> Option<String> {
    multipart_disposition_value(headers, "name")
}

fn multipart_file_name(headers: &str) -> Option<String> {
    multipart_disposition_value(headers, "filename")
}

fn multipart_content_type(headers: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let line = line.trim();
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-type")
            .then(|| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn multipart_disposition_value(headers: &str, key: &str) -> Option<String> {
    for line in headers.lines() {
        let line = line.trim();
        if !line
            .to_ascii_lowercase()
            .starts_with("content-disposition:")
        {
            continue;
        }
        for part in line.split(';').skip(1) {
            let part = part.trim();
            let Some(value) = part
                .strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
            else {
                continue;
            };
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut idx = 0usize;
    while idx + 3 <= bytes.len() {
        let n =
            ((bytes[idx] as u32) << 16) | ((bytes[idx + 1] as u32) << 8) | bytes[idx + 2] as u32;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        idx += 3;
    }
    let rem = bytes.len().saturating_sub(idx);
    if rem == 1 {
        let n = (bytes[idx] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((bytes[idx] as u32) << 16) | ((bytes[idx + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn values_from_pairs(
    pairs: &BTreeMap<String, Vec<String>>,
    fields: &[ApiMockContractField],
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for field in fields.iter().filter(|field| field.enabled) {
        let value = pairs
            .get(&field.name)
            .map(|values| typed_value_from_strings(field, values))
            .or_else(|| super::contract::api_mock_default_json(field))
            .unwrap_or(Value::Null);
        insert_contract_value(&mut out, field, value);
    }
    out
}

fn values_from_multipart(
    parts: &BTreeMap<String, Vec<MultipartValue>>,
    fields: &[ApiMockContractField],
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for field in fields.iter().filter(|field| field.enabled) {
        let value = parts
            .get(&field.name)
            .map(|values| typed_value_from_multipart(field, values))
            .or_else(|| super::contract::api_mock_default_json(field))
            .unwrap_or(Value::Null);
        insert_contract_value(&mut out, field, value);
    }
    out
}

fn values_from_json(
    map: &Map<String, Value>,
    fields: &[ApiMockContractField],
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for field in fields.iter().filter(|field| field.enabled) {
        let value = map
            .get(&field.name)
            .cloned()
            .or_else(|| map.get(&field.python_name).cloned())
            .or_else(|| super::contract::api_mock_default_json(field))
            .unwrap_or(Value::Null);
        insert_contract_value(&mut out, field, value);
    }
    out
}

fn insert_contract_value(
    out: &mut BTreeMap<String, Value>,
    field: &ApiMockContractField,
    value: Value,
) {
    out.insert(field.python_name.clone(), value.clone());
    if field.name != field.python_name {
        out.insert(field.name.clone(), value);
    }
}

fn typed_value_from_strings(field: &ApiMockContractField, values: &[String]) -> Value {
    if matches!(field.kind, ApiMockContractFieldKind::Array) {
        return Value::Array(
            values
                .iter()
                .map(|value| {
                    typed_scalar_value(
                        field.item_kind.unwrap_or(ApiMockContractFieldKind::String),
                        value,
                    )
                })
                .collect(),
        );
    }
    values
        .last()
        .map(|value| typed_scalar_value(field.kind, value))
        .unwrap_or(Value::Null)
}

fn typed_value_from_multipart(field: &ApiMockContractField, values: &[MultipartValue]) -> Value {
    if matches!(field.kind, ApiMockContractFieldKind::File) {
        return values
            .iter()
            .rev()
            .find_map(multipart_file_value)
            .unwrap_or(Value::Null);
    }
    if matches!(field.kind, ApiMockContractFieldKind::Array)
        && matches!(field.item_kind, Some(ApiMockContractFieldKind::File))
    {
        return Value::Array(values.iter().filter_map(multipart_file_value).collect());
    }
    let strings: Vec<_> = values.iter().map(multipart_value_text).collect();
    typed_value_from_strings(field, &strings)
}

fn multipart_file_value(value: &MultipartValue) -> Option<Value> {
    let MultipartValue::File {
        filename,
        content_type,
        content_base64,
        size,
    } = value
    else {
        return None;
    };
    Some(json!({
        "__rriter_type": "file",
        "filename": filename,
        "content_type": content_type,
        "content_base64": content_base64,
        "size": size,
    }))
}

fn multipart_value_text(value: &MultipartValue) -> String {
    match value {
        MultipartValue::Text(text) => text.clone(),
        MultipartValue::File { filename, .. } => filename.clone(),
    }
}

fn typed_scalar_value(kind: ApiMockContractFieldKind, value: &str) -> Value {
    match kind {
        ApiMockContractFieldKind::Integer => value
            .parse::<i64>()
            .map(|value| json!(value))
            .unwrap_or_else(|_| Value::String(value.to_string())),
        ApiMockContractFieldKind::Number => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_string())),
        ApiMockContractFieldKind::Boolean => match value.to_ascii_lowercase().as_str() {
            "true" | "1" => Value::Bool(true),
            "false" | "0" => Value::Bool(false),
            _ => Value::String(value.to_string()),
        },
        ApiMockContractFieldKind::Object
        | ApiMockContractFieldKind::Array
        | ApiMockContractFieldKind::Any => {
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
        }
        ApiMockContractFieldKind::String
        | ApiMockContractFieldKind::Bytes
        | ApiMockContractFieldKind::File => Value::String(value.to_string()),
    }
}

fn map_to_value(map: &BTreeMap<String, Value>) -> Value {
    Value::Object(
        map.iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

async fn proxy_request(
    proxy_client: reqwest::Client,
    proxy_base_url: String,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(url) = proxy_url(&proxy_base_url, &uri) else {
        return response_text(
            StatusCode::BAD_GATEWAY,
            "text/plain",
            "proxy base url is empty",
        );
    };
    let Ok(req_method) = reqwest::Method::from_bytes(method.as_str().as_bytes()) else {
        return response_text(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain",
            "invalid method",
        );
    };
    let mut request = proxy_client.request(req_method, url).body(body);
    for (name, value) in headers.iter() {
        if safe_proxy_header(name)
            && let Ok(value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes())
        {
            request = request.header(name.as_str(), value);
        }
    }
    match request.send().await {
        Ok(upstream) => {
            let status =
                StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut builder = Response::builder().status(status);
            for (name, value) in upstream.headers().iter() {
                if let Ok(name) = HeaderName::from_bytes(name.as_str().as_bytes())
                    && safe_proxy_header(&name)
                    && let Ok(value) = HeaderValue::from_bytes(value.as_bytes())
                {
                    builder = builder.header(name, value);
                }
            }
            let bytes = match upstream.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => return proxy_body_read_error_response(error),
            };
            builder.body(Body::from(bytes)).unwrap_or_else(|_| {
                response_text(StatusCode::BAD_GATEWAY, "text/plain", "bad proxy response")
            })
        }
        Err(err) => response_text(StatusCode::BAD_GATEWAY, "text/plain", err.to_string()),
    }
}

fn proxy_body_read_error_response(error: impl std::fmt::Display) -> Response {
    response_text(
        StatusCode::BAD_GATEWAY,
        "text/plain",
        format!("failed to read proxy response body: {error}"),
    )
}

fn response_text(
    status: StatusCode,
    content_type: &'static str,
    body: impl Into<Body>,
) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(body.into())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn api_method_from_http(method: &Method) -> Option<ApiMethod> {
    match method.as_str() {
        "GET" => Some(ApiMethod::Get),
        "POST" => Some(ApiMethod::Post),
        "PUT" => Some(ApiMethod::Put),
        "PATCH" => Some(ApiMethod::Patch),
        "DELETE" => Some(ApiMethod::Delete),
        "HEAD" => Some(ApiMethod::Head),
        "OPTIONS" => Some(ApiMethod::Options),
        "TRACE" => Some(ApiMethod::Trace),
        _ => None,
    }
}

fn format_api_mock_bind_error(addr: SocketAddr, error: &std::io::Error) -> String {
    format_api_mock_bind_error_for_platform(addr, error, crate::platform::CURRENT_PLATFORM)
}

fn format_api_mock_bind_error_for_platform(
    addr: SocketAddr,
    error: &std::io::Error,
    platform: crate::platform::PlatformKind,
) -> String {
    match error.kind() {
        std::io::ErrorKind::AddrInUse => format!(
            "Cannot start API Mock on {addr}: the port is already in use. Choose another port or stop the process using it."
        ),
        std::io::ErrorKind::PermissionDenied => {
            if platform == crate::platform::PlatformKind::Windows && !addr.ip().is_loopback() {
                format!(
                    "Cannot start API Mock on {addr}: access was denied. Allow RRiter through Windows Firewall or bind to 127.0.0.1 for local-only access. ({error})"
                )
            } else {
                format!("Cannot start API Mock on {addr}: access was denied. ({error})")
            }
        }
        std::io::ErrorKind::AddrNotAvailable => format!(
            "Cannot start API Mock on {addr}: this address is not available on the current machine. ({error})"
        ),
        _ => format!("Cannot start API Mock on {addr}: {error}"),
    }
}

fn socket_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let ip = host
        .parse::<IpAddr>()
        .map_err(|_| format!("Invalid bind host: {}", host))?;
    Ok(SocketAddr::new(ip, port))
}

fn proxy_url(base: &str, uri: &Uri) -> Option<String> {
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    Some(format!("{}{}", base, path))
}

fn should_proxy_unmatched(snapshot: &ApiMockServerSnapshot) -> bool {
    !snapshot.proxy_base_url.trim().is_empty()
}

fn safe_proxy_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

fn push_event(event: ApiMockServerEvent) {
    crate::platform::lock_recover(&EVENTS).push(event);
}

fn push_request_event(method: &str, path: &str, status: u16, action: &str) {
    push_event(ApiMockServerEvent::Request {
        method: method.to_string(),
        path: path.to_string(),
        status,
        action: action.to_string(),
    });
}

fn push_log_event(text: &str) {
    push_event(ApiMockServerEvent::Log {
        text: text.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    #[test]
    fn proxy_url_preserves_path_query() {
        let uri: Uri = "/v1/users?page=1".parse().expect("uri");
        assert_eq!(
            proxy_url("https://backend.test/api/", &uri),
            Some("https://backend.test/api/v1/users?page=1".to_string())
        );
    }

    #[test]
    fn unmatched_paths_proxy_with_base_url() {
        let mut snapshot = ApiMockServerSnapshot {
            bind_host: "127.0.0.1".to_string(),
            port: 4010,
            mode: crate::app::api_mock::types::ApiMockMode::MockAll,
            proxy_base_url: "https://backend.test".to_string(),
            python_runtime: Default::default(),
            routes: Vec::new(),
        };

        assert!(should_proxy_unmatched(&snapshot));
        snapshot.mode = crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest;
        assert!(should_proxy_unmatched(&snapshot));
        snapshot.mode = crate::app::api_mock::types::ApiMockMode::MockSelectedOnly;
        assert!(should_proxy_unmatched(&snapshot));
        snapshot.mode = crate::app::api_mock::types::ApiMockMode::MockAll;
        snapshot.proxy_base_url.clear();
        assert!(!should_proxy_unmatched(&snapshot));
    }

    #[test]
    fn hop_by_hop_headers_are_not_proxied() {
        assert!(!safe_proxy_header(&HeaderName::from_static("connection")));
        assert!(!safe_proxy_header(&HeaderName::from_static("host")));
        assert!(safe_proxy_header(&HeaderName::from_static("authorization")));
    }

    #[test]
    fn bind_errors_explain_port_and_address_failures() {
        let addr: SocketAddr = "127.0.0.1:4010".parse().unwrap();
        let in_use = format_api_mock_bind_error(
            addr,
            &std::io::Error::new(std::io::ErrorKind::AddrInUse, "busy"),
        );
        assert!(in_use.contains("already in use"));
        assert!(in_use.contains("4010"));

        let unavailable = format_api_mock_bind_error(
            addr,
            &std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "missing"),
        );
        assert!(unavailable.contains("not available"));
        assert!(unavailable.contains("missing"));

        let public_addr: SocketAddr = "0.0.0.0:4010".parse().unwrap();
        let denied = format_api_mock_bind_error_for_platform(
            public_addr,
            &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            crate::platform::PlatformKind::Windows,
        );
        assert!(denied.contains("Windows Firewall"));
        assert!(denied.contains("127.0.0.1"));
    }

    #[test]
    fn contract_query_values_keep_typed_last_value_and_python_name() {
        let mut pairs = BTreeMap::new();
        pairs.insert("page".to_string(), vec!["1".to_string(), "2".to_string()]);
        let mut field = crate::app::api_mock::types::ApiMockContractField::new(
            "page",
            crate::app::api_mock::types::ApiMockContractFieldKind::Integer,
            false,
        );
        field.default_value = Some("1".to_string());

        let values = values_from_pairs(&pairs, &[field]);

        assert_eq!(values.get("page"), Some(&serde_json::json!(2)));
    }

    #[test]
    fn multipart_file_values_keep_metadata_and_content() {
        let body = Bytes::from_static(
            b"--rr\r\nContent-Disposition: form-data; name=\"image\"; filename=\"avatar.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n\x00\xff\x10\r\n--rr\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\npic\r\n--rr--\r\n",
        );
        let parts = multipart_values(&body, "multipart/form-data; boundary=rr").expect("parts");
        let image = crate::app::api_mock::types::ApiMockContractField::new(
            "image",
            crate::app::api_mock::types::ApiMockContractFieldKind::File,
            true,
        );
        let title = crate::app::api_mock::types::ApiMockContractField::new(
            "title",
            crate::app::api_mock::types::ApiMockContractFieldKind::String,
            false,
        );

        let values = values_from_multipart(&parts, &[image, title]);
        let image = values.get("image").expect("image");

        assert_eq!(
            image.get("filename"),
            Some(&serde_json::json!("avatar.bin"))
        );
        assert_eq!(
            image.get("content_type"),
            Some(&serde_json::json!("application/octet-stream"))
        );
        assert_eq!(
            image.get("content_base64"),
            Some(&serde_json::json!("AP8Q"))
        );
        assert_eq!(image.get("size"), Some(&serde_json::json!(3)));
        assert_eq!(values.get("title"), Some(&serde_json::json!("pic")));
    }

    #[test]
    fn r3_108_proxy_body_read_error_returns_bad_gateway_not_upstream_success() {
        let response = proxy_body_read_error_response("stream failed");
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

}
