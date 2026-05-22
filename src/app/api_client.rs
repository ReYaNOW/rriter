use crate::editor::Editor;
use crate::scroll::ScrollState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
pub struct ApiSpecEntry {
    pub id: ApiSpecId,
    pub title: String,
    pub version: String,
    pub openapi_version: String,
    pub source: ApiSpecSource,
    pub last_loaded: Option<u64>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiLoadPayload {
    pub entry: ApiSpecEntry,
    pub model: ApiSpecModel,
    pub raw_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
}

pub struct ApiClientState {
    pub specs: Vec<ApiSpecEntry>,
    pub models: FxHashMap<ApiSpecId, ApiSpecModel>,
    pub selected_spec: Option<ApiSpecId>,
    pub next_id: u64,
    pub import_menu_open: bool,
    pub import_url_open: bool,
    pub import_error: Option<String>,
    pub loading: FxHashSet<ApiSpecId>,
    pub collapsed_tags: FxHashSet<(ApiSpecId, String)>,
    pub panel_scroll: ScrollState,
    pub route_scroll: ScrollState,
    pub input_editor: Editor,
    pub focused: Option<ApiFocus>,
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
            loading: FxHashSet::default(),
            collapsed_tags: FxHashSet::default(),
            panel_scroll: ScrollState::new(7.0),
            route_scroll: ScrollState::new(7.0),
            input_editor: Editor::new(512),
            focused: None,
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
                for spec in &mut state.specs {
                    spec.selected = Some(spec.id) == state.selected_spec;
                }
            }
        }
        state
    }

    pub fn persist(&self) {
        let saved = ApiSpecsPersist {
            specs: self.specs.clone(),
            selected_spec: self.selected_spec,
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
        }
        self.persist();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ApiSpecsPersist {
    specs: Vec<ApiSpecEntry>,
    selected_spec: Option<ApiSpecId>,
    next_id: u64,
}

pub fn validate_api_url(input: &str) -> Result<Url, ApiLoadError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL пустой",
        ));
    }
    let parsed = Url::parse(raw).map_err(|_| {
        ApiLoadError::new(ApiLoadErrorKind::InvalidUrl, "URL не распознан")
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL должен быть http или https",
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
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
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
            Some(raw) => parse_openapi_payload(id, ApiSpecSource::Url(url), raw, None),
            None => Err(ApiLoadError::new(
                ApiLoadErrorKind::Io,
                "URL cache пустой",
            )),
        };
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

fn load_local_spec(id: ApiSpecId, path: &Path) -> Result<ApiLoadPayload, ApiLoadError> {
    let bytes = std::fs::read(path).map_err(|err| {
        ApiLoadError::new(
            ApiLoadErrorKind::Io,
            format!("файл не прочитан: {}", err),
        )
    })?;
    if bytes.len() > API_MAX_SPEC_BYTES {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "openapi.json слишком большой",
        ));
    }
    let raw = String::from_utf8(bytes).map_err(|_| {
        ApiLoadError::new(ApiLoadErrorKind::InvalidJson, "JSON не UTF-8")
    })?;
    parse_openapi_payload(id, ApiSpecSource::Local(path.to_path_buf()), raw, None)
}

fn load_url_spec(id: ApiSpecId, url: &str) -> Result<ApiLoadPayload, ApiLoadError> {
    validate_api_url(url)?;
    let raw = fetch_json(url)?;
    parse_openapi_payload(
        id,
        ApiSpecSource::Url(url.to_string()),
        raw,
        Some(ApiUrlStatus::Ok(200)),
    )
}

fn fetch_json(url: &str) -> Result<String, ApiLoadError> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(API_FETCH_TIMEOUT))
        .build()
        .new_agent();
    let mut response = agent.get(url).call().map_err(classify_ureq_error)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::HttpStatus(status),
            format!("HTTP {}", status),
        ));
    }
    if let Some(content_len) = response.body().content_length()
        && content_len > API_MAX_SPEC_BYTES as u64
    {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "ответ больше лимита",
        ));
    }
    response
        .body_mut()
        .with_config()
        .limit(API_MAX_SPEC_BYTES as u64)
        .read_to_string()
        .map_err(classify_ureq_error)
}

