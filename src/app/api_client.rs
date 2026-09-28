use crate::app::api_mock::contract::api_mock_default_handler_body;
use crate::app::api_mock::persist::{load_api_mocks_checked, save_api_mocks};
use crate::app::api_mock::server::apply_api_mock_server_event;
use crate::app::api_mock::ty_check::{
    ApiMockSourcePart, ApiMockTyDiagnostic, build_api_mock_virtual_source, spawn_api_mock_ty_check,
};
use crate::app::api_mock::types::ApiMockServerEvent;
use crate::app::api_mock::types::{
    ApiMockFieldConstraints, ApiMockState, default_api_mock_python_body,
    default_api_mock_python_script, is_legacy_api_mock_python_body,
};
use crate::app::api_mock::{merge::build_api_mock_routes, types::ApiMockServerSnapshot};
use crate::editor::Editor;
use crate::highlighter::{ColorSpan, Highlighter};
use crate::scroll::ScrollState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write as _;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::{Host, Url};

pub const API_FETCH_TIMEOUT: Duration = Duration::from_secs(12);
pub const API_MAX_SPEC_BYTES: usize = 8 * 1024 * 1024;
pub const API_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const API_MANUAL_MOCK_SPEC_ID: ApiSpecId = ApiSpecId(0);

pub(crate) fn api_content_type_is_json(content_type: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json"
        || media_type
            .split_once('/')
            .is_some_and(|(_, subtype)| subtype.ends_with("+json"))
}

pub(crate) fn api_mock_contract_field_prop_value(
    field: &crate::app::api_mock::types::ApiMockContractField,
    prop: crate::ui_system::ApiMockContractFieldProp,
) -> String {
    match prop {
        crate::ui_system::ApiMockContractFieldProp::Required
        | crate::ui_system::ApiMockContractFieldProp::Nullable => String::new(),
        crate::ui_system::ApiMockContractFieldProp::Default => {
            field.default_value.clone().unwrap_or_default()
        }
        crate::ui_system::ApiMockContractFieldProp::Enum => field.enum_values.join(", "),
        crate::ui_system::ApiMockContractFieldProp::MinLength => field
            .constraints
            .min_length
            .map(|value| value.to_string())
            .unwrap_or_default(),
        crate::ui_system::ApiMockContractFieldProp::MaxLength => field
            .constraints
            .max_length
            .map(|value| value.to_string())
            .unwrap_or_default(),
        crate::ui_system::ApiMockContractFieldProp::Pattern => {
            field.constraints.pattern.clone().unwrap_or_default()
        }
        crate::ui_system::ApiMockContractFieldProp::Minimum => {
            field.constraints.minimum.clone().unwrap_or_default()
        }
        crate::ui_system::ApiMockContractFieldProp::Maximum => {
            field.constraints.maximum.clone().unwrap_or_default()
        }
        crate::ui_system::ApiMockContractFieldProp::MinItems => field
            .constraints
            .min_items
            .map(|value| value.to_string())
            .unwrap_or_default(),
        crate::ui_system::ApiMockContractFieldProp::MaxItems => field
            .constraints
            .max_items
            .map(|value| value.to_string())
            .unwrap_or_default(),
    }
}

const API_MAX_MULTIPART_BODY_BYTES: usize = 64 * 1024 * 1024;
const API_SCHEMA_MAX_DEPTH: usize = 12;
const API_SCHEMA_MAX_COUNT: usize = 16_384;
const API_SCHEMA_MAX_PROPERTIES: usize = 160;
const API_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const API_REACH_TIMEOUT: Duration = Duration::from_millis(1200);
const API_PYTHON_LIST_TIMEOUT: Duration = Duration::from_secs(30);
const API_PYTHON_INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const API_PYTHON_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const API_UNTAGGED_GROUP: &str = "Без тэга";

static API_HTTP_CLIENTS: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<ApiHttpClientKey, reqwest::blocking::Client>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ApiHttpClientKey {
    host: Option<String>,
    ip: Option<IpAddr>,
    port: Option<u16>,
    proxy: Option<crate::platform::SystemProxyConfig>,
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
    pub route_groups: Vec<ApiRouteGroup>,
    pub route_display_paths: Vec<String>,
    pub security_schemes: Vec<ApiSecurityScheme>,
    pub root_security: Vec<ApiSecurityRequirement>,
    pub schema_arena: Vec<ApiSchema>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApiRouteGroup {
    pub start: usize,
    pub len: usize,
}

impl ApiSpecModel {
    pub fn rebuild_route_layout_cache(&mut self) {
        self.route_groups.clear();
        self.route_display_paths.clear();
        self.route_display_paths.reserve(self.routes.len());
        for route in &self.routes {
            let mut path = String::with_capacity(route.path.len() + 8);
            write_api_path_display(&route.path, &mut path);
            self.route_display_paths.push(path);
        }

        let mut start = 0usize;
        while start < self.routes.len() {
            let tag = self.routes[start].tag.as_str();
            let mut end = start + 1;
            while end < self.routes.len() && self.routes[end].tag == tag {
                end += 1;
            }
            self.route_groups.push(ApiRouteGroup {
                start,
                len: end - start,
            });
            start = end;
        }
    }
}

pub(crate) fn api_route_matches_filter(
    route: &ApiRouteRow,
    display_path: &str,
    filter: &str,
) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    [
        display_path,
        route.path.as_str(),
        route.tag.as_str(),
        route.summary.as_str(),
        route.description.as_str(),
        route.operation_id.as_str(),
        route.method.chip_str(),
    ]
    .into_iter()
    .any(|text| contains_ascii_case_insensitive(text, filter))
}

