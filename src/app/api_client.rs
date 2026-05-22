use crate::editor::Editor;
use crate::scroll::ScrollState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::{Host, Url};

pub const API_FETCH_TIMEOUT: Duration = Duration::from_secs(12);
pub const API_MAX_SPEC_BYTES: usize = 8 * 1024 * 1024;
pub const API_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const API_SCHEMA_MAX_DEPTH: usize = 12;
const API_SCHEMA_MAX_COUNT: usize = 768;
const API_SCHEMA_MAX_PROPERTIES: usize = 160;
const API_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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
    pub path_params: Vec<ApiParam>,
    pub query_params: Vec<ApiParam>,
    pub request_body: Option<ApiRequestBody>,
    pub responses: Vec<ApiResponseSummary>,
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
    pub default_value: Option<String>,
    pub example: Option<String>,
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
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Unknown,
}

impl ApiPrimitiveType {
    fn from_schema(schema: Option<&Value>) -> Self {
        let Some(schema) = schema else {
            return Self::Unknown;
        };
        match schema.get("type").and_then(Value::as_str) {
            Some("string") => Self::String,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiSchemaRef(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSchema {
    pub name: String,
    pub kind: ApiSchemaKind,
    pub properties: Vec<ApiSchemaProperty>,
    pub item: Option<ApiSchemaRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiSchemaKind {
    Object,
    Array,
    String,
    Integer,
    Number,
    Boolean,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSchemaProperty {
    pub name: String,
    pub required: bool,
    pub schema: ApiSchemaRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponseSummary {
    pub status: String,
    pub description: String,
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
}

#[derive(Clone, Debug)]
pub struct ApiClientTabState {
    pub route_idx: Option<usize>,
    pub server_idx: usize,
    pub path_values: Vec<ApiInputValue>,
    pub query_values: Vec<ApiInputValue>,
    pub body_json: String,
    pub response: Option<ApiJobResponse>,
    pub pending: bool,
    pub tab_scroll: ScrollState,
}

impl Default for ApiClientTabState {
    fn default() -> Self {
        Self {
            route_idx: None,
            server_idx: 0,
            path_values: Vec::new(),
            query_values: Vec::new(),
            body_json: "{\n  \n}".to_string(),
            response: None,
            pending: false,
            tab_scroll: ScrollState::new(7.0),
        }
    }
}

impl PartialEq for ApiClientTabState {
    fn eq(&self, other: &Self) -> bool {
        self.route_idx == other.route_idx
            && self.server_idx == other.server_idx
            && self.path_values == other.path_values
            && self.query_values == other.query_values
            && self.body_json == other.body_json
            && self.response == other.response
            && self.pending == other.pending
    }
}

impl Eq for ApiClientTabState {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiFocus {
    ImportUrl,
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
    Body {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    Response {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
}

pub struct ApiClientState {
    pub specs: Vec<ApiSpecEntry>,
    pub models: FxHashMap<ApiSpecId, ApiSpecModel>,
    pub selected_spec: Option<ApiSpecId>,
    pub next_id: u64,
    pub import_menu_open: bool,
    pub import_url_open: bool,
    pub import_error: Option<String>,
    pub import_error_at: Option<u64>,
    pub loading: FxHashSet<ApiSpecId>,
    pub collapsed_tags: FxHashSet<(ApiSpecId, String)>,
    pub collapsed_route_roots: FxHashSet<ApiSpecId>,
    pub panel_scroll: ScrollState,
    pub route_scroll: ScrollState,
    pub input_editor: Editor,
    pub focused: Option<ApiFocus>,
    pub last_resolved_host: Option<ApiResolvedHost>,
}

impl Default for ApiClientState {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            models: FxHashMap::default(),
            selected_spec: None,
            next_id: 1,
            import_menu_open: false,
            import_url_open: false,
            import_error: None,
            import_error_at: None,
            loading: FxHashSet::default(),
            collapsed_tags: FxHashSet::default(),
            collapsed_route_roots: FxHashSet::default(),
            panel_scroll: ScrollState::new(7.0),
            route_scroll: ScrollState::new(7.0),
            input_editor: Editor::new(512),
            focused: None,
            last_resolved_host: None,
        }
    }
}

impl ApiClientState {
    pub fn load_persisted() -> Self {
        let mut state = Self::default();
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
        self.loading.remove(&id);
        self.collapsed_tags.retain(|(spec_id, _)| *spec_id != id);
        self.collapsed_route_roots.remove(&id);
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
        ApiFocus::PathParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::QueryParam {
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
                        .unwrap_or("default")
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
                    let responses = parse_responses(op.get("responses"));
                    model.routes.push(ApiRouteRow {
                        tag,
                        method,
                        path: path.to_string(),
                        summary,
                        operation_id,
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
        a.tag
            .to_lowercase()
            .cmp(&b.tag.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.method.sort_rank().cmp(&b.method.sort_rank()))
    });
    model
        .routes
        .dedup_by(|a, b| a.tag == b.tag && a.path == b.path && a.method == b.method);
    Ok(model)
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
            let default_value = schema
                .and_then(|schema| schema.get("default"))
                .and_then(value_to_string);
            let example = item.get("example").and_then(value_to_string).or_else(|| {
                schema
                    .and_then(|schema| schema.get("example"))
                    .and_then(value_to_string)
            });
            out.push(ApiParam {
                name: name.to_string(),
                location,
                required: item
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(matches!(location, ApiParamLocation::Path)),
                primitive_type: ApiPrimitiveType::from_schema(schema),
                default_value,
                example,
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
        .get_key_value("application/json")
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
            arena[idx]
                .properties
                .sort_unstable_by(|a, b| a.name.cmp(&b.name));
        }
    } else if matches!(arena[idx].kind, ApiSchemaKind::Array)
        && let Some(items) = schema.get("items")
    {
        arena[idx].item = normalize_schema(items, components, arena, depth + 1, guard);
    }
    Some(ApiSchemaRef(idx))
}

fn schema_kind(schema: &Value) -> ApiSchemaKind {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => ApiSchemaKind::Object,
        Some("array") => ApiSchemaKind::Array,
        Some("string") => ApiSchemaKind::String,
        Some("integer") => ApiSchemaKind::Integer,
        Some("number") => ApiSchemaKind::Number,
        Some("boolean") => ApiSchemaKind::Boolean,
        _ if schema.get("properties").is_some() => ApiSchemaKind::Object,
        _ => ApiSchemaKind::Unknown,
    }
}

fn parse_responses(value: Option<&Value>) -> Vec<ApiResponseSummary> {
    let mut out = Vec::new();
    if let Some(map) = value.and_then(Value::as_object) {
        for (status, body) in map {
            out.push(ApiResponseSummary {
                status: status.to_string(),
                description: body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    out.sort_unstable_by(|a, b| a.status.cmp(&b.status));
    out
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
    if api.specs.is_empty() {
        content_h += 34.0 * scale;
    }
    content_h += api.specs.len() as f32 * 122.0 * scale;
    if let Some(model) = api.selected_model() {
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
    let mut content_h = pad + 38.0 * scale;
    if !route.summary.is_empty() {
        content_h += 28.0 * scale;
    }
    content_h += 28.0 * scale;
    content_h += model.servers.len().max(1) as f32 * 30.0 * scale + 42.0 * scale;
    if !route.path_params.is_empty() {
        content_h += 28.0 * scale + route.path_params.len() as f32 * 40.0 * scale + 8.0 * scale;
    }
    if !route.query_params.is_empty() {
        content_h += 28.0 * scale + route.query_params.len() as f32 * 40.0 * scale + 8.0 * scale;
    }
    if let Some(body) = &route.request_body {
        content_h += 28.0 * scale;
        if body.is_multipart {
            let prop_count = body
                .schema
                .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
                .map(|schema| schema.properties.len())
                .unwrap_or(0);
            content_h += 28.0 * scale + prop_count as f32 * 26.0 * scale;
        } else {
            content_h += 236.0 * scale;
        }
    }
    content_h += 84.0 * scale;
    if tab_state.response.is_some() {
        content_h += 208.0 * scale;
    } else if tab_state.pending {
        content_h += 24.0 * scale;
    }
    (content_h + pad + 36.0 * scale - visible_h).max(0.0)
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
            if !item.value.is_empty() {
                pairs.append_pair(&item.name, &item.value);
            }
        }
    }
    Ok(url.to_string())
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiJobRequest {
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub method: ApiMethod,
    pub url: String,
    pub body_json: Option<String>,
    pub resolved_host: Option<ApiResolvedHost>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiJobResponse {
    pub spec_id: ApiSpecId,
    pub route_idx: usize,
    pub status: Option<u16>,
    pub elapsed_ms: u128,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub truncated: bool,
    pub error: Option<ApiLoadError>,
    pub resolved_host: Option<ApiResolvedHost>,
}

pub fn spawn_api_request(job: ApiJobRequest) -> Receiver<ApiJobResponse> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let response = run_api_request(job);
        let _ = tx.send(response);
    });
    rx
}

fn run_api_request(job: ApiJobRequest) -> ApiJobResponse {
    let started = std::time::Instant::now();
    let mut response = ApiJobResponse {
        spec_id: job.spec_id,
        route_idx: job.route_idx,
        status: None,
        elapsed_ms: 0,
        headers: Vec::new(),
        body: String::new(),
        truncated: false,
        error: None,
        resolved_host: job.resolved_host.clone(),
    };
    let client = api_http_client(job.resolved_host.as_ref());
    let result = match job.method {
        ApiMethod::Get => client.get(&job.url).send(),
        ApiMethod::Delete => client.delete(&job.url).send(),
        ApiMethod::Head => client.head(&job.url).send(),
        ApiMethod::Options => client.request(reqwest::Method::OPTIONS, &job.url).send(),
        ApiMethod::Trace => client.request(reqwest::Method::TRACE, &job.url).send(),
        ApiMethod::Post => client
            .post(&job.url)
            .header("Content-Type", "application/json")
            .body(job.body_json.clone().unwrap_or_default())
            .send(),
        ApiMethod::Put => client
            .put(&job.url)
            .header("Content-Type", "application/json")
            .body(job.body_json.clone().unwrap_or_default())
            .send(),
        ApiMethod::Patch => client
            .patch(&job.url)
            .header("Content-Type", "application/json")
            .body(job.body_json.clone().unwrap_or_default())
            .send(),
    };
    response.elapsed_ms = started.elapsed().as_millis();
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
    scale: f32,
) -> usize {
    let mut x = 0.0;
    for (byte_idx, ch) in line.char_indices() {
        let adv = renderer
            .get_ui_glyph(ch)
            .map(|glyph| glyph.advance * scale)
            .unwrap_or(8.0);
        if target_x <= x + adv * 0.5 {
            return byte_idx;
        }
        x += adv;
    }
    line.len()
}

fn api_multiline_byte_at_pointer(
    renderer: &mut crate::renderer::Renderer,
    text: &str,
    rect: (f32, f32, f32, f32),
    mx: f32,
    my: f32,
    scale: f32,
) -> usize {
    let (x, y, w, h) = rect;
    let text_x = x + 10.0 * scale;
    let text_y = y + 22.0 * scale;
    let line_h = 18.0 * scale;
    let max_lines = ((h - 16.0 * scale).max(line_h) / line_h).floor() as usize;
    let target_line = ((my - text_y).max(0.0) / line_h).floor() as usize;
    let target_x = (mx - text_x).clamp(0.0, (w - 20.0 * scale).max(0.0));
    let mut line_start = 0usize;
    for (line_idx, line) in text.split('\n').take(max_lines.max(1)).enumerate() {
        if line_idx == target_line {
            return line_start + api_line_byte_at_x(renderer, line, target_x, 0.76);
        }
        line_start = line_start.saturating_add(line.len()).saturating_add(1);
    }
    text.len()
}

impl crate::app::App {
    fn place_api_cursor_from_last_click(&mut self, id: crate::ui_system::UiId, multiline: bool) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        let text = self.ide_panel.api.input_editor.get_full_text();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        let scale = renderer.scale_factor;
        let cursor = if multiline {
            api_multiline_byte_at_pointer(renderer, &text, rect, mx, my, scale)
        } else {
            let target_x =
                (mx - (rect.0 + 8.0 * scale)).clamp(0.0, (rect.2 - 16.0 * scale).max(0.0));
            api_line_byte_at_x(renderer, &text, target_x, 0.76)
        };
        self.ide_panel.api.input_editor.cursor = cursor;
        self.ide_panel.api.input_editor.selection_anchor = Some(cursor);
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
                crate::app::EditorTabKind::ApiClient(meta, _) if meta.spec_id == id
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
                ApiClientTabMeta { spec_id: id, title },
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

    pub fn open_api_route(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        self.open_api_spec_tab(spec_id);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.route_idx = Some(route_idx);
            state.response = None;
            self.sync_api_tab_inputs(spec_id, route_idx);
        }
        self.save_tabs_state();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
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
        let body_json = default_body_for_route(route, model);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.path_values = path_values;
            state.query_values = query_values;
            state.body_json = body_json;
        }
    }

    pub fn focus_api_input(&mut self, focus: ApiFocus) {
        self.commit_api_focus();
        let text = self.api_focus_text(&focus);
        let old_version = self.ide_panel.api.input_editor.version;
        self.ide_panel.api.input_editor.set_text_clean(&text);
        self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
        self.ide_panel.api.input_editor.selection_anchor =
            Some(self.ide_panel.api.input_editor.cursor);
        self.ide_panel.api.focused = Some(focus);
        self.search_focused = false;
        self.settings_ignore_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.file_tree_focused = false;
    }

    fn api_focus_text(&self, focus: &ApiFocus) -> String {
        match focus {
            ApiFocus::ImportUrl => self.ide_panel.api.input_editor.get_full_text(),
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
                        state
                            .response
                            .as_ref()
                            .map(|response| response.body.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
        }
    }

    pub fn commit_api_focus(&mut self) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        let text = self.ide_panel.api.input_editor.get_full_text();
        match focus {
            ApiFocus::ImportUrl => {}
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

    pub fn handle_api_client_click(&mut self, id: crate::ui_system::UiId) -> bool {
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
                    self.ide_panel.api.select_spec(id);
                    self.ensure_api_model_loaded(id);
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
            crate::ui_system::UiId::ApiTryRequest => {
                self.start_active_api_request();
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
                self.focus_api_input(ApiFocus::Body { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
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
        true
    }

    pub fn handle_api_client_keyboard_input(&mut self, key_event: &winit::event::KeyEvent) -> bool {
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
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let is_body = matches!(self.ide_panel.api.focused, Some(ApiFocus::Body { .. }));
        let is_response = matches!(self.ide_panel.api.focused, Some(ApiFocus::Response { .. }));
        match key_event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
            | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                if matches!(self.ide_panel.api.focused, Some(ApiFocus::ImportUrl)) {
                    self.commit_api_focus();
                    self.start_api_url_import_from_input();
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
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.ide_panel.api.input_editor.get_selection() {
                    self.set_clipboard_text(text);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) if ctrl => {
                if !is_response && let Some(text) = self.ide_panel.api.input_editor.get_selection()
                {
                    self.set_clipboard_text(text);
                    self.ide_panel.api.input_editor.delete_selection();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) if ctrl => {
                if !is_response && let Some(text) = self.get_clipboard_text() {
                    let clean = if is_body {
                        text
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ) if ctrl && shift => {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ) if ctrl => {
                let _ = self.ide_panel.api.input_editor.undo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY) if ctrl => {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
                if is_response {
                } else if ctrl {
                    self.ide_panel.api.input_editor.delete_word_backward();
                } else {
                    self.ide_panel.api.input_editor.backspace();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
                if is_response {
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
                if is_body || is_response =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, false, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown)
                if is_body || is_response =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, true, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home) => {
                self.ide_panel.api.input_editor.move_home(shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End) => {
                self.ide_panel.api.input_editor.move_end(shift);
            }
            _ if !is_response
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
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                }
            }
            _ => {}
        }
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
        if state.pending {
            return;
        }
        let spec_id = meta.spec_id;
        let requested_route_idx = state.route_idx;
        let needs_input_sync = requested_route_idx.is_none()
            || (state.path_values.is_empty()
                && state.query_values.is_empty()
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
        if state.pending {
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
            .is_some_and(|body| !body.is_multipart);
        let path_values = state.path_values.clone();
        let query_values = state.query_values.clone();
        let body_json_text = state.body_json.clone();
        let server = server.clone();
        if route.method.can_send_body() && is_json_body && !json_body_is_valid(&body_json_text) {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.response = Some(ApiJobResponse {
                    spec_id,
                    route_idx,
                    status: None,
                    elapsed_ms: 0,
                    headers: Vec::new(),
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
        let url = match build_request_url(&server, &path, &path_values, &query_values) {
            Ok(url) => url,
            Err(err) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.response = Some(ApiJobResponse {
                        spec_id,
                        route_idx,
                        status: None,
                        elapsed_ms: 0,
                        headers: Vec::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let body_json = method
            .can_send_body()
            .then_some(body_json_text)
            .filter(|body| !body.trim().is_empty());
        let job = ApiJobRequest {
            spec_id,
            route_idx,
            method,
            resolved_host: resolve_api_url_host(&url),
            url,
            body_json,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.pending = true;
        }
        self.api_request_rx.push(spawn_api_request(job));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn poll_api_client(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.api_import_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.api_import_file_rx = None;
                if let Some(path) = result {
                    self.start_api_local_import(path);
                }
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
            match self.api_request_rx[idx].try_recv() {
                Ok(result) => {
                    self.api_request_rx.remove(idx);
                    self.apply_api_job_response(result);
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => idx += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.api_request_rx.remove(idx);
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
        let focused_body = focused_response.then(|| result.body.clone());
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == result.spec_id
                && state.route_idx == Some(result.route_idx)
            {
                state.pending = false;
                state.response = Some(result);
                break;
            }
        }
        if resolved.is_some() {
            self.ide_panel.api.last_resolved_host = resolved;
            self.ide_panel.api.persist();
        }
        if let Some(body) = focused_body {
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&body);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
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
    state.body_json = default_body_for_route(route, model);
}

fn default_body_for_route(route: &ApiRouteRow, model: &ApiSpecModel) -> String {
    let Some(body) = &route.request_body else {
        return String::new();
    };
    let Some(schema_ref) = body.schema else {
        return "{\n  \n}".to_string();
    };
    schema_example_json(schema_ref, model, 0)
}

fn schema_example_json(schema_ref: ApiSchemaRef, model: &ApiSpecModel, depth: usize) -> String {
    if depth > 6 {
        return "null".to_string();
    }
    let Some(schema) = model.schema_arena.get(schema_ref.0) else {
        return "null".to_string();
    };
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
        ApiSchemaKind::String => "\"\"".to_string(),
        ApiSchemaKind::Integer | ApiSchemaKind::Number => "0".to_string(),
        ApiSchemaKind::Boolean => "false".to_string(),
        ApiSchemaKind::Unknown => "null".to_string(),
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

fn api_cache_dir() -> PathBuf {
    api_config_dir().join("api_cache")
}

fn save_url_cache(id: ApiSpecId, raw: &str) {
    let dir = api_cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}.json", id.0)), raw);
}

fn read_url_cache(id: ApiSpecId) -> Option<String> {
    std::fs::read_to_string(api_cache_dir().join(format!("{}.json", id.0))).ok()
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
                spec_id: model.id,
                route_idx: 0,
                status: Some(200),
                elapsed_ms: 1,
                headers: Vec::new(),
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
