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
    pub uv: ApiPythonRuntimeState,
    pub route_overrides: Vec<ApiMockRouteOverride>,
    pub manual_routes: Vec<ApiManualRoute>,
}

impl Default for ApiMockState {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_host: "0.0.0.0".to_string(),
            port: 4010,
            mode: ApiMockMode::MockAll,
            proxy_base_url: String::new(),
            server_status: ApiMockServerStatus::Stopped,
            check_status: ApiMockCheckStatus::Idle,
            uv: ApiPythonRuntimeState::default(),
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
        version: u64,
    },
    Ok {
        route_idx: usize,
        version: u64,
        message: String,
    },
    Failed {
        route_idx: usize,
        version: u64,
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
    #[serde(default = "default_mock_python_enabled")]
    pub enabled: bool,
    pub prelude: String,
    pub body: String,
    pub timeout_ms: u64,
}

pub fn default_api_mock_python_body() -> String {
    "    \n    \n    return json_response({\"ok\": True})".to_string()
}

pub fn default_api_mock_python_script() -> ApiMockPythonScript {
    ApiMockPythonScript {
        enabled: true,
        prelude: String::new(),
        body: default_api_mock_python_body(),
        timeout_ms: 1000,
    }
}

pub fn api_mock_path_param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
                .map(str::to_string)
        })
        .collect()
}

pub fn api_mock_sanitize_python_param(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        out.insert(0, '_');
    }
    if matches!(out.as_str(), "req" | "query" | "body" | "fields") {
        out.push_str("_param");
    }
    out
}

fn default_mock_python_enabled() -> bool {
    true
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiPythonRuntimeState {
    #[serde(default)]
    pub mode: ApiPythonRuntimeMode,
    pub configured_path: Option<PathBuf>,
    pub detected_path: Option<PathBuf>,
    #[serde(default)]
    pub custom_python_path: Option<PathBuf>,
    #[serde(default = "default_mock_python_version")]
    pub python_version: String,
    #[serde(default)]
    pub status: ApiPythonRuntimeStatus,
    #[serde(default)]
    pub last_error: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiPythonRuntimeMode {
    #[default]
    UvManaged,
    CustomPython,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiPythonRuntimeStatus {
    #[default]
    Unknown,
    Missing,
    Ready,
    Invalid,
}

impl Default for ApiPythonRuntimeState {
    fn default() -> Self {
        Self {
            mode: ApiPythonRuntimeMode::UvManaged,
            configured_path: None,
            detected_path: None,
            custom_python_path: None,
            python_version: default_mock_python_version(),
            status: ApiPythonRuntimeStatus::Unknown,
            last_error: String::new(),
        }
    }
}

impl ApiPythonRuntimeState {
    pub fn selected_uv_path(&self) -> Option<PathBuf> {
        self.configured_path
            .clone()
            .or_else(|| self.detected_path.clone())
    }

    pub fn runtime_config(&self) -> ApiPythonRuntimeConfig {
        ApiPythonRuntimeConfig {
            mode: self.mode,
            uv_path: self.selected_uv_path(),
            custom_python_path: self.custom_python_path.clone(),
            python_version: self.python_version.trim().to_string(),
        }
    }
}

pub type ApiUvState = ApiPythonRuntimeState;
pub type ApiUvStatus = ApiPythonRuntimeStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiPythonRuntimeConfig {
    pub mode: ApiPythonRuntimeMode,
    pub uv_path: Option<PathBuf>,
    pub custom_python_path: Option<PathBuf>,
    pub python_version: String,
}

impl Default for ApiPythonRuntimeConfig {
    fn default() -> Self {
        Self {
            mode: ApiPythonRuntimeMode::UvManaged,
            uv_path: None,
            custom_python_path: None,
            python_version: default_mock_python_version(),
        }
    }
}

fn default_mock_python_version() -> String {
    "3.13".to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockRuntimeRoute {
    pub id: String,
    pub source_key: String,
    pub method: ApiMethod,
    pub path: String,
    pub enabled: bool,
    pub response: ApiMockResponse,
    pub generated_status: u16,
    pub generated_content_type: &'static str,
    pub generated_body: String,
    pub python: Option<ApiMockPythonScript>,
    pub input_fields: Vec<ApiMockField>,
    pub output_fields: Vec<ApiMockField>,
    pub origin: ApiMockRouteOrigin,
}

impl ApiMockRuntimeRoute {
    pub fn static_response_text(&self) -> (u16, &'static str, String) {
        match &self.response {
            ApiMockResponse::Generated => (
                self.generated_status,
                self.generated_content_type,
                self.generated_body.clone(),
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
    pub python_runtime: ApiPythonRuntimeConfig,
    pub routes: Vec<ApiMockRuntimeRoute>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiMockServerEvent {
    Log {
        text: String,
    },
    Running {
        url: String,
    },
    Stopped,
    Failed(String),
    Request {
        method: String,
        path: String,
        status: u16,
        action: String,
    },
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