fn contains_ascii_case_insensitive(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if !needle.is_ascii() {
        return text.contains(needle);
    }
    let needle = needle.as_bytes();
    text.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
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
    pub description: String,
    pub operation_id: String,
    pub security: Option<Vec<ApiSecurityRequirement>>,
    pub path_params: Vec<ApiParam>,
    pub query_params: Vec<ApiParam>,
    pub request_body: Option<ApiRequestBody>,
    pub responses: Vec<ApiResponseSummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApiRouteTextField {
    Path,
    Summary,
    Description,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApiRouteTextSelection {
    pub(crate) field: ApiRouteTextField,
    pub(crate) anchor: usize,
    pub(crate) cursor: usize,
    pub(crate) selecting: bool,
}

impl ApiRouteTextSelection {
    pub(crate) fn range(self, text: &str) -> Option<(usize, usize)> {
        let start = self.anchor.min(self.cursor).min(text.len());
        let end = self.anchor.max(self.cursor).min(text.len());
        if start >= end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            None
        } else {
            Some((start, end))
        }
    }
}

pub(crate) fn api_route_selected_text<'a>(
    selection: ApiRouteTextSelection,
    text: &'a str,
) -> Option<&'a str> {
    let (start, end) = selection.range(text)?;
    text.get(start..end)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApiDescriptionLineKind {
    Text,
    Heading,
    ListItem,
}

pub(crate) const API_DESCRIPTION_LIST_MARKER: &str = "•";
pub(crate) const API_DESCRIPTION_LIST_MARKER_INDENT: f32 = 10.0;
pub(crate) const API_DESCRIPTION_LIST_CONTENT_INDENT: f32 = 26.0;

pub(crate) fn api_description_line_color(
    kind: ApiDescriptionLineKind,
    primary: [f32; 4],
) -> [f32; 4] {
    match kind {
        ApiDescriptionLineKind::Text
        | ApiDescriptionLineKind::Heading
        | ApiDescriptionLineKind::ListItem => primary,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApiDescriptionInlineKind {
    Text,
    Bold,
    Code,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApiDescriptionInlineSpan<'a> {
    pub(crate) kind: ApiDescriptionInlineKind,
    pub(crate) text: &'a str,
    pub(crate) source_start: usize,
    pub(crate) source_end: usize,
}

pub(crate) struct ApiDescriptionInlineSpans<'a> {
    text: &'a str,
    cursor: usize,
    bold: bool,
}

impl<'a> Iterator for ApiDescriptionInlineSpans<'a> {
    type Item = ApiDescriptionInlineSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor < self.text.len() {
            if self.text[self.cursor..].starts_with("**") {
                if self.bold {
                    self.bold = false;
                    self.cursor += 2;
                    continue;
                }
                if self.text[self.cursor + 2..].contains("**") {
                    self.bold = true;
                    self.cursor += 2;
                    continue;
                }
            }

            if self.text.as_bytes()[self.cursor] == b'`' {
                let delimiter_len = self.text.as_bytes()[self.cursor..]
                    .iter()
                    .take_while(|byte| **byte == b'`')
                    .count();
                let content_start = self.cursor + delimiter_len;
                let delimiter = &self.text[self.cursor..content_start];
                if let Some(relative_end) = self.text[content_start..].find(delimiter) {
                    let content_end = content_start + relative_end;
                    if content_end > content_start {
                        self.cursor = content_end + delimiter_len;
                        return Some(ApiDescriptionInlineSpan {
                            kind: ApiDescriptionInlineKind::Code,
                            text: &self.text[content_start..content_end],
                            source_start: content_start,
                            source_end: content_end,
                        });
                    }
                }
            }

            let source_start = self.cursor;
            let kind = if self.bold {
                ApiDescriptionInlineKind::Bold
            } else {
                ApiDescriptionInlineKind::Text
            };
            let mut source_end = self.text.len();
            let mut scan = self.cursor;
            while scan < self.text.len() {
                if self.text[scan..].starts_with("**") {
                    if self.bold || self.text[scan + 2..].contains("**") {
                        source_end = scan;
                        break;
                    }
                }
                if self.text.as_bytes()[scan] == b'`' {
                    let delimiter_len = self.text.as_bytes()[scan..]
                        .iter()
                        .take_while(|byte| **byte == b'`')
                        .count();
                    let content_start = scan + delimiter_len;
                    let delimiter = &self.text[scan..content_start];
                    if self.text[content_start..].find(delimiter).is_some() {
                        source_end = scan;
                        break;
                    }
                }
                scan += self.text[scan..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
            }
            if source_end == source_start {
                continue;
            }
            self.cursor = source_end;
            return Some(ApiDescriptionInlineSpan {
                kind,
                text: &self.text[source_start..source_end],
                source_start,
                source_end,
            });
        }
        None
    }
}

pub(crate) fn api_description_inline_spans(text: &str) -> ApiDescriptionInlineSpans<'_> {
    ApiDescriptionInlineSpans {
        text,
        cursor: 0,
        bold: false,
    }
}

