use crate::app::api_client::{ApiMethod, ApiSpecEntry, ApiSpecSource};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockState {
    pub enabled: bool,
    pub bind_host: String,
    pub port: u16,
    pub mode: ApiMockMode,
    pub proxy_base_url: String,
    pub server_status: ApiMockServerStatus,
    #[serde(skip)]
    pub check_status: ApiMockCheckStatus,
    pub uv: ApiUvState,
    pub route_overrides: Vec<ApiMockRouteOverride>,
    pub manual_routes: Vec<ApiManualRoute>,
}

impl Default for ApiMockState {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_host: "127.0.0.1".to_string(),
            port: 4010,
            mode: ApiMockMode::MockAll,
            proxy_base_url: String::new(),
            server_status: ApiMockServerStatus::Stopped,
            check_status: ApiMockCheckStatus::Idle,
            uv: ApiUvState::default(),
            route_overrides: Vec::new(),
            manual_routes: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiMockMode {
    #[default]
    MockAll,
    MockSelectedOnly,
    MockSelectedProxyRest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiMockServerStatus {
    Stopped,
    Starting,
    Running { url: String },
    Stopping,
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiMockCheckStatus {
    #[default]
    Idle,
    Pending {
        route_idx: usize,
    },
    Ok {
        route_idx: usize,
        message: String,
    },
    Failed {
        route_idx: usize,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockRouteOverride {
    pub source_key: String,
    pub method: ApiMethod,
    pub path: String,
    pub enabled: bool,
    pub response: ApiMockResponse,
    pub python: Option<ApiMockPythonScript>,
    pub extra_input_fields: Vec<ApiMockField>,
    pub extra_output_fields: Vec<ApiMockField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiManualRoute {
    pub stable_id: String,
    pub method: ApiMethod,
    pub path: String,
    pub enabled: bool,
    pub response: ApiMockResponse,
    pub python: Option<ApiMockPythonScript>,
    pub input_fields: Vec<ApiMockField>,
    pub output_fields: Vec<ApiMockField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockPythonScript {
    pub prelude: String,
    pub body: String,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockField {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub locked: bool,
    pub default_value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiMockResponse {
    Generated,
    Json(String),
    Text(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiUvState {
    pub configured_path: Option<PathBuf>,
    pub detected_path: Option<PathBuf>,
    pub status: ApiUvStatus,
    pub last_error: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiUvStatus {
    #[default]
    Unknown,
    Missing,
    Ready,
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockRuntimeRoute {
    pub id: String,
    pub source_key: String,
    pub method: ApiMethod,
    pub path: String,
    pub enabled: bool,
    pub response: ApiMockResponse,
    pub python: Option<ApiMockPythonScript>,
    pub input_fields: Vec<ApiMockField>,
    pub output_fields: Vec<ApiMockField>,
    pub origin: ApiMockRouteOrigin,
}

impl ApiMockRuntimeRoute {
    pub fn static_response_text(&self) -> (u16, &'static str, String) {
        match &self.response {
            ApiMockResponse::Generated => (
                200,
                "application/json",
                format!(
                    "{{\"mock\":true,\"source\":\"RRiter generated mock\",\"method\":\"{}\",\"path\":\"{}\"}}",
                    self.method.as_str(),
                    self.path
                ),
            ),
            ApiMockResponse::Json(text) => (200, "application/json", text.clone()),
            ApiMockResponse::Text(text) => (200, "text/plain; charset=utf-8", text.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockServerSnapshot {
    pub bind_host: String,
    pub port: u16,
    pub mode: ApiMockMode,
    pub proxy_base_url: String,
    pub uv_path: Option<PathBuf>,
    pub routes: Vec<ApiMockRuntimeRoute>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiMockServerEvent {
    Running { url: String },
    Stopped,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiMockRouteOrigin {
    Manual,
    OpenApi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiMockRouteDecision<'a> {
    Mock(&'a ApiMockRuntimeRoute),
    Proxy,
    NotFound,
}

pub fn api_mock_source_key(entry: &ApiSpecEntry) -> String {
    match &entry.source {
        ApiSpecSource::Url(raw) => Url::parse(raw)
            .map(|mut url| {
                url.set_fragment(None);
                url.to_string()
            })
            .unwrap_or_else(|_| format!("url:{}#{}", raw, entry.title)),
        ApiSpecSource::Local(path) => path
            .canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| format!("file:{}#{}", path.to_string_lossy(), entry.title)),
    }
}

pub fn api_mock_route_key(source_key: &str, method: ApiMethod, path: &str) -> String {
    format!("{} {} {}", source_key, method.as_str(), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::{ApiSpecId, ApiUrlStatus};

    #[test]
    fn url_source_key_drops_fragment_for_refresh_stability() {
        let entry = ApiSpecEntry {
            id: ApiSpecId(1),
            title: "Demo".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.test/openapi.json#v1".to_string()),
            last_loaded: None,
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: Some(ApiUrlStatus::Ok(200)),
            selected: true,
            error: None,
        };

        assert_eq!(
            api_mock_source_key(&entry),
            "https://example.test/openapi.json"
        );
        assert_eq!(
            api_mock_route_key(&api_mock_source_key(&entry), ApiMethod::Get, "/users"),
            "https://example.test/openapi.json GET /users"
        );
    }
}
