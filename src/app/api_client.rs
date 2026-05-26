use crate::app::api_mock::persist::{load_api_mocks, save_api_mocks};
use crate::app::api_mock::server::{
    apply_api_mock_server_event, drain_api_mock_server_events, start_api_mock_server,
    stop_api_mock_server,
};
use crate::app::api_mock::types::ApiMockServerEvent;
use crate::app::api_mock::ty_check::{
    build_api_mock_virtual_source, spawn_api_mock_ty_check, ApiMockSourcePart,
    ApiMockTyDiagnostic,
};
use crate::app::api_mock::types::{
    ApiMockState, default_api_mock_python_body, default_api_mock_python_script,
};
use crate::app::api_mock::{merge::build_api_mock_routes, types::ApiMockServerSnapshot};
use crate::editor::Editor;
use crate::highlighter::{ColorSpan, Highlighter};
use crate::scroll::ScrollState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::{Host, Url};

pub const API_FETCH_TIMEOUT: Duration = Duration::from_secs(12);
pub const API_MAX_SPEC_BYTES: usize = 8 * 1024 * 1024;
pub const API_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const API_MANUAL_MOCK_SPEC_ID: ApiSpecId = ApiSpecId(0);
const API_MAX_MULTIPART_BODY_BYTES: usize = 64 * 1024 * 1024;
const API_SCHEMA_MAX_DEPTH: usize = 12;
const API_SCHEMA_MAX_COUNT: usize = 768;
const API_SCHEMA_MAX_PROPERTIES: usize = 160;
const API_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const API_REACH_TIMEOUT: Duration = Duration::from_millis(1200);
const API_UNTAGGED_GROUP: &str = "Без тэга";

