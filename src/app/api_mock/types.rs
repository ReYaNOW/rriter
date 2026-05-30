use crate::app::api_client::{
    ApiMethod, ApiParam, ApiPrimitiveType, ApiRouteRow, ApiSchema, ApiSchemaKind, ApiSpecEntry,
    ApiSpecModel, ApiSpecSource,
};
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
    #[serde(default)]
    pub contract: ApiMockPythonContract,
    #[serde(default)]
    pub contract_source: String,
    pub prelude: String,
    pub body: String,
    pub timeout_ms: u64,
}

pub fn default_api_mock_python_body() -> String {
    "    return Response(ok=True)".to_string()
}

pub fn is_legacy_api_mock_python_body(text: &str) -> bool {
    matches!(
        text.trim(),
        "return json_response({\"ok\": True})"
            | "return json_response({})"
            | "return Response(ok=True)"
            | "response = Response()\n    response.ok = True\n    return response"
    )
}

pub fn default_api_mock_python_script() -> ApiMockPythonScript {
    ApiMockPythonScript {
        enabled: true,
        contract: ApiMockPythonContract::default(),
        contract_source: String::new(),
        prelude: String::new(),
        body: default_api_mock_python_body(),
        timeout_ms: 1000,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockPythonContract {
    #[serde(default)]
    pub path_params: ApiMockClassSpec,
    #[serde(default)]
    pub query: ApiMockClassSpec,
    #[serde(default)]
    pub body: ApiMockClassSpec,
    #[serde(default)]
    pub response: ApiMockClassSpec,
}

impl ApiMockPythonContract {
    pub fn is_empty(&self) -> bool {
        self.path_params.fields.is_empty()
            && self.query.fields.is_empty()
            && self.body.fields.is_empty()
            && self.response.fields.is_empty()
            && !self.response.enabled
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockClassSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub fields: Vec<ApiMockContractField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockContractField {
    pub name: String,
    pub python_name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: ApiMockContractFieldKind,
    #[serde(default)]
    pub item_kind: Option<ApiMockContractFieldKind>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub constraints: ApiMockFieldConstraints,
}

impl ApiMockContractField {
    pub fn new(name: impl Into<String>, kind: ApiMockContractFieldKind, required: bool) -> Self {
        let name = name.into();
        Self {
            python_name: api_mock_sanitize_python_param(&name),
            name,
            enabled: true,
            kind,
            item_kind: None,
            required,
            nullable: false,
            enum_values: Vec::new(),
            default_value: None,
            examples: Vec::new(),
            constraints: ApiMockFieldConstraints::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiMockContractFieldKind {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Bytes,
    #[default]
    Any,
}

impl ApiMockContractFieldKind {
    pub fn from_primitive(kind: ApiPrimitiveType) -> Self {
        match kind {
            ApiPrimitiveType::String | ApiPrimitiveType::Date | ApiPrimitiveType::DateTime => {
                Self::String
            }
            ApiPrimitiveType::Integer => Self::Integer,
            ApiPrimitiveType::Number => Self::Number,
            ApiPrimitiveType::Boolean => Self::Boolean,
            ApiPrimitiveType::Array => Self::Array,
            ApiPrimitiveType::Object => Self::Object,
            ApiPrimitiveType::Bytes => Self::Bytes,
            ApiPrimitiveType::Unknown => Self::Any,
        }
    }

    pub fn from_schema_kind(kind: ApiSchemaKind) -> Self {
        match kind {
            ApiSchemaKind::String | ApiSchemaKind::Date | ApiSchemaKind::DateTime => Self::String,
            ApiSchemaKind::Integer => Self::Integer,
            ApiSchemaKind::Number => Self::Number,
            ApiSchemaKind::Boolean => Self::Boolean,
            ApiSchemaKind::Array => Self::Array,
            ApiSchemaKind::Object => Self::Object,
            ApiSchemaKind::Bytes => Self::Bytes,
            ApiSchemaKind::Unknown => Self::Any,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMockFieldConstraints {
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub minimum: Option<String>,
    #[serde(default)]
    pub maximum: Option<String>,
    #[serde(default)]
    pub exclusive_minimum: bool,
    #[serde(default)]
    pub exclusive_maximum: bool,
    #[serde(default)]
    pub min_items: Option<usize>,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub nullable: bool,
}

pub fn api_mock_effective_contract(
    script: &ApiMockPythonScript,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
) -> ApiMockPythonContract {
    if script.contract.is_empty() {
        default_contract_from_route(route, model)
    } else {
        let mut contract = script.contract.clone();
        if !contract.response.enabled && contract.response.fields.is_empty() {
            contract.response = response_class_from_route(route, model);
        }
        contract
    }
}

pub fn default_contract_from_route(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
) -> ApiMockPythonContract {
    let mut contract = ApiMockPythonContract::default();
    contract.path_params.fields = route.path_params.iter().map(field_from_param).collect();
    for name in api_mock_path_param_names(&route.path) {
        if contract
            .path_params
            .fields
            .iter()
            .any(|field| field.name == name)
        {
            continue;
        }
        contract.path_params.fields.push(ApiMockContractField::new(
            name,
            ApiMockContractFieldKind::String,
            true,
        ));
    }
    contract.path_params.enabled = !contract.path_params.fields.is_empty();
    contract.query.fields = route.query_params.iter().map(field_from_param).collect();
    contract.query.enabled = !contract.query.fields.is_empty();
    if let Some(schema_ref) = route.request_body.as_ref().and_then(|body| body.schema)
        && let Some(schema) = model.schema_arena.get(schema_ref.0)
    {
        contract.body.fields = body_fields_from_schema(schema, model);
        contract.body.enabled = true;
    } else if route.request_body.is_some() {
        contract.body.fields.push(ApiMockContractField::new(
            "raw",
            ApiMockContractFieldKind::Any,
            route
                .request_body
                .as_ref()
                .is_some_and(|body| body.required),
        ));
        contract.body.enabled = true;
    }
    contract.response = response_class_from_route(route, model);
    contract
}

pub fn default_contract_for_manual_route(path: &str) -> ApiMockPythonContract {
    let mut contract = ApiMockPythonContract::default();
    contract.path_params.fields = api_mock_path_param_names(path)
        .into_iter()
        .map(|name| ApiMockContractField::new(name, ApiMockContractFieldKind::String, true))
        .collect();
    contract.path_params.enabled = !contract.path_params.fields.is_empty();
    contract.response = default_response_class();
    contract
}

fn field_from_param(param: &ApiParam) -> ApiMockContractField {
    let mut field = ApiMockContractField::new(
        param.name.clone(),
        ApiMockContractFieldKind::from_primitive(param.primitive_type),
        param.required,
    );
    field.item_kind = param
        .item_type
        .map(ApiMockContractFieldKind::from_primitive);
    field.enum_values = param.enum_values.clone();
    field.default_value = param.default_value.clone();
    field.examples = param.examples.clone();
    field.constraints = param.constraints.clone();
    field.nullable = field.constraints.nullable;
    field
}

fn body_fields_from_schema(schema: &ApiSchema, model: &ApiSpecModel) -> Vec<ApiMockContractField> {
    if schema.properties.is_empty() {
        let mut field = field_from_schema("raw", schema, false, model);
        field.required = true;
        return vec![field];
    }
    schema
        .properties
        .iter()
        .filter_map(|prop| {
            let schema = model.schema_arena.get(prop.schema.0)?;
            Some(field_from_schema(&prop.name, schema, prop.required, model))
        })
        .collect()
}

fn response_class_from_route(route: &ApiRouteRow, model: &ApiSpecModel) -> ApiMockClassSpec {
    let fields = route
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .or_else(|| {
            route
                .responses
                .iter()
                .find(|response| response.status == "default")
        })
        .or_else(|| route.responses.first())
        .and_then(|response| response.schema)
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
        .map(|schema| body_fields_from_schema(schema, model))
        .unwrap_or_default();
    if fields.is_empty() {
        default_response_class()
    } else {
        ApiMockClassSpec {
            enabled: true,
            fields,
        }
    }
}

fn default_response_class() -> ApiMockClassSpec {
    let mut ok = ApiMockContractField::new("ok", ApiMockContractFieldKind::Boolean, false);
    ok.default_value = Some("true".to_string());
    ApiMockClassSpec {
        enabled: true,
        fields: vec![ok],
    }
}

fn field_from_schema(
    name: &str,
    schema: &ApiSchema,
    required: bool,
    model: &ApiSpecModel,
) -> ApiMockContractField {
    let mut field = ApiMockContractField::new(
        name.to_string(),
        ApiMockContractFieldKind::from_schema_kind(schema.kind),
        required,
    );
    field.item_kind = schema
        .item
        .and_then(|item_ref| model.schema_arena.get(item_ref.0))
        .map(|item| ApiMockContractFieldKind::from_schema_kind(item.kind));
    field.enum_values = schema.enum_values.clone();
    field.default_value = schema.default_value.clone();
    field.examples = schema.examples.clone();
    field.constraints = schema.constraints.clone();
    field.nullable = field.constraints.nullable;
    field
}

pub fn api_mock_path_param_names(path: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let name = &rest[..close];
        if !name.is_empty() && !names.iter().any(|item| item == name) {
            names.push(name.to_string());
        }
        rest = &rest[close + 1..];
    }
    names
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
    pub contract: ApiMockPythonContract,
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

    #[test]
    fn default_python_body_returns_response_class() {
        assert_eq!(
            default_api_mock_python_body(),
            "    return Response(ok=True)"
        );
        assert!(is_legacy_api_mock_python_body(
            "return json_response({\"ok\": True})"
        ));
    }
}
