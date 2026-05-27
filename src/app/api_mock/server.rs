use super::merge::{api_mock_path_params, resolve_api_mock_route};
use super::python_worker::{PythonMockRequest, call_python_route};
use super::types::{
    ApiMockRouteDecision, ApiMockServerEvent, ApiMockServerSnapshot, ApiMockServerStatus,
};
use crate::app::api_client::ApiMethod;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use axum::routing::any;
use serde_json::Value;
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
                let request = python_request(&method, &uri, &headers, &body, &route.path);
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
    route_pattern: &str,
) -> PythonMockRequest {
    let mut header_map = BTreeMap::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            header_map.insert(name.as_str().to_string(), value.to_string());
        }
    }
    let mut query = BTreeMap::new();
    if let Some(raw_query) = uri.query() {
        for pair in raw_query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            query.insert(key.to_string(), value.to_string());
        }
    }
    let body = std::str::from_utf8(body)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or(Value::Null);
    PythonMockRequest {
        method: method.as_str().to_string(),
        path: uri.path().to_string(),
        headers: header_map,
        params: api_mock_path_params(route_pattern, uri.path()).unwrap_or_default(),
        query,
        body,
    }
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
}