fn classify_ureq_error(err: ureq::Error) -> ApiLoadError {
    match err {
        ureq::Error::StatusCode(code) => {
            ApiLoadError::new(ApiLoadErrorKind::HttpStatus(code), format!("HTTP {}", code))
        }
        ureq::Error::Timeout(_) => {
            ApiLoadError::new(ApiLoadErrorKind::Timeout, "таймаут запроса")
        }
        ureq::Error::HostNotFound => {
            ApiLoadError::new(ApiLoadErrorKind::Dns, "DNS не нашел host")
        }
        ureq::Error::ConnectionFailed => ApiLoadError::new(
            ApiLoadErrorKind::NoInternet,
            "соединение не установлено",
        ),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => {
            ApiLoadError::new(ApiLoadErrorKind::Tls, "TLS ошибка")
        }
        ureq::Error::Io(err) => {
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
        other => ApiLoadError::new(ApiLoadErrorKind::Other, other.to_string()),
    }
}

fn parse_openapi_payload(
    id: ApiSpecId,
    source: ApiSpecSource,
    raw: String,
    url_status: Option<ApiUrlStatus>,
) -> Result<ApiLoadPayload, ApiLoadError> {
    let root: Value = serde_json::from_str(&raw)
        .map_err(|err| ApiLoadError::new(ApiLoadErrorKind::InvalidJson, err.to_string()))?;
    let model = parse_openapi_model(id, &root)?;
    let entry = ApiSpecEntry {
        id,
        title: model.title.clone(),
        version: model.version.clone(),
        openapi_version: model.openapi_version.clone(),
        source,
        last_loaded: Some(now_epoch_secs()),
        last_url_status: url_status,
        selected: true,
        error: None,
    };
    Ok(ApiLoadPayload {
        entry,
        model,
        raw_json: Some(raw),
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
            .then_with(|| a.method.cmp(&b.method))
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

fn parse_parameters(
    value: Option<&Value>,
    root: &Value,
) -> Vec<ApiParam> {
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
            let example = item
                .get("example")
                .and_then(value_to_string)
                .or_else(|| schema.and_then(|schema| schema.get("example")).and_then(value_to_string));
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
    };
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(API_FETCH_TIMEOUT))
        .build()
        .new_agent();
    let result = match job.method {
        ApiMethod::Get => agent.get(&job.url).call(),
        ApiMethod::Delete => agent.delete(&job.url).call(),
        ApiMethod::Head => agent.head(&job.url).call(),
        ApiMethod::Options => agent.options(&job.url).call(),
        ApiMethod::Trace => agent.trace(&job.url).call(),
        ApiMethod::Post => agent
            .post(&job.url)
            .header("Content-Type", "application/json")
            .send(job.body_json.as_deref().unwrap_or("")),
        ApiMethod::Put => agent
            .put(&job.url)
            .header("Content-Type", "application/json")
            .send(job.body_json.as_deref().unwrap_or("")),
        ApiMethod::Patch => agent
            .patch(&job.url)
            .header("Content-Type", "application/json")
            .send(job.body_json.as_deref().unwrap_or("")),
    };
    response.elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(mut res) => {
            response.status = Some(res.status().as_u16());
            for (name, value) in res.headers() {
                if let Ok(v) = value.to_str() {
                    response.headers.push((name.as_str().to_string(), v.to_string()));
                }
            }
            match res
                .body_mut()
                .with_config()
                .limit(API_MAX_RESPONSE_BYTES as u64)
                .read_to_string()
            {
                Ok(body) => response.body = body,
                Err(ureq::Error::BodyExceedsLimit(_)) => {
                    response.truncated = true;
                    response.body = "Ответ больше лимита".to_string();
                }
                Err(err) => response.error = Some(classify_ureq_error(err)),
            }
        }
        Err(err) => response.error = Some(classify_ureq_error(err)),
    }
    response
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn format_last_loaded(last_loaded: Option<u64>) -> String {
    let Some(loaded) = last_loaded else {
        return "не загружено".to_string();
    };
    let now = now_epoch_secs();
    let age = now.saturating_sub(loaded);
    if age < 60 {
        format!("{age} сек назад")
    } else if age < 3600 {
        format!("{} мин назад", age / 60)
    } else if age < 86_400 {
        format!("{} ч назад", age / 3600)
    } else {
        format!("{} д назад", age / 86_400)
    }
}

impl crate::app::App {
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
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        let id = self.ide_panel.api.alloc_spec_id();
        self.ide_panel.api.import_error = None;
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
            icon_key: "json",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta { spec_id: id, title },
                ApiClientTabState::default(),
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

    fn active_api_tab_mut_for(
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

    fn sync_api_tab_inputs(&mut self, spec_id: ApiSpecId, route_idx: usize) {
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
        self.ide_panel.api.input_editor = Editor::new(text.len().saturating_add(512));
        self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        let _ = self.ide_panel.api.input_editor.insert_str(&text);
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
                if idx < self.ide_panel.api.specs.len() {
                    let id = self.ide_panel.api.specs[idx].id;
                    self.ide_panel.api.specs.remove(idx);
                    self.ide_panel.api.models.remove(&id);
                    self.ide_panel.api.loading.remove(&id);
                    if self.ide_panel.api.selected_spec == Some(id) {
                        self.ide_panel.api.selected_spec =
                            self.ide_panel.api.specs.first().map(|entry| entry.id);
                    }
                    self.ide_panel.api.persist();
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
            }
            crate::ui_system::UiId::ApiBodyInput(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.focus_api_input(ApiFocus::Body {
                    spec_id,
                    route_idx,
                });
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

    pub fn handle_api_client_keyboard_input(
        &mut self,
        key_event: &winit::event::KeyEvent,
    ) -> bool {
        if self.ide_panel.api.focused.is_none() {
            return self.active_tab_is_api_client();
        }
        if key_event.state != winit::event::ElementState::Pressed {
            return true;
        }
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let is_body = matches!(self.ide_panel.api.focused, Some(ApiFocus::Body { .. }));
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
                if let Some(text) = self.ide_panel.api.input_editor.get_selection() {
                    self.set_clipboard_text(text);
                    self.ide_panel.api.input_editor.delete_selection();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) if ctrl => {
                if let Some(text) = self.get_clipboard_text() {
                    let clean = if is_body {
                        text
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
                if ctrl {
                    self.ide_panel.api.input_editor.delete_word_backward();
                } else {
                    self.ide_panel.api.input_editor.backspace();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
                if ctrl {
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
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp) if is_body => {}
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown)
                if is_body => {}
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home) => {
                self.ide_panel.api.input_editor.move_home(shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End) => {
                self.ide_panel.api.input_editor.move_end(shift);
            }
            _ if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() => {
                if let Some(text) = key_event.text.as_ref().and_then(|s| (!s.is_empty()).then_some(s))
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
        let Some(route_idx) = state.route_idx else {
            return;
        };
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
        if route.method.can_send_body()
            && is_json_body
            && !json_body_is_valid(&body_json_text)
        {
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
            url,
            body_json,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.pending = true;
            state.response = None;
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
                if state.route_idx.is_none()
                    && let Some(model) = self.ide_panel.api.models.get(&id)
                    && !model.routes.is_empty()
                {
                    state.route_idx = Some(0);
                    state.path_values = model.routes[0]
                        .path_params
                        .iter()
                        .map(|param| ApiInputValue {
                            name: param.name.clone(),
                            value: param.default_value.clone().unwrap_or_default(),
                        })
                        .collect();
                    state.query_values = model.routes[0]
                        .query_params
                        .iter()
                        .map(|param| ApiInputValue {
                            name: param.name.clone(),
                            value: param.default_value.clone().unwrap_or_default(),
                        })
                        .collect();
                    state.body_json = default_body_for_route(&model.routes[0], model);
                }
            }
        }
    }

    fn apply_api_job_response(&mut self, result: ApiJobResponse) {
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
    }
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
        assert_eq!(model.routes[0].path_params[0].name, "id");
        assert_eq!(model.routes[0].query_params[0].name, "verbose");
        assert!(!model.schema_arena.is_empty());
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
}
