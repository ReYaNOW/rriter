use super::merge::{api_mock_path_params, resolve_api_mock_route};
use super::python_worker::{PythonMockRequest, call_python_route};
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
use tokio::sync::oneshot;

static SERVER: LazyLock<Mutex<Option<ApiMockServerHandle>>> = LazyLock::new(|| Mutex::new(None));
static EVENTS: LazyLock<Mutex<Vec<ApiMockServerEvent>>> = LazyLock::new(|| Mutex::new(Vec::new()));

struct ApiMockServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Clone)]
struct ApiMockAxumState {
    snapshot: Arc<ApiMockServerSnapshot>,
    proxy_client: reqwest::Client,
}

pub fn drain_api_mock_server_events() -> Vec<ApiMockServerEvent> {
    EVENTS
        .lock()
        .map(|mut events| events.drain(..).collect())
        .unwrap_or_default()
}

pub fn start_api_mock_server(snapshot: ApiMockServerSnapshot) -> Result<(), String> {
    let mut server = SERVER
        .lock()
        .map_err(|_| "Mock server lock failed".to_string())?;
    if server.is_some() {
        return Ok(());
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("rriter-api-mock".to_string())
        .spawn(move || run_server_thread(snapshot, shutdown_rx))
        .map_err(|err| err.to_string())?;
    *server = Some(ApiMockServerHandle {
        shutdown: Some(shutdown_tx),
    });
    Ok(())
}

pub fn stop_api_mock_server() {
    if let Ok(mut server) = SERVER.lock()
        && let Some(mut handle) = server.take()
        && let Some(shutdown) = handle.shutdown.take()
    {
        let _ = shutdown.send(());
    }
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

fn run_server_thread(snapshot: ApiMockServerSnapshot, shutdown_rx: oneshot::Receiver<()>) {
    push_log_event("tokio runtime: creating multi-thread runtime");
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            push_event(ApiMockServerEvent::Failed(err.to_string()));
            clear_server_handle();
            return;
        }
    };

    runtime.block_on(async move {
        push_log_event("bind address: resolving");
        let addr = match socket_addr(&snapshot.bind_host, snapshot.port) {
            Ok(addr) => addr,
            Err(err) => {
                push_event(ApiMockServerEvent::Failed(err));
                clear_server_handle();
                return;
            }
        };
        push_log_event(&format!("tcp bind: {addr}"));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(err) => {
                push_event(ApiMockServerEvent::Failed(err.to_string()));
                clear_server_handle();
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
        let state = ApiMockAxumState {
            snapshot: Arc::new(snapshot),
            proxy_client: reqwest::Client::new(),
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
        clear_server_handle();
    });
}

async fn handle_mock_request(
    State(state): State<ApiMockAxumState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
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
    match resolve_api_mock_route(
        &state.snapshot.routes,
        state.snapshot.mode,
        api_method,
        path,
    ) {
        ApiMockRouteDecision::Mock(route) => {
            if let Some(script) = route.python.as_ref().filter(|script| script.enabled) {
                let request = python_request(&method, &uri, &headers, &body, route);
                let response =
                    match call_python_route(&state.snapshot.python_runtime, script, request) {
                        Ok(output) => {
                            let status =
                                StatusCode::from_u16(output.status).unwrap_or(StatusCode::OK);
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
                        Err(err) => {
                            response_text(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", err)
                        }
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
            let response = proxy_request(state, method, uri, headers, body).await;
            push_request_event(
                &method_label,
                &path_label,
                response.status().as_u16(),
                "proxy",
            );
            response
        }
        ApiMockRouteDecision::NotFound => {
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
        && let Some(pairs) = multipart_pairs(body, content_type)
    {
        let fields = values_from_pairs(&pairs, &route.contract.body.fields);
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

fn multipart_pairs(body: &Bytes, content_type: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let boundary = content_type
        .split(';')
        .filter_map(|part| part.trim().strip_prefix("boundary="))
        .next()?
        .trim_matches('"');
    if boundary.is_empty() {
        return None;
    }
    let marker = format!("--{boundary}");
    let text = std::str::from_utf8(body).ok()?;
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for raw_part in text.split(&marker).skip(1) {
        let part = raw_part.trim_start_matches("\r\n");
        if part.starts_with("--") {
            break;
        }
        let Some((header_text, value_text)) = part.split_once("\r\n\r\n") else {
            continue;
        };
        let Some(name) = multipart_field_name(header_text) else {
            continue;
        };
        let value = multipart_file_name(header_text).unwrap_or_else(|| {
            value_text
                .trim_end_matches("\r\n")
                .trim_end_matches("--")
                .to_string()
        });
        out.entry(name).or_default().push(value);
    }
    Some(out)
}

fn multipart_field_name(headers: &str) -> Option<String> {
    multipart_disposition_value(headers, "name")
}

fn multipart_file_name(headers: &str) -> Option<String> {
    multipart_disposition_value(headers, "filename")
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
        ApiMockContractFieldKind::String | ApiMockContractFieldKind::Bytes => {
            Value::String(value.to_string())
        }
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
    state: ApiMockAxumState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(url) = proxy_url(&state.snapshot.proxy_base_url, &uri) else {
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
    let mut request = state.proxy_client.request(req_method, url).body(body);
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
            let bytes = upstream.bytes().await.unwrap_or_default();
            builder.body(Body::from(bytes)).unwrap_or_else(|_| {
                response_text(StatusCode::BAD_GATEWAY, "text/plain", "bad proxy response")
            })
        }
        Err(err) => response_text(StatusCode::BAD_GATEWAY, "text/plain", err.to_string()),
    }
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
    if let Ok(mut events) = EVENTS.lock() {
        events.push(event);
    }
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

fn clear_server_handle() {
    if let Ok(mut server) = SERVER.lock() {
        *server = None;
    }
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
    fn hop_by_hop_headers_are_not_proxied() {
        assert!(!safe_proxy_header(&HeaderName::from_static("connection")));
        assert!(!safe_proxy_header(&HeaderName::from_static("host")));
        assert!(safe_proxy_header(&HeaderName::from_static("authorization")));
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
}
