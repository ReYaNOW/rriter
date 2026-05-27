use crate::app::api_mock::persist::{load_api_mocks, save_api_mocks};
use crate::app::api_mock::server::{
    apply_api_mock_server_event, drain_api_mock_server_events, start_api_mock_server,
    stop_api_mock_server,
};
use crate::app::api_mock::contract::api_mock_default_handler_body;
use crate::app::api_mock::ty_check::{
    ApiMockSourcePart, ApiMockTyDiagnostic, build_api_mock_virtual_source, spawn_api_mock_ty_check,
};
use crate::app::api_mock::types::ApiMockServerEvent;
use crate::app::api_mock::types::{
    ApiMockFieldConstraints, ApiMockState, default_contract_from_route,
    default_api_mock_python_body, default_api_mock_python_script, is_legacy_api_mock_python_body,
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
    pub constraints: ApiMockFieldConstraints,
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
    pub constraints: ApiMockFieldConstraints,
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
    MockContract {
        route_idx: usize,
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
    pub(crate) mock_hover_target: Option<ApiMockHoverTarget>,
    pub(crate) mock_hover_request: Option<ApiMockHoverRequest>,
    mock_lsp_opened: Option<(PathBuf, i32)>,
    pub mock_highlighter: Highlighter,
    pub mock_highlight_target: Option<(usize, ApiMockSourcePart, u64)>,
    pub mock_highlight_spans: Vec<ColorSpan>,
    pub mock_highlight_cache: FxHashMap<(usize, ApiMockSourcePart), Vec<ColorSpan>>,
    pub mock_python_scrolls: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    pub mock_python_scrolls_x: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    pub(crate) mock_python_editors: FxHashMap<(usize, ApiMockSourcePart), Editor>,
    pub last_resolved_host: Option<ApiResolvedHost>,
    body_json_validation: Option<ApiJsonValidationState>,
    body_json_validation_pending: Option<(ApiSpecId, usize, u64)>,
    body_json_validation_rx: Option<Receiver<ApiJsonValidationResult>>,
    python_version_list_rx: Option<Receiver<ApiPythonVersionListResult>>,
    python_install_rx: Option<Receiver<ApiPythonInstallEvent>>,
    python_path_pick_rx: Option<Receiver<ApiPythonPathPickResult>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApiMockHoverTarget {
    pub route_idx: usize,
    pub part: ApiMockSourcePart,
    pub edit_byte: usize,
    pub version: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ApiMockHoverRequest {
    pub request_id: i32,
    pub target: ApiMockHoverTarget,
    pub source: String,
    pub source_cursor: usize,
    pub anchor: (f32, f32),
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
            mock_hover_target: None,
            mock_hover_request: None,
            mock_lsp_opened: None,
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
        ApiFocus::MockContract { .. }
        | ApiFocus::MockPrelude { .. }
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

include!("api_client/api_client_loading_parser.rs");
include!("api_client/api_client_layout_input.rs");
include!("api_client/api_client_request_runtime.rs");
include!("api_client/api_client_app_text_methods.rs");
include!("api_client/api_client_app_focus_methods.rs");
include!("api_client/api_client_app_click_methods.rs");
include!("api_client/api_client_app_mock_contract_methods.rs");
include!("api_client/api_client_app_mock_methods.rs");
include!("api_client/api_client_app_request_methods.rs");
include!("api_client/api_client_defaults_persist.rs");
include!("api_client/api_client_tests.rs");