static API_HTTP_CLIENTS: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<ApiHttpClientKey, reqwest::blocking::Client>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ApiHttpClientKey {
    host: Option<String>,
    ip: Option<IpAddr>,
    port: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApiSpecId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiSpecSource {
    Local(PathBuf),
    Url(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiUrlStatus {
    Ok(u16),
    Failed(ApiLoadErrorKind),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiResolvedHost {
    pub host: String,
    pub ip: IpAddr,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiSpecEntry {
    pub id: ApiSpecId,
    pub title: String,
    pub version: String,
    pub openapi_version: String,
    pub source: ApiSpecSource,
    pub last_loaded: Option<u64>,
    #[serde(default)]
    pub last_fetch_secs: Option<f64>,
    #[serde(default)]
    pub last_parse_secs: Option<f64>,
    pub last_url_status: Option<ApiUrlStatus>,
    pub selected: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiSpecModel {
    pub id: ApiSpecId,
    pub title: String,
    pub version: String,
    pub openapi_version: String,
    pub servers: Vec<ApiServer>,
    pub routes: Vec<ApiRouteRow>,
    pub security_schemes: Vec<ApiSecurityScheme>,
    pub root_security: Vec<ApiSecurityRequirement>,
    pub schema_arena: Vec<ApiSchema>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiServer {
    pub url: String,
    pub description: String,
    pub variables: Vec<ApiServerVariable>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiServerVariable {
    pub name: String,
    pub default_value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRouteRow {
    pub tag: String,
    pub method: ApiMethod,
    pub path: String,
    pub summary: String,
    pub operation_id: String,
    pub security: Option<Vec<ApiSecurityRequirement>>,
    pub path_params: Vec<ApiParam>,
    pub query_params: Vec<ApiParam>,
    pub request_body: Option<ApiRequestBody>,
    pub responses: Vec<ApiResponseSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSecurityRequirement {
    pub schemes: Vec<ApiSecurityRequirementScheme>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSecurityRequirementScheme {
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSecurityScheme {
    pub name: String,
    pub kind: ApiSecuritySchemeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiSecuritySchemeKind {
    ApiKey {
        name: String,
        location: ApiSecurityApiKeyLocation,
    },
    Http {
        scheme: String,
        bearer_format: String,
    },
    OAuth2 {
        flows: Vec<ApiOAuthFlow>,
    },
    OpenIdConnect {
        open_id_connect_url: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiSecurityApiKeyLocation {
    Header,
    Query,
    Cookie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiOAuthFlow {
    Implicit,
    Password,
    ClientCredentials,
    AuthorizationCode,
}

impl ApiSecurityScheme {
    pub(crate) fn token_capable(&self) -> bool {
        match &self.kind {
            ApiSecuritySchemeKind::Http { scheme, .. } => scheme.eq_ignore_ascii_case("bearer"),
            ApiSecuritySchemeKind::OAuth2 { .. } | ApiSecuritySchemeKind::OpenIdConnect { .. } => {
                true
            }
            _ => false,
        }
    }

    pub(crate) fn summary(&self) -> String {
        match &self.kind {
            ApiSecuritySchemeKind::ApiKey { location, .. } => match location {
                ApiSecurityApiKeyLocation::Header => "apiKey header".to_string(),
                ApiSecurityApiKeyLocation::Query => "apiKey query".to_string(),
                ApiSecurityApiKeyLocation::Cookie => "apiKey cookie".to_string(),
            },
            ApiSecuritySchemeKind::Http {
                scheme,
                bearer_format,
            } => {
                if bearer_format.is_empty() {
                    format!("http {scheme}")
                } else {
                    format!("http {scheme} {bearer_format}")
                }
            }
            ApiSecuritySchemeKind::OAuth2 { flows } => {
                let mut out = String::from("oauth2");
                for flow in flows {
                    out.push(' ');
                    out.push_str(match flow {
                        ApiOAuthFlow::Implicit => "implicit",
                        ApiOAuthFlow::Password => "password",
                        ApiOAuthFlow::ClientCredentials => "client",
                        ApiOAuthFlow::AuthorizationCode => "code",
                    });
                }
                out
            }
            ApiSecuritySchemeKind::OpenIdConnect { .. } => "openIdConnect".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ApiMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Trace,
}

impl ApiMethod {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "patch" => Some(Self::Patch),
            "delete" => Some(Self::Delete),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
        }
    }

    pub fn chip_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POS",
            Self::Patch => "PAT",
            Self::Put => "PUT",
            Self::Delete => "DEL",
            Self::Head => "HEA",
            Self::Options => "OPT",
            Self::Trace => "TRA",
        }
    }

    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Get => 0,
            Self::Post => 1,
            Self::Patch => 2,
            Self::Put => 3,
            Self::Delete => 4,
            Self::Head => 5,
            Self::Options => 6,
            Self::Trace => 7,
        }
    }

    pub fn can_send_body(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiParam {
    pub name: String,
    pub location: ApiParamLocation,
    pub required: bool,
    pub primitive_type: ApiPrimitiveType,
    pub item_type: Option<ApiPrimitiveType>,
    pub enum_values: Vec<String>,
    pub default_value: Option<String>,
    pub example: Option<String>,
    pub examples: Vec<String>,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiParamLocation {
    Path,
    Query,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiPrimitiveType {
    String,
    Date,
    DateTime,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Bytes,
    Unknown,
}

impl ApiPrimitiveType {
    fn from_schema(schema: Option<&Value>) -> Self {
        let Some(schema) = schema else {
            return Self::Unknown;
        };
        if schema
            .get("format")
            .and_then(Value::as_str)
            .is_some_and(|fmt| matches!(fmt, "binary" | "byte"))
        {
            return Self::Bytes;
        }
        if schema.get("type").and_then(Value::as_str) == Some("string") {
            return match schema.get("format").and_then(Value::as_str) {
                Some("date") => Self::Date,
                Some("date-time") => Self::DateTime,
                _ => Self::String,
            };
        }
        match schema.get("type").and_then(Value::as_str) {
            Some("integer") => Self::Integer,
            Some("number") => Self::Number,
            Some("boolean") => Self::Boolean,
            Some("array") => Self::Array,
            Some("object") => Self::Object,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequestBody {
    pub required: bool,
    pub content_type: String,
    pub schema: Option<ApiSchemaRef>,
    pub is_multipart: bool,
    pub is_form_urlencoded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiSchemaRef(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSchema {
    pub name: String,
    pub kind: ApiSchemaKind,
    pub properties: Vec<ApiSchemaProperty>,
    pub item: Option<ApiSchemaRef>,
    pub enum_values: Vec<String>,
    pub default_value: Option<String>,
    pub examples: Vec<String>,
    pub max_chars: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiSchemaKind {
    Object,
    Array,
    String,
    Date,
    DateTime,
    Integer,
    Number,
    Boolean,
    Bytes,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSchemaProperty {
    pub name: String,
    pub required: bool,
    pub schema: ApiSchemaRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiBodyFilePickResult {
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub name: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponseSummary {
    pub status: String,
    pub description: String,
    pub content_type: String,
    pub example: Option<String>,
    pub schema: Option<ApiSchemaRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiLoadErrorKind {
    InvalidUrl,
    InvalidHost,
    InvalidDomain,
    Dns,
    NoInternet,
    ConnectRefused,
    Timeout,
    Tls,
    HttpStatus(u16),
    InvalidJson,
    UnsupportedOpenApi,
    TooLarge,
    Io,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiLoadError {
    pub kind: ApiLoadErrorKind,
    pub message: String,
}

impl ApiLoadError {
    fn new(kind: ApiLoadErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApiLoadPayload {
    pub entry: ApiSpecEntry,
    pub model: ApiSpecModel,
    pub raw_json: Option<String>,
    pub resolved_host: Option<ApiResolvedHost>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApiLoadResult {
    pub id: ApiSpecId,
    pub result: Result<ApiLoadPayload, ApiLoadError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiInputValue {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiClientTabMeta {
    pub spec_id: ApiSpecId,
    pub title: String,
    pub route_identity: Option<ApiClientRouteIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiClientRouteIdentity {
    OpenApi {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    Manual {
        stable_id: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApiResponseView {
    #[default]
    Body,
    Headers,
}

#[derive(Clone, Debug)]
pub struct ApiClientTabState {
    pub route_idx: Option<usize>,
    pub auth_view: bool,
    pub server_idx: usize,
    pub path_values: Vec<ApiInputValue>,
    pub query_values: Vec<ApiInputValue>,
    pub body_values: Vec<ApiInputValue>,
    pub body_json: String,
    pub response: Option<ApiJobResponse>,
    pub response_view: ApiResponseView,
    pub pending: bool,
    pub pending_request_id: Option<u64>,
    pub tab_scroll: ScrollState,
    pub body_scroll: ScrollState,
    pub body_scroll_x: ScrollState,
    pub response_scroll: ScrollState,
    pub response_scroll_x: ScrollState,
    pub view_scrolls: Vec<ApiViewScrollMemory>,
    pub route_states: Vec<ApiRouteStateMemory>,
}

#[derive(Clone, Debug)]
pub struct ApiViewScrollMemory {
    pub auth_view: bool,
    pub route_idx: Option<usize>,
    pub current: f32,
    pub target: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRouteStateMemory {
    pub route_idx: usize,
    pub path_values: Vec<ApiInputValue>,
    pub query_values: Vec<ApiInputValue>,
    pub body_values: Vec<ApiInputValue>,
    pub body_json: String,
    pub response: Option<ApiJobResponse>,
    pub response_view: ApiResponseView,
    pub pending: bool,
    pub pending_request_id: Option<u64>,
}

impl Default for ApiClientTabState {
    fn default() -> Self {
        Self {
            route_idx: None,
            auth_view: false,
            server_idx: 0,
            path_values: Vec::new(),
            query_values: Vec::new(),
            body_values: Vec::new(),
            body_json: "{\n  \n}".to_string(),
            response: None,
            response_view: ApiResponseView::Body,
            pending: false,
            pending_request_id: None,
            tab_scroll: ScrollState::new(7.0),
            body_scroll: ScrollState::new(7.0),
            body_scroll_x: ScrollState::new(7.0),
            response_scroll: ScrollState::new(7.0),
            response_scroll_x: ScrollState::new(7.0),
            view_scrolls: Vec::new(),
            route_states: Vec::new(),
        }
    }
}

impl ApiClientTabState {
    pub fn remember_route_state(&mut self) {
        let Some(route_idx) = self.route_idx else {
            return;
        };
        let saved = ApiRouteStateMemory {
            route_idx,
            path_values: self.path_values.clone(),
            query_values: self.query_values.clone(),
            body_values: self.body_values.clone(),
            body_json: self.body_json.clone(),
            response: self.response.clone(),
            response_view: self.response_view,
            pending: self.pending,
            pending_request_id: self.pending_request_id,
        };
        if let Some(slot) = self
            .route_states
            .iter_mut()
            .find(|saved| saved.route_idx == route_idx)
        {
            *slot = saved;
        } else {
            self.route_states.push(saved);
        }
    }

    pub fn restore_route_state(&mut self, route_idx: usize) -> bool {
        let Some(saved) = self
            .route_states
            .iter()
            .find(|saved| saved.route_idx == route_idx)
            .cloned()
        else {
            return false;
        };
        self.route_idx = Some(route_idx);
        self.path_values = saved.path_values;
        self.query_values = saved.query_values;
        self.body_values = saved.body_values;
        self.body_json = saved.body_json;
        self.response = saved.response;
        self.response_view = saved.response_view;
        self.pending = saved.pending;
        self.pending_request_id = saved.pending_request_id;
        self.body_scroll.current = 0.0;
        self.body_scroll.target = 0.0;
        self.body_scroll_x.current = 0.0;
        self.body_scroll_x.target = 0.0;
        self.response_scroll.current = 0.0;
        self.response_scroll.target = 0.0;
        self.response_scroll_x.current = 0.0;
        self.response_scroll_x.target = 0.0;
        true
    }

    pub fn remember_view_scroll(&mut self) {
        let key = (self.auth_view, self.route_idx);
        if let Some(saved) = self
            .view_scrolls
            .iter_mut()
            .find(|saved| (saved.auth_view, saved.route_idx) == key)
        {
            saved.current = self.tab_scroll.current;
            saved.target = self.tab_scroll.target;
        } else {
            self.view_scrolls.push(ApiViewScrollMemory {
                auth_view: self.auth_view,
                route_idx: self.route_idx,
                current: self.tab_scroll.current,
                target: self.tab_scroll.target,
            });
        }
    }

    pub fn restore_view_scroll(&mut self, auth_view: bool, route_idx: Option<usize>) {
        if let Some(saved) = self
            .view_scrolls
            .iter()
            .find(|saved| saved.auth_view == auth_view && saved.route_idx == route_idx)
        {
            self.tab_scroll.current = saved.current;
            self.tab_scroll.target = saved.target;
        } else {
            self.tab_scroll.current = 0.0;
            self.tab_scroll.target = 0.0;
        }
        self.tab_scroll.is_dragging = false;
    }
}

impl PartialEq for ApiClientTabState {
    fn eq(&self, other: &Self) -> bool {
        self.route_idx == other.route_idx
            && self.auth_view == other.auth_view
            && self.server_idx == other.server_idx
            && self.path_values == other.path_values
            && self.query_values == other.query_values
            && self.body_values == other.body_values
            && self.body_json == other.body_json
            && self.response == other.response
            && self.response_view == other.response_view
            && self.pending == other.pending
            && self.pending_request_id == other.pending_request_id
            && self.route_states == other.route_states
    }
}

impl Eq for ApiClientTabState {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiFocus {
    ImportUrl,
    MockProxyBase,
    MockPythonUvPath,
    MockPythonVersion,
    MockPythonCustomPath,
    MockManualPath {
        manual_idx: usize,
    },
    MockPrelude {
        route_idx: usize,
    },
    MockBody {
        route_idx: usize,
    },
    MockSignature {
        route_idx: usize,
    },
    MockStaticResponse {
        route_idx: usize,
    },
    AuthValue {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthRefreshToken {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthUsername {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthPassword {
        spec_id: ApiSpecId,
        scheme: String,
    },
    PathParam {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    QueryParam {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    BodyField {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    Body {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    Response {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiAuthStore {
    #[serde(default)]
    pub entries: Vec<ApiAuthEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiAuthEntry {
    pub spec_id: ApiSpecId,
    pub scheme: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl ApiAuthStore {
    pub(crate) fn entry(&self, spec_id: ApiSpecId, scheme: &str) -> Option<&ApiAuthEntry> {
        self.entries
            .iter()
            .find(|entry| entry.spec_id == spec_id && entry.scheme == scheme)
    }

    fn entry_mut(&mut self, spec_id: ApiSpecId, scheme: &str) -> &mut ApiAuthEntry {
        if let Some(idx) = self
            .entries
            .iter()
            .position(|entry| entry.spec_id == spec_id && entry.scheme == scheme)
        {
            return &mut self.entries[idx];
        }
        self.entries.push(ApiAuthEntry {
            spec_id,
            scheme: scheme.to_string(),
            ..Default::default()
        });
        let idx = self.entries.len().saturating_sub(1);
        &mut self.entries[idx]
    }

    fn remove(&mut self, spec_id: ApiSpecId, scheme: &str) {
        self.entries
            .retain(|entry| !(entry.spec_id == spec_id && entry.scheme == scheme));
    }

    fn retain_spec(&mut self, spec_id: ApiSpecId) {
        self.entries.retain(|entry| entry.spec_id != spec_id);
    }
}

pub struct ApiClientState {
    pub specs: Vec<ApiSpecEntry>,
    pub models: FxHashMap<ApiSpecId, ApiSpecModel>,
    pub auth: ApiAuthStore,
    pub mock: ApiMockState,
    pub selected_spec: Option<ApiSpecId>,
    pub next_id: u64,
    pub import_menu_open: bool,
    pub import_url_open: bool,
    pub import_error: Option<String>,
    pub import_error_at: Option<u64>,
    pub loading: FxHashSet<ApiSpecId>,
    pub collapsed_tags: FxHashSet<(ApiSpecId, String)>,
    pub collapsed_route_roots: FxHashSet<ApiSpecId>,
    pub expanded_mock_routes: FxHashSet<(ApiSpecId, usize)>,
    pub panel_scroll: ScrollState,
    pub route_scroll: ScrollState,
    pub next_request_id: u64,
    pub input_editor: Editor,
    pub input_scroll_x: ScrollState,
    pub focused: Option<ApiFocus>,
    pub mock_guide_open: bool,
    pub mock_guide_scroll: ScrollState,
    pub mock_server_detail_open: bool,
    pub mock_server_logs: Vec<ApiMockServerLogLine>,
    pub mock_server_log_scroll: ScrollState,
    pub mock_python_runtime_open: bool,
    pub mock_python_version_picker_open: bool,
    pub mock_python_versions_loading: bool,
    pub mock_python_versions: Vec<ApiPythonVersionRow>,
    pub mock_python_versions_scroll: ScrollState,
    pub mock_python_install_running: bool,
    pub mock_python_install_log: Vec<ApiPythonInstallLogLine>,
    pub mock_python_install_log_scroll: ScrollState,
    pub mock_ty_due: Option<Instant>,
    pub mock_ty_pending: Option<(usize, u64)>,
    pub mock_ty_diagnostics: Vec<ApiMockTyDiagnostic>,
    pub mock_highlighter: Highlighter,
    pub mock_highlight_target: Option<(usize, ApiMockSourcePart, u64)>,
    pub mock_highlight_spans: Vec<ColorSpan>,
    pub mock_highlight_cache: FxHashMap<(usize, ApiMockSourcePart), Vec<ColorSpan>>,
    pub mock_python_scrolls: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    pub mock_python_scrolls_x: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    mock_python_editors: FxHashMap<(usize, ApiMockSourcePart), Editor>,
    pub last_resolved_host: Option<ApiResolvedHost>,
    body_json_validation: Option<ApiJsonValidationState>,
    body_json_validation_pending: Option<(ApiSpecId, usize, u64)>,
    body_json_validation_rx: Option<Receiver<ApiJsonValidationResult>>,
    python_version_list_rx: Option<Receiver<ApiPythonVersionListResult>>,
    python_install_rx: Option<Receiver<ApiPythonInstallEvent>>,
    python_path_pick_rx: Option<Receiver<ApiPythonPathPickResult>>,
}

#[derive(Clone, Debug)]
pub struct ApiMockServerLogLine {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ApiPythonVersionRow {
    pub version: String,
    pub installed: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct ApiPythonInstallLogLine {
    pub text: String,
    pub kind: ApiPythonInstallLogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiPythonInstallLogKind {
    Info,
    Ok,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ApiPythonRuntimeDialogLayout {
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub pad: f32,
    pub content_w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApiPythonPathPickKind {
    Uv,
    CustomPython,
}

struct ApiPythonPathPickResult {
    kind: ApiPythonPathPickKind,
    path: Option<PathBuf>,
}

struct ApiPythonVersionListResult {
    rows: Vec<ApiPythonVersionRow>,
    error: Option<String>,
}

enum ApiPythonInstallEvent {
    Line(ApiPythonInstallLogLine),
    Done(Result<(), String>),
}

#[derive(Clone, Copy)]
struct ApiJsonValidationState {
    spec_id: ApiSpecId,
    route_idx: usize,
    version: u64,
    valid: bool,
}

struct ApiJsonValidationResult {
    spec_id: ApiSpecId,
    route_idx: usize,
    version: u64,
    valid: bool,
}

impl Default for ApiClientState {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            models: FxHashMap::default(),
            auth: ApiAuthStore::default(),
            mock: ApiMockState::default(),
            selected_spec: None,
            next_id: 1,
            import_menu_open: false,
            import_url_open: false,
            import_error: None,
            import_error_at: None,
            loading: FxHashSet::default(),
            collapsed_tags: FxHashSet::default(),
            collapsed_route_roots: FxHashSet::default(),
            expanded_mock_routes: FxHashSet::default(),
            panel_scroll: ScrollState::new(7.0),
            route_scroll: ScrollState::new(7.0),
            next_request_id: 1,
            input_editor: Editor::new(512),
            input_scroll_x: ScrollState::new(7.0),
            focused: None,
            mock_guide_open: false,
            mock_guide_scroll: ScrollState::new(7.0),
            mock_server_detail_open: false,
            mock_server_logs: Vec::new(),
            mock_server_log_scroll: ScrollState::new(7.0),
            mock_python_runtime_open: false,
            mock_python_version_picker_open: false,
            mock_python_versions_loading: false,
            mock_python_versions: Vec::new(),
            mock_python_versions_scroll: ScrollState::new(7.0),
            mock_python_install_running: false,
            mock_python_install_log: Vec::new(),
            mock_python_install_log_scroll: ScrollState::new(7.0),
            mock_ty_due: None,
            mock_ty_pending: None,
            mock_ty_diagnostics: Vec::new(),
            mock_highlighter: Highlighter::new(),
            mock_highlight_target: None,
            mock_highlight_spans: Vec::new(),
            mock_highlight_cache: FxHashMap::default(),
            mock_python_scrolls: FxHashMap::default(),
            mock_python_scrolls_x: FxHashMap::default(),
            mock_python_editors: FxHashMap::default(),
            last_resolved_host: None,
            body_json_validation: None,
            body_json_validation_pending: None,
            body_json_validation_rx: None,
            python_version_list_rx: None,
            python_install_rx: None,
            python_path_pick_rx: None,
        }
    }
}

impl ApiClientState {
    pub fn load_persisted() -> Self {
        let mut state = Self::default();
        state.auth = load_api_auth();
        state.mock = load_api_mocks();
        clear_legacy_api_python_runtime_message(&mut state);
        if let Ok(content) = std::fs::read_to_string(api_specs_path()) {
            if let Ok(saved) = serde_json::from_str::<ApiSpecsPersist>(&content) {
                state.specs = saved.specs;
                state.selected_spec = saved.selected_spec;
                state.next_id = saved.next_id.max(1);
                state.last_resolved_host = saved.last_resolved_host;
                if let Some(resolved) = state.last_resolved_host.clone() {
                    spawn_api_preconnect(resolved);
                }
                for spec in &mut state.specs {
                    spec.selected = Some(spec.id) == state.selected_spec;
                }
                for entry in &state.specs {
                    if let ApiSpecSource::Url(url) = &entry.source
                        && let Some(raw) = read_url_cache(entry.id)
                        && let Ok(payload) = parse_openapi_payload(
                            entry.id,
                            ApiSpecSource::Url(url.clone()),
                            raw,
                            entry.last_url_status.clone(),
                            None,
                        )
                    {
                        state.models.insert(entry.id, payload.model);
                    }
                }
            }
        }
        state
    }

    pub fn persist(&self) {
        let saved = ApiSpecsPersist {
            specs: self.specs.clone(),
            selected_spec: self.selected_spec,
            last_resolved_host: self.last_resolved_host.clone(),
            next_id: self.next_id.max(
                self.specs
                    .iter()
                    .map(|entry| entry.id.0.saturating_add(1))
                    .max()
                    .unwrap_or(1),
            ),
        };
        if let Ok(content) = serde_json::to_string_pretty(&saved) {
            if let Some(dir) = api_specs_path().parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(api_specs_path(), content);
        }
        save_api_auth(&self.auth);
        save_api_mocks(&self.mock);
    }

    pub fn body_json_valid_for(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
        version: u64,
    ) -> Option<bool> {
        self.body_json_validation
            .filter(|state| {
                state.spec_id == spec_id && state.route_idx == route_idx && state.version == version
            })
            .map(|state| state.valid)
    }

    pub fn alloc_spec_id(&mut self) -> ApiSpecId {
        let id = ApiSpecId(self.next_id.max(1));
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }

    pub fn selected_model(&self) -> Option<&ApiSpecModel> {
        let id = self.selected_spec?;
        self.models.get(&id)
    }

    pub fn selected_entry(&self) -> Option<&ApiSpecEntry> {
        let id = self.selected_spec?;
        self.specs.iter().find(|entry| entry.id == id)
    }

    pub fn mock_server_snapshot(&self) -> ApiMockServerSnapshot {
        let specs = self.specs.iter().filter_map(|entry| {
            let model = self.models.get(&entry.id)?;
            Some((entry, model))
        });
        ApiMockServerSnapshot {
            bind_host: self.mock.bind_host.clone(),
            port: self.mock.port,
            mode: self.mock.mode,
            proxy_base_url: self.mock.proxy_base_url.clone(),
            python_runtime: self.mock.uv.runtime_config(),
            routes: build_api_mock_routes(specs, &self.mock),
        }
    }

    pub fn select_spec(&mut self, id: ApiSpecId) {
        self.selected_spec = Some(id);
        for entry in &mut self.specs {
            entry.selected = entry.id == id;
        }
        self.route_scroll.current = 0.0;
        self.route_scroll.target = 0.0;
    }

    pub fn upsert_loaded(&mut self, payload: ApiLoadPayload) {
        let id = payload.entry.id;
        self.loading.remove(&id);
        if let Some(existing) = self.specs.iter_mut().find(|entry| entry.id == id) {
            *existing = payload.entry.clone();
        } else {
            self.specs.push(payload.entry.clone());
        }
        self.models.insert(id, payload.model);
        if payload.resolved_host.is_some() {
            self.last_resolved_host = payload.resolved_host;
        }
        self.select_spec(id);
        if let Some(raw) = payload.raw_json {
            save_url_cache(id, &raw);
        }
        self.persist();
    }

    pub fn mark_load_error(&mut self, id: ApiSpecId, err: ApiLoadError) {
        self.loading.remove(&id);
        if let Some(entry) = self.specs.iter_mut().find(|entry| entry.id == id) {
            entry.error = Some(err.message.clone());
            if matches!(entry.source, ApiSpecSource::Url(_)) {
                entry.last_url_status = Some(ApiUrlStatus::Failed(err.kind.clone()));
            }
        } else {
            self.import_error = Some(err.message.clone());
            self.import_error_at = Some(now_epoch_secs());
        }
        self.persist();
    }

    pub fn remove_spec(&mut self, idx: usize) -> Option<ApiSpecId> {
        if idx >= self.specs.len() {
            return None;
        }
        let id = self.specs[idx].id;
        self.specs.remove(idx);
        self.models.remove(&id);
        self.auth.retain_spec(id);
        self.loading.remove(&id);
        self.collapsed_tags.retain(|(spec_id, _)| *spec_id != id);
        self.collapsed_route_roots.remove(&id);
        self.expanded_mock_routes
            .retain(|(spec_id, _)| *spec_id != id);
        if self.selected_spec == Some(id) {
            self.selected_spec = self.specs.first().map(|entry| entry.id);
            for entry in &mut self.specs {
                entry.selected = Some(entry.id) == self.selected_spec;
            }
        }
        self.persist();
        Some(id)
    }

    fn clear_stale_keyboard_focus(&mut self, active: Option<(ApiSpecId, Option<usize>)>) -> bool {
        let Some(focus) = self.focused.as_ref() else {
            return false;
        };
        if api_focus_targets_active_tab(focus, active) {
            return true;
        }
        self.focused = None;
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ApiSpecsPersist {
    specs: Vec<ApiSpecEntry>,
    selected_spec: Option<ApiSpecId>,
    #[serde(default)]
    last_resolved_host: Option<ApiResolvedHost>,
    next_id: u64,
}

fn api_focus_targets_active_tab(
    focus: &ApiFocus,
    active: Option<(ApiSpecId, Option<usize>)>,
) -> bool {
    match focus {
        ApiFocus::ImportUrl => true,
        ApiFocus::MockProxyBase => true,
        ApiFocus::MockPythonUvPath => true,
        ApiFocus::MockPythonVersion => true,
        ApiFocus::MockPythonCustomPath => true,
        ApiFocus::MockManualPath { .. } => true,
        ApiFocus::MockPrelude { .. }
        | ApiFocus::MockBody { .. }
        | ApiFocus::MockSignature { .. }
        | ApiFocus::MockStaticResponse { .. } => true,
        ApiFocus::AuthValue { spec_id, .. }
        | ApiFocus::AuthRefreshToken { spec_id, .. }
        | ApiFocus::AuthUsername { spec_id, .. }
        | ApiFocus::AuthPassword { spec_id, .. } => {
            active.is_some_and(|(active_spec, _)| active_spec == *spec_id)
        }
        ApiFocus::PathParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::QueryParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::BodyField {
            spec_id, route_idx, ..
        }
        | ApiFocus::Body { spec_id, route_idx }
        | ApiFocus::Response { spec_id, route_idx } => {
            active.is_some_and(|(active_spec, active_route)| {
                active_spec == *spec_id && active_route == Some(*route_idx)
            })
        }
    }
}

fn api_focus_order_for_view(
    spec_id: ApiSpecId,
    model: &ApiSpecModel,
    state: &ApiClientTabState,
) -> Vec<ApiFocus> {
    let mut out = Vec::new();
    if state.auth_view {
        for scheme in &model.security_schemes {
            if matches!(
                scheme.kind,
                ApiSecuritySchemeKind::Http { ref scheme, .. } if scheme.eq_ignore_ascii_case("basic")
            ) {
                out.push(ApiFocus::AuthUsername {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
                out.push(ApiFocus::AuthPassword {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
            } else {
                out.push(ApiFocus::AuthValue {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
            }
        }
        return out;
    }

    let Some(route_idx) = state
        .route_idx
        .or_else(|| (!model.routes.is_empty()).then_some(0))
    else {
        return out;
    };
    let Some(route) = model.routes.get(route_idx) else {
        return out;
    };
    for param in &route.path_params {
        out.push(ApiFocus::PathParam {
            spec_id,
            route_idx,
            name: param.name.clone(),
        });
    }
    for param in &route.query_params {
        out.push(ApiFocus::QueryParam {
            spec_id,
            route_idx,
            name: param.name.clone(),
        });
    }
    if let Some(body) = &route.request_body {
        if body.is_multipart || body.is_form_urlencoded {
            if let Some(schema) = body
                .schema
                .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            {
                for prop in &schema.properties {
                    out.push(ApiFocus::BodyField {
                        spec_id,
                        route_idx,
                        name: prop.name.clone(),
                    });
                }
            }
        } else {
            out.push(ApiFocus::Body { spec_id, route_idx });
        }
    }
    out
}

pub fn validate_api_url(input: &str) -> Result<Url, ApiLoadError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL пустой",
        ));
    }
    let parsed = Url::parse(raw)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidUrl, "URL не распознан"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL должен быть http или https",
        ));
    }
    if parsed.fragment().is_some() {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL должен указывать на openapi.json без #fragment",
        ));
    }
    let Some(host) = parsed.host() else {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidHost,
            "host обязателен",
        ));
    };
    match host {
        Host::Domain(domain) => {
            if !valid_domain(domain) {
                return Err(ApiLoadError::new(
                    ApiLoadErrorKind::InvalidDomain,
                    "домен невалиден",
                ));
            }
        }
        Host::Ipv4(_) | Host::Ipv6(_) => {}
    }
    Ok(parsed)
}

fn valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    let domain = domain.trim_end_matches('.');
    if domain.is_empty() {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

fn api_client_key(resolved: Option<&ApiResolvedHost>) -> ApiHttpClientKey {
    ApiHttpClientKey {
        host: resolved.map(|r| r.host.clone()),
        ip: resolved.map(|r| r.ip),
        port: resolved.map(|r| r.port),
    }
}

fn api_http_client(resolved: Option<&ApiResolvedHost>) -> reqwest::blocking::Client {
    let key = api_client_key(resolved);
    if let Ok(mut clients) = API_HTTP_CLIENTS.lock() {
        if let Some(client) = clients.get(&key) {
            return client.clone();
        }
        let mut builder = reqwest::blocking::Client::builder()
            .timeout(API_FETCH_TIMEOUT)
            .pool_idle_timeout(API_POOL_IDLE_TIMEOUT)
            .use_rustls_tls();
        if let Some(resolved) = resolved {
            builder = builder.resolve(&resolved.host, SocketAddr::new(resolved.ip, resolved.port));
        }
        if let Ok(client) = builder.build() {
            clients.insert(key, client.clone());
            return client;
        }
    }
    reqwest::blocking::Client::new()
}

fn resolve_api_url_host(url: &str) -> Option<ApiResolvedHost> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default()?;
    let ip = match parsed.host()? {
        Host::Ipv4(ip) => IpAddr::V4(ip),
        Host::Ipv6(ip) => IpAddr::V6(ip),
        Host::Domain(_) => (host.as_str(), port).to_socket_addrs().ok()?.next()?.ip(),
    };
    Some(ApiResolvedHost { host, ip, port })
}

fn spawn_api_preconnect(resolved: ApiResolvedHost) {
    std::thread::spawn(move || {
        let client = api_http_client(Some(&resolved));
        let url = format!("https://{}/", resolved.host);
        let _ = client.head(url).send();
    });
}

pub fn spawn_load_local(id: ApiSpecId, path: PathBuf) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = load_local_spec(id, &path);
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

pub fn spawn_load_url(id: ApiSpecId, url: String) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = load_url_spec(id, &url);
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

pub fn spawn_load_cached_url(id: ApiSpecId, url: String) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match read_url_cache(id) {
            Some(raw) => parse_openapi_payload(id, ApiSpecSource::Url(url), raw, None, None),
            None => Err(ApiLoadError::new(ApiLoadErrorKind::Io, "URL cache пустой")),
        };
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

fn load_local_spec(id: ApiSpecId, path: &Path) -> Result<ApiLoadPayload, ApiLoadError> {
    let bytes = std::fs::read(path).map_err(|err| {
        ApiLoadError::new(ApiLoadErrorKind::Io, format!("файл не прочитан: {}", err))
    })?;
    if bytes.len() > API_MAX_SPEC_BYTES {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "openapi.json слишком большой",
        ));
    }
    let raw = String::from_utf8(bytes)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidJson, "JSON не UTF-8"))?;
    parse_openapi_payload(
        id,
        ApiSpecSource::Local(path.to_path_buf()),
        raw,
        None,
        None,
    )
}

fn load_url_spec(id: ApiSpecId, url: &str) -> Result<ApiLoadPayload, ApiLoadError> {
    validate_api_url(url)?;
    let resolved = resolve_api_url_host(url);
    let fetch_started = std::time::Instant::now();
    let raw = fetch_json(url, resolved.as_ref())?;
    let fetch_secs = fetch_started.elapsed().as_secs_f64();
    let mut payload = parse_openapi_payload(
        id,
        ApiSpecSource::Url(url.to_string()),
        raw,
        Some(ApiUrlStatus::Ok(200)),
        Some(fetch_secs),
    )?;
    payload.resolved_host = resolved;
    Ok(payload)
}

fn fetch_json(url: &str, resolved: Option<&ApiResolvedHost>) -> Result<String, ApiLoadError> {
    let client = api_http_client(resolved);
    let mut response = client
        .get(url)
        .header("Accept", "application/json, */*")
        .send()
        .map_err(classify_reqwest_error)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::HttpStatus(status),
            format!("HTTP {}", status),
        ));
    }
    if let Some(content_len) = response.content_length()
        && content_len > API_MAX_SPEC_BYTES as u64
    {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "ответ больше лимита",
        ));
    }
    read_limited_text(&mut response, API_MAX_SPEC_BYTES)
}

fn read_limited_text(
    response: &mut reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<String, ApiLoadError> {
    let mut raw = Vec::new();
    let mut limited = response.take(max_bytes as u64 + 1);
    limited.read_to_end(&mut raw).map_err(classify_io_error)?;
    if raw.len() > max_bytes {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "ответ больше лимита",
        ));
    }
    String::from_utf8(raw)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidJson, "ответ не UTF-8"))
}

fn classify_reqwest_error(err: reqwest::Error) -> ApiLoadError {
    if err.is_timeout() {
        return ApiLoadError::new(ApiLoadErrorKind::Timeout, "таймаут запроса");
    }
    if let Some(status) = err.status() {
        return ApiLoadError::new(
            ApiLoadErrorKind::HttpStatus(status.as_u16()),
            format!("HTTP {}", status.as_u16()),
        );
    }
    if err.is_decode() {
        return ApiLoadError::new(ApiLoadErrorKind::InvalidJson, err.to_string());
    }
    if err.is_connect() {
        let text = err.to_string();
        let kind = if text.contains("dns") || text.contains("Name or service not known") {
            ApiLoadErrorKind::Dns
        } else if text.contains("tls") || text.contains("certificate") {
            ApiLoadErrorKind::Tls
        } else {
            ApiLoadErrorKind::NoInternet
        };
        return ApiLoadError::new(kind, text);
    }
    ApiLoadError::new(ApiLoadErrorKind::Other, err.to_string())
}

fn classify_io_error(err: std::io::Error) -> ApiLoadError {
    let kind = match err.kind() {
        std::io::ErrorKind::ConnectionRefused => ApiLoadErrorKind::ConnectRefused,
        std::io::ErrorKind::TimedOut => ApiLoadErrorKind::Timeout,
        std::io::ErrorKind::NotConnected
        | std::io::ErrorKind::NetworkDown
        | std::io::ErrorKind::NetworkUnreachable => ApiLoadErrorKind::NoInternet,
        _ => ApiLoadErrorKind::Io,
    };
    ApiLoadError::new(kind, err.to_string())
}

fn parse_openapi_payload(
    id: ApiSpecId,
    source: ApiSpecSource,
    raw: String,
    url_status: Option<ApiUrlStatus>,
    fetch_secs: Option<f64>,
) -> Result<ApiLoadPayload, ApiLoadError> {
    let parse_started = std::time::Instant::now();
    let root: Value = serde_json::from_str(&raw).map_err(|err| {
        let message = match source {
            ApiSpecSource::Url(_) => "URL не ведет на валидный openapi.json".to_string(),
            ApiSpecSource::Local(_) => err.to_string(),
        };
        ApiLoadError::new(ApiLoadErrorKind::InvalidJson, message)
    })?;
    let model = parse_openapi_model(id, &root)?;
    let parse_secs = parse_started.elapsed().as_secs_f64();
    let entry = ApiSpecEntry {
        id,
        title: model.title.clone(),
        version: model.version.clone(),
        openapi_version: model.openapi_version.clone(),
        source,
        last_loaded: Some(now_epoch_secs()),
        last_fetch_secs: fetch_secs,
        last_parse_secs: Some(parse_secs),
        last_url_status: url_status,
        selected: true,
        error: None,
    };
    Ok(ApiLoadPayload {
        entry,
        model,
        raw_json: Some(raw),
        resolved_host: None,
    })
}

pub fn parse_openapi_model(id: ApiSpecId, root: &Value) -> Result<ApiSpecModel, ApiLoadError> {
    let Some(openapi_version) = root.get("openapi").and_then(Value::as_str) else {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::UnsupportedOpenApi,
            "нет поля openapi",
        ));
    };
    if !openapi_version.starts_with("3.") {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::UnsupportedOpenApi,
            "поддерживается OpenAPI 3.x",
        ));
    }
    let info = root.get("info").unwrap_or(&Value::Null);
    let title = info
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("OpenAPI")
        .to_string();
    let version = info
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut model = ApiSpecModel {
        id,
        title,
        version,
        openapi_version: openapi_version.to_string(),
        servers: parse_servers(root.get("servers")),
        routes: Vec::new(),
        security_schemes: parse_security_schemes(root),
        root_security: parse_security_requirements(root.get("security")).unwrap_or_default(),
        schema_arena: Vec::new(),
    };
    if model.servers.is_empty() {
        model.servers.push(ApiServer {
            url: "/".to_string(),
            description: String::new(),
            variables: Vec::new(),
        });
    }

    let components = root
        .get("components")
        .and_then(|v| v.get("schemas"))
        .and_then(Value::as_object);

    let tag_order = parse_tag_order(root.get("tags"));
    if let Some(paths) = root.get("paths").and_then(Value::as_object) {
        for (path, path_item) in paths {
            let path_params = parse_parameters(path_item.get("parameters"), root);
            if let Some(path_obj) = path_item.as_object() {
                for (method_key, op) in path_obj {
                    let Some(method) = ApiMethod::from_key(method_key.as_str()) else {
                        continue;
                    };
                    let mut params = path_params.clone();
                    params.extend(parse_parameters(op.get("parameters"), root));
                    let mut path_params = Vec::new();
                    let mut query_params = Vec::new();
                    for param in params {
                        match param.location {
                            ApiParamLocation::Path => path_params.push(param),
                            ApiParamLocation::Query => query_params.push(param),
                        }
                    }
                    path_params.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    path_params.dedup_by(|a, b| a.name == b.name);
                    query_params.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    query_params.dedup_by(|a, b| a.name == b.name);
                    let tag = op
                        .get("tags")
                        .and_then(Value::as_array)
                        .and_then(|tags| tags.first())
                        .and_then(Value::as_str)
                        .filter(|tag| !tag.is_empty())
                        .unwrap_or(API_UNTAGGED_GROUP)
                        .to_string();
                    let summary = op
                        .get("summary")
                        .or_else(|| op.get("description"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let operation_id = op
                        .get("operationId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let request_body = parse_request_body(
                        op.get("requestBody"),
                        components,
                        &mut model.schema_arena,
                    );
                    let responses =
                        parse_responses(op.get("responses"), components, &mut model.schema_arena);
                    model.routes.push(ApiRouteRow {
                        tag,
                        method,
                        path: path.to_string(),
                        summary,
                        operation_id,
                        security: parse_security_requirements(op.get("security")),
                        path_params,
                        query_params,
                        request_body,
                        responses,
                    });
                }
            }
        }
    }
    model.routes.sort_unstable_by(|a, b| {
        api_route_tag_rank(&a.tag, &tag_order)
            .cmp(&api_route_tag_rank(&b.tag, &tag_order))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.method.sort_rank().cmp(&b.method.sort_rank()))
    });
    model
        .routes
        .dedup_by(|a, b| a.tag == b.tag && a.path == b.path && a.method == b.method);
    Ok(model)
}

fn parse_tag_order(value: Option<&Value>) -> FxHashMap<String, usize> {
    let mut out = FxHashMap::default();
    if let Some(tags) = value.and_then(Value::as_array) {
        for tag in tags {
            let Some(name) = tag.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !name.is_empty() && !out.contains_key(name) {
                out.insert(name.to_string(), out.len());
            }
        }
    }
    out
}

fn api_route_tag_rank<'a>(
    tag: &'a str,
    tag_order: &FxHashMap<String, usize>,
) -> (u8, usize, &'a str) {
    if tag == API_UNTAGGED_GROUP {
        return (2, usize::MAX, tag);
    }
    if let Some(rank) = tag_order.get(tag) {
        (0, *rank, tag)
    } else {
        (1, usize::MAX, tag)
    }
}

fn parse_servers(value: Option<&Value>) -> Vec<ApiServer> {
    let mut servers = Vec::new();
    if let Some(items) = value.and_then(Value::as_array) {
        for item in items {
            let Some(url) = item.get("url").and_then(Value::as_str) else {
                continue;
            };
            let mut variables = Vec::new();
            if let Some(vars) = item.get("variables").and_then(Value::as_object) {
                for (name, var) in vars {
                    if let Some(default_value) = var.get("default").and_then(Value::as_str) {
                        variables.push(ApiServerVariable {
                            name: name.to_string(),
                            default_value: default_value.to_string(),
                        });
                    }
                }
            }
            variables.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            servers.push(ApiServer {
                url: url.to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                variables,
            });
        }
    }
    servers
}

fn parse_security_schemes(root: &Value) -> Vec<ApiSecurityScheme> {
    let mut schemes = Vec::new();
    let Some(items) = root
        .get("components")
        .and_then(|v| v.get("securitySchemes"))
        .and_then(Value::as_object)
    else {
        return schemes;
    };
    for (name, value) in items {
        let Some(kind) = parse_security_scheme_kind(value) else {
            continue;
        };
        schemes.push(ApiSecurityScheme {
            name: name.to_string(),
            kind,
        });
    }
    schemes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    schemes
}

fn parse_security_scheme_kind(value: &Value) -> Option<ApiSecuritySchemeKind> {
    match value.get("type").and_then(Value::as_str)? {
        "apiKey" => {
            let name = value.get("name").and_then(Value::as_str)?;
            let location = match value.get("in").and_then(Value::as_str)? {
                "header" => ApiSecurityApiKeyLocation::Header,
                "query" => ApiSecurityApiKeyLocation::Query,
                "cookie" => ApiSecurityApiKeyLocation::Cookie,
                _ => return None,
            };
            Some(ApiSecuritySchemeKind::ApiKey {
                name: name.to_string(),
                location,
            })
        }
        "http" => Some(ApiSecuritySchemeKind::Http {
            scheme: value
                .get("scheme")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase(),
            bearer_format: value
                .get("bearerFormat")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "oauth2" => Some(ApiSecuritySchemeKind::OAuth2 {
            flows: parse_oauth_flows(value.get("flows")),
        }),
        "openIdConnect" => Some(ApiSecuritySchemeKind::OpenIdConnect {
            open_id_connect_url: value
                .get("openIdConnectUrl")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn parse_oauth_flows(value: Option<&Value>) -> Vec<ApiOAuthFlow> {
    let Some(flows) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, flow) in [
        ("implicit", ApiOAuthFlow::Implicit),
        ("password", ApiOAuthFlow::Password),
        ("clientCredentials", ApiOAuthFlow::ClientCredentials),
        ("authorizationCode", ApiOAuthFlow::AuthorizationCode),
    ] {
        if flows.contains_key(key) {
            out.push(flow);
        }
    }
    out
}

fn parse_security_requirements(value: Option<&Value>) -> Option<Vec<ApiSecurityRequirement>> {
    let items = value?.as_array()?;
    let mut requirements = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let mut schemes = Vec::new();
        for (name, scopes) in obj {
            let scopes = scopes
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            schemes.push(ApiSecurityRequirementScheme {
                name: name.to_string(),
                scopes,
            });
        }
        schemes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        requirements.push(ApiSecurityRequirement { schemes });
    }
    Some(requirements)
}

fn parse_parameters(value: Option<&Value>, root: &Value) -> Vec<ApiParam> {
    let mut out = Vec::new();
    if let Some(items) = value.and_then(Value::as_array) {
        for item in items {
            let item = resolve_parameter_ref(item, root).unwrap_or(item);
            let Some(location) = item.get("in").and_then(Value::as_str) else {
                continue;
            };
            let location = match location {
                "path" => ApiParamLocation::Path,
                "query" => ApiParamLocation::Query,
                _ => continue,
            };
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let schema = item.get("schema");
            let item_schema = schema.and_then(|schema| schema.get("items"));
            let resolved_schema =
                schema.and_then(|schema| resolve_schema_ref(schema, root).or(Some(schema)));
            let resolved_item_schema =
                item_schema.and_then(|schema| resolve_schema_ref(schema, root).or(Some(schema)));
            let enum_values = schema_enum_values(resolved_schema)
                .or_else(|| schema_enum_values(resolved_item_schema))
                .unwrap_or_default();
            let default_value = schema
                .and_then(|schema| schema.get("default"))
                .or_else(|| resolved_schema.and_then(|schema| schema.get("default")))
                .or_else(|| resolved_item_schema.and_then(|schema| schema.get("default")))
                .and_then(value_to_string);
            let examples = parameter_examples(item, resolved_schema, resolved_item_schema);
            let example = examples.first().cloned();
            out.push(ApiParam {
                name: name.to_string(),
                location,
                required: item
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(matches!(location, ApiParamLocation::Path)),
                primitive_type: ApiPrimitiveType::from_schema(resolved_schema.or(schema)),
                item_type: resolved_item_schema
                    .or(item_schema)
                    .map(|schema| ApiPrimitiveType::from_schema(Some(schema))),
                enum_values,
                default_value,
                example,
                examples,
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    out
}

fn resolve_schema_ref<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let ref_s = schema.get("$ref").and_then(Value::as_str)?;
    root.pointer(ref_s.strip_prefix('#')?)
}

fn parameter_examples(
    item: &Value,
    schema: Option<&Value>,
    item_schema: Option<&Value>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(example) = item.get("example").and_then(value_to_string) {
        out.push(example);
    }
    if let Some(items) = item.get("examples").and_then(Value::as_object) {
        for item in items.values() {
            let value = item
                .get("value")
                .and_then(value_to_string)
                .or_else(|| value_to_string(item));
            if let Some(value) = value {
                out.push(value);
            }
        }
    }
    if let Some(schema) = schema {
        out.extend(schema_examples(schema));
    }
    if let Some(item_schema) = item_schema {
        out.extend(schema_examples(item_schema));
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn schema_enum_values(schema: Option<&Value>) -> Option<Vec<String>> {
    schema?
        .get("enum")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(value_to_string).collect())
}

fn resolve_parameter_ref<'a>(item: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let ref_s = item.get("$ref").and_then(Value::as_str)?;
    root.pointer(ref_s.strip_prefix('#')?)
}

fn parse_request_body(
    value: Option<&Value>,
    components: Option<&serde_json::Map<String, Value>>,
    arena: &mut Vec<ApiSchema>,
) -> Option<ApiRequestBody> {
    let body = value?;
    let content = body.get("content").and_then(Value::as_object)?;
    let (content_type, media) = content
        .get_key_value("application/x-www-form-urlencoded")
        .or_else(|| content.get_key_value("application/json"))
        .or_else(|| content.get_key_value("multipart/form-data"))
        .or_else(|| content.iter().next())?;
    let schema = media
        .get("schema")
        .and_then(|schema| normalize_schema(schema, components, arena, 0, &mut Vec::new()));
    Some(ApiRequestBody {
        required: body
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        content_type: content_type.to_string(),
        schema,
        is_multipart: content_type == "multipart/form-data",
        is_form_urlencoded: content_type == "application/x-www-form-urlencoded",
    })
}

fn normalize_schema(
    schema: &Value,
    components: Option<&serde_json::Map<String, Value>>,
    arena: &mut Vec<ApiSchema>,
    depth: usize,
    guard: &mut Vec<String>,
) -> Option<ApiSchemaRef> {
    if depth > API_SCHEMA_MAX_DEPTH || arena.len() >= API_SCHEMA_MAX_COUNT {
        return None;
    }
    if let Some(ref_s) = schema.get("$ref").and_then(Value::as_str) {
        let name = ref_s.strip_prefix("#/components/schemas/")?;
        if guard.iter().any(|seen| seen == name) {
            return None;
        }
        let target = components?.get(name)?;
        guard.push(name.to_string());
        let out = normalize_schema_named(name, target, components, arena, depth + 1, guard);
        guard.pop();
        return out;
    }
    normalize_schema_named("", schema, components, arena, depth, guard)
}

fn normalize_schema_named(
    name: &str,
    schema: &Value,
    components: Option<&serde_json::Map<String, Value>>,
    arena: &mut Vec<ApiSchema>,
    depth: usize,
    guard: &mut Vec<String>,
) -> Option<ApiSchemaRef> {
    if depth > API_SCHEMA_MAX_DEPTH || arena.len() >= API_SCHEMA_MAX_COUNT {
        return None;
    }
    let idx = arena.len();
    arena.push(ApiSchema {
        name: name.to_string(),
        kind: schema_kind(schema),
        properties: Vec::new(),
        item: None,
        enum_values: schema_enum_values(Some(schema)).unwrap_or_default(),
        default_value: schema.get("default").and_then(value_to_string),
        examples: schema_examples(schema),
        max_chars: schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()),
    });
    if matches!(arena[idx].kind, ApiSchemaKind::Object) {
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<FxHashSet<_>>()
            })
            .unwrap_or_default();
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            let mut count = 0usize;
            for (prop_name, prop_schema) in props {
                if count >= API_SCHEMA_MAX_PROPERTIES {
                    break;
                }
                if let Some(prop_ref) =
                    normalize_schema(prop_schema, components, arena, depth + 1, guard)
                {
                    arena[idx].properties.push(ApiSchemaProperty {
                        name: prop_name.to_string(),
                        required: required.contains(prop_name.as_str()),
                        schema: prop_ref,
                    });
                    count += 1;
                }
            }
        }
    } else if matches!(arena[idx].kind, ApiSchemaKind::Array)
        && let Some(items) = schema.get("items")
    {
        arena[idx].item = normalize_schema(items, components, arena, depth + 1, guard);
    }
    Some(ApiSchemaRef(idx))
}

fn schema_kind(schema: &Value) -> ApiSchemaKind {
    if schema
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|fmt| matches!(fmt, "binary" | "byte"))
    {
        return ApiSchemaKind::Bytes;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => ApiSchemaKind::Object,
        Some("array") => ApiSchemaKind::Array,
        Some("string") => match schema.get("format").and_then(Value::as_str) {
            Some("date") => ApiSchemaKind::Date,
            Some("date-time") => ApiSchemaKind::DateTime,
            _ => ApiSchemaKind::String,
        },
        Some("integer") => ApiSchemaKind::Integer,
        Some("number") => ApiSchemaKind::Number,
        Some("boolean") => ApiSchemaKind::Boolean,
        _ if schema.get("properties").is_some() => ApiSchemaKind::Object,
        _ => ApiSchemaKind::Unknown,
    }
}

fn schema_examples(schema: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(example) = schema.get("example").and_then(value_to_string) {
        out.push(example);
    }
    if let Some(items) = schema.get("examples").and_then(Value::as_array) {
        for item in items {
            if let Some(value) = value_to_string(item) {
                out.push(value);
            }
        }
    } else if let Some(items) = schema.get("examples").and_then(Value::as_object) {
        for item in items.values() {
            let value = item
                .get("value")
                .and_then(value_to_string)
                .or_else(|| value_to_string(item));
            if let Some(value) = value {
                out.push(value);
            }
        }
    }
    let mut deduped = Vec::with_capacity(out.len());
    for value in out {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

fn parse_responses(
    value: Option<&Value>,
    components: Option<&serde_json::Map<String, Value>>,
    arena: &mut Vec<ApiSchema>,
) -> Vec<ApiResponseSummary> {
    let mut out = Vec::new();
    if let Some(map) = value.and_then(Value::as_object) {
        for (status, body) in map {
            let (content_type, example, schema) = parse_response_media(body, components, arena);
            out.push(ApiResponseSummary {
                status: status.to_string(),
                description: body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                content_type,
                example,
                schema,
            });
        }
    }
    out.sort_unstable_by(|a, b| a.status.cmp(&b.status));
    out
}

fn parse_response_media(
    body: &Value,
    components: Option<&serde_json::Map<String, Value>>,
    arena: &mut Vec<ApiSchema>,
) -> (String, Option<String>, Option<ApiSchemaRef>) {
    let Some(content) = body.get("content").and_then(Value::as_object) else {
        return (String::new(), None, None);
    };
    let Some((content_type, media)) = content
        .get_key_value("application/json")
        .or_else(|| content.get_key_value("application/problem+json"))
        .or_else(|| content.iter().find(|(kind, _)| kind.contains("json")))
        .or_else(|| content.iter().next())
    else {
        return (String::new(), None, None);
    };
    let example = media.get("example").and_then(value_to_string).or_else(|| {
        media
            .get("examples")
            .and_then(Value::as_object)
            .and_then(|examples| examples.values().next())
            .and_then(|example| {
                example
                    .get("value")
                    .and_then(value_to_string)
                    .or_else(|| value_to_string(example))
            })
    });
    let schema = media
        .get("schema")
        .and_then(|schema| normalize_schema(schema, components, arena, 0, &mut Vec::new()));
    (content_type.to_string(), example, schema)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Null => None,
        _ => serde_json::to_string(value).ok(),
    }
}

pub fn grouped_route_ranges(
    routes: &[ApiRouteRow],
    collapsed: &FxHashSet<(ApiSpecId, String)>,
    spec_id: ApiSpecId,
) -> Vec<(String, usize, usize, bool)> {
    let mut groups = Vec::new();
    let mut start = 0usize;
    while start < routes.len() {
        let tag = routes[start].tag.clone();
        let mut end = start + 1;
        while end < routes.len() && routes[end].tag == tag {
            end += 1;
        }
        let is_collapsed = collapsed.contains(&(spec_id, tag.clone()));
        groups.push((tag, start, end - start, is_collapsed));
        start = end;
    }
    groups
}

pub fn api_panel_max_scroll(api: &ApiClientState, visible_h: f32, scale: f32) -> f32 {
    let pad = 10.0 * scale;
    let mut content_h = pad + 40.0 * scale;
    if api.import_menu_open {
        content_h += 28.0 * scale * 2.0 + 12.0 * scale;
    }
    if api.import_url_open {
        content_h += 42.0 * scale;
    }
    if api.import_error.is_some() {
        content_h += 24.0 * scale;
    }
    content_h += 392.0 * scale + api.mock.manual_routes.len().min(8) as f32 * 34.0 * scale;
    if api.specs.is_empty() {
        content_h += 34.0 * scale;
    }
    content_h += api.specs.len() as f32 * 122.0 * scale;
    if let Some(model) = api.selected_model() {
        content_h += 28.0 * scale;
        content_h += 34.0 * scale;
        if !api.collapsed_route_roots.contains(&model.id) {
            for (_, _, len, collapsed) in
                grouped_route_ranges(&model.routes, &api.collapsed_tags, model.id)
            {
                content_h += 28.0 * scale;
                if !collapsed {
                    content_h += len as f32 * 30.0 * scale;
                }
            }
        }
    }
    (content_h + pad + 36.0 * scale - visible_h).max(0.0)
}

pub fn api_tab_max_scroll(
    model: Option<&ApiSpecModel>,
    tab_state: &ApiClientTabState,
    visible_h: f32,
    scale: f32,
) -> f32 {
    let Some(model) = model else {
        return 0.0;
    };
    if tab_state.auth_view {
        let pad = 28.0 * scale;
        let content_h = pad
            + 38.0 * scale
            + if model.security_schemes.is_empty() {
                28.0 * scale
            } else {
                model
                    .security_schemes
                    .iter()
                    .map(|scheme| api_auth_scheme_row_height(scheme, scale))
                    .sum::<f32>()
            }
            + {
                let route_count = api_auth_related_route_count(model).min(12);
                if route_count == 0 {
                    0.0
                } else {
                    44.0 * scale + route_count as f32 * 34.0 * scale
                }
            };
        return (content_h + pad + 36.0 * scale - visible_h).max(0.0);
    }
    let Some(route_idx) = tab_state
        .route_idx
        .or_else(|| (!model.routes.is_empty()).then_some(0))
    else {
        return 0.0;
    };
    let Some(route) = model.routes.get(route_idx) else {
        return 0.0;
    };
    let pad = 28.0 * scale;
    let mut content_h = pad + 42.0 * scale;
    if !route.summary.is_empty() {
        content_h += 30.0 * scale;
    }
    content_h += 558.0 * scale;
    content_h += 28.0 * scale;
    content_h += model.servers.len().max(1) as f32 * 34.0 * scale + 42.0 * scale;
    let auth_scheme_indices = api_route_auth_scheme_indices(model, route);
    if !auth_scheme_indices.is_empty() {
        content_h += 28.0 * scale
            + auth_scheme_indices
                .iter()
                .filter_map(|idx| model.security_schemes.get(*idx))
                .map(|scheme| api_auth_scheme_row_height(scheme, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if !route.path_params.is_empty() {
        content_h += 28.0 * scale
            + route
                .path_params
                .iter()
                .map(|param| api_param_row_height(param, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if !route.query_params.is_empty() {
        content_h += 28.0 * scale
            + route
                .query_params
                .iter()
                .map(|param| api_param_row_height(param, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if let Some(body) = &route.request_body {
        content_h += 28.0 * scale;
        if body.is_multipart || body.is_form_urlencoded {
            content_h += body
                .schema
                .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
                .map(|schema| {
                    schema
                        .properties
                        .iter()
                        .filter_map(|prop| model.schema_arena.get(prop.schema.0))
                        .map(|schema| api_body_prop_row_height(schema, model, scale))
                        .sum::<f32>()
                })
                .unwrap_or(0.0);
        } else {
            content_h += api_body_text_area_height(&tab_state.body_json, scale) + 16.0 * scale;
        }
    }
    content_h += 84.0 * scale;
    if let Some(response) = &tab_state.response {
        let response_text = api_response_text(response, tab_state.response_view);
        content_h += 62.0 * scale + api_response_text_area_height(response_text, scale);
        if api_response_has_auth_tokens(response) {
            content_h += model
                .security_schemes
                .iter()
                .filter(|scheme| scheme.token_capable())
                .count() as f32
                * 30.0
                * scale;
        }
    } else if tab_state.pending {
        content_h += 24.0 * scale;
    }
    (content_h + pad + 36.0 * scale - visible_h).max(0.0)
}

pub fn api_auth_scheme_row_height(scheme: &ApiSecurityScheme, scale: f32) -> f32 {
    if matches!(
        scheme.kind,
        ApiSecuritySchemeKind::Http { ref scheme, .. } if scheme.eq_ignore_ascii_case("basic")
    ) {
        92.0 * scale
    } else if scheme.token_capable() {
        72.0 * scale
    } else {
        58.0 * scale
    }
}

pub fn api_param_row_height(param: &ApiParam, scale: f32) -> f32 {
    let mut meta_lines = 0usize;
    if param.default_value.is_some() {
        meta_lines += 1;
    }
    if !param.enum_values.is_empty() || !param.examples.is_empty() {
        meta_lines += 1;
    } else if param.example.is_some() {
        meta_lines += 1;
    }
    let meta_h = if meta_lines == 0 {
        0.0
    } else {
        32.0 + meta_lines.saturating_sub(1) as f32 * 20.0
    };
    (46.0 * scale).max((meta_h + 14.0) * scale)
}

pub fn api_body_prop_row_height(schema: &ApiSchema, model: &ApiSpecModel, scale: f32) -> f32 {
    let mut meta_lines = usize::from(schema.max_chars.is_some());
    if schema.default_value.is_some() {
        meta_lines += 1;
    }
    if !api_schema_allowed_values(schema, model).is_empty() || !schema.examples.is_empty() {
        meta_lines += 1;
    }
    if !api_schema_allowed_values(schema, model).is_empty() {
        meta_lines += schema.examples.len().min(3);
    }
    let meta_h = if meta_lines == 0 {
        0.0
    } else {
        32.0 + meta_lines.saturating_sub(1) as f32 * 20.0
    };
    (46.0 * scale).max((meta_h + 14.0) * scale)
}

pub fn api_auth_route_rank(route: &ApiRouteRow) -> Option<u8> {
    if api_route_has_auth_word(route, "login")
        || api_route_has_auth_word(route, "signin")
        || api_route_has_auth_word(route, "sign-in")
        || api_route_has_auth_word(route, "token")
    {
        Some(0)
    } else if api_route_has_auth_word(route, "refresh") {
        Some(1)
    } else if api_route_has_auth_word(route, "logout")
        || api_route_has_auth_word(route, "session")
        || api_route_has_auth_word(route, "oauth")
        || api_route_has_auth_word(route, "jwt")
        || api_route_has_auth_word(route, "auth")
    {
        Some(2)
    } else {
        None
    }
}

pub fn api_auth_related_route_count(model: &ApiSpecModel) -> usize {
    model
        .routes
        .iter()
        .filter(|route| api_auth_route_rank(route).is_some())
        .count()
}

pub fn api_response_has_auth_tokens(response: &ApiJobResponse) -> bool {
    serde_json::from_str::<Value>(&response.body)
        .ok()
        .is_some_and(|json| {
            json.get("access_token").and_then(Value::as_str).is_some()
                || json.get("refresh_token").and_then(Value::as_str).is_some()
        })
}

fn api_route_has_auth_word(route: &ApiRouteRow, needle: &str) -> bool {
    contains_ascii_ci(&route.path, needle)
        || contains_ascii_ci(&route.operation_id, needle)
        || contains_ascii_ci(&route.summary, needle)
        || contains_ascii_ci(&route.tag, needle)
}

fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return false;
    }
    h.windows(n.len())
        .any(|window| window.iter().zip(n).all(|(a, b)| a.eq_ignore_ascii_case(b)))
}

pub fn api_schema_allowed_values<'a>(
    schema: &'a ApiSchema,
    model: &'a ApiSpecModel,
) -> &'a [String] {
    if !schema.enum_values.is_empty() {
        &schema.enum_values
    } else if matches!(schema.kind, ApiSchemaKind::Array) {
        schema
            .item
            .and_then(|item| model.schema_arena.get(item.0))
            .map(|item| item.enum_values.as_slice())
            .unwrap_or(&[])
    } else {
        &[]
    }
}

pub fn api_schema_is_array_input(schema: &ApiSchema) -> bool {
    matches!(schema.kind, ApiSchemaKind::Array)
}

pub fn split_api_array_values(value: &str) -> Vec<String> {
    api_array_value_parts(value)
        .map(ToString::to_string)
        .collect()
}

pub fn api_array_value_parts(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(['\n', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

pub fn api_array_edit_parts(value: &str) -> (Vec<&str>, &str) {
    if let Some(split_idx) = value.rfind(['\n', ',']) {
        let draft_start = split_idx.saturating_add(1);
        (
            api_array_value_parts(&value[..split_idx]).collect(),
            value[draft_start..].trim_start(),
        )
    } else {
        (Vec::new(), value.trim_start())
    }
}

fn api_array_editor_text(value: &str) -> String {
    let mut out = split_api_array_values(value).join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn finish_api_array_editor_draft(editor: &mut Editor) {
    let text = api_array_editor_text(&editor.get_full_text());
    editor.set_text_clean(&text);
    editor.cursor = editor.len();
    editor.selection_anchor = Some(editor.cursor);
}

fn backspace_api_array_editor(editor: &mut Editor) {
    if editor.selection_anchor.is_some() && editor.selection_anchor != Some(editor.cursor) {
        editor.delete_selection();
        return;
    }
    let text = editor.get_full_text();
    if editor.cursor == text.len() && (text.ends_with('\n') || text.ends_with(',')) {
        let trimmed = text.trim_end_matches(['\n', ',']);
        if let Some(prev_split) = trimmed.rfind(['\n', ',']) {
            let mut next = trimmed[..prev_split].to_string();
            if !next.trim().is_empty() {
                next.push('\n');
            }
            editor.set_text_clean(&next);
        } else {
            editor.set_text_clean("");
        }
        editor.cursor = editor.len();
        editor.selection_anchor = Some(editor.cursor);
    } else {
        editor.backspace();
    }
}

fn push_api_array_value(value: &mut String, item: &str) {
    let item = item.trim();
    if item.is_empty() {
        return;
    }
    if api_array_value_parts(value).any(|part| part == item) {
        return;
    }
    if !value.trim().is_empty() {
        value.push('\n');
    }
    value.push_str(item);
}

pub fn api_schema_is_file_input(schema: &ApiSchema, model: &ApiSpecModel) -> bool {
    matches!(schema.kind, ApiSchemaKind::Bytes) || api_schema_is_multi_file_input(schema, model)
}

pub fn api_schema_is_multi_file_input(schema: &ApiSchema, model: &ApiSpecModel) -> bool {
    matches!(schema.kind, ApiSchemaKind::Array)
        && schema
            .item
            .and_then(|item| model.schema_arena.get(item.0))
            .is_some_and(|item| matches!(item.kind, ApiSchemaKind::Bytes))
}

pub fn build_request_url(
    server: &ApiServer,
    path_template: &str,
    path_values: &[ApiInputValue],
    query_values: &[ApiInputValue],
) -> Result<String, ApiLoadError> {
    let mut server_url = server.url.clone();
    for var in &server.variables {
        let needle = format!("{{{}}}", var.name);
        server_url = server_url.replace(&needle, &var.default_value);
    }
    if server_url == "/" {
        server_url = "http://localhost".to_string();
    }
    let mut path = path_template.to_string();
    for item in path_values {
        let needle = format!("{{{}}}", item.name);
        path = path.replace(&needle, &percent_encode_path_param(&item.value));
    }
    let base = server_url.trim_end_matches('/');
    let full = if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    };
    let mut url = validate_api_url(&full)?;
    {
        let mut pairs = url.query_pairs_mut();
        for item in query_values {
            if item.value.contains('\n') {
                for value in item
                    .value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    pairs.append_pair(&item.name, value);
                }
            } else if !item.value.is_empty() {
                pairs.append_pair(&item.name, &item.value);
            }
        }
    }
    Ok(url.to_string())
}

fn route_security_requirements<'a>(
    route: &'a ApiRouteRow,
    model: &'a ApiSpecModel,
) -> &'a [ApiSecurityRequirement] {
    route
        .security
        .as_deref()
        .unwrap_or(model.root_security.as_slice())
}

pub fn api_route_auth_scheme_indices(model: &ApiSpecModel, route: &ApiRouteRow) -> Vec<usize> {
    let mut out = Vec::new();
    for requirement in route_security_requirements(route, model) {
        for req_scheme in &requirement.schemes {
            if let Some(idx) = model
                .security_schemes
                .iter()
                .position(|scheme| scheme.name == req_scheme.name)
                && !out.contains(&idx)
            {
                out.push(idx);
            }
        }
    }
    out
}

pub fn api_route_auth_missing(
    model: &ApiSpecModel,
    route: &ApiRouteRow,
    auth: &ApiAuthStore,
) -> bool {
    let requirements = route_security_requirements(route, model);
    requirements
        .iter()
        .any(|requirement| !requirement.schemes.is_empty())
        && !requirements
            .iter()
            .any(|requirement| auth_requirement_satisfied(model, requirement, auth))
}

fn prepared_auth_for_route(
    model: &ApiSpecModel,
    route: &ApiRouteRow,
    auth: &ApiAuthStore,
) -> Vec<ApiPreparedAuthPart> {
    for requirement in route_security_requirements(route, model) {
        let mut parts = Vec::new();
        for req_scheme in &requirement.schemes {
            let Some(scheme) = model
                .security_schemes
                .iter()
                .find(|scheme| scheme.name == req_scheme.name)
            else {
                parts.clear();
                break;
            };
            let Some(entry) = auth.entry(model.id, &scheme.name) else {
                parts.clear();
                break;
            };
            let Some(part) = prepared_auth_part(scheme, entry) else {
                parts.clear();
                break;
            };
            parts.push(part);
        }
        if parts.len() == requirement.schemes.len() {
            return parts;
        }
    }
    Vec::new()
}

fn auth_requirement_satisfied(
    model: &ApiSpecModel,
    requirement: &ApiSecurityRequirement,
    auth: &ApiAuthStore,
) -> bool {
    requirement.schemes.iter().all(|req_scheme| {
        let Some(scheme) = model
            .security_schemes
            .iter()
            .find(|scheme| scheme.name == req_scheme.name)
        else {
            return false;
        };
        auth.entry(model.id, &scheme.name)
            .and_then(|entry| prepared_auth_part(scheme, entry))
            .is_some()
    })
}

fn prepared_auth_part(
    scheme: &ApiSecurityScheme,
    entry: &ApiAuthEntry,
) -> Option<ApiPreparedAuthPart> {
    match &scheme.kind {
        ApiSecuritySchemeKind::ApiKey { name, location } => {
            let value = non_empty_auth_value(entry)?;
            Some(match location {
                ApiSecurityApiKeyLocation::Header => ApiPreparedAuthPart::Header {
                    name: name.clone(),
                    value,
                },
                ApiSecurityApiKeyLocation::Query => ApiPreparedAuthPart::Query {
                    name: name.clone(),
                    value,
                },
                ApiSecurityApiKeyLocation::Cookie => ApiPreparedAuthPart::Cookie {
                    name: name.clone(),
                    value,
                },
            })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
            (!entry.username.is_empty() && !entry.password.is_empty()).then(|| {
                ApiPreparedAuthPart::Basic {
                    username: entry.username.clone(),
                    password: entry.password.clone(),
                }
            })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("bearer") => {
            bearer_token(entry).map(|token| ApiPreparedAuthPart::Bearer { token })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("digest") => {
            non_empty_auth_value(entry).map(|value| ApiPreparedAuthPart::Digest { value })
        }
        ApiSecuritySchemeKind::OAuth2 { .. } | ApiSecuritySchemeKind::OpenIdConnect { .. } => {
            bearer_token(entry).map(|token| ApiPreparedAuthPart::Bearer { token })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } => {
            non_empty_auth_value(entry).map(|value| ApiPreparedAuthPart::Header {
                name: "Authorization".to_string(),
                value: format!("{scheme} {value}"),
            })
        }
    }
}

fn non_empty_auth_value(entry: &ApiAuthEntry) -> Option<String> {
    (!entry.value.is_empty()).then(|| entry.value.clone())
}

fn bearer_token(entry: &ApiAuthEntry) -> Option<String> {
    if !entry.value.is_empty() {
        Some(entry.value.clone())
    } else if !entry.access_token.is_empty() {
        Some(entry.access_token.clone())
    } else {
        None
    }
}

fn append_auth_query(url: &mut String, auth_parts: &[ApiPreparedAuthPart]) {
    if !auth_parts
        .iter()
        .any(|part| matches!(part, ApiPreparedAuthPart::Query { .. }))
    {
        return;
    }
    let Ok(mut parsed) = Url::parse(url) else {
        return;
    };
    {
        let mut pairs = parsed.query_pairs_mut();
        for part in auth_parts {
            if let ApiPreparedAuthPart::Query { name, value } = part {
                pairs.append_pair(name, value);
            }
        }
    }
    *url = parsed.to_string();
}

fn percent_encode_path_param(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

pub fn json_body_is_valid(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok()
}

pub const API_BODY_TEXT_SCALE: f32 = 1.0;
pub(crate) const API_MOCK_TY_POPUP_BYTE: usize = usize::MAX;

pub fn api_text_area_line_height(scale: f32) -> f32 {
    26.0 * scale
}

pub fn api_body_text_area_height(text: &str, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h + line_h * 3.0 + 16.0 * scale).clamp(260.0 * scale, 620.0 * scale)
}

pub fn api_response_text_area_height(text: &str, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h + 16.0 * scale).clamp(300.0 * scale, 620.0 * scale)
}

pub fn api_text_area_max_scroll(text: &str, visible_h: f32, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h - visible_h.max(line_h)).max(0.0)
}

pub fn api_text_area_max_scroll_x<F>(text: &str, visible_w: f32, mut measure: F) -> f32
where
    F: FnMut(&str) -> f32,
{
    let longest = text.split('\n').map(&mut measure).fold(0.0, f32::max);
    (longest - visible_w.max(1.0) + 20.0).max(0.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiJobRequest {
    pub request_id: u64,
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub method: ApiMethod,
    pub url: String,
    pub auth_parts: Vec<ApiPreparedAuthPart>,
    pub body_json: Option<String>,
    pub body_form: Option<Vec<ApiInputValue>>,
    pub body_multipart: Option<Vec<ApiMultipartPart>>,
    pub resolved_host: Option<ApiResolvedHost>,
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
    let server_reach_ms = measure_api_server_reach_ms(job.resolved_host.as_ref());
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
            response.timing_text =
                format_api_timing_text(response.elapsed_ms, response.server_reach_ms);
            response.error = Some(err);
            return response;
        }
    };
    response.elapsed_ms = started.elapsed().as_millis();
    response.timing_text = format_api_timing_text(response.elapsed_ms, response.server_reach_ms);
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

fn format_api_timing_text(elapsed_ms: u128, server_reach_ms: Option<u128>) -> String {
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
) -> usize {
    let mut x = 0.0;
    for (byte_idx, ch) in line.char_indices() {
        let adv = renderer.char_advance(ch);
        if target_x <= x + adv * 0.5 {
            return byte_idx;
        }
        x += adv;
    }
    line.len()
}

fn api_mock_path_param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
                .map(str::to_string)
        })
        .collect()
}

fn api_mock_sanitize_python_param(name: &str) -> String {
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

pub(crate) fn api_mock_body_editor_text(text: &str) -> String {
    let text = text
        .strip_prefix("    \n")
        .or_else(|| text.strip_prefix('\n'))
        .unwrap_or(text);
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

fn set_api_multiline_cursor_at_pointer(
    editor: &mut Editor,
    renderer: &mut crate::renderer::Renderer,
    rect: (f32, f32, f32, f32),
    mx: f32,
    my: f32,
    scale: f32,
    scroll_y: f32,
    scroll_x: f32,
    is_click: bool,
) {
    let (x, y, _, _) = rect;
    let old_line_height = renderer.line_height;
    let old_left_padding = renderer.left_padding;
    let old_last_scroll_x = renderer.last_scroll_x;
    let old_inlay_hints = std::mem::take(&mut renderer.current_python_inlay_hints);

    renderer.line_height = api_text_area_line_height(scale);
    renderer.left_padding = x + 10.0 * scale;
    renderer.last_scroll_x = scroll_x;
    editor.set_cursor_at_pos(mx, my - (y + 10.0 * scale) + scroll_y, renderer, is_click);

    renderer.line_height = old_line_height;
    renderer.left_padding = old_left_padding;
    renderer.last_scroll_x = old_last_scroll_x;
    renderer.current_python_inlay_hints = old_inlay_hints;
}

impl crate::app::App {
    fn pulse_api_cursor_blink(&mut self) {
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
    }

    fn queue_api_body_json_validation(&mut self) {
        let Some(ApiFocus::Body { spec_id, route_idx }) = self.ide_panel.api.focused else {
            return;
        };
        let version = self.ide_panel.api.input_editor.version;
        if self
            .ide_panel
            .api
            .body_json_validation
            .is_some_and(|state| {
                state.spec_id == spec_id && state.route_idx == route_idx && state.version == version
            })
            || self.ide_panel.api.body_json_validation_pending
                == Some((spec_id, route_idx, version))
        {
            return;
        }
        let text = self.ide_panel.api.input_editor.get_full_text();
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.body_json_validation_pending = Some((spec_id, route_idx, version));
        self.ide_panel.api.body_json_validation_rx = Some(rx);
        std::thread::spawn(move || {
            let valid = json_body_is_valid(&text);
            let _ = tx.send(ApiJsonValidationResult {
                spec_id,
                route_idx,
                version,
                valid,
            });
        });
    }

    fn api_text_scroll_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll.current
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Prelude))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Body))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Signature))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    fn api_text_scroll_x_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll_x.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll_x.current
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Prelude))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Body))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Signature))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub(crate) fn api_text_max_scroll_x_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let Some((meta, state)) = self.active_api_tab() else {
            return 0.0;
        };
        let text = match id {
            crate::ui_system::UiId::ApiBodyScrollX(route_idx)
            | crate::ui_system::UiId::ApiBodyInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Body { spec_id, route_idx: focused_route })
                        if spec_id == meta.spec_id && focused_route == route_idx
                ) {
                    self.ide_panel.api.input_editor.get_full_text()
                } else {
                    state.body_json.clone()
                }
            }
            crate::ui_system::UiId::ApiResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Response { spec_id, route_idx: focused_route })
                        if spec_id == meta.spec_id && focused_route == route_idx
                ) {
                    self.ide_panel.api.input_editor.get_full_text()
                } else {
                    state
                        .response
                        .as_ref()
                        .map(|response| {
                            api_response_text(response, state.response_view).to_string()
                        })
                        .unwrap_or_default()
                }
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .api_route_python_script(route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Prelude))
                    {
                        self.ide_panel.api.input_editor.get_full_text()
                    } else {
                        script.prelude.clone()
                    }
                })
                .unwrap_or_default(),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .api_route_python_script(route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Body))
                    {
                        self.ide_panel.api.input_editor.get_full_text()
                    } else {
                        api_mock_body_editor_text(&script.body)
                    }
                })
                .unwrap_or_default(),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.api_mock_signature_for_route(route_idx).unwrap_or_default()
            }
            _ => return 0.0,
        };
        let visible_w = (rect.2
            - 20.0
                * self
                    .renderer
                    .as_ref()
                    .map(|r| r.scale_factor)
                    .unwrap_or(1.0))
        .max(1.0);
        let Some(renderer) = self.renderer.as_mut() else {
            return 0.0;
        };
        api_text_area_max_scroll_x(&text, visible_w, |line| {
            renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
        })
    }

    fn api_one_line_max_scroll_x_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let visible_w = (rect.2 - 16.0 * scale).max(1.0);
        let text = self.ide_panel.api.input_editor.get_full_text();
        let Some(renderer) = self.renderer.as_mut() else {
            return 0.0;
        };
        let text_w = renderer.measure_ui_width(&text, 0.88);
        (text_w - visible_w + 20.0 * scale).max(0.0)
    }

    fn sync_api_one_line_scroll_target(&mut self, immediate: bool) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        if self.api_focus_is_array_input(&focus) {
            return;
        }
        let Some((id, false)) = self.api_focus_ui_target(&focus) else {
            return;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let visible_w = (rect.2 - 16.0 * scale).max(1.0);
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let cursor_x = renderer.measure_ui_width(&text[..cursor], 0.88);
        let max_scroll = self.api_one_line_max_scroll_x_for_ui(id);
        let scroll = &mut self.ide_panel.api.input_scroll_x;
        let mut target = scroll.target;
        if cursor_x - target > visible_w {
            target = cursor_x - visible_w + 10.0 * scale;
        } else if cursor_x < target {
            target = cursor_x;
        }
        scroll.target = target.clamp(0.0, max_scroll);
        if immediate {
            scroll.current = scroll.target;
            scroll.velocity = 0.0;
        }
    }

    fn api_mock_part_for_ui(id: crate::ui_system::UiId) -> Option<(usize, ApiMockSourcePart)> {
        match id {
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Prelude))
            }
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Body))
            }
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Signature))
            }
            _ => None,
        }
    }

    fn sync_api_multiline_scroll_target(&mut self, id: crate::ui_system::UiId, immediate: bool) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_h = api_text_area_line_height(scale);
        let visible_h = (rect.3 - 16.0 * scale).max(line_h);
        let visible_w = (rect.2 - 20.0 * scale).max(1.0);
        let cursor_line = text[..cursor].bytes().filter(|byte| *byte == b'\n').count();
        let line_start = text[..cursor]
            .rfind('\n')
            .map(|idx| idx.saturating_add(1))
            .unwrap_or(0);
        let cursor_line_text = &text[line_start..cursor];
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let cursor_x = renderer.measure_ui_width(cursor_line_text, API_BODY_TEXT_SCALE);
        let max_scroll_x = api_text_area_max_scroll_x(&text, visible_w, |line| {
            renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
        });
        let max_scroll_y = api_text_area_max_scroll(&text, visible_h, scale);

        let cursor_y = cursor_line as f32 * line_h;
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let mut scroll_y_current = self.api_text_scroll_for_ui(id);
        let mut scroll_x_current = self.api_text_scroll_x_for_ui(id);
        let edge = 10.0 * scale;
        if cursor_y + line_h - scroll_y_current > visible_h {
            scroll_y_current = cursor_y + line_h - visible_h + edge;
        } else if cursor_y < scroll_y_current {
            scroll_y_current = cursor_y;
        }
        if cursor_x - scroll_x_current > visible_w {
            scroll_x_current = cursor_x - visible_w + edge;
        } else if cursor_x < scroll_x_current {
            scroll_x_current = cursor_x;
        }
        let target_y = scroll_y_current.clamp(0.0, max_scroll_y);
        let target_x = scroll_x_current.clamp(0.0, max_scroll_x);

        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.body_scroll.target = target_y;
                    state.body_scroll_x.target = target_x;
                    if immediate {
                        state.body_scroll.current = target_y;
                        state.body_scroll.velocity = 0.0;
                        state.body_scroll_x.current = target_x;
                        state.body_scroll_x.velocity = 0.0;
                    }
                }
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.response_scroll.target = target_y;
                    state.response_scroll_x.target = target_x;
                    if immediate {
                        state.response_scroll.current = target_y;
                        state.response_scroll.velocity = 0.0;
                        state.response_scroll_x.current = target_x;
                        state.response_scroll_x.velocity = 0.0;
                    }
                }
            }
            _ => {
                if let Some(key) = Self::api_mock_part_for_ui(id) {
                    let scroll_y = self
                        .ide_panel
                        .api
                        .mock_python_scrolls
                        .entry(key)
                        .or_insert_with(|| ScrollState::new(7.0));
                    scroll_y.target = target_y;
                    if immediate {
                        scroll_y.current = target_y;
                        scroll_y.velocity = 0.0;
                    }
                    let scroll_x = self
                        .ide_panel
                        .api
                        .mock_python_scrolls_x
                        .entry(key)
                        .or_insert_with(|| ScrollState::new(7.0));
                    scroll_x.target = target_x;
                    if immediate {
                        scroll_x.current = target_x;
                        scroll_x.velocity = 0.0;
                    }
                }
            }
        }
    }

    pub(crate) fn drag_api_text_scrollbar_x_from_last_mouse(&mut self) -> bool {
        let Some((id, body)) = self.active_api_tab().and_then(|(_, state)| {
            let route_idx = state.route_idx?;
            if state.body_scroll_x.is_dragging {
                Some((crate::ui_system::UiId::ApiBodyScrollX(route_idx), true))
            } else if state.response_scroll_x.is_dragging {
                Some((crate::ui_system::UiId::ApiResponseScrollX(route_idx), false))
            } else {
                None
            }
        }) else {
            return false;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let max_scroll = self.api_text_max_scroll_x_for_ui(id);
        let Some((meta, _)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let mx = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.last_mouse_x)
            .unwrap_or(0.0);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            let scroll = if body {
                &mut state.body_scroll_x
            } else {
                &mut state.response_scroll_x
            };
            let ratio = (mx - rect.0 - scroll.drag_offset) / rect.2.max(0.0001);
            scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
            scroll.current = scroll.target;
        }
        true
    }

    fn api_focus_ui_target(&self, focus: &ApiFocus) -> Option<(crate::ui_system::UiId, bool)> {
        match focus {
            ApiFocus::ImportUrl => Some((crate::ui_system::UiId::ApiImportUrlInput, false)),
            ApiFocus::MockProxyBase => Some((crate::ui_system::UiId::ApiMockProxyBaseInput, false)),
            ApiFocus::MockPythonUvPath => {
                Some((crate::ui_system::UiId::ApiMockPythonUvPathInput, false))
            }
            ApiFocus::MockPythonVersion => {
                Some((crate::ui_system::UiId::ApiMockPythonVersionInput, false))
            }
            ApiFocus::MockPythonCustomPath => Some((
                crate::ui_system::UiId::ApiMockPythonCustomPathInput,
                false,
            )),
            ApiFocus::MockManualPath { manual_idx } => Some((
                crate::ui_system::UiId::ApiMockManualRoutePath(*manual_idx),
                false,
            )),
            ApiFocus::MockPrelude { route_idx } => Some((
                crate::ui_system::UiId::ApiMockPreludeInput(*route_idx),
                true,
            )),
            ApiFocus::MockBody { route_idx } => {
                Some((crate::ui_system::UiId::ApiMockBodyInput(*route_idx), true))
            }
            ApiFocus::MockSignature { route_idx } => Some((
                crate::ui_system::UiId::ApiMockSignatureInput(*route_idx),
                true,
            )),
            ApiFocus::MockStaticResponse { route_idx } => Some((
                crate::ui_system::UiId::ApiMockStaticResponseInput(*route_idx),
                true,
            )),
            ApiFocus::Body { route_idx, .. } => {
                Some((crate::ui_system::UiId::ApiBodyInput(*route_idx), true))
            }
            ApiFocus::Response { route_idx, .. } => {
                Some((crate::ui_system::UiId::ApiResponseBody(*route_idx), true))
            }
            ApiFocus::AuthValue { spec_id, scheme }
            | ApiFocus::AuthRefreshToken { spec_id, scheme }
            | ApiFocus::AuthUsername { spec_id, scheme }
            | ApiFocus::AuthPassword { spec_id, scheme } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .security_schemes
                    .iter()
                    .position(|item| item.name == *scheme)?;
                let id = match focus {
                    ApiFocus::AuthUsername { .. } => crate::ui_system::UiId::ApiAuthUsername(idx),
                    ApiFocus::AuthPassword { .. } => crate::ui_system::UiId::ApiAuthPassword(idx),
                    ApiFocus::AuthRefreshToken { .. } => {
                        crate::ui_system::UiId::ApiAuthRefreshToken(idx)
                    }
                    _ => crate::ui_system::UiId::ApiAuthValue(idx),
                };
                Some((id, false))
            }
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .path_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiPathParamInput(*route_idx, idx),
                    false,
                ))
            }
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .query_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiQueryParamInput(*route_idx, idx),
                    false,
                ))
            }
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => {
                let model = self.ide_panel.api.models.get(spec_id)?;
                let route = model.routes.get(*route_idx)?;
                let root = route.request_body.as_ref()?.schema?;
                let idx = model
                    .schema_arena
                    .get(root.0)?
                    .properties
                    .iter()
                    .position(|prop| prop.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiBodyFieldInput(*route_idx, idx),
                    false,
                ))
            }
        }
    }

    fn place_api_cursor_from_last_click(&mut self, id: crate::ui_system::UiId, multiline: bool) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        if self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.api_focus_is_array_input(focus))
        {
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
            self.pulse_api_cursor_blink();
            return;
        }
        let scroll_y = if multiline {
            self.api_text_scroll_for_ui(id)
        } else {
            0.0
        };
        let scroll_x = if multiline {
            self.api_text_scroll_x_for_ui(id)
        } else {
            self.ide_panel.api.input_scroll_x.current
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        let scale = renderer.scale_factor;
        let cursor = if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                rect,
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                true,
            );
            self.ide_panel.api.input_editor.cursor
        } else {
            let text = self.ide_panel.api.input_editor.get_full_text();
            let visible_w = (rect.2 - 16.0 * scale).max(0.0);
            let target_x = if mx <= rect.0 {
                scroll_x
            } else if mx >= rect.0 + rect.2 {
                scroll_x + visible_w
            } else {
                scroll_x + (mx - (rect.0 + 8.0 * scale)).clamp(0.0, visible_w)
            };
            api_line_byte_at_x(renderer, &text, target_x)
        };
        self.ide_panel.api.input_editor.cursor = cursor;
        self.ide_panel.api.input_editor.selection_anchor = Some(cursor);
        let now = std::time::Instant::now();
        let dx = mx - self.last_click_pos.0;
        let dy = my - self.last_click_pos.1;
        if now.duration_since(self.last_click_time).as_millis() < 400 && dx * dx + dy * dy < 25.0 {
            self.click_count = self.click_count.saturating_add(1);
        } else {
            self.click_count = 1;
        }
        self.last_click_time = now;
        self.last_click_pos = (mx, my);
        if self.click_count == 2 {
            self.ide_panel.api.input_editor.select_word();
        }
        if multiline {
            self.sync_api_multiline_scroll_target(id, true);
        } else {
            self.sync_api_one_line_scroll_target(true);
        }
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
    }

    pub(crate) fn drag_api_text_cursor_from_last_mouse(&mut self) -> bool {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return false;
        };
        let Some((id, multiline)) = self.api_focus_ui_target(&focus) else {
            return false;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let scroll_y = self.api_text_scroll_for_ui(id);
        let scroll_x = if multiline {
            self.api_text_scroll_x_for_ui(id)
        } else {
            self.ide_panel.api.input_scroll_x.current
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        let scale = renderer.scale_factor;
        let cursor = if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                rect,
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                false,
            );
            self.ide_panel.api.input_editor.cursor
        } else {
            let text = self.ide_panel.api.input_editor.get_full_text();
            let visible_w = (rect.2 - 16.0 * scale).max(0.0);
            let target_x = if mx <= rect.0 {
                scroll_x
            } else if mx >= rect.0 + rect.2 {
                scroll_x + visible_w
            } else {
                scroll_x + (mx - (rect.0 + 8.0 * scale)).clamp(0.0, visible_w)
            };
            api_line_byte_at_x(renderer, &text, target_x)
        };
        if self.ide_panel.api.input_editor.selection_anchor.is_none() {
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
        }
        self.ide_panel.api.input_editor.cursor = cursor;
        if multiline {
            self.sync_api_multiline_scroll_target(id, false);
        } else {
            let max_scroll = self.api_one_line_max_scroll_x_for_ui(id);
            let edge = 18.0 * scale;
            let scroll = &mut self.ide_panel.api.input_scroll_x;
            scroll.anim_speed = 7.0;
            if mx < rect.0 + edge {
                scroll.scroll_by(-edge);
                scroll.clamp_target(0.0, max_scroll);
            } else if mx > rect.0 + rect.2 - edge {
                scroll.scroll_by(edge);
                scroll.clamp_target(0.0, max_scroll);
            }
        }
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
        true
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_api_file_picker(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.api_import_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Импорт openapi.json")
                .add_filter("OpenAPI JSON", &["json"])
                .pick_file();
            let _ = tx.send(file);
        });
    }

    fn trigger_api_body_file_picker(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
        multi: bool,
    ) {
        let (tx, rx) = mpsc::channel();
        self.api_body_file_rx = Some(rx);
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().set_title("Выбрать файл");
            let paths = if multi {
                dialog.pick_files().unwrap_or_default()
            } else {
                dialog.pick_file().into_iter().collect()
            };
            let _ = tx.send(ApiBodyFilePickResult {
                spec_id,
                route_idx,
                name,
                paths,
            });
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_path_picker(&mut self, kind: ApiPythonPathPickKind) {
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_path_pick_rx = Some(rx);
        std::thread::spawn(move || {
            let title = match kind {
                ApiPythonPathPickKind::Uv => "Выбрать исполняемый файл uv",
                ApiPythonPathPickKind::CustomPython => "Выбрать исполняемый файл Python",
            };
            let path = rfd::FileDialog::new().set_title(title).pick_file();
            let _ = tx.send(ApiPythonPathPickResult { kind, path });
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_version_list(&mut self) {
        let Some(uv_path) = self.ide_panel.api.mock.uv.selected_uv_path() else {
            self.ide_panel.api.mock.uv.status =
                crate::app::api_mock::types::ApiPythonRuntimeStatus::Missing;
            self.ide_panel.api.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_version_list_rx = Some(rx);
        self.ide_panel.api.mock_python_versions_loading = true;
        self.ide_panel.api.mock_python_version_picker_open = true;
        self.ide_panel.api.mock_python_versions_scroll.current = 0.0;
        self.ide_panel.api.mock_python_versions_scroll.target = 0.0;
        std::thread::spawn(move || {
            let result = Command::new(uv_path)
                .arg("python")
                .arg("list")
                .arg("--all-versions")
                .output();
            let payload = match result {
                Ok(output) if output.status.success() => ApiPythonVersionListResult {
                    rows: parse_uv_python_list(&String::from_utf8_lossy(&output.stdout)),
                    error: None,
                },
                Ok(output) => ApiPythonVersionListResult {
                    rows: Vec::new(),
                    error: Some(format!(
                        "Ошибка списка версий: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                },
                Err(err) => ApiPythonVersionListResult {
                    rows: Vec::new(),
                    error: Some(format!("Ошибка запуска uv: {err}")),
                },
            };
            let _ = tx.send(payload);
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_install(&mut self) {
        if self.ide_panel.api.mock_python_install_running {
            return;
        }
        let Some(uv_path) = self.ide_panel.api.mock.uv.selected_uv_path() else {
            self.ide_panel.api.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let version = self.ide_panel.api.mock.uv.python_version.trim().to_string();
        if version.is_empty() {
            self.ide_panel.api.mock.uv.last_error = "Выберите версию Python.".to_string();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_install_rx = Some(rx);
        self.ide_panel.api.mock_python_install_running = true;
        self.ide_panel.api.mock_python_install_log.clear();
        self.ide_panel
            .api
            .mock_python_install_log
            .push(ApiPythonInstallLogLine {
                text: format!("uv python install {version}"),
                kind: ApiPythonInstallLogKind::Info,
            });
        std::thread::spawn(move || {
            let spawn = Command::new(uv_path)
                .arg("python")
                .arg("install")
                .arg(&version)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match spawn {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "Ошибка запуска uv: {err}"
                    ))));
                    return;
                }
            };
            if let Some(stdout) = child.stdout.take() {
                spawn_api_python_log_reader(stdout, tx.clone(), ApiPythonInstallLogKind::Info);
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_api_python_log_reader(stderr, tx.clone(), ApiPythonInstallLogKind::Error);
            }
            match child.wait() {
                Ok(status) if status.success() => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Ok(())));
                }
                Ok(status) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "uv завершился с кодом {:?}",
                        status.code()
                    ))));
                }
                Err(err) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "Ошибка ожидания uv: {err}"
                    ))));
                }
            }
        });
    }

    fn apply_api_body_file_pick(&mut self, result: ApiBodyFilePickResult) {
        if result.paths.is_empty() {
            return;
        }
        let new_value = result
            .paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some((_, state)) = self.active_api_tab_mut_for(result.spec_id)
            && state.route_idx == Some(result.route_idx)
            && let Some(value) = state
                .body_values
                .iter_mut()
                .find(|value| value.name == result.name)
        {
            value.value = new_value.clone();
        }
        if matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::BodyField {
                spec_id,
                route_idx,
                ref name,
            }) if spec_id == result.spec_id && route_idx == result.route_idx && name == &result.name
        ) {
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&new_value);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        }
    }

    pub fn start_api_local_import(&mut self, path: PathBuf) {
        let id = self.ide_panel.api.alloc_spec_id();
        self.ide_panel.api.loading.insert(id);
        self.api_load_rx.push(spawn_load_local(id, path));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn start_api_url_import_from_input(&mut self) {
        let raw = self.ide_panel.api.input_editor.get_full_text();
        let url = match validate_api_url(&raw) {
            Ok(url) => url.to_string(),
            Err(err) => {
                self.ide_panel.api.import_error = Some(err.message);
                self.ide_panel.api.import_error_at = Some(now_epoch_secs());
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        let id = self.ide_panel.api.alloc_spec_id();
        self.ide_panel.api.import_error = None;
        self.ide_panel.api.import_error_at = None;
        self.ide_panel.api.import_url_open = false;
        self.ide_panel.api.focused = None;
        self.ide_panel.api.loading.insert(id);
        self.api_load_rx.push(spawn_load_url(id, url));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn refresh_api_spec(&mut self, id: ApiSpecId) {
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        self.ide_panel.api.loading.insert(id);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(spawn_load_local(id, path)),
            ApiSpecSource::Url(url) => self.api_load_rx.push(spawn_load_url(id, url)),
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn ensure_api_model_loaded(&mut self, id: ApiSpecId) {
        if self.ide_panel.api.models.contains_key(&id) || self.ide_panel.api.loading.contains(&id) {
            return;
        }
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        self.ide_panel.api.loading.insert(id);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(spawn_load_local(id, path)),
            ApiSpecSource::Url(url) => self.api_load_rx.push(spawn_load_cached_url(id, url)),
        }
    }

    pub fn open_api_spec_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "API".to_string());

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && !state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        let mut api_state = ApiClientTabState::default();
        if let Some(model) = self.ide_panel.api.models.get(&id)
            && let Some(route) = model.routes.first()
        {
            api_state.route_idx = Some(0);
            fill_api_tab_inputs(&mut api_state, route, model);
        }

        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: api_state.route_idx.map(|route_idx| {
                        ApiClientRouteIdentity::OpenApi {
                            spec_id: id,
                            route_idx,
                        }
                    }),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        while self.highlighter.rx.try_recv().is_ok() {}
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_api_auth_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| format!("Auth · {}", entry.title))
            .unwrap_or_else(|| "API Auth".to_string());

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        let api_state = ApiClientTabState {
            auth_view: true,
            ..Default::default()
        };
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: None,
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_api_route(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        self.open_api_spec_tab(spec_id);
        let mut needs_input_sync = false;
        if let Some((meta, state)) = self.active_api_tab_mut_for(spec_id) {
            state.remember_view_scroll();
            state.remember_route_state();
            state.auth_view = false;
            meta.route_identity = Some(ApiClientRouteIdentity::OpenApi { spec_id, route_idx });
            if !state.restore_route_state(route_idx) {
                state.route_idx = Some(route_idx);
                state.response = None;
                state.response_view = ApiResponseView::Body;
                state.pending = false;
                state.pending_request_id = None;
                state.body_scroll.current = 0.0;
                state.body_scroll.target = 0.0;
                state.body_scroll_x.current = 0.0;
                state.body_scroll_x.target = 0.0;
                state.response_scroll.current = 0.0;
                state.response_scroll.target = 0.0;
                state.response_scroll_x.current = 0.0;
                state.response_scroll_x.target = 0.0;
                needs_input_sync = true;
            }
            state.restore_view_scroll(false, Some(route_idx));
        }
        if needs_input_sync {
            self.sync_api_tab_inputs(spec_id, route_idx);
        }
        self.save_tabs_state();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn open_api_manual_route(&mut self, manual_idx: usize) {
        self.commit_api_focus();
        let Some(route) = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .get(manual_idx)
            .cloned()
        else {
            return;
        };
        let stable_id = route.stable_id.clone();
        let title = api_manual_route_title(route.method, &route.path);
        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(
                    ApiClientTabMeta {
                        route_identity:
                            Some(ApiClientRouteIdentity::Manual { stable_id: tab_id }),
                        ..
                    },
                    _
                ) if tab_id == &stable_id
            )
        }) {
            self.switch_to_tab(idx);
            if let Some((meta, state)) = self.active_api_tab_mut_for(API_MANUAL_MOCK_SPEC_ID) {
                meta.title = title.clone();
                state.route_idx = Some(manual_idx);
            }
            self.base_title = title;
            if let Some(window) = self.window.as_ref() {
                crate::app::App::update_window_title(window, &self.base_title, false);
                window.request_redraw();
            }
            return;
        }

        let api_state = ApiClientTabState {
            route_idx: Some(manual_idx),
            ..Default::default()
        };
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: API_MANUAL_MOCK_SPEC_ID,
                    title,
                    route_identity: Some(ApiClientRouteIdentity::Manual { stable_id }),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    fn sync_api_manual_route_tabs(&mut self) {
        let routes = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .map(|(idx, route)| {
                (
                    idx,
                    route.stable_id.clone(),
                    api_manual_route_title(route.method, &route.path),
                )
            })
            .collect::<Vec<_>>();
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
            let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind else {
                continue;
            };
            let Some(ApiClientRouteIdentity::Manual { stable_id }) = &meta.route_identity else {
                continue;
            };
            if let Some((manual_idx, _, title)) = routes.iter().find(|(_, id, _)| id == stable_id) {
                meta.title = title.clone();
                state.route_idx = Some(*manual_idx);
                tab.base_title = title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = title.clone();
                }
            } else {
                meta.title = "Mock removed".to_string();
                state.route_idx = None;
                tab.base_title = meta.title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = meta.title.clone();
                }
            }
        }
    }

    pub fn active_tab_is_api_client(&self) -> bool {
        self.tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_api_client())
    }

    pub fn active_api_tab(&self) -> Option<(&ApiClientTabMeta, &ApiClientTabState)> {
        let tab = self.tabs.get(self.active_tab)?;
        match &tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) => Some((meta, state)),
            _ => None,
        }
    }

    pub(crate) fn active_api_tab_mut_for(
        &mut self,
        spec_id: ApiSpecId,
    ) -> Option<(&mut ApiClientTabMeta, &mut ApiClientTabState)> {
        let tab = self.tabs.get_mut(self.active_tab)?;
        match &mut tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) if meta.spec_id == spec_id => {
                Some((meta, state))
            }
            _ => None,
        }
    }

    pub(crate) fn sync_api_tab_inputs(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return;
        };
        let Some(route) = model.routes.get(route_idx) else {
            return;
        };
        let path_values = route
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
            .collect::<Vec<_>>();
        let query_values = route
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
            .collect::<Vec<_>>();
        let body_values = default_body_values_for_route(route, model);
        let body_json = default_body_for_route(route, model);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.path_values = path_values;
            state.query_values = query_values;
            state.body_values = body_values;
            state.body_json = body_json;
            state.body_scroll.current = 0.0;
            state.body_scroll.target = 0.0;
            state.body_scroll_x.current = 0.0;
            state.body_scroll_x.target = 0.0;
            state.response_scroll.current = 0.0;
            state.response_scroll.target = 0.0;
            state.response_scroll_x.current = 0.0;
            state.response_scroll_x.target = 0.0;
        }
    }

    pub fn focus_api_input(&mut self, focus: ApiFocus) {
        let focus_changed = self.ide_panel.api.focused.as_ref() != Some(&focus);
        if focus_changed {
            self.commit_api_focus();
            self.stash_active_api_mock_editor();
            let is_array = self.api_focus_is_array_input(&focus);
            let mut text = self.api_focus_text(&focus);
            if is_array {
                text = api_array_editor_text(&text);
            }
            let old_version = self.ide_panel.api.input_editor.version;
            if let Some(key) = Self::api_mock_editor_key_for_focus(&focus)
                && let Some(editor) = self.ide_panel.api.mock_python_editors.remove(&key)
            {
                self.ide_panel.api.input_editor = editor;
            } else {
                self.ide_panel.api.input_editor.set_text_clean(&text);
                self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
            }
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
            self.ide_panel.api.input_scroll_x.current = 0.0;
            self.ide_panel.api.input_scroll_x.target = 0.0;
            self.ide_panel.api.input_scroll_x.velocity = 0.0;
        }
        self.ide_panel.api.focused = Some(focus);
        self.search_focused = false;
        self.settings_ignore_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
        if focus_changed && let Some((route_idx, _)) = self.api_mock_python_focus_target() {
            self.queue_api_mock_python_tools(route_idx);
        }
    }

    fn focus_next_api_input(&mut self, reverse: bool) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return false;
        };
        let order = api_focus_order_for_view(spec_id, model, state);
        if order.is_empty() {
            return false;
        }
        let current = self.ide_panel.api.focused.clone();
        let current_idx = current
            .as_ref()
            .and_then(|focus| order.iter().position(|item| item == focus));
        let next_idx = if reverse {
            current_idx
                .unwrap_or(0)
                .checked_sub(1)
                .unwrap_or(order.len() - 1)
        } else {
            current_idx.map(|idx| (idx + 1) % order.len()).unwrap_or(0)
        };
        let next = order[next_idx].clone();
        self.focus_api_input(next);
        self.sync_api_one_line_scroll_target(true);
        true
    }

    fn api_focus_text(&self, focus: &ApiFocus) -> String {
        match focus {
            ApiFocus::ImportUrl => self.ide_panel.api.input_editor.get_full_text(),
            ApiFocus::MockProxyBase => self.ide_panel.api.mock.proxy_base_url.clone(),
            ApiFocus::MockPythonUvPath => self
                .ide_panel
                .api
                .mock
                .uv
                .selected_uv_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockPythonVersion => self.ide_panel.api.mock.uv.python_version.clone(),
            ApiFocus::MockPythonCustomPath => self
                .ide_panel
                .api
                .mock
                .uv
                .custom_python_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockManualPath { manual_idx } => self
                .ide_panel
                .api
                .mock
                .manual_routes
                .get(*manual_idx)
                .map(|route| route.path.clone())
                .unwrap_or_default(),
            ApiFocus::MockPrelude { route_idx } => self
                .api_route_python_script(*route_idx)
                .map(|script| script.prelude.clone())
                .unwrap_or_default(),
            ApiFocus::MockBody { route_idx } => self
                .api_route_python_script(*route_idx)
                .map(|script| api_mock_body_editor_text(&script.body))
                .unwrap_or_default(),
            ApiFocus::MockSignature { route_idx } => self
                .api_mock_signature_for_route(*route_idx)
                .unwrap_or_default(),
            ApiFocus::MockStaticResponse { route_idx } => self
                .api_route_override(*route_idx)
                .map(|override_route| match &override_route.response {
                    crate::app::api_mock::types::ApiMockResponse::Generated => self
                        .api_mock_generated_preview(*route_idx)
                        .unwrap_or_else(|| "{}".to_string()),
                    crate::app::api_mock::types::ApiMockResponse::Json(text)
                    | crate::app::api_mock::types::ApiMockResponse::Text(text) => text.clone(),
                })
                .unwrap_or_else(|| {
                    self.api_mock_generated_preview(*route_idx)
                        .unwrap_or_else(|| "{}".to_string())
                }),
            ApiFocus::AuthValue { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| {
                    if !entry.access_token.is_empty() {
                        entry.access_token.clone()
                    } else {
                        entry.value.clone()
                    }
                })
                .unwrap_or_default(),
            ApiFocus::AuthRefreshToken { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.refresh_token.clone())
                .unwrap_or_default(),
            ApiFocus::AuthUsername { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.username.clone())
                .unwrap_or_default(),
            ApiFocus::AuthPassword { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.password.clone())
                .unwrap_or_default(),
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.path_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.query_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.body_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::Body { spec_id, route_idx } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        Some(state.body_json.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            ApiFocus::Response { spec_id, route_idx } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.response.as_ref().map(|response| {
                            api_response_text(response, state.response_view).to_string()
                        })
                    }
                    _ => None,
                })
                .unwrap_or_default(),
        }
    }

    fn api_mock_generated_preview(&self, route_idx: usize) -> Option<String> {
        let (meta, _) = self.active_api_tab()?;
        let model = self.ide_panel.api.models.get(&meta.spec_id)?;
        let route = model.routes.get(route_idx)?;
        Some(api_generated_response_for_route(route, model).2)
    }

    fn apply_response_token_to_auth(
        &mut self,
        route_idx: usize,
        scheme_idx: usize,
        save_access: bool,
        save_refresh: bool,
    ) {
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.route_idx != Some(route_idx) {
            return;
        }
        let spec_id = meta.spec_id;
        let Some(response) = state.response.as_ref() else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&response.body) else {
            return;
        };
        let access_token = json.get("access_token").and_then(Value::as_str);
        let refresh_token = json.get("refresh_token").and_then(Value::as_str);
        if (!save_access || access_token.is_none()) && (!save_refresh || refresh_token.is_none()) {
            return;
        }
        let token_type = json
            .get("token_type")
            .and_then(Value::as_str)
            .unwrap_or("Bearer")
            .to_string();
        let expires_at = json
            .get("expires_in")
            .and_then(Value::as_u64)
            .map(|secs| now_epoch_secs().saturating_add(secs));
        let Some(scheme_name) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.security_schemes.get(scheme_idx))
            .filter(|scheme| scheme.token_capable())
            .map(|scheme| scheme.name.clone())
        else {
            return;
        };
        let entry = self.ide_panel.api.auth.entry_mut(spec_id, &scheme_name);
        if save_access && let Some(token) = access_token {
            entry.access_token = token.to_string();
            entry.value = token.to_string();
        }
        if save_refresh && let Some(token) = refresh_token {
            entry.refresh_token = token.to_string();
            entry.value = token.to_string();
        }
        entry.token_type = token_type;
        entry.expires_at = expires_at;
        self.ide_panel.api.persist();
    }

    pub fn commit_api_focus(&mut self) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        let mut text = self.ide_panel.api.input_editor.get_full_text();
        if self.api_focus_is_array_input(&focus) {
            text = split_api_array_values(&text).join("\n");
        }
        match focus {
            ApiFocus::ImportUrl => {}
            ApiFocus::MockProxyBase => {
                self.ide_panel.api.mock.proxy_base_url = text.trim().to_string();
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonUvPath => {
                self.ide_panel.api.mock.uv.configured_path = non_empty_path(&text);
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonVersion => {
                let version = text.trim();
                self.ide_panel.api.mock.uv.python_version = if version.is_empty() {
                    "3.13".to_string()
                } else {
                    version.to_string()
                };
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonCustomPath => {
                self.ide_panel.api.mock.uv.custom_python_path = non_empty_path(&text);
                self.ide_panel.api.persist();
            }
            ApiFocus::MockManualPath { manual_idx } => {
                let mut path = text.trim().to_string();
                if !path.starts_with('/') {
                    path.insert(0, '/');
                }
                if let Some(route) = self.ide_panel.api.mock.manual_routes.get_mut(manual_idx) {
                    route.path = if path == "/" {
                        format!("/mock-{}", manual_idx.saturating_add(1))
                    } else {
                        path
                    };
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockPrelude { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(route_idx) {
                    script.prelude = text;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockBody { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(route_idx) {
                    script.body = text;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockSignature { .. } => {}
            ApiFocus::MockStaticResponse { route_idx } => {
                self.ensure_api_route_override(route_idx);
                let generated = self
                    .api_mock_generated_preview(route_idx)
                    .unwrap_or_else(|| "{}".to_string());
                if let Some(override_route) = self.api_route_override_mut(route_idx) {
                    let was_enabled = override_route.enabled;
                    override_route.response = if text.trim() == generated.trim() {
                        crate::app::api_mock::types::ApiMockResponse::Generated
                    } else {
                        crate::app::api_mock::types::ApiMockResponse::Json(text)
                    };
                    override_route.enabled = was_enabled;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::AuthValue { spec_id, scheme } => {
                let entry = self.ide_panel.api.auth.entry_mut(spec_id, &scheme);
                entry.value = text.clone();
                if !text.is_empty() && entry.token_type.is_empty() {
                    entry.token_type = "Bearer".to_string();
                }
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthRefreshToken { spec_id, scheme } => {
                self.ide_panel
                    .api
                    .auth
                    .entry_mut(spec_id, &scheme)
                    .refresh_token = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthUsername { spec_id, scheme } => {
                self.ide_panel.api.auth.entry_mut(spec_id, &scheme).username = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthPassword { spec_id, scheme } => {
                self.ide_panel.api.auth.entry_mut(spec_id, &scheme).password = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.path_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                }
            }
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.query_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                }
            }
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.body_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                }
            }
            ApiFocus::Body { spec_id, route_idx } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.body_json = text;
                }
            }
            ApiFocus::Response { .. } => {}
        }
    }

    fn api_focus_is_array_input(&self, focus: &ApiFocus) -> bool {
        match focus {
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.path_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.query_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx).map(|route| (model, route)))
                .and_then(|(model, route)| {
                    let root = route.request_body.as_ref()?.schema?;
                    let prop = model
                        .schema_arena
                        .get(root.0)?
                        .properties
                        .iter()
                        .find(|prop| prop.name == *name)?;
                    model.schema_arena.get(prop.schema.0)
                })
                .is_some_and(api_schema_is_array_input),
            _ => false,
        }
    }

    pub fn handle_api_client_click(&mut self, id: crate::ui_system::UiId) -> bool {
        if matches!(
            id,
            crate::ui_system::UiId::ApiImportUrlInput
                | crate::ui_system::UiId::ApiMockProxyBaseInput
                | crate::ui_system::UiId::ApiMockPythonUvPathInput
                | crate::ui_system::UiId::ApiMockPythonCustomPathInput
                | crate::ui_system::UiId::ApiMockManualRoutePath(_)
                | crate::ui_system::UiId::ApiAuthValue(_)
                | crate::ui_system::UiId::ApiAuthRefreshToken(_)
                | crate::ui_system::UiId::ApiAuthUsername(_)
                | crate::ui_system::UiId::ApiAuthPassword(_)
                | crate::ui_system::UiId::ApiPathParamInput(_, _)
                | crate::ui_system::UiId::ApiQueryParamInput(_, _)
                | crate::ui_system::UiId::ApiBodyInput(_)
                | crate::ui_system::UiId::ApiBodyFieldInput(_, _)
                | crate::ui_system::UiId::ApiResponseBody(_)
                | crate::ui_system::UiId::ApiMockStaticResponseInput(_)
                | crate::ui_system::UiId::ApiMockSignatureInput(_)
                | crate::ui_system::UiId::ApiMockPreludeInput(_)
                | crate::ui_system::UiId::ApiMockBodyInput(_)
        ) {
            self.is_dragging = true;
            self.ide_panel.is_dragging_terminal = false;
        }
        match id {
            crate::ui_system::UiId::ApiImportAdd => {
                self.ide_panel.api.import_menu_open = !self.ide_panel.api.import_menu_open;
            }
            crate::ui_system::UiId::ApiImportFile => {
                self.ide_panel.api.import_menu_open = false;
                self.trigger_api_file_picker();
            }
            crate::ui_system::UiId::ApiImportUrl => {
                self.ide_panel.api.import_menu_open = false;
                self.ide_panel.api.import_url_open = true;
                self.focus_api_input(ApiFocus::ImportUrl);
            }
            crate::ui_system::UiId::ApiImportUrlInput => {
                self.ide_panel.api.import_url_open = true;
                self.focus_api_input(ApiFocus::ImportUrl);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiImportUrlConfirm => {
                self.commit_api_focus();
                self.start_api_url_import_from_input();
            }
            crate::ui_system::UiId::ApiMockServerToggle => {
                self.toggle_api_mock_server();
            }
            crate::ui_system::UiId::ApiMockServerDetails => {
                self.commit_api_focus();
                self.ide_panel.api.mock_server_detail_open = true;
                self.ide_panel.api.mock_guide_open = false;
                self.ide_panel.api.mock_python_runtime_open = false;
            }
            crate::ui_system::UiId::ApiMockServerDetailsClose => {
                self.ide_panel.api.mock_server_detail_open = false;
            }
            crate::ui_system::UiId::ApiMockServerLogArea => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiMockServerLogScrollY => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_y)
                    .unwrap_or(0.0);
                if let Some(rect) = self
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::ApiMockServerLogArea)
                {
                    let max_scroll = api_mock_server_log_max_scroll(
                        self.ide_panel.api.mock_server_logs.len(),
                        rect.3,
                        self.renderer
                            .as_ref()
                            .map(|renderer| renderer.scale_factor)
                            .unwrap_or(1.0),
                    );
                    let ratio = ((mx - rect.1) / rect.3.max(1.0)).clamp(0.0, 1.0);
                    self.ide_panel.api.mock_server_log_scroll.target = ratio * max_scroll;
                    self.ide_panel.api.mock_server_log_scroll.current =
                        self.ide_panel.api.mock_server_log_scroll.target;
                }
            }
            crate::ui_system::UiId::ApiMockModeSelect => {
                self.commit_api_focus();
                self.ide_panel.api.mock.mode = match self.ide_panel.api.mock.mode {
                    crate::app::api_mock::types::ApiMockMode::MockAll => {
                        crate::app::api_mock::types::ApiMockMode::MockSelectedOnly
                    }
                    crate::app::api_mock::types::ApiMockMode::MockSelectedOnly => {
                        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
                    }
                    crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest => {
                        crate::app::api_mock::types::ApiMockMode::MockAll
                    }
                };
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockProxyBaseInput => {
                self.focus_api_input(ApiFocus::MockProxyBase);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockGuideOpen => {
                self.commit_api_focus();
                self.ide_panel.api.mock_guide_open = true;
                self.ide_panel.api.mock_server_detail_open = false;
                self.ide_panel.api.mock_python_runtime_open = false;
            }
            crate::ui_system::UiId::ApiMockGuideClose => {
                self.ide_panel.api.mock_guide_open = false;
            }
            crate::ui_system::UiId::ApiMockGuideBody
            | crate::ui_system::UiId::ApiMockGuideScrollY => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiMockPythonManage => {
                self.commit_api_focus();
                clear_legacy_api_python_runtime_message(&mut self.ide_panel.api);
                self.ide_panel.api.mock_python_runtime_open = true;
                self.ide_panel.api.mock_guide_open = false;
                self.ide_panel.api.mock_server_detail_open = false;
                if matches!(
                    self.ide_panel.api.mock.uv.mode,
                    crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
                ) && self.ide_panel.api.mock.uv.selected_uv_path().is_none()
                {
                    crate::app::api_mock::python_bootstrap::refresh_uv_status(
                        &mut self.ide_panel.api.mock.uv,
                    );
                }
            }
            crate::ui_system::UiId::ApiMockPythonManageClose => {
                self.commit_api_focus();
                self.ide_panel.api.mock_python_runtime_open = false;
                self.ide_panel.api.mock_python_version_picker_open = false;
            }
            crate::ui_system::UiId::ApiMockPythonModeToggle => {
                self.commit_api_focus();
                clear_legacy_api_python_runtime_message(&mut self.ide_panel.api);
                self.ide_panel.api.mock.uv.mode = match self.ide_panel.api.mock.uv.mode {
                    crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged => {
                        crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython
                    }
                    crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython => {
                        crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
                    }
                };
                self.ide_panel.api.mock_python_version_picker_open = false;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockPythonCheckRuntime => {
                self.commit_api_focus();
                crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(
                    &mut self.ide_panel.api.mock.uv,
                );
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockPythonPrepareVersion => {
                self.commit_api_focus();
                self.trigger_api_python_install();
            }
            crate::ui_system::UiId::ApiMockPythonPickUvPath => {
                self.commit_api_focus();
                self.trigger_api_python_path_picker(ApiPythonPathPickKind::Uv);
            }
            crate::ui_system::UiId::ApiMockPythonUvPathInput => {
                self.focus_api_input(ApiFocus::MockPythonUvPath);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockPythonVersionInput => {
                self.commit_api_focus();
                if self.ide_panel.api.mock_python_version_picker_open {
                    self.ide_panel.api.mock_python_version_picker_open = false;
                } else {
                    self.trigger_api_python_version_list();
                }
            }
            crate::ui_system::UiId::ApiMockPythonVersionOption(idx) => {
                self.commit_api_focus();
                if let Some(row) = self.ide_panel.api.mock_python_versions.get(idx) {
                    self.ide_panel.api.mock.uv.python_version = row.version.clone();
                    self.ide_panel.api.mock_python_version_picker_open = false;
                    self.ide_panel.api.mock_python_versions_scroll.current = 0.0;
                    self.ide_panel.api.mock_python_versions_scroll.target = 0.0;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiMockPythonPickCustomPath => {
                self.commit_api_focus();
                self.trigger_api_python_path_picker(ApiPythonPathPickKind::CustomPython);
            }
            crate::ui_system::UiId::ApiMockPythonCustomPathInput => {
                self.focus_api_input(ApiFocus::MockPythonCustomPath);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockManualRoutePath(manual_idx) => {
                self.focus_api_input(ApiFocus::MockManualPath { manual_idx });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockRouteEnable(route_idx) => {
                self.toggle_api_route_mock(route_idx);
            }
            crate::ui_system::UiId::ApiMockRouteDetailsToggle(route_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let key = (meta.spec_id, route_idx);
                if self.ide_panel.api.expanded_mock_routes.contains(&key) {
                    self.ide_panel.api.expanded_mock_routes.remove(&key);
                } else {
                    self.ide_panel.api.expanded_mock_routes.insert(key);
                }
            }
            crate::ui_system::UiId::ApiMockRoutePythonToggle(route_idx) => {
                self.toggle_api_route_python(route_idx);
            }
            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockStaticResponse { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockSignature { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockAddManualRoute => {
                self.add_api_manual_route();
            }
            crate::ui_system::UiId::ApiMockManualRouteOpen(manual_idx) => {
                self.open_api_manual_route(manual_idx);
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockPrelude { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockBody { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockPreludeReset(route_idx) => {
                self.reset_api_route_python_part(route_idx, ApiMockSourcePart::Prelude);
            }
            crate::ui_system::UiId::ApiMockBodyReset(route_idx) => {
                self.reset_api_route_python_part(route_idx, ApiMockSourcePart::Body);
            }
            crate::ui_system::UiId::ApiMockManualRouteMethod(manual_idx) => {
                self.commit_api_focus();
                if let Some(route) = self.ide_panel.api.mock.manual_routes.get_mut(manual_idx) {
                    route.method = match route.method {
                        ApiMethod::Get => ApiMethod::Post,
                        ApiMethod::Post => ApiMethod::Put,
                        ApiMethod::Put => ApiMethod::Patch,
                        ApiMethod::Patch => ApiMethod::Delete,
                        ApiMethod::Delete => ApiMethod::Get,
                        ApiMethod::Head | ApiMethod::Options | ApiMethod::Trace => ApiMethod::Get,
                    };
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiMockAddInputField(_)
            | crate::ui_system::UiId::ApiMockAddOutputField(_) => {}
            crate::ui_system::UiId::ApiMockManualRouteRemove(idx) => {
                if idx < self.ide_panel.api.mock.manual_routes.len() {
                    self.ide_panel.api.mock.manual_routes.remove(idx);
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiSpecOpen(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    self.open_api_spec_tab(id);
                }
            }
            crate::ui_system::UiId::ApiSpecRefresh(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    self.refresh_api_spec(id);
                }
            }
            crate::ui_system::UiId::ApiSpecRemove(idx) => {
                if let Some(id) = self.ide_panel.api.remove_spec(idx) {
                    let mut tab_idxs = self
                        .tabs
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, tab)| match &tab.kind {
                            crate::app::EditorTabKind::ApiClient(meta, _) if meta.spec_id == id => {
                                Some(idx)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    while let Some(tab_idx) = tab_idxs.pop() {
                        self.close_tab_at(tab_idx);
                    }
                }
            }
            crate::ui_system::UiId::ApiSpecSelect(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    let already_selected = self.ide_panel.api.selected_spec == Some(id);
                    self.ide_panel.api.select_spec(id);
                    self.ensure_api_model_loaded(id);
                    if already_selected {
                        if self.ide_panel.api.collapsed_route_roots.contains(&id) {
                            self.ide_panel.api.collapsed_route_roots.remove(&id);
                        } else {
                            self.ide_panel.api.collapsed_route_roots.insert(id);
                        }
                    }
                }
            }
            crate::ui_system::UiId::ApiAuthRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.open_api_auth_tab(spec_id);
                }
            }
            crate::ui_system::UiId::ApiRouteTag(group_idx) => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    if let Some(model) = self.ide_panel.api.models.get(&spec_id) {
                        let groups = grouped_route_ranges(
                            &model.routes,
                            &self.ide_panel.api.collapsed_tags,
                            spec_id,
                        );
                        if let Some((tag, _, _, _)) = groups.get(group_idx) {
                            let key = (spec_id, tag.clone());
                            if self.ide_panel.api.collapsed_tags.contains(&key) {
                                self.ide_panel.api.collapsed_tags.remove(&key);
                            } else {
                                self.ide_panel.api.collapsed_tags.insert(key);
                            }
                        }
                    }
                }
            }
            crate::ui_system::UiId::ApiRoutesRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    if self.ide_panel.api.collapsed_route_roots.contains(&spec_id) {
                        self.ide_panel.api.collapsed_route_roots.remove(&spec_id);
                    } else {
                        self.ide_panel.api.collapsed_route_roots.insert(spec_id);
                    }
                }
            }
            crate::ui_system::UiId::ApiRouteRow(route_idx) => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.open_api_route(spec_id, route_idx);
                }
            }
            crate::ui_system::UiId::ApiServerSelect(idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.server_idx = idx;
                }
            }
            crate::ui_system::UiId::ApiAuthValue(scheme_idx)
            | crate::ui_system::UiId::ApiAuthRefreshToken(scheme_idx)
            | crate::ui_system::UiId::ApiAuthUsername(scheme_idx)
            | crate::ui_system::UiId::ApiAuthPassword(scheme_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let scheme = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                    .unwrap_or_default();
                if scheme.is_empty() {
                    return true;
                }
                let focus = match id {
                    crate::ui_system::UiId::ApiAuthUsername(_) => {
                        ApiFocus::AuthUsername { spec_id, scheme }
                    }
                    crate::ui_system::UiId::ApiAuthPassword(_) => {
                        ApiFocus::AuthPassword { spec_id, scheme }
                    }
                    crate::ui_system::UiId::ApiAuthRefreshToken(_) => {
                        ApiFocus::AuthRefreshToken { spec_id, scheme }
                    }
                    _ => ApiFocus::AuthValue { spec_id, scheme },
                };
                self.focus_api_input(focus);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiAuthSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiAuthAccessSave(_)
            | crate::ui_system::UiId::ApiAuthRefreshSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiAuthAccessClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    let entry = self.ide_panel.api.auth.entry_mut(spec_id, &scheme);
                    entry.access_token.clear();
                    entry.value.clear();
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiAuthRefreshClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    self.ide_panel
                        .api
                        .auth
                        .entry_mut(spec_id, &scheme)
                        .refresh_token
                        .clear();
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiAuthClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    self.ide_panel.api.auth.remove(spec_id, &scheme);
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiTryRequest => {
                self.start_active_api_request();
            }
            crate::ui_system::UiId::ApiPathParamAllowedValue(route_idx, param_idx, value_idx)
            | crate::ui_system::UiId::ApiQueryParamAllowedValue(route_idx, param_idx, value_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| match id {
                        crate::ui_system::UiId::ApiPathParamAllowedValue(_, _, _) => {
                            route.path_params.get(param_idx).map(|param| (true, param))
                        }
                        _ => route
                            .query_params
                            .get(param_idx)
                            .map(|param| (false, param)),
                    })
                    .and_then(|(path, param)| {
                        let values = if param.enum_values.is_empty() {
                            &param.examples
                        } else {
                            &param.enum_values
                        };
                        Some((
                            path,
                            param.name.clone(),
                            matches!(param.primitive_type, ApiPrimitiveType::Array),
                            values.get(value_idx)?.clone(),
                        ))
                    });
                if let Some((path, name, is_array, value)) = picked
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    let values = if path {
                        &mut state.path_values
                    } else {
                        &mut state.query_values
                    };
                    if let Some(field) = values.iter_mut().find(|field| field.name == name) {
                        if is_array {
                            push_api_array_value(&mut field.value, &value);
                        } else {
                            field.value = value;
                        }
                    }
                    self.ide_panel.api.focused = None;
                }
            }
            crate::ui_system::UiId::ApiResponseBodyTab(route_idx)
            | crate::ui_system::UiId::ApiResponseHeadersTab(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let view = match id {
                    crate::ui_system::UiId::ApiResponseBodyTab(_) => ApiResponseView::Body,
                    crate::ui_system::UiId::ApiResponseHeadersTab(_) => ApiResponseView::Headers,
                    _ => ApiResponseView::Body,
                };
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.response_view = view;
                    state.response_scroll.current = 0.0;
                    state.response_scroll.target = 0.0;
                    state.response_scroll_x.current = 0.0;
                    state.response_scroll_x.target = 0.0;
                }
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Response {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if focused_spec == spec_id && focused_route == route_idx
                ) {
                    self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiResponseUseAccessToken(route_idx, scheme_idx) => {
                self.apply_response_token_to_auth(route_idx, scheme_idx, true, false);
            }
            crate::ui_system::UiId::ApiResponseSaveRefreshToken(route_idx, scheme_idx) => {
                self.apply_response_token_to_auth(route_idx, scheme_idx, false, true);
            }
            crate::ui_system::UiId::ApiPathParamInput(route_idx, param_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.path_params.get(param_idx))
                    .map(|param| param.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::PathParam {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiQueryParamInput(route_idx, param_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.query_params.get(param_idx))
                    .map(|param| param.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::QueryParam {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiBodyInput(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.is_dragging = true;
                self.ide_panel.is_dragging_terminal = false;
                self.focus_api_input(ApiFocus::Body { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiBodyScrollX(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let max_scroll = self.api_text_max_scroll_x_for_ui(id);
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_x)
                    .unwrap_or(0.0);
                if let Some(rect) = self.ui_registry.rect_for(id)
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.body_scroll_x.is_dragging = true;
                    state.body_scroll_x.drag_offset = 0.0;
                    let ratio = (mx - rect.0) / rect.2.max(0.0001);
                    state.body_scroll_x.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                    state.body_scroll_x.current = state.body_scroll_x.target;
                }
            }
            crate::ui_system::UiId::ApiBodyFieldInput(route_idx, prop_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.request_body.as_ref())
                    .and_then(|body| body.schema)
                    .and_then(|schema_ref| {
                        self.ide_panel
                            .api
                            .models
                            .get(&spec_id)
                            .and_then(|model| model.schema_arena.get(schema_ref.0))
                    })
                    .and_then(|schema| schema.properties.get(prop_idx))
                    .map(|prop| prop.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::BodyField {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, value_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx).map(|route| (model, route)))
                    .and_then(|(model, route)| {
                        let root = route.request_body.as_ref()?.schema?;
                        let prop = model.schema_arena.get(root.0)?.properties.get(prop_idx)?;
                        let schema = model.schema_arena.get(prop.schema.0)?;
                        let allowed = api_schema_allowed_values(schema, model);
                        let values = if allowed.is_empty() {
                            schema.examples.as_slice()
                        } else {
                            allowed
                        };
                        Some((
                            prop.name.clone(),
                            api_schema_is_array_input(schema),
                            values.get(value_idx)?.clone(),
                        ))
                    });
                let mut applied = None;
                if let Some((name, is_array, value)) = picked
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && let Some(field) = state
                        .body_values
                        .iter_mut()
                        .find(|field| field.name == name)
                {
                    if is_array {
                        push_api_array_value(&mut field.value, &value);
                    } else {
                        field.value = value.clone();
                    }
                    applied = Some((field.name.clone(), field.value.clone(), is_array));
                }
                if let Some((field_name, value, _)) = &applied
                    && matches!(
                        self.ide_panel.api.focused,
                        Some(ApiFocus::BodyField {
                            spec_id: f_spec,
                            route_idx: f_route,
                            ref name,
                        }) if f_spec == spec_id && f_route == route_idx && name == field_name
                    )
                {
                    let old_version = self.ide_panel.api.input_editor.version;
                    self.ide_panel.api.input_editor.set_text_clean(value);
                    self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
                }
                if applied.is_some_and(|(_, _, is_array)| is_array) {
                    self.ide_panel.api.focused = None;
                }
            }
            crate::ui_system::UiId::ApiBodyFilePick(route_idx, prop_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx).map(|route| (model, route)))
                    .and_then(|(model, route)| {
                        let root = route.request_body.as_ref()?.schema?;
                        let prop = model.schema_arena.get(root.0)?.properties.get(prop_idx)?;
                        let schema = model.schema_arena.get(prop.schema.0)?;
                        Some((
                            prop.name.clone(),
                            api_schema_is_multi_file_input(schema, model),
                        ))
                    });
                if let Some((name, multi)) = picked {
                    self.trigger_api_body_file_picker(spec_id, route_idx, name, multi);
                }
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.is_dragging = true;
                self.ide_panel.is_dragging_terminal = false;
                self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiResponseScrollX(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let max_scroll = self.api_text_max_scroll_x_for_ui(id);
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_x)
                    .unwrap_or(0.0);
                if let Some(rect) = self.ui_registry.rect_for(id)
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.response_scroll_x.is_dragging = true;
                    state.response_scroll_x.drag_offset = 0.0;
                    let ratio = (mx - rect.0) / rect.2.max(0.0001);
                    state.response_scroll_x.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                    state.response_scroll_x.current = state.response_scroll_x.target;
                }
            }
            crate::ui_system::UiId::ApiTabBody => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            _ => return false,
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        self.pulse_api_cursor_blink();
        true
    }

    pub fn api_python_runtime_overlay_active(&self) -> bool {
        self.ide_panel.api.mock_python_runtime_open
    }

    pub fn api_runtime_poll_pending(&self) -> bool {
        self.ide_panel.api.python_version_list_rx.is_some()
            || self.ide_panel.api.python_install_rx.is_some()
            || self.ide_panel.api.python_path_pick_rx.is_some()
    }

    pub fn scroll_api_python_runtime_overlay(&mut self, dy: f32) -> bool {
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let s = renderer.scale_factor;
        let layout = api_python_runtime_dialog_layout(renderer.width, renderer.height, s);
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        if self.ide_panel.api.mock_python_version_picker_open {
            let rect = api_python_version_list_rect(layout, s);
            if api_point_in_rect(mx, my, rect) {
                let max_scroll =
                    api_python_version_list_max_scroll(self.ide_panel.api.mock_python_versions.len(), s);
                self.ide_panel.api.mock_python_versions_scroll.anim_speed = 7.0;
                self.ide_panel.api.mock_python_versions_scroll.scroll_by(dy);
                self.ide_panel
                    .api
                    .mock_python_versions_scroll
                    .clamp_target(0.0, max_scroll);
                return true;
            }
        }
        if api_python_install_log_visible(&self.ide_panel.api) {
            let rect = api_python_install_log_rect(layout, s);
            if api_point_in_rect(mx, my, rect) {
                let max_scroll = api_python_install_log_max_scroll(
                    self.ide_panel.api.mock_python_install_log.len(),
                    rect.3,
                    s,
                );
                self.ide_panel.api.mock_python_install_log_scroll.anim_speed = 7.0;
                self.ide_panel.api.mock_python_install_log_scroll.scroll_by(dy);
                self.ide_panel
                    .api
                    .mock_python_install_log_scroll
                    .clamp_target(0.0, max_scroll);
                return true;
            }
        }
        true
    }

    pub fn ui_id_is_api_python_runtime_overlay(id: crate::ui_system::UiId) -> bool {
        matches!(
            id,
            crate::ui_system::UiId::ApiMockPythonManageClose
                | crate::ui_system::UiId::ApiMockPythonModeToggle
                | crate::ui_system::UiId::ApiMockPythonCheckRuntime
                | crate::ui_system::UiId::ApiMockPythonPrepareVersion
                | crate::ui_system::UiId::ApiMockPythonPickUvPath
                | crate::ui_system::UiId::ApiMockPythonPickCustomPath
                | crate::ui_system::UiId::ApiMockPythonVersionOption(_)
                | crate::ui_system::UiId::ApiMockPythonUvPathInput
                | crate::ui_system::UiId::ApiMockPythonVersionInput
                | crate::ui_system::UiId::ApiMockPythonCustomPathInput
        )
    }

    pub fn toggle_api_mock_server(&mut self) {
        self.commit_api_focus();
        if matches!(
            self.ide_panel.api.mock.server_status,
            crate::app::api_mock::types::ApiMockServerStatus::Running { .. }
                | crate::app::api_mock::types::ApiMockServerStatus::Starting
        ) {
            self.ide_panel.api.mock.server_status =
                crate::app::api_mock::types::ApiMockServerStatus::Stopping;
            push_api_mock_server_log(&mut self.ide_panel.api, "server stop requested".to_string());
            stop_api_mock_server();
            return;
        }
        let snapshot = self.ide_panel.api.mock_server_snapshot();
        self.ide_panel.api.mock.server_status =
            crate::app::api_mock::types::ApiMockServerStatus::Starting;
        push_api_mock_server_log(
            &mut self.ide_panel.api,
            format!("server start requested {}:{}", snapshot.bind_host, snapshot.port),
        );
        if let Err(err) = start_api_mock_server(snapshot) {
            self.ide_panel.api.mock.server_status =
                crate::app::api_mock::types::ApiMockServerStatus::Failed(err.clone());
            push_api_mock_server_log(&mut self.ide_panel.api, format!("server start failed: {err}"));
        }
    }

    fn api_route_override(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.ide_panel.api.mock.route_overrides.iter().find(|item| {
            item.source_key == source_key && item.method == route.method && item.path == route.path
        })
    }

    fn api_route_override_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)
            .cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        self.ide_panel
            .api
            .mock
            .route_overrides
            .iter_mut()
            .find(|item| {
                item.source_key == source_key
                    && item.method == route.method
                    && item.path == route.path
            })
    }

    fn ensure_api_route_override(&mut self, route_idx: usize) {
        if self.api_route_override(route_idx).is_some() {
            return;
        }
        self.add_api_route_override(route_idx, false);
    }

    fn add_api_route_override(&mut self, route_idx: usize, enabled: bool) {
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()
        else {
            return;
        };
        let Some(route) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        self.ide_panel.api.mock.route_overrides.push(
            crate::app::api_mock::types::ApiMockRouteOverride {
                source_key: crate::app::api_mock::types::api_mock_source_key(&entry),
                method: route.method,
                path: route.path,
                enabled,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            },
        );
    }

    fn api_route_python_script(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockPythonScript> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.ide_panel
            .api
            .mock
            .route_overrides
            .iter()
            .find_map(|item| {
                (item.source_key == source_key
                    && item.method == route.method
                    && item.path == route.path)
                .then_some(item.python.as_ref().filter(|script| script.enabled))
                .flatten()
            })
    }

    fn api_route_python_script_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockPythonScript> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)
            .cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        self.ide_panel
            .api
            .mock
            .route_overrides
            .iter_mut()
            .find_map(|item| {
                (item.source_key == source_key
                    && item.method == route.method
                    && item.path == route.path)
                .then_some(item.python.as_mut().filter(|script| script.enabled))
                .flatten()
            })
    }

    fn api_mock_python_focus_target(&self) -> Option<(usize, ApiMockSourcePart)> {
        match self.ide_panel.api.focused {
            Some(ApiFocus::MockPrelude { route_idx }) => {
                Some((route_idx, ApiMockSourcePart::Prelude))
            }
            Some(ApiFocus::MockBody { route_idx }) => Some((route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }

    fn api_mock_editor_key_for_focus(focus: &ApiFocus) -> Option<(usize, ApiMockSourcePart)> {
        match focus {
            ApiFocus::MockPrelude { route_idx } => Some((*route_idx, ApiMockSourcePart::Prelude)),
            ApiFocus::MockBody { route_idx } => Some((*route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }

    fn stash_active_api_mock_editor(&mut self) {
        let Some(key) = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .and_then(Self::api_mock_editor_key_for_focus)
        else {
            return;
        };
        let editor = std::mem::replace(&mut self.ide_panel.api.input_editor, Editor::new(512));
        self.ide_panel.api.mock_python_editors.insert(key, editor);
    }

    fn api_mock_route_context(
        &self,
        route_idx: usize,
    ) -> Option<(ApiMethod, String, ApiRouteRow, ApiSpecModel)> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let model = self.ide_panel.api.models.get(&spec_id)?.clone();
        let route = model.routes.get(route_idx)?.clone();
        Some((route.method, route.path.clone(), route, model))
    }

    fn api_mock_signature_for_route(&self, route_idx: usize) -> Option<String> {
        let (_, path, _, _) = self.api_mock_route_context(route_idx)?;
        let mut out = String::from("def handler(\n    req: Request,");
        for name in api_mock_path_param_names(&path) {
            out.push_str("\n    ");
            out.push_str(&api_mock_sanitize_python_param(&name));
            out.push_str(": str,");
        }
        out.push_str("\n    query: Query,\n    body: Body | None,\n    fields: Fields,\n) -> dict[str, Any]:");
        Some(out)
    }

    fn api_mock_script_for_tools(
        &self,
        route_idx: usize,
    ) -> Option<crate::app::api_mock::types::ApiMockPythonScript> {
        let mut script = self.api_route_python_script(route_idx)?.clone();
        script.body = api_mock_body_editor_text(&script.body);
        if let Some((focused_route, part)) = self.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            let text = self.ide_panel.api.input_editor.get_full_text();
            match part {
                ApiMockSourcePart::Prelude => script.prelude = text,
                ApiMockSourcePart::Signature => {}
                ApiMockSourcePart::Body => script.body = text,
            }
        }
        Some(script)
    }

    fn api_mock_edit_text_for_part(
        &self,
        route_idx: usize,
        part: ApiMockSourcePart,
        script: &crate::app::api_mock::types::ApiMockPythonScript,
    ) -> String {
        if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.get_full_text()
        } else {
            match part {
                ApiMockSourcePart::Prelude => script.prelude.clone(),
                ApiMockSourcePart::Signature => {
                    self.api_mock_signature_for_route(route_idx).unwrap_or_default()
                }
                ApiMockSourcePart::Body => script.body.clone(),
            }
        }
    }

    fn api_mock_virtual_path(route_idx: usize) -> PathBuf {
        std::env::temp_dir().join(format!("rriter_api_mock_route_{route_idx}.py"))
    }

    fn map_api_mock_spans_to_edit(
        spans: &[ColorSpan],
        virtual_source: &crate::app::api_mock::ty_check::ApiMockVirtualSource,
        part: ApiMockSourcePart,
    ) -> Vec<ColorSpan> {
        let mut out = Vec::with_capacity(spans.len().min(128));
        for span in spans {
            match part {
                ApiMockSourcePart::Prelude => {
                    let start = span.start.max(virtual_source.prelude_start);
                    let end = span.end.min(virtual_source.prelude_end);
                    if start < end {
                        out.push(ColorSpan {
                            start: start - virtual_source.prelude_start,
                            end: end - virtual_source.prelude_start,
                            color: span.color,
                        });
                    }
                }
                ApiMockSourcePart::Signature => {
                    let start = span.start.max(virtual_source.signature_start);
                    let end = span.end.min(virtual_source.signature_end);
                    if start < end {
                        out.push(ColorSpan {
                            start: start - virtual_source.signature_start,
                            end: end - virtual_source.signature_start,
                            color: span.color,
                        });
                    }
                }
                ApiMockSourcePart::Body => {
                    for line in &virtual_source.body_lines {
                        let start = span.start.max(line.source_start);
                        let end = span.end.min(line.source_end);
                        if start < end {
                            out.push(ColorSpan {
                                start: line.edit_start + start - line.source_start,
                                end: line.edit_start + end - line.source_start,
                                color: span.color,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    fn refresh_api_mock_python_highlight(&mut self, route_idx: usize, part: ApiMockSourcePart) {
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        let virtual_source = build_api_mock_virtual_source(method, &path, &route, &model, &script);
        let edit_text = self.api_mock_edit_text_for_part(route_idx, part, &script);
        let version = self.ide_panel.api.input_editor.version;
        let edit_cursor = if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.cursor
        } else {
            0
        };
        let source_cursor = virtual_source.edit_offset_to_source(
            part,
            &edit_text,
            edit_cursor,
        );
        self.ide_panel.api.mock_highlight_spans = self
            .ide_panel
            .api
            .mock_highlight_cache
            .get(&(route_idx, part))
            .cloned()
            .unwrap_or_default();
        self.ide_panel.api.mock_highlight_target = Some((route_idx, part, version));
        let source = virtual_source.source.clone();
        self.ide_panel.api.mock_highlighter.spans.clear();
        self.ide_panel
            .api
            .mock_highlighter
            .reset(version, source, "py".to_string(), source_cursor);
        if self.ide_panel.api.mock_highlighter.sync_highlight_after_edit(
            version,
            None,
            None,
            None,
            None,
            Duration::from_millis(4),
        ) {
            let spans = self.ide_panel.api.mock_highlighter.spans.clone();
            for cache_part in [
                ApiMockSourcePart::Prelude,
                ApiMockSourcePart::Signature,
                ApiMockSourcePart::Body,
            ] {
                let edit_spans =
                    Self::map_api_mock_spans_to_edit(&spans, &virtual_source, cache_part);
                self.ide_panel
                    .api
                    .mock_highlight_cache
                    .insert((route_idx, cache_part), edit_spans);
            }
            self.ide_panel.api.mock_highlight_spans = self
                .ide_panel
                .api
                .mock_highlight_cache
                .get(&(route_idx, part))
                .cloned()
                .unwrap_or_default();
        }
    }

    fn queue_api_mock_python_tools(&mut self, route_idx: usize) {
        if let Some((focused_route, part)) = self.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            self.refresh_api_mock_python_highlight(route_idx, part);
            self.ide_panel.api.mock_ty_due = Some(Instant::now() + Duration::from_millis(450));
        }
    }

    fn ensure_active_api_mock_highlight(&mut self) -> bool {
        let Some((spec_id, route_idx)) = self
            .active_api_tab()
            .and_then(|(meta, state)| state.route_idx.map(|route_idx| (meta.spec_id, route_idx)))
        else {
            return false;
        };
        if !self
            .ide_panel
            .api
            .expanded_mock_routes
            .contains(&(spec_id, route_idx))
        {
            return false;
        }
        if self.api_route_python_script(route_idx).is_none() {
            return false;
        }
        if self
            .ide_panel
            .api
            .mock_highlight_target
            .is_some_and(|(highlight_route, _, _)| highlight_route == route_idx)
        {
            return false;
        }
        let missing_cache = [
            ApiMockSourcePart::Prelude,
            ApiMockSourcePart::Signature,
            ApiMockSourcePart::Body,
        ]
        .into_iter()
        .any(|part| {
            !self
                .ide_panel
                .api
                .mock_highlight_cache
                .contains_key(&(route_idx, part))
        });
        if !missing_cache {
            return false;
        }
        self.refresh_api_mock_python_highlight(route_idx, ApiMockSourcePart::Body);
        true
    }

    pub fn toggle_api_route_mock(&mut self, route_idx: usize) {
        self.commit_api_focus();
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()
        else {
            return;
        };
        let Some(route) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        if let Some(override_route) =
            self.ide_panel
                .api
                .mock
                .route_overrides
                .iter_mut()
                .find(|item| {
                    item.source_key == source_key
                        && item.method == route.method
                        && item.path == route.path
                })
        {
            override_route.enabled = !override_route.enabled;
        } else {
            self.ide_panel.api.mock.route_overrides.push(
                crate::app::api_mock::types::ApiMockRouteOverride {
                    source_key,
                    method: route.method,
                    path: route.path,
                    enabled: true,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    extra_input_fields: Vec::new(),
                    extra_output_fields: Vec::new(),
                },
            );
        }
        self.ide_panel.api.persist();
    }

    pub fn toggle_api_route_python(&mut self, route_idx: usize) {
        self.commit_api_focus();
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()
        else {
            return;
        };
        let Some(route) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        let idx = if let Some(idx) =
            self.ide_panel
                .api
                .mock
                .route_overrides
                .iter()
                .position(|item| {
                    item.source_key == source_key
                        && item.method == route.method
                        && item.path == route.path
                }) {
            idx
        } else {
            self.ide_panel.api.mock.route_overrides.push(
                crate::app::api_mock::types::ApiMockRouteOverride {
                    source_key,
                    method: route.method,
                    path: route.path,
                    enabled: false,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    extra_input_fields: Vec::new(),
                    extra_output_fields: Vec::new(),
                },
            );
            self.ide_panel
                .api
                .mock
                .route_overrides
                .len()
                .saturating_sub(1)
        };
        let focused_this_route = self
            .api_mock_python_focus_target()
            .is_some_and(|(focused_route, _)| focused_route == route_idx);
        let mut disabled_active_script = false;
        if let Some(override_route) = self.ide_panel.api.mock.route_overrides.get_mut(idx) {
            if let Some(script) = override_route.python.as_mut() {
                script.enabled = !script.enabled;
                disabled_active_script = !script.enabled;
            } else {
                override_route.python = Some(default_api_mock_python_script());
            }
        }
        if disabled_active_script && focused_this_route {
            self.stash_active_api_mock_editor();
            self.ide_panel.api.focused = None;
        }
        self.ide_panel.api.persist();
    }

    pub fn reset_api_route_python_part(&mut self, route_idx: usize, part: ApiMockSourcePart) {
        self.commit_api_focus();
        let Some(script) = self.api_route_python_script_mut(route_idx) else {
            return;
        };
        match part {
            ApiMockSourcePart::Prelude => script.prelude.clear(),
            ApiMockSourcePart::Signature => return,
            ApiMockSourcePart::Body => script.body = default_api_mock_python_body(),
        }
        self.ide_panel
            .api
            .mock_python_editors
            .remove(&(route_idx, part));
        self.ide_panel
            .api
            .mock_highlight_cache
            .retain(|(cached_route, _), _| *cached_route != route_idx);
        self.ide_panel.api.mock_highlight_target = None;
        self.ide_panel.api.mock_highlight_spans.clear();
        self.ide_panel.api.mock_ty_diagnostics.clear();
        if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            let text = match part {
                ApiMockSourcePart::Prelude | ApiMockSourcePart::Signature => String::new(),
                ApiMockSourcePart::Body => default_api_mock_python_body(),
            };
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&text);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
        }
        self.ide_panel.api.persist();
        self.queue_api_mock_python_tools(route_idx);
    }

    pub fn add_api_manual_route(&mut self) {
        self.commit_api_focus();
        let next = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .len()
            .saturating_add(1);
        self.ide_panel
            .api
            .mock
            .manual_routes
            .push(crate::app::api_mock::types::ApiManualRoute {
                stable_id: format!("manual-{}-{}", now_epoch_secs(), next),
                method: ApiMethod::Get,
                path: format!("/mock-{}", next),
                enabled: true,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                input_fields: Vec::new(),
                output_fields: Vec::new(),
            });
        self.ide_panel.api.persist();
        self.open_api_manual_route(next.saturating_sub(1));
    }

    fn start_api_mock_ty_check_now(&mut self, route_idx: usize, version: u64) {
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        self.ide_panel.api.mock.check_status =
            crate::app::api_mock::types::ApiMockCheckStatus::Pending { route_idx, version };
        self.ide_panel.api.mock_ty_diagnostics.clear();
        self.ide_panel.api.mock_ty_pending = Some((route_idx, version));
        self.api_mock_ty_rx = Some(spawn_api_mock_ty_check(
            route_idx,
            version,
            method,
            path,
            route,
            model,
            script,
        ));
    }

    pub(crate) fn api_mock_completion_focus(&self) -> Option<(usize, ApiMockSourcePart)> {
        self.api_mock_python_focus_target()
    }

    pub(crate) fn api_input_current_word_prefix(&self) -> String {
        let editor = &self.ide_panel.api.input_editor;
        let mut p = editor.cursor;
        while p > 0 {
            let b = editor.byte_at(p - 1);
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                break;
            }
            p -= 1;
        }
        if p == editor.cursor {
            return String::new();
        }
        let mut out = Vec::with_capacity(editor.cursor - p);
        for i in p..editor.cursor {
            out.push(editor.byte_at(i));
        }
        String::from_utf8(out).unwrap_or_default()
    }

    fn api_input_after_python_member_dot(&self) -> bool {
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let prefix = &text[line_start..cursor];
        let bytes = prefix.as_bytes();
        let mut idx = bytes.len();
        while idx > 0 && (bytes[idx - 1].is_ascii_alphanumeric() || bytes[idx - 1] == b'_') {
            idx -= 1;
        }
        idx >= 2 && bytes.get(idx - 1) == Some(&b'.')
            && bytes
                .get(idx - 2)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    }

    fn api_mock_completion_indices(prefix: &str, word: &str) -> Option<Vec<usize>> {
        if prefix.is_empty() {
            return Some(Vec::new());
        }
        let prefix = prefix.to_lowercase();
        let word_lower = word.to_lowercase();
        if let Some(start) = word_lower.find(&prefix) {
            return Some((start..start + prefix.len()).collect());
        }
        None
    }

    fn api_mock_autocomplete_anchor(&mut self) -> Option<(f32, f32)> {
        let focus = self.ide_panel.api.focused.as_ref()?;
        let (id, multiline) = self.api_focus_ui_target(focus)?;
        let rect = self.ui_registry.rect_for(id)?;
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_idx = text[..line_start].bytes().filter(|b| *b == b'\n').count();
        let x = rect.0
            + 10.0 * scale
            + renderer.measure_ui_width(&text[line_start..cursor], API_BODY_TEXT_SCALE);
        let y = if multiline {
            rect.1 + 10.0 * scale + line_idx as f32 * api_text_area_line_height(scale)
        } else {
            rect.1 + rect.3 * 0.55
        };
        Some((x, y))
    }

    pub(crate) fn request_api_mock_ty_autocomplete(&mut self, trigger: Option<&str>) {
        let Some((route_idx, part)) = self.api_mock_python_focus_target() else {
            return;
        };
        let prefix = self.api_input_current_word_prefix();
        if trigger.is_none() && prefix.is_empty() && !self.api_input_after_python_member_dot() {
            self.close_autocomplete();
            return;
        }
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        let virtual_source = build_api_mock_virtual_source(method, &path, &route, &model, &script);
        let edit_text = self.ide_panel.api.input_editor.get_full_text();
        let source_cursor = virtual_source.edit_offset_to_source(
            part,
            &edit_text,
            self.ide_panel.api.input_editor.cursor,
        );
        let mut line_offsets = vec![0usize];
        for (idx, b) in virtual_source.source.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(idx + 1);
            }
        }
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&virtual_source.source, source_cursor, &line_offsets);
        let path = Self::api_mock_virtual_path(route_idx);
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        lsp.notify_change(
            &path,
            "py",
            &virtual_source.source,
            self.ide_panel.api.input_editor.version as i32,
        );
        if let Some(id) = lsp.request_ty_completion(&path, "py", line, col, trigger) {
            self.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
            self.autocomplete_pending_request_id = Some(id);
            self.autocomplete_pending_request_mode = None;
            self.autocomplete_pending_request_path = None;
            self.autocomplete_pending_context_key = None;
            self.autocomplete_apply_pending_response = false;
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
            }
            self.autocomplete_anchor = self.api_mock_autocomplete_anchor();
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
        }
    }

    pub(crate) fn update_api_mock_ty_autocomplete(
        &mut self,
        items: Vec<crate::lsp::LspCompletionItem>,
    ) {
        if self.api_mock_python_focus_target().is_none() {
            return;
        }
        let prefix = self.api_input_current_word_prefix();
        if prefix.is_empty() && !self.api_input_after_python_member_dot() {
            self.close_autocomplete();
            return;
        }
        let prefix_lower = prefix.to_lowercase();
        let mut out = Vec::new();
        for item in items.into_iter().take(120) {
            let item: crate::app::AutocompleteItem = item.into();
            if prefix_lower.is_empty() || item.word.to_lowercase().contains(&prefix_lower) {
                if let Some(indices) = Self::api_mock_completion_indices(&prefix, &item.word) {
                    out.push((item, indices));
                }
            }
        }
        out.sort_unstable_by_key(|(item, _)| {
            let lower = item.word.to_lowercase();
            (
                !lower.starts_with(&prefix_lower),
                matches!(item.kind, crate::highlighter::SymbolKind::Unknown),
                item.word.len(),
            )
        });
        out.truncate(60);
        self.autocomplete_options = out;
        self.autocomplete_active = !self.autocomplete_options.is_empty();
        if !self.autocomplete_active {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            return;
        }
        self.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.autocomplete_anchor = self.api_mock_autocomplete_anchor();
    }

    pub(crate) fn apply_api_mock_autocomplete(&mut self) -> bool {
        let Some((route_idx, _)) = self.api_mock_python_focus_target() else {
            return false;
        };
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return true;
        }
        let item = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .clone();
        let selected = item
            .insert_text
            .as_deref()
            .or_else(|| item.text_edit.as_ref().map(|edit| edit.new_text.as_str()))
            .unwrap_or(&item.word)
            .to_string();
        let prefix_len = self.api_input_current_word_prefix().len();
        for _ in 0..prefix_len {
            self.ide_panel.api.input_editor.backspace();
        }
        let _ = self.ide_panel.api.input_editor.insert_str(&selected);
        if !item.additional_text_edits.is_empty()
            && let Some(script) = self.api_route_python_script_mut(route_idx)
        {
            for edit in item.additional_text_edits {
                let text = edit.new_text.trim_matches(|c| c == '\n' || c == '\r');
                if text.starts_with("import ") || text.starts_with("from ") {
                    if !script.prelude.trim().is_empty() && !script.prelude.ends_with('\n') {
                        script.prelude.push('\n');
                    }
                    script.prelude.push_str(text);
                    script.prelude.push('\n');
                }
            }
        }
        self.commit_api_focus();
        self.queue_api_mock_python_tools(route_idx);
        self.close_autocomplete();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }

    pub fn handle_api_client_keyboard_input(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        if key_event.state == winit::event::ElementState::Pressed
            && ctrl
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit4)
            && self.active_tab_is_api_client()
        {
            self.close_tab_at(self.active_tab);
            return true;
        }
        if self.ide_panel.api.focused.is_none() {
            return self.active_tab_is_api_client();
        }
        let active = self
            .active_api_tab()
            .map(|(meta, state)| (meta.spec_id, state.route_idx));
        if !self.ide_panel.api.clear_stale_keyboard_focus(active) {
            return false;
        }
        if key_event.state != winit::event::ElementState::Pressed {
            return true;
        }
        let shift = self.modifiers.shift_key();
        if self.api_mock_python_focus_target().is_some() && self.autocomplete_active {
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.close_autocomplete();
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown) => {
                    let len = self.autocomplete_options.len();
                    if len > 0 {
                        self.autocomplete_selected_idx = (self.autocomplete_selected_idx + 1) % len;
                        self.ensure_autocomplete_visible();
                    }
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp) => {
                    let len = self.autocomplete_options.len();
                    if len > 0 {
                        self.autocomplete_selected_idx = if self.autocomplete_selected_idx == 0 {
                            len.saturating_sub(1)
                        } else {
                            self.autocomplete_selected_idx.saturating_sub(1)
                        };
                        self.ensure_autocomplete_visible();
                    }
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(
                    winit::keyboard::KeyCode::Enter
                    | winit::keyboard::KeyCode::NumpadEnter
                    | winit::keyboard::KeyCode::Tab,
                ) => {
                    self.apply_api_mock_autocomplete();
                    return true;
                }
                _ => {}
            }
        }
        let mock_python_target = self.api_mock_python_focus_target();
        let input_version_before = self.ide_panel.api.input_editor.version;
        let mut typed_text: Option<String> = None;
        let is_body = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Body { .. }
                    | ApiFocus::MockPrelude { .. }
                    | ApiFocus::MockBody { .. }
                    | ApiFocus::MockStaticResponse { .. }
            )
        );
        let is_signature = matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::MockSignature { .. })
        );
        let is_response = matches!(self.ide_panel.api.focused, Some(ApiFocus::Response { .. }));
        let is_readonly = is_response || is_signature;
        let is_array = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.api_focus_is_array_input(focus));
        match key_event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Tab) => {
                self.focus_next_api_input(shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
            | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                if matches!(self.ide_panel.api.focused, Some(ApiFocus::ImportUrl)) {
                    self.commit_api_focus();
                    self.start_api_url_import_from_input();
                } else if is_array {
                    finish_api_array_editor_draft(&mut self.ide_panel.api.input_editor);
                } else if is_signature {
                } else if mock_python_target.is_some() {
                    let _ = self.ide_panel.api.input_editor.insert_str("\n");
                    typed_text = Some("\n".to_string());
                } else if is_body && shift {
                    let _ = self.ide_panel.api.input_editor.insert_str("\n");
                } else {
                    self.commit_api_focus();
                    self.ide_panel.api.focused = None;
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA) if ctrl => {
                self.ide_panel.api.input_editor.select_all();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyW)
                if ctrl && mock_python_target.is_some() =>
            {
                let text = self.ide_panel.api.input_editor.get_full_text();
                if let Some((start, end)) = crate::highlighter::ast_select_expand_range(
                    &text,
                    "py",
                    self.ide_panel.api.input_editor.cursor,
                    self.ide_panel.api.input_editor.selection_anchor,
                ) {
                    self.ide_panel.api.input_editor.selection_anchor = Some(start);
                    self.ide_panel.api.input_editor.cursor = end;
                } else {
                    self.ide_panel.api.input_editor.select_expand();
                }
                self.close_autocomplete();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.ide_panel.api.input_editor.get_selection() {
                    self.set_clipboard_text(text);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) if ctrl => {
                if !is_readonly && let Some(text) = self.ide_panel.api.input_editor.get_selection()
                {
                    self.set_clipboard_text(text);
                    self.ide_panel.api.input_editor.delete_selection();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) if ctrl => {
                if !is_readonly && let Some(text) = self.get_clipboard_text() {
                    let clean = if is_body {
                        text
                    } else if is_array {
                        text.replace('\r', "")
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                    typed_text = Some(clean);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                if ctrl && shift && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                if ctrl && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.undo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY)
                if ctrl && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
                if is_readonly {
                } else if is_array && !ctrl {
                    backspace_api_array_editor(&mut self.ide_panel.api.input_editor);
                } else if ctrl {
                    self.ide_panel.api.input_editor.delete_word_backward();
                } else {
                    self.ide_panel.api.input_editor.backspace();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
                if is_readonly {
                } else if ctrl {
                    self.ide_panel.api.input_editor.delete_word_forward();
                } else {
                    self.ide_panel.api.input_editor.delete_forward();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft) => {
                if ctrl {
                    self.ide_panel.api.input_editor.move_word_left(shift);
                } else {
                    self.ide_panel.api.input_editor.move_left(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
                if ctrl {
                    self.ide_panel.api.input_editor.move_word_right(shift);
                } else {
                    self.ide_panel.api.input_editor.move_right(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp)
                if is_body || is_readonly =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, false, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown)
                if is_body || is_readonly =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, true, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home) => {
                self.ide_panel.api.input_editor.move_home(shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End) => {
                self.ide_panel.api.input_editor.move_end(shift);
            }
            _ if !is_readonly
                && !ctrl
                && !self.modifiers.alt_key()
                && !self.modifiers.super_key() =>
            {
                if let Some(text) = key_event
                    .text
                    .as_ref()
                    .and_then(|s| (!s.is_empty()).then_some(s))
                {
                    let clean = if is_body {
                        text.to_string()
                    } else if is_array {
                        text.replace('\r', "")
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                    typed_text = Some(clean);
                }
            }
            _ => {}
        }
        if let Some((route_idx, _)) = mock_python_target
            && self.ide_panel.api.input_editor.version != input_version_before
        {
            self.queue_api_mock_python_tools(route_idx);
            if let Some(text) = typed_text.as_deref()
                && (text == "." || text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            {
                self.request_api_mock_ty_autocomplete((text == ".").then_some("."));
            } else if self.autocomplete_active {
                self.request_api_mock_ty_autocomplete(None);
            }
        }
        if let Some((id, multiline)) = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .and_then(|focus| self.api_focus_ui_target(focus))
        {
            if multiline {
                self.sync_api_multiline_scroll_target(id, false);
            } else if !is_array {
                self.sync_api_one_line_scroll_target(false);
            }
        }
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }

    pub fn start_active_api_request(&mut self) {
        self.commit_api_focus();
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        if let Some(ApiClientRouteIdentity::Manual { stable_id }) = &meta.route_identity {
            self.start_active_manual_api_request(stable_id.clone());
            return;
        }
        let spec_id = meta.spec_id;
        let requested_route_idx = state.route_idx;
        let needs_input_sync = requested_route_idx.is_none()
            || (state.path_values.is_empty()
                && state.query_values.is_empty()
                && state.body_values.is_empty()
                && state.body_json == ApiClientTabState::default().body_json);
        let route_idx = {
            let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
                return;
            };
            let route_idx = requested_route_idx.unwrap_or(0);
            if model.routes.get(route_idx).is_none() {
                return;
            }
            route_idx
        };
        if needs_input_sync {
            self.sync_api_tab_inputs(spec_id, route_idx);
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.route_idx = Some(route_idx);
            }
        }
        let Some((_, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return;
        };
        let Some(route) = model.routes.get(route_idx) else {
            return;
        };
        let Some(server) = model
            .servers
            .get(state.server_idx)
            .or_else(|| model.servers.first())
        else {
            return;
        };
        let method = route.method;
        let path = route.path.clone();
        let is_json_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| !body.is_multipart && !body.is_form_urlencoded);
        let is_multipart_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_multipart);
        let is_form_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_form_urlencoded);
        let path_values = state.path_values.clone();
        let query_values = state.query_values.clone();
        let body_values = state.body_values.clone();
        let body_json_text = state.body_json.clone();
        let server = server.clone();
        if route.method.can_send_body() && is_json_body && !json_body_is_valid(&body_json_text) {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.response = Some(ApiJobResponse {
                    request_id: 0,
                    spec_id,
                    route_idx,
                    status: None,
                    elapsed_ms: 0,
                    server_reach_ms: None,
                    timing_text: String::new(),
                    headers: Vec::new(),
                    headers_text: String::new(),
                    body: String::new(),
                    truncated: false,
                    error: Some(ApiLoadError::new(
                        ApiLoadErrorKind::InvalidJson,
                        "JSON body невалиден",
                    )),
                    resolved_host: None,
                });
            }
            return;
        }
        let mut url = match build_request_url(&server, &path, &path_values, &query_values) {
            Ok(url) => url,
            Err(err) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.response = Some(ApiJobResponse {
                        request_id: 0,
                        spec_id,
                        route_idx,
                        status: None,
                        elapsed_ms: 0,
                        server_reach_ms: None,
                        timing_text: String::new(),
                        headers: Vec::new(),
                        headers_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let auth_parts = prepared_auth_for_route(model, route, &self.ide_panel.api.auth);
        append_auth_query(&mut url, &auth_parts);
        let body_multipart = (method.can_send_body() && is_multipart_body)
            .then(|| api_multipart_parts_for_route(route, model, &body_values));
        let body_form = (method.can_send_body() && is_form_body).then_some(body_values);
        let body_json = (method.can_send_body() && is_json_body)
            .then_some(body_json_text)
            .filter(|body| !body.trim().is_empty());
        let request_id = self.ide_panel.api.next_request_id.max(1);
        self.ide_panel.api.next_request_id = request_id.saturating_add(1).max(1);
        let job = ApiJobRequest {
            request_id,
            spec_id,
            route_idx,
            method,
            resolved_host: resolve_api_url_host(&url),
            url,
            auth_parts,
            body_json,
            body_form,
            body_multipart,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.pending = true;
            state.pending_request_id = Some(request_id);
        }
        self.api_request_rx
            .push((request_id, spawn_api_request(job)));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn start_active_manual_api_request(&mut self, stable_id: String) {
        let Some((manual_idx, route)) = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .find(|(_, route)| route.stable_id == stable_id)
            .map(|(idx, route)| (idx, route.clone()))
        else {
            return;
        };
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        let spec_id = meta.spec_id;
        let server = ApiServer {
            url: api_mock_lan_url(&self.ide_panel.api.mock),
            description: String::new(),
            variables: Vec::new(),
        };
        let url = match build_request_url(&server, &route.path, &[], &[]) {
            Ok(url) => url,
            Err(err) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.route_idx = Some(manual_idx);
                    state.response = Some(ApiJobResponse {
                        request_id: 0,
                        spec_id,
                        route_idx: manual_idx,
                        status: None,
                        elapsed_ms: 0,
                        server_reach_ms: None,
                        timing_text: String::new(),
                        headers: Vec::new(),
                        headers_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let request_id = self.ide_panel.api.next_request_id.max(1);
        self.ide_panel.api.next_request_id = request_id.saturating_add(1).max(1);
        let job = ApiJobRequest {
            request_id,
            spec_id,
            route_idx: manual_idx,
            method: route.method,
            resolved_host: resolve_api_url_host(&url),
            url,
            auth_parts: Vec::new(),
            body_json: None,
            body_form: None,
            body_multipart: None,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.route_idx = Some(manual_idx);
            state.pending = true;
            state.pending_request_id = Some(request_id);
        }
        self.api_request_rx
            .push((request_id, spawn_api_request(job)));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn poll_api_client(&mut self) -> bool {
        let mut changed = false;
        let events = drain_api_mock_server_events();
        if !events.is_empty() {
            for event in events {
                match event {
                    ApiMockServerEvent::Log { text } => {
                        push_api_mock_server_log(&mut self.ide_panel.api, text);
                    }
                    ApiMockServerEvent::Request {
                        method,
                        path,
                        status,
                        action,
                    } => {
                        push_api_mock_server_log(
                            &mut self.ide_panel.api,
                            format!("{method} {path} -> {status} · {action}"),
                        );
                    }
                    other => {
                        push_api_mock_server_log(
                            &mut self.ide_panel.api,
                            api_mock_server_event_text(&other),
                        );
                        apply_api_mock_server_event(
                            &mut self.ide_panel.api.mock.server_status,
                            other,
                        );
                    }
                }
            }
            changed = true;
        }
        if self.ensure_active_api_mock_highlight() {
            changed = true;
        }
        if let Some((route_idx, part, version)) = self.ide_panel.api.mock_highlight_target
            && self.ide_panel.api.mock_highlighter.poll(version)
        {
            let spans = self.ide_panel.api.mock_highlighter.spans.clone();
            if let Some((method, path, route, model)) = self.api_mock_route_context(route_idx)
                && let Some(script) = self.api_mock_script_for_tools(route_idx)
            {
                let virtual_source =
                    build_api_mock_virtual_source(method, &path, &route, &model, &script);
                for cache_part in [
                    ApiMockSourcePart::Prelude,
                    ApiMockSourcePart::Signature,
                    ApiMockSourcePart::Body,
                ] {
                    let edit_spans =
                        Self::map_api_mock_spans_to_edit(&spans, &virtual_source, cache_part);
                    self.ide_panel
                        .api
                        .mock_highlight_cache
                        .insert((route_idx, cache_part), edit_spans);
                }
                self.ide_panel.api.mock_highlight_spans = self
                    .ide_panel
                    .api
                    .mock_highlight_cache
                    .get(&(route_idx, part))
                    .cloned()
                    .unwrap_or_default();
            }
            changed = true;
        }
        if self.api_mock_ty_rx.is_none()
            && let Some(due) = self.ide_panel.api.mock_ty_due
        {
            if Instant::now() >= due {
                self.ide_panel.api.mock_ty_due = None;
                if let Some((route_idx, _)) = self.api_mock_python_focus_target() {
                    let version = self.ide_panel.api.input_editor.version;
                    self.start_api_mock_ty_check_now(route_idx, version);
                }
            }
            changed = true;
        }
        if let Some(rx) = self.api_mock_ty_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if self.ide_panel.api.mock_ty_pending
                        == Some((result.route_idx, result.version))
                    {
                        self.ide_panel.api.mock_ty_pending = None;
                        self.ide_panel.api.mock_ty_diagnostics = result.diagnostics;
                        self.ide_panel.api.mock.check_status = if result.ok {
                            crate::app::api_mock::types::ApiMockCheckStatus::Ok {
                                route_idx: result.route_idx,
                                version: result.version,
                                message: result.message,
                            }
                        } else {
                            crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                                route_idx: result.route_idx,
                                version: result.version,
                                message: result.message,
                            }
                        };
                        changed = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.api_mock_ty_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.mock_ty_diagnostics.clear();
                    self.ide_panel.api.mock.check_status =
                        crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                            route_idx: 0,
                            version: 0,
                            message: "Ty check worker stopped".to_string(),
                        };
                    self.ide_panel.api.mock_ty_pending = None;
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.body_json_validation_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if self.ide_panel.api.body_json_validation_pending
                        == Some((result.spec_id, result.route_idx, result.version))
                    {
                        self.ide_panel.api.body_json_validation_pending = None;
                    }
                    self.ide_panel.api.body_json_validation = Some(ApiJsonValidationState {
                        spec_id: result.spec_id,
                        route_idx: result.route_idx,
                        version: result.version,
                        valid: result.valid,
                    });
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.body_json_validation_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.body_json_validation_rx = Some(rx);
                }
            }
        }
        if let Some(rx) = &self.api_import_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.api_import_file_rx = None;
                if let Some(path) = result {
                    self.start_api_local_import(path);
                }
                changed = true;
            }
        }
        if let Some(rx) = &self.api_body_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.api_body_file_rx = None;
                self.apply_api_body_file_pick(result);
                changed = true;
            }
        }
        if let Some(rx) = self.ide_panel.api.python_path_pick_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if let Some(path) = result.path {
                        match result.kind {
                            ApiPythonPathPickKind::Uv => {
                                self.ide_panel.api.mock.uv.configured_path = Some(path);
                                crate::app::api_mock::python_bootstrap::refresh_uv_status(
                                    &mut self.ide_panel.api.mock.uv,
                                );
                            }
                            ApiPythonPathPickKind::CustomPython => {
                                self.ide_panel.api.mock.uv.custom_python_path = Some(path);
                                crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(
                                    &mut self.ide_panel.api.mock.uv,
                                );
                            }
                        }
                        self.ide_panel.api.persist();
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.python_path_pick_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.python_version_list_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    self.ide_panel.api.mock_python_versions_loading = false;
                    if let Some(error) = result.error {
                        self.ide_panel.api.mock.uv.last_error = error;
                    } else {
                        self.ide_panel.api.mock_python_versions = result.rows;
                        self.ide_panel.api.mock.uv.last_error.clear();
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.python_version_list_rx = Some(rx);
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.mock_python_versions_loading = false;
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.python_install_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(ApiPythonInstallEvent::Line(line)) => {
                        push_api_python_install_log(&mut self.ide_panel.api, line);
                        changed = true;
                    }
                    Ok(ApiPythonInstallEvent::Done(result)) => {
                        self.ide_panel.api.mock_python_install_running = false;
                        keep = false;
                        match result {
                            Ok(()) => {
                                self.ide_panel.api.mock.uv.status =
                                    crate::app::api_mock::types::ApiPythonRuntimeStatus::Ready;
                                self.ide_panel.api.mock.uv.last_error.clear();
                                push_api_python_install_log(
                                    &mut self.ide_panel.api,
                                    ApiPythonInstallLogLine {
                                        text: "Готово".to_string(),
                                        kind: ApiPythonInstallLogKind::Ok,
                                    },
                                );
                            }
                            Err(err) => {
                                self.ide_panel.api.mock.uv.status =
                                    crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid;
                                self.ide_panel.api.mock.uv.last_error = err.clone();
                                push_api_python_install_log(
                                    &mut self.ide_panel.api,
                                    ApiPythonInstallLogLine {
                                        text: err,
                                        kind: ApiPythonInstallLogKind::Error,
                                    },
                                );
                            }
                        }
                        self.ide_panel.api.persist();
                        changed = true;
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.ide_panel.api.mock_python_install_running = false;
                        keep = false;
                        changed = true;
                        break;
                    }
                }
            }
            if keep && self.ide_panel.api.mock_python_install_running {
                self.ide_panel.api.python_install_rx = Some(rx);
                changed = true;
            }
        }

        let mut idx = 0usize;
        while idx < self.api_load_rx.len() {
            match self.api_load_rx[idx].try_recv() {
                Ok(result) => {
                    self.api_load_rx.remove(idx);
                    match result.result {
                        Ok(payload) => {
                            let id = payload.entry.id;
                            self.ide_panel.api.upsert_loaded(payload);
                            self.update_api_tabs_after_model_load(id);
                        }
                        Err(err) => self.ide_panel.api.mark_load_error(result.id, err),
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => idx += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.api_load_rx.remove(idx);
                    changed = true;
                }
            }
        }

        let mut idx = 0usize;
        while idx < self.api_request_rx.len() {
            let request_id = self.api_request_rx[idx].0;
            match self.api_request_rx[idx].1.try_recv() {
                Ok(result) => {
                    self.api_request_rx.remove(idx);
                    self.apply_api_job_response(result);
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => idx += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.api_request_rx.remove(idx);
                    self.clear_api_pending_request(request_id);
                    changed = true;
                }
            }
        }
        changed
    }

    fn update_api_tabs_after_model_load(&mut self, id: ApiSpecId) {
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == id
            {
                if let Some(entry) = self.ide_panel.api.specs.iter().find(|entry| entry.id == id) {
                    meta.title = entry.title.clone();
                    tab.base_title = entry.title.clone();
                }
                if let Some(model) = self.ide_panel.api.models.get(&id)
                    && !model.routes.is_empty()
                {
                    let route_idx = state.route_idx.unwrap_or(0).min(model.routes.len() - 1);
                    state.route_idx = Some(route_idx);
                    if state.path_values.is_empty()
                        && state.query_values.is_empty()
                        && state.body_values.is_empty()
                        && state.body_json == ApiClientTabState::default().body_json
                    {
                        fill_api_tab_inputs(state, &model.routes[route_idx], model);
                    }
                }
            }
        }
    }

    fn apply_api_job_response(&mut self, result: ApiJobResponse) {
        let resolved = result.resolved_host.clone();
        let focused_response = matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::Response { spec_id, route_idx })
                if spec_id == result.spec_id && route_idx == result.route_idx
        );
        let mut focused_text = None;
        let mut applied = false;
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == result.spec_id
            {
                if state.route_idx == Some(result.route_idx)
                    && state.pending_request_id == Some(result.request_id)
                {
                    state.pending = false;
                    state.pending_request_id = None;
                    state.response_scroll.current = 0.0;
                    state.response_scroll.target = 0.0;
                    state.response_scroll_x.current = 0.0;
                    state.response_scroll_x.target = 0.0;
                    if focused_response {
                        focused_text =
                            Some(api_response_text(&result, state.response_view).to_string());
                    }
                    state.response = Some(result.clone());
                    applied = true;
                    break;
                }
                if let Some(saved) = state.route_states.iter_mut().find(|saved| {
                    saved.route_idx == result.route_idx
                        && saved.pending_request_id == Some(result.request_id)
                }) {
                    saved.pending = false;
                    saved.pending_request_id = None;
                    saved.response = Some(result.clone());
                    applied = true;
                    break;
                }
            }
        }
        if !applied {
            return;
        }
        if let Some(resolved) = resolved {
            self.ide_panel.api.last_resolved_host = Some(resolved);
            self.ide_panel.api.persist();
        }
        if let Some(text) = focused_text {
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&text);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        }
    }

    fn clear_api_pending_request(&mut self, request_id: u64) {
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind
                && state.pending_request_id == Some(request_id)
            {
                state.pending = false;
                state.pending_request_id = None;
                break;
            }
            if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind
                && let Some(saved) = state
                    .route_states
                    .iter_mut()
                    .find(|saved| saved.pending_request_id == Some(request_id))
            {
                saved.pending = false;
                saved.pending_request_id = None;
                break;
            }
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn persist_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo API", "version": "1.2.3"},
            "servers": [
                {"url": "https://api.example.com/{version}", "variables": {"version": {"default": "v1"}}}
            ],
            "components": {
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": {"type": "string"},
                            "age": {"type": "integer"}
                        }
                    }
                }
            },
            "paths": {
                "/pets/{id}": {
                    "get": {
                        "tags": ["pets"],
                        "summary": "Read pet",
                        "parameters": [
                            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}},
                            {"name": "verbose", "in": "query", "schema": {"type": "boolean"}}
                        ],
                        "responses": {"200": {"description": "ok"}}
                    },
                    "post": {
                        "tags": ["pets"],
                        "requestBody": {
                            "content": {
                                "application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}
                            }
                        },
                        "responses": {"201": {"description": "created"}}
                    }
                }
            }
        })
    }

    fn form_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Form API", "version": "1.0.0"},
            "paths": {
                "/token": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["username"],
                                        "properties": {
                                            "username": {"type": "string", "maxLength": 500},
                                            "password": {"type": "string"}
                                        }
                                    }
                                },
                                "application/json": {
                                    "schema": {"type": "object"}
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        })
    }

    fn auth_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Auth API", "version": "1.0.0"},
            "components": {
                "securitySchemes": {
                    "HeaderKey": {"type": "apiKey", "in": "header", "name": "X-API-Key"},
                    "QueryKey": {"type": "apiKey", "in": "query", "name": "api_key"},
                    "CookieKey": {"type": "apiKey", "in": "cookie", "name": "session"},
                    "BasicAuth": {"type": "http", "scheme": "basic"},
                    "BearerJwt": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
                    "DigestAuth": {"type": "http", "scheme": "digest"},
                    "OAuthAll": {
                        "type": "oauth2",
                        "flows": {
                            "implicit": {"authorizationUrl": "/oauth/authorize", "scopes": {}},
                            "password": {"tokenUrl": "/oauth/token", "scopes": {}},
                            "clientCredentials": {"tokenUrl": "/oauth/token", "scopes": {}},
                            "authorizationCode": {
                                "authorizationUrl": "/oauth/authorize",
                                "tokenUrl": "/oauth/token",
                                "scopes": {}
                            }
                        }
                    },
                    "Oidc": {
                        "type": "openIdConnect",
                        "openIdConnectUrl": "/.well-known/openid-configuration"
                    }
                }
            },
            "security": [
                {"HeaderKey": [], "BearerJwt": []},
                {"QueryKey": []}
            ],
            "paths": {
                "/items": {
                    "get": {
                        "responses": {"200": {"description": "ok"}}
                    }
                },
                "/basic": {
                    "get": {
                        "security": [{"BasicAuth": []}],
                        "responses": {"200": {"description": "ok"}}
                    }
                },
                "/public": {
                    "get": {
                        "security": [],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        })
    }

    #[test]
    fn stale_api_focus_clears_so_editor_ctrl_shortcuts_are_not_swallowed() {
        let mut state = ApiClientState::default();
        state.focused = Some(ApiFocus::Body {
            spec_id: ApiSpecId(1),
            route_idx: 0,
        });

        assert!(!state.clear_stale_keyboard_focus(Some((ApiSpecId(2), Some(0)))));
        assert_eq!(state.focused, None);

        state.focused = Some(ApiFocus::Response {
            spec_id: ApiSpecId(1),
            route_idx: 2,
        });
        assert!(state.clear_stale_keyboard_focus(Some((ApiSpecId(1), Some(2)))));
        assert!(state.focused.is_some());

        state.focused = Some(ApiFocus::ImportUrl);
        assert!(state.clear_stale_keyboard_focus(None));
    }

    #[test]
    fn api_input_vertical_arrows_move_cursor_and_shift_selects() {
        let mut editor = Editor::new(64);
        editor.insert_str("abc\ndefg\nhi");
        editor.cursor = 1;

        move_api_input_vertical(&mut editor, true, false);
        assert_eq!(editor.cursor, 5);
        assert_eq!(editor.selection_anchor, None);

        move_api_input_vertical(&mut editor, true, true);
        assert_eq!(editor.cursor, 10);
        assert_eq!(editor.selection_anchor, Some(5));

        move_api_input_vertical(&mut editor, false, false);
        assert_eq!(editor.cursor, 5);
        assert_eq!(editor.selection_anchor, None);
    }

    #[test]
    fn api_array_editor_uses_blocks_plus_draft() {
        assert_eq!(api_array_editor_text("alpha\nbeta"), "alpha\nbeta\n");
        assert_eq!(
            api_array_edit_parts("alpha\nbeta\ngam"),
            (vec!["alpha", "beta"], "gam")
        );

        let mut editor = Editor::new(64);
        editor.set_text_clean(&api_array_editor_text("alpha\nbeta"));
        editor.cursor = editor.len();
        editor.selection_anchor = Some(editor.cursor);
        editor.insert_str("gam");
        finish_api_array_editor_draft(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\ngam\n");

        backspace_api_array_editor(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\n");
        editor.insert_str("x");
        backspace_api_array_editor(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\n");
    }

    #[test]
    fn api_text_area_horizontal_scroll_uses_longest_line() {
        let max = api_text_area_max_scroll_x("short\nvery-long-line", 40.0, |line| {
            line.len() as f32 * 10.0
        });
        assert_eq!(max, 120.0);
        assert_eq!(
            api_text_area_max_scroll_x("tiny", 100.0, |line| line.len() as f32 * 10.0),
            0.0
        );
    }

    #[test]
    fn api_tab_prefill_uses_selected_restored_route() {
        let model = parse_openapi_model(ApiSpecId(9), &sample_spec()).expect("parse");
        let post_idx = model
            .routes
            .iter()
            .position(|route| route.method == ApiMethod::Post)
            .expect("post route");
        let mut state = ApiClientTabState {
            route_idx: Some(post_idx),
            ..Default::default()
        };

        fill_api_tab_inputs(&mut state, &model.routes[post_idx], &model);

        assert!(state.path_values.is_empty());
        assert!(state.query_values.is_empty());
        assert!(state.body_json.contains("\"name\": \"\""));
        assert!(state.body_json.contains("\"age\": 0"));
    }

    #[test]
    fn url_validation_rejects_bad_parts() {
        assert!(validate_api_url("https://example.com/openapi.json").is_ok());
        assert_eq!(
            validate_api_url("ftp://example.com/openapi.json")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidUrl
        );
        assert_eq!(
            validate_api_url("http://[:::1]").unwrap_err().kind,
            ApiLoadErrorKind::InvalidUrl
        );
        assert_eq!(
            validate_api_url("https://-bad.example/openapi.json")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidDomain
        );
        assert_eq!(
            validate_api_url("https://api.example.com/docs#post-/items")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidUrl
        );
    }

    #[test]
    fn parse_openapi_extracts_compact_routes_servers_and_schema() {
        let model = parse_openapi_model(ApiSpecId(7), &sample_spec()).expect("parse");
        assert_eq!(model.title, "Demo API");
        assert_eq!(model.version, "1.2.3");
        assert_eq!(model.openapi_version, "3.1.0");
        assert_eq!(model.servers.len(), 1);
        assert_eq!(model.routes.len(), 2);
        assert_eq!(model.routes[0].tag, "pets");
        assert_eq!(model.routes[0].method, ApiMethod::Get);
        assert_eq!(model.routes[1].method, ApiMethod::Post);
        assert_eq!(model.routes[0].path_params[0].name, "id");
        assert_eq!(model.routes[0].query_params[0].name, "verbose");
        assert!(!model.schema_arena.is_empty());
    }

    #[test]
    fn parse_openapi_parameter_array_item_ref_keeps_enum() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Bookings", "version": "1.0.0"},
            "components": {
                "schemas": {
                    "StateEnum": {
                        "type": "string",
                        "enum": ["CREATED", "ACCEPTED"],
                        "default": "CREATED"
                    }
                }
            },
            "paths": {
                "/car_washes/bookings": {
                    "get": {
                        "parameters": [
                            {
                                "name": "state_in",
                                "in": "query",
                                "schema": {
                                    "type": "array",
                                    "items": {"$ref": "#/components/schemas/StateEnum"}
                                }
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(12), &spec).expect("parse");
        let param = &model.routes[0].query_params[0];
        assert_eq!(param.name, "state_in");
        assert_eq!(param.primitive_type, ApiPrimitiveType::Array);
        assert_eq!(param.item_type, Some(ApiPrimitiveType::String));
        assert_eq!(param.default_value.as_deref(), Some("CREATED"));
        assert_eq!(param.enum_values, vec!["CREATED", "ACCEPTED"]);
    }

    #[test]
    fn parse_openapi_date_and_datetime_types_keep_examples() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Dates", "version": "1.0.0"},
            "paths": {
                "/events": {
                    "get": {
                        "parameters": [
                            {
                                "name": "day",
                                "in": "query",
                                "schema": {"type": "string", "format": "date", "example": "2026-05-25"}
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    },
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "starts_at": {
                                                "type": "string",
                                                "format": "date-time",
                                                "examples": ["2026-05-25T12:30:00Z"]
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(30), &spec).expect("parse");
        let param = &model.routes[0].query_params[0];
        assert_eq!(param.primitive_type, ApiPrimitiveType::Date);
        assert_eq!(param.examples, vec!["2026-05-25"]);

        let body = model.routes[1].request_body.as_ref().expect("body");
        let root = body.schema.expect("schema");
        let prop = model.schema_arena[root.0].properties[0].schema;
        let schema = &model.schema_arena[prop.0];
        assert_eq!(schema.kind, ApiSchemaKind::DateTime);
        assert_eq!(schema.examples, vec!["2026-05-25T12:30:00Z"]);
    }

    #[test]
    fn parse_openapi_security_schemes_and_operation_security() {
        let model = parse_openapi_model(ApiSpecId(11), &auth_spec()).expect("parse");
        assert_eq!(model.security_schemes.len(), 8);
        assert_eq!(model.root_security.len(), 2);
        let names = model
            .security_schemes
            .iter()
            .map(|scheme| scheme.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"HeaderKey"));
        assert!(names.contains(&"QueryKey"));
        assert!(names.contains(&"CookieKey"));
        assert!(names.contains(&"BasicAuth"));
        assert!(names.contains(&"BearerJwt"));
        assert!(names.contains(&"DigestAuth"));
        assert!(names.contains(&"OAuthAll"));
        assert!(names.contains(&"Oidc"));
        assert!(model.security_schemes.iter().any(|scheme| matches!(
            scheme.kind,
            ApiSecuritySchemeKind::Http { ref scheme, ref bearer_format }
                if scheme == "bearer" && bearer_format == "JWT"
        )));
        assert!(model.security_schemes.iter().any(|scheme| matches!(
            scheme.kind,
            ApiSecuritySchemeKind::OAuth2 { ref flows }
                if flows == &vec![
                    ApiOAuthFlow::Implicit,
                    ApiOAuthFlow::Password,
                    ApiOAuthFlow::ClientCredentials,
                    ApiOAuthFlow::AuthorizationCode,
                ]
        )));
        let public = model
            .routes
            .iter()
            .find(|route| route.path == "/public")
            .expect("public route");
        assert_eq!(public.security, Some(Vec::new()));
    }

    #[test]
    fn auth_selection_respects_or_and_and_security_empty() {
        let model = parse_openapi_model(ApiSpecId(12), &auth_spec()).expect("parse");
        let items = model
            .routes
            .iter()
            .find(|route| route.path == "/items")
            .expect("items route");
        let public = model
            .routes
            .iter()
            .find(|route| route.path == "/public")
            .expect("public route");
        let mut auth = ApiAuthStore::default();
        assert_eq!(
            api_route_auth_scheme_indices(&model, items)
                .iter()
                .filter_map(|idx| model.security_schemes.get(*idx))
                .map(|scheme| scheme.name.as_str())
                .collect::<Vec<_>>(),
            vec!["BearerJwt", "HeaderKey", "QueryKey"]
        );
        assert!(api_route_auth_scheme_indices(&model, public).is_empty());
        assert!(api_route_auth_missing(&model, items, &auth));
        assert!(!api_route_auth_missing(&model, public, &auth));

        auth.entry_mut(model.id, "HeaderKey").value = "header-secret".to_string();
        auth.entry_mut(model.id, "QueryKey").value = "query-secret".to_string();
        assert!(!api_route_auth_missing(&model, items, &auth));

        let parts = prepared_auth_for_route(&model, items, &auth);
        assert_eq!(
            parts,
            vec![ApiPreparedAuthPart::Query {
                name: "api_key".to_string(),
                value: "query-secret".to_string(),
            }]
        );

        auth.entry_mut(model.id, "BearerJwt").access_token = "jwt".to_string();
        let parts = prepared_auth_for_route(&model, items, &auth);
        assert_eq!(parts.len(), 2);
        assert!(parts.contains(&ApiPreparedAuthPart::Header {
            name: "X-API-Key".to_string(),
            value: "header-secret".to_string(),
        }));
        assert!(parts.contains(&ApiPreparedAuthPart::Bearer {
            token: "jwt".to_string(),
        }));

        auth.entry_mut(model.id, "BearerJwt").value = "refresh".to_string();
        let parts = prepared_auth_for_route(&model, items, &auth);
        assert!(parts.contains(&ApiPreparedAuthPart::Bearer {
            token: "refresh".to_string(),
        }));

        assert!(prepared_auth_for_route(&model, public, &auth).is_empty());
    }

    #[test]
    fn auth_request_assembly_sets_headers_cookies_query_and_basic() {
        let mut url = "https://api.example.com/items".to_string();
        append_auth_query(
            &mut url,
            &[ApiPreparedAuthPart::Query {
                name: "api_key".to_string(),
                value: "q v".to_string(),
            }],
        );
        assert_eq!(url, "https://api.example.com/items?api_key=q+v");

        let client = reqwest::blocking::Client::new();
        let request = apply_auth_to_builder(
            client.get("https://api.example.com/items"),
            &[
                ApiPreparedAuthPart::Header {
                    name: "X-API-Key".to_string(),
                    value: "secret".to_string(),
                },
                ApiPreparedAuthPart::Cookie {
                    name: "session".to_string(),
                    value: "abc".to_string(),
                },
                ApiPreparedAuthPart::Bearer {
                    token: "jwt".to_string(),
                },
            ],
        )
        .build()
        .expect("request");
        assert_eq!(request.headers()["X-API-Key"], "secret");
        assert_eq!(request.headers()["Cookie"], "session=abc");
        assert_eq!(request.headers()["Authorization"], "Bearer jwt");

        let basic = apply_auth_to_builder(
            client.get("https://api.example.com/basic"),
            &[ApiPreparedAuthPart::Basic {
                username: "user".to_string(),
                password: "pass".to_string(),
            }],
        )
        .build()
        .expect("request");
        assert_eq!(basic.headers()["Authorization"], "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn auth_capture_saves_tokens_refresh_and_cookie_keys() {
        let model = parse_openapi_model(ApiSpecId(13), &auth_spec()).expect("parse");
        let mut auth = ApiAuthStore::default();
        let response = ApiJobResponse {
            request_id: 1,
            spec_id: model.id,
            route_idx: 0,
            status: Some(200),
            elapsed_ms: 1,
            server_reach_ms: None,
            timing_text: String::new(),
            headers: vec![(
                "set-cookie".to_string(),
                "session=cookie-secret; HttpOnly; Path=/".to_string(),
            )],
            headers_text: String::new(),
            body: serde_json::json!({
                "access_token": "access",
                "refresh_token": "refresh",
                "token_type": "Bearer",
                "expires_in": 60,
                "scope": "read write"
            })
            .to_string(),
            truncated: false,
            error: None,
            resolved_host: None,
        };

        assert!(capture_response_auth(
            &mut auth,
            model.id,
            &model.security_schemes,
            &response
        ));
        let bearer = auth.entry(model.id, "BearerJwt").expect("bearer auth");
        assert_eq!(bearer.access_token, "access");
        assert_eq!(bearer.refresh_token, "refresh");
        assert_eq!(bearer.value, "access");
        assert_eq!(bearer.scopes, vec!["read".to_string(), "write".to_string()]);
        assert!(bearer.expires_at.is_some());
        assert_eq!(
            auth.entry(model.id, "CookieKey")
                .expect("cookie auth")
                .value,
            "cookie-secret"
        );
    }

    #[test]
    fn api_auth_persist_roundtrip_uses_separate_file() {
        let _guard = persist_test_lock().lock().expect("lock");
        let _ = std::fs::remove_dir_all(api_config_dir());

        let mut auth = ApiAuthStore::default();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").access_token = "access".to_string();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").refresh_token = "refresh".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").username = "user".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").password = "pass".to_string();
        save_api_auth(&auth);

        let loaded = load_api_auth();
        assert_eq!(
            loaded
                .entry(ApiSpecId(7), "BearerJwt")
                .map(|entry| (entry.access_token.as_str(), entry.refresh_token.as_str())),
            Some(("access", "refresh"))
        );
        assert_eq!(
            loaded
                .entry(ApiSpecId(7), "BasicAuth")
                .map(|entry| (entry.username.as_str(), entry.password.as_str())),
            Some(("user", "pass"))
        );

        let _ = std::fs::remove_dir_all(api_config_dir());
    }

    #[test]
    fn api_method_display_and_sort_order_match_client_rows() {
        assert_eq!(ApiMethod::Get.chip_str(), "GET");
        assert_eq!(ApiMethod::Post.chip_str(), "POS");
        assert_eq!(ApiMethod::Patch.chip_str(), "PAT");
        assert_eq!(ApiMethod::Put.chip_str(), "PUT");
        assert_eq!(ApiMethod::Delete.chip_str(), "DEL");
        assert_eq!(ApiMethod::Head.chip_str(), "HEA");
        assert_eq!(ApiMethod::Options.chip_str(), "OPT");
        assert_eq!(ApiMethod::Trace.chip_str(), "TRA");

        let mut methods = [
            ApiMethod::Trace,
            ApiMethod::Put,
            ApiMethod::Get,
            ApiMethod::Delete,
            ApiMethod::Patch,
            ApiMethod::Options,
            ApiMethod::Post,
            ApiMethod::Head,
        ];
        methods.sort_unstable_by_key(|method| (*method).sort_rank());
        assert_eq!(
            methods,
            [
                ApiMethod::Get,
                ApiMethod::Post,
                ApiMethod::Patch,
                ApiMethod::Put,
                ApiMethod::Delete,
                ApiMethod::Head,
                ApiMethod::Options,
                ApiMethod::Trace,
            ]
        );
    }

    #[test]
    fn api_path_display_spaces_path_params_without_changing_path() {
        assert_eq!(
            format_api_path_display("/sites/{id}/complete"),
            "/sites/ {id} /complete"
        );
        assert_eq!(
            format_api_path_display("/orgs/{org_id}/sites/{site_id}"),
            "/orgs/ {org_id} /sites/ {site_id}"
        );
    }

    #[test]
    fn route_grouping_uses_sorted_tag_ranges() {
        let model = parse_openapi_model(ApiSpecId(1), &sample_spec()).expect("parse");
        let groups = grouped_route_ranges(&model.routes, &FxHashSet::default(), model.id);
        assert_eq!(groups, vec![("pets".to_string(), 0, 2, false)]);
    }

    #[test]
    fn json_validator_catches_trailing_comma() {
        assert!(json_body_is_valid(r#"{"a": 1}"#));
        assert!(!json_body_is_valid(r#"{"a": 1,}"#));
    }

    #[test]
    fn request_url_builder_applies_server_vars_path_and_query() {
        let server = ApiServer {
            url: "https://api.example.com/{version}".to_string(),
            description: String::new(),
            variables: vec![ApiServerVariable {
                name: "version".to_string(),
                default_value: "v1".to_string(),
            }],
        };
        let url = build_request_url(
            &server,
            "/pets/{id}",
            &[ApiInputValue {
                name: "id".to_string(),
                value: "a b".to_string(),
            }],
            &[ApiInputValue {
                name: "verbose".to_string(),
                value: "true".to_string(),
            }],
        )
        .expect("url");
        assert_eq!(url, "https://api.example.com/v1/pets/a%20b?verbose=true");
    }

    #[test]
    fn form_urlencoded_body_prefers_fields_over_json() {
        let model = parse_openapi_model(ApiSpecId(21), &form_spec()).expect("parse");
        let route = &model.routes[0];
        let body = route.request_body.as_ref().expect("body");
        assert_eq!(body.content_type, "application/x-www-form-urlencoded");
        assert!(body.is_form_urlencoded);
        assert!(!body.is_multipart);

        let mut state = ApiClientTabState::default();
        fill_api_tab_inputs(&mut state, route, &model);
        assert_eq!(state.body_json, "");
        assert_eq!(
            state.body_values,
            vec![
                ApiInputValue {
                    name: "username".to_string(),
                    value: String::new(),
                },
                ApiInputValue {
                    name: "password".to_string(),
                    value: String::new(),
                },
            ]
        );

        let fields = [
            ApiInputValue {
                name: "username".to_string(),
                value: "alice".to_string(),
            },
            ApiInputValue {
                name: "password".to_string(),
                value: String::new(),
            },
        ];
        let pairs = api_form_pairs(&fields);
        assert_eq!(pairs, vec![("username", "alice")]);
    }

    #[test]
    fn json_body_uses_first_schema_example() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Body Example", "version": "1.0.0"},
            "paths": {
                "/users": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "examples": [
                                            {"name": "Ada", "age": 37},
                                            {"name": "Grace", "age": 85}
                                        ],
                                        "properties": {
                                            "name": {"type": "string"},
                                            "age": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(44), &spec).expect("parse");
        let route = &model.routes[0];
        assert_eq!(
            default_body_for_route(route, &model),
            "{\"name\":\"Ada\",\"age\":37}"
        );
    }

    #[test]
    fn form_urlencoded_ref_body_uses_schema_property_order() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Auth", "version": "1.0.0"},
            "components": {
                "schemas": {
                    "Login": {
                        "type": "object",
                        "required": ["username", "password"],
                        "properties": {
                            "password": {"type": "string"},
                            "username": {"type": "string"}
                        }
                    }
                }
            },
            "paths": {
                "/jwt/login": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {"$ref": "#/components/schemas/Login"}
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(22), &spec).expect("parse");
        let mut state = ApiClientTabState::default();
        fill_api_tab_inputs(&mut state, &model.routes[0], &model);

        assert_eq!(state.body_values[0].name, "password");
        assert_eq!(state.body_values[1].name, "username");
    }

    #[test]
    fn form_and_multipart_field_rows_stay_compact() {
        let model = parse_openapi_model(ApiSpecId(24), &form_spec()).expect("parse");
        let route = &model.routes[0];
        let schema = route
            .request_body
            .as_ref()
            .and_then(|body| body.schema)
            .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            .expect("schema");
        let username = schema
            .properties
            .iter()
            .find(|prop| prop.name == "username")
            .and_then(|prop| model.schema_arena.get(prop.schema.0))
            .expect("username");
        assert_eq!(api_body_prop_row_height(username, &model, 1.0), 46.0);

        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Upload", "version": "1.0.0"},
            "paths": {
                "/upload": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "multipart/form-data": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "kind": {
                                                "type": "string",
                                                "enum": ["avatar", "cover", "doc"]
                                            },
                                            "file": {
                                                "type": "string",
                                                "format": "binary"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(25), &spec).expect("parse");
        let route = &model.routes[0];
        let schema = route
            .request_body
            .as_ref()
            .and_then(|body| body.schema)
            .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            .expect("schema");
        for prop in &schema.properties {
            let prop_schema = model.schema_arena.get(prop.schema.0).expect("prop");
            let expected = 46.0;
            assert_eq!(api_body_prop_row_height(prop_schema, &model, 1.0), expected);
        }
    }

    #[test]
    fn auth_view_focus_uses_single_token_field_and_routes_include_refresh_flow() {
        let model = parse_openapi_model(ApiSpecId(26), &auth_spec()).expect("parse");
        let state = ApiClientTabState {
            auth_view: true,
            ..Default::default()
        };
        let order = api_focus_order_for_view(model.id, &model, &state);
        assert!(order.contains(&ApiFocus::AuthValue {
            spec_id: model.id,
            scheme: "BearerJwt".to_string(),
        }));
        assert!(!order.contains(&ApiFocus::AuthRefreshToken {
            spec_id: model.id,
            scheme: "BearerJwt".to_string(),
        }));

        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "JWT", "version": "1.0.0"},
            "paths": {
                "/jwt/login": {"post": {"responses": {"200": {"description": "ok"}}}},
                "/jwt/refresh": {"post": {"responses": {"200": {"description": "ok"}}}},
                "/users": {"get": {"responses": {"200": {"description": "ok"}}}}
            }
        });
        let model = parse_openapi_model(ApiSpecId(27), &spec).expect("parse");
        assert_eq!(api_auth_related_route_count(&model), 2);
        assert_eq!(api_auth_route_rank(&model.routes[0]), Some(0));
        assert_eq!(api_auth_route_rank(&model.routes[1]), Some(1));
        assert_eq!(api_auth_route_rank(&model.routes[2]), None);
    }

    #[test]
    fn api_response_auth_token_detection_handles_access_or_refresh() {
        let response = ApiJobResponse {
            request_id: 1,
            spec_id: ApiSpecId(28),
            route_idx: 0,
            status: Some(200),
            elapsed_ms: 1,
            server_reach_ms: None,
            timing_text: String::new(),
            headers: Vec::new(),
            headers_text: String::new(),
            body: r#"{"access_token":"a"}"#.to_string(),
            truncated: false,
            resolved_host: None,
            error: None,
        };
        assert!(api_response_has_auth_tokens(&response));

        let response = ApiJobResponse {
            body: r#"{"refresh_token":"r"}"#.to_string(),
            ..response
        };
        assert!(api_response_has_auth_tokens(&response));
    }

    #[test]
    fn api_tab_keeps_response_when_switching_routes() {
        let mut state = ApiClientTabState {
            route_idx: Some(0),
            path_values: vec![ApiInputValue {
                name: "id".to_string(),
                value: "first".to_string(),
            }],
            response: Some(ApiJobResponse {
                request_id: 7,
                spec_id: ApiSpecId(29),
                route_idx: 0,
                status: Some(200),
                elapsed_ms: 3,
                server_reach_ms: None,
                timing_text: "3ms".to_string(),
                headers: Vec::new(),
                headers_text: String::new(),
                body: "{\"ok\":true}".to_string(),
                truncated: false,
                error: None,
                resolved_host: None,
            }),
            ..Default::default()
        };
        state.remember_route_state();
        state.route_idx = Some(1);
        state.path_values.clear();
        state.response = None;

        assert!(state.restore_route_state(0));
        assert_eq!(state.path_values[0].value, "first");
        assert_eq!(
            state
                .response
                .as_ref()
                .map(|response| response.body.as_str()),
            Some("{\"ok\":true}")
        );
    }

    #[test]
    fn api_focus_order_tabs_through_form_fields() {
        let model = parse_openapi_model(ApiSpecId(23), &form_spec()).expect("parse");
        let state = ApiClientTabState {
            route_idx: Some(0),
            ..Default::default()
        };
        let order = api_focus_order_for_view(model.id, &model, &state);

        assert_eq!(
            order,
            vec![
                ApiFocus::BodyField {
                    spec_id: model.id,
                    route_idx: 0,
                    name: "username".to_string(),
                },
                ApiFocus::BodyField {
                    spec_id: model.id,
                    route_idx: 0,
                    name: "password".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_openapi_rejects_missing_or_old_version() {
        assert_eq!(
            parse_openapi_model(ApiSpecId(1), &serde_json::json!({}))
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::UnsupportedOpenApi
        );
        assert_eq!(
            parse_openapi_model(
                ApiSpecId(1),
                &serde_json::json!({"openapi": "2.0", "paths": {}})
            )
            .unwrap_err()
            .message,
            "поддерживается OpenAPI 3.x"
        );
    }

    #[test]
    fn last_loaded_text_uses_now_then_minutes_without_seconds() {
        let now = now_epoch_secs();
        assert_eq!(
            format_last_loaded_at(Some(now.saturating_sub(30)), now),
            "только что"
        );
        assert_eq!(
            format_last_loaded_at(Some(now.saturating_sub(60)), now),
            "1 мин назад"
        );
        assert_eq!(format_last_loaded_at(None, now), "не загружено");
        assert!(api_timing_visible_at(Some(now.saturating_sub(9)), now));
        assert!(!api_timing_visible_at(Some(now.saturating_sub(10)), now));
        assert!(!api_timing_visible_at(None, now));
    }

    #[test]
    fn api_state_remove_spec_clears_model_loading_collapsed_and_selection() {
        let first = ApiSpecId(1);
        let second = ApiSpecId(2);
        let mut state = ApiClientState::default();
        state.specs.push(ApiSpecEntry {
            id: first,
            title: "One".to_string(),
            version: String::new(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/one.json".to_string()),
            last_loaded: Some(1),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: true,
            error: None,
        });
        state.specs.push(ApiSpecEntry {
            id: second,
            title: "Two".to_string(),
            version: String::new(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/two.json".to_string()),
            last_loaded: Some(2),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: false,
            error: None,
        });
        state.selected_spec = Some(first);
        state.models.insert(first, ApiSpecModel::default());
        state.loading.insert(first);
        state.collapsed_tags.insert((first, "pets".to_string()));

        assert_eq!(state.remove_spec(0), Some(first));
        assert_eq!(state.selected_spec, Some(second));
        assert!(!state.models.contains_key(&first));
        assert!(!state.loading.contains(&first));
        assert!(state.collapsed_tags.is_empty());
        assert!(state.specs[0].selected);
        assert_eq!(state.remove_spec(99), None);
    }

    #[test]
    fn api_specs_persist_roundtrip_keeps_imported_sources_and_selection() {
        let _guard = persist_test_lock().lock().expect("lock");
        let _ = std::fs::remove_dir_all(api_config_dir());

        let mut state = ApiClientState::default();
        state.next_id = 8;
        state.selected_spec = Some(ApiSpecId(7));
        state.specs.push(ApiSpecEntry {
            id: ApiSpecId(7),
            title: "Persisted".to_string(),
            version: "1.0".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/openapi.json".to_string()),
            last_loaded: Some(123),
            last_fetch_secs: Some(0.1234),
            last_parse_secs: Some(0.0456),
            last_url_status: Some(ApiUrlStatus::Ok(200)),
            selected: true,
            error: None,
        });
        save_url_cache(ApiSpecId(7), &sample_spec().to_string());
        state.persist();

        let loaded = ApiClientState::load_persisted();
        assert_eq!(loaded.next_id, 8);
        assert_eq!(loaded.selected_spec, Some(ApiSpecId(7)));
        assert_eq!(loaded.specs.len(), 1);
        assert_eq!(loaded.specs[0].title, "Persisted");
        assert_eq!(
            loaded.specs[0].source,
            ApiSpecSource::Url("https://example.com/openapi.json".to_string())
        );
        assert_eq!(loaded.specs[0].last_loaded, Some(123));
        assert_eq!(loaded.specs[0].last_fetch_secs, Some(0.1234));
        assert_eq!(loaded.specs[0].last_parse_secs, Some(0.0456));
        assert!(loaded.specs[0].selected);
        assert!(loaded.models.contains_key(&ApiSpecId(7)));
        assert!(loaded.loading.is_empty());

        let _ = std::fs::remove_dir_all(api_config_dir());
    }

    #[test]
    fn api_scroll_limits_are_finite_and_shrink_when_routes_collapsed() {
        let mut state = ApiClientState::default();
        let model = parse_openapi_model(ApiSpecId(5), &sample_spec()).expect("parse");
        state.specs.push(ApiSpecEntry {
            id: model.id,
            title: model.title.clone(),
            version: model.version.clone(),
            openapi_version: model.openapi_version.clone(),
            source: ApiSpecSource::Url("https://example.com/openapi.json".to_string()),
            last_loaded: Some(1),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: Some(ApiUrlStatus::Ok(200)),
            selected: true,
            error: None,
        });
        state.selected_spec = Some(model.id);
        state.models.insert(model.id, model.clone());

        let expanded = api_panel_max_scroll(&state, 120.0, 1.0);
        state.collapsed_tags.insert((model.id, "pets".to_string()));
        let collapsed = api_panel_max_scroll(&state, 120.0, 1.0);
        assert!(expanded.is_finite());
        assert!(collapsed.is_finite());
        assert!(collapsed < expanded);

        let tab_state = ApiClientTabState {
            route_idx: Some(0),
            response: Some(ApiJobResponse {
                request_id: 0,
                spec_id: model.id,
                route_idx: 0,
                status: Some(200),
                elapsed_ms: 1,
                server_reach_ms: Some(1),
                timing_text: "1 ms (~1 ms до сервера)".to_string(),
                headers: Vec::new(),
                headers_text: String::new(),
                body: "{}".to_string(),
                truncated: false,
                error: None,
                resolved_host: None,
            }),
            ..Default::default()
        };
        let tab_max = api_tab_max_scroll(Some(&model), &tab_state, 180.0, 1.0);
        assert!(tab_max.is_finite());
        assert!(tab_max > 0.0);
        assert_eq!(api_tab_max_scroll(None, &tab_state, 180.0, 1.0), 0.0);
    }
}