pub(crate) fn api_route_force_emoji_presentation(next: Option<char>) -> bool {
    next == Some('\u{FE0F}')
}

pub(crate) fn api_description_line_parts(line: &str) -> (ApiDescriptionLineKind, usize, &str) {
    let leading = line.len().saturating_sub(line.trim_start().len());
    let trimmed = &line[leading..];

    let mut hashes = 0usize;
    for byte in trimmed.as_bytes().iter().copied() {
        if byte == b'#' && hashes < 6 {
            hashes += 1;
        } else {
            break;
        }
    }
    if hashes > 0
        && (hashes == trimmed.len()
            || trimmed
                .as_bytes()
                .get(hashes)
                .is_some_and(|byte| byte.is_ascii_whitespace()))
    {
        let mut content_start = leading + hashes;
        while line
            .as_bytes()
            .get(content_start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            content_start += 1;
        }
        return (
            ApiDescriptionLineKind::Heading,
            content_start,
            &line[content_start..],
        );
    }

    if trimmed == "-" || trimmed.starts_with("- ") || trimmed.starts_with("-	") {
        let mut content_start = leading + 1;
        while line
            .as_bytes()
            .get(content_start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            content_start += 1;
        }
        return (
            ApiDescriptionLineKind::ListItem,
            content_start,
            &line[content_start..],
        );
    }

    (ApiDescriptionLineKind::Text, 0, line)
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
    Time,
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
                Some("time") => Self::Time,
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
    pub media: Vec<ApiRequestBodyMedia>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequestBodyMedia {
    pub content_type: String,
    pub schema: Option<ApiSchemaRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiSchemaRef(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSchema {
    pub name: String,
    pub description: String,
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
    Time,
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
    pub media: Vec<ApiResponseMedia>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponseMedia {
    pub content_type: String,
    pub example: Option<String>,
    pub examples: Vec<ApiResponseExample>,
    pub schema: Option<ApiSchemaRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponseExample {
    pub label: String,
    pub value: String,
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
    pub generation: u64,
    pub result: Result<ApiLoadPayload, ApiLoadError>,
}

#[derive(Debug)]
pub struct ApiLoadReceiver {
    pub id: ApiSpecId,
    pub generation: u64,
    pub rx: crate::ui_waker::OneShot<ApiLoadResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiLoadTicket {
    pub generation: u64,
    pub select_on_success: bool,
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
    pub route_method: Option<ApiMethod>,
    pub route_path: String,
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
    Curl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApiInputDocView {
    #[default]
    Input,
    Schema,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApiOutputDocView {
    #[default]
    Example,
    Schema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiSchemaPaneFocus {
    Input,
    Output,
}

include!("api_client/api_client_tab_auth_state.rs");
include!("api_client/api_client_runtime_state.rs");
include!("api_client/api_client_loading_parser.rs");
include!("api_client/api_client_layout_input.rs");
include!("api_client/api_client_request_runtime.rs");
include!("api_client/api_client_app_text_methods.rs");
include!("api_client/api_client_app_tabs.rs");
include!("api_client/api_client_app_focus_methods.rs");
include!("api_client/api_client_app_click_methods.rs");
include!("api_client/api_client_app_mock_contract_methods.rs");
include!("api_client/api_client_app_mock_methods.rs");
include!("api_client/api_client_mock_config.rs");
include!("api_client/api_client_app_request_methods.rs");
include!("api_client/api_client_defaults_persist.rs");
include!("api_client/api_client_input_state.rs");
include!("api_client/api_client_request_state.rs");
include!("api_client/api_client_text_state.rs");
include!("api_client/api_client_click_state.rs");
include!("api_client/api_mock_contract_state.rs");
include!("api_client/api_mock_editor_state.rs");
include!("api_client/api_mock_routes_state.rs");
include!("api_client/api_client_tests.rs");
