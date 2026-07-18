use super::contract::{
    api_mock_contract_from_state_text, api_mock_contract_source_text, api_mock_default_literal,
    api_mock_handler_signature_text, api_mock_python_type, api_mock_type_source_prefix,
    api_mock_type_source_suffix, enabled_fields,
};
use super::python_env::api_mock_python_dir;
use super::python_bootstrap::python_command;
use super::types::{
    ApiMockClassSpec, ApiMockPythonContract, ApiMockPythonScript, ApiPythonRuntimeConfig,
    ApiPythonRuntimeMode, api_mock_effective_contract,
};
use crate::app::api_client::{ApiMethod, ApiRouteRow, ApiSpecModel};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const TY_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockTyCheckResult {
    pub route_idx: usize,
    pub version: u64,
    pub ok: bool,
    pub message: String,
    pub diagnostics: Vec<ApiMockTyDiagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApiMockSourcePart {
    Contract,
    Prelude,
    Signature,
    Body,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockTyDiagnostic {
    pub part: ApiMockSourcePart,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiMockBodyLineMap {
    pub edit_start: usize,
    pub edit_end: usize,
    pub source_start: usize,
    pub source_end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiMockContractLineMap {
    pub edit_start: usize,
    pub edit_end: usize,
    pub source_start: usize,
    pub source_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockVirtualSource {
    pub source: String,
    pub contract_text: String,
    pub contract_start: usize,
    pub contract_end: usize,
    pub contract_lines: Vec<ApiMockContractLineMap>,
    pub prelude_start: usize,
    pub prelude_end: usize,
    pub signature_start: usize,
    pub signature_end: usize,
    pub body_lines: Vec<ApiMockBodyLineMap>,
}

impl ApiMockVirtualSource {
    pub fn edit_offset_to_source(
        &self,
        part: ApiMockSourcePart,
        edit_text: &str,
        offset: usize,
    ) -> usize {
        let offset = offset.min(edit_text.len());
        match part {
            ApiMockSourcePart::Contract => {
                for line in &self.contract_lines {
                    if offset >= line.edit_start && offset <= line.edit_end {
                        return line.source_start.saturating_add(offset - line.edit_start);
                    }
                }
                self.contract_lines
                    .last()
                    .map(|line| line.source_end)
                    .unwrap_or(self.contract_end)
            }
            ApiMockSourcePart::Prelude => self.prelude_start.saturating_add(offset),
            ApiMockSourcePart::Signature => self.signature_start.saturating_add(offset),
            ApiMockSourcePart::Body => {
                for line in &self.body_lines {
                    if offset >= line.edit_start && offset <= line.edit_end {
                        return line.source_start.saturating_add(offset - line.edit_start);
                    }
                }
                self.body_lines
                    .last()
                    .map(|line| line.source_end)
                    .unwrap_or(self.source.len())
            }
        }
    }

    pub fn source_offset_to_edit(
        &self,
        part: ApiMockSourcePart,
        source_offset: usize,
    ) -> Option<usize> {
        match part {
            ApiMockSourcePart::Contract => self.contract_lines.iter().find_map(|line| {
                if source_offset < line.source_start || source_offset > line.source_end {
                    return None;
                }
                let edit_len = line.edit_end.saturating_sub(line.edit_start);
                let source_col = source_offset.saturating_sub(line.source_start);
                Some(line.edit_start + source_col.min(edit_len))
            }),
            ApiMockSourcePart::Prelude => (source_offset >= self.prelude_start
                && source_offset <= self.prelude_end)
                .then_some(source_offset - self.prelude_start),
            ApiMockSourcePart::Signature => (source_offset >= self.signature_start
                && source_offset <= self.signature_end)
                .then_some(source_offset - self.signature_start),
            ApiMockSourcePart::Body => self.body_lines.iter().find_map(|line| {
                (source_offset >= line.source_start && source_offset <= line.source_end).then_some(
                    (line.edit_start + source_offset - line.source_start).min(line.edit_end),
                )
            }),
        }
    }

    pub fn contract_source_span_to_edit(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let start = self.source_offset_to_edit(ApiMockSourcePart::Contract, start)?;
        let end = self.source_offset_to_edit(ApiMockSourcePart::Contract, end)?;
        (start < end).then_some((start, end))
    }
}

pub fn spawn_api_mock_ty_check(
    route_idx: usize,
    version: u64,
    runtime: ApiPythonRuntimeConfig,
    method: ApiMethod,
    path: String,
    route: ApiRouteRow,
    model: ApiSpecModel,
    script: ApiMockPythonScript,
) -> Receiver<ApiMockTyCheckResult> {
    let (tx, rx) = mpsc::channel();
    let worker_tx = tx.clone();
    if let Err(err) = crate::platform::spawn_named("rriter-api-mock-ty-check", move || {
        let result = run_api_mock_ty_check(
            route_idx, version, &runtime, method, &path, &route, &model, &script,
        );
        let _ = worker_tx.send(result);
    }) {
        let _ = tx.send(ApiMockTyCheckResult {
            route_idx,
            version,
            ok: false,
            message: format!("не удалось запустить Ty check worker: {err}"),
            diagnostics: Vec::new(),
        });
    }
    rx
}

fn run_api_mock_ty_check(
    route_idx: usize,
    version: u64,
    runtime: &ApiPythonRuntimeConfig,
    method: ApiMethod,
    path: &str,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    script: &ApiMockPythonScript,
) -> ApiMockTyCheckResult {
    let virtual_source = build_api_mock_virtual_source(method, path, route, model, script);
    let dir = api_mock_python_dir().join("ty-check");
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return ApiMockTyCheckResult {
            route_idx,
            version,
            ok: false,
            message: err.to_string(),
            diagnostics: Vec::new(),
        };
    }
    let file = dir.join(format!("mock_route_{}.py", route_idx));
    if let Err(err) = std::fs::write(&file, &virtual_source.source) {
        return ApiMockTyCheckResult {
            route_idx,
            version,
            ok: false,
            message: err.to_string(),
            diagnostics: Vec::new(),
        };
    }
    run_ty(route_idx, version, runtime, &file, &virtual_source, script)
}

fn run_ty(
    route_idx: usize,
    version: u64,
    runtime: &ApiPythonRuntimeConfig,
    file: &PathBuf,
    virtual_source: &ApiMockVirtualSource,
    script: &ApiMockPythonScript,
) -> ApiMockTyCheckResult {
    let output = api_mock_ty_command(runtime, file).and_then(|mut command| {
        crate::platform::run_command_output(&mut command, TY_CHECK_TIMEOUT)
            .map_err(|err| format!("ty not available: {err}"))
    });
    match output {
        Ok(output) => {
            let mut message = String::new();
            message.push_str(String::from_utf8_lossy(&output.stdout).trim());
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.trim().is_empty() {
                if !message.is_empty() {
                    message.push('\n');
                }
                message.push_str(err.trim());
            }
            if message.is_empty() {
                message = if output.status.success() {
                    "Ty check passed".to_string()
                } else {
                    "Ty check failed".to_string()
                };
            }
            ApiMockTyCheckResult {
                route_idx,
                version,
                ok: output.status.success(),
                diagnostics: parse_api_mock_ty_diagnostics(&message, virtual_source, script),
                message,
            }
        }
        Err(err) => ApiMockTyCheckResult {
            route_idx,
            version,
            ok: false,
            message: err,
            diagnostics: Vec::new(),
        },
    }
}

fn api_mock_ty_command(
    runtime: &ApiPythonRuntimeConfig,
    file: &PathBuf,
) -> Result<Command, String> {
    match runtime.mode {
        ApiPythonRuntimeMode::UvManaged => {
            let uv_path = runtime
                .uv_path
                .as_ref()
                .ok_or_else(|| "uv path is not configured".to_string())?;
            let version = if runtime.python_version.trim().is_empty() {
                "3.13"
            } else {
                runtime.python_version.trim()
            };
            let mut command = Command::new(uv_path);
            command
                .arg("run")
                .arg("--no-project")
                .arg("--python")
                .arg(version)
                .arg("--no-python-downloads")
                .arg("ty")
                .arg("check")
                .arg(file);
            Ok(command)
        }
        ApiPythonRuntimeMode::CustomPython => {
            let python_path = runtime
                .custom_python_path
                .as_ref()
                .ok_or_else(|| "Python path is not configured".to_string())?;
            let mut command = python_command(python_path);
            command.arg("-m").arg("ty").arg("check").arg(file);
            Ok(command)
        }
    }
}

fn parse_api_mock_ty_diagnostics(
    message: &str,
    virtual_source: &ApiMockVirtualSource,
    script: &ApiMockPythonScript,
) -> Vec<ApiMockTyDiagnostic> {
    let mut out = Vec::new();
    let mut last_message = String::new();
    for line in message.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("error") {
            last_message.clear();
            last_message.push_str(trimmed);
            if let Some(idx) = rest.find(':') {
                last_message = rest[idx + 1..].trim().to_string();
            }
        }
        let Some((line_one, col_one)) = parse_ty_line_col(trimmed) else {
            continue;
        };
        if let Some(diag) = map_ty_location_to_edit(
            line_one,
            col_one,
            if last_message.is_empty() {
                message.lines().next().unwrap_or("ty error")
            } else {
                last_message.as_str()
            },
            virtual_source,
            script,
        ) {
            out.push(diag);
        }
    }
    out
}

fn parse_ty_line_col(line: &str) -> Option<(usize, usize)> {
    let mut nums = [0usize; 2];
    let mut found = 0usize;
    for part in line.rsplit(':') {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if found < nums.len() {
            nums[found] = digits.parse().ok()?;
            found += 1;
        }
        if found == 2 {
            return Some((nums[1], nums[0]));
        }
    }
    None
}

fn map_ty_location_to_edit(
    source_line_one: usize,
    source_col_one: usize,
    message: &str,
    virtual_source: &ApiMockVirtualSource,
    script: &ApiMockPythonScript,
) -> Option<ApiMockTyDiagnostic> {
    let source_offset =
        source_line_col_to_offset(&virtual_source.source, source_line_one, source_col_one)?;
    let prelude = virtual_source
        .source_offset_to_edit(ApiMockSourcePart::Prelude, source_offset)
        .map(|offset| (ApiMockSourcePart::Prelude, offset));
    let contract = virtual_source
        .source_offset_to_edit(ApiMockSourcePart::Contract, source_offset)
        .map(|offset| (ApiMockSourcePart::Contract, offset));
    let body = virtual_source
        .source_offset_to_edit(ApiMockSourcePart::Body, source_offset)
        .map(|offset| (ApiMockSourcePart::Body, offset));
    let (part, offset) = contract.or(prelude).or(body)?;
    let edit_text = match part {
        ApiMockSourcePart::Contract => virtual_source.contract_text.as_str(),
        ApiMockSourcePart::Prelude => script.prelude.as_str(),
        ApiMockSourcePart::Signature => return None,
        ApiMockSourcePart::Body => script.body.as_str(),
    };
    let (line, col) = edit_offset_to_line_col(edit_text, offset);
    let line_text = edit_text.split('\n').nth(line).unwrap_or("");
    let start_col = col.min(line_text.chars().count());
    let end_col = next_token_end_col(line_text, start_col).max(start_col.saturating_add(1));
    Some(ApiMockTyDiagnostic {
        part,
        line,
        start_col,
        end_col,
        message: message.to_string(),
    })
}

fn source_line_col_to_offset(source: &str, line_one: usize, col_one: usize) -> Option<usize> {
    let mut offset = 0usize;
    for (idx, line) in source.split('\n').enumerate() {
        if idx + 1 == line_one {
            let col = col_one.saturating_sub(1);
            return Some(offset + crate::editor::byte_offset_for_char_col(line, col).min(line.len()));
        }
        offset = offset.saturating_add(line.len()).saturating_add(1);
    }
    None
}

fn edit_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut remaining = offset.min(text.len());
    for (line_idx, line) in text.split('\n').enumerate() {
        if remaining <= line.len() {
            return (line_idx, line[..remaining].chars().count());
        }
        remaining = remaining.saturating_sub(line.len().saturating_add(1));
    }
    (0, 0)
}


fn next_token_end_col(line: &str, start_col: usize) -> usize {
    let mut col = 0usize;
    let mut in_token = false;
    for ch in line.chars() {
        if col < start_col {
            col += 1;
            continue;
        }
        let token_char = ch.is_ascii_alphanumeric() || ch == '_';
        if !in_token && token_char {
            in_token = true;
        } else if in_token && !token_char {
            return col;
        } else if !in_token && !ch.is_ascii_whitespace() {
            return col.saturating_add(1);
        }
        col += 1;
    }
    col
}

fn hidden_contract_source(
    source: &str,
    source_start: usize,
    contract: &ApiMockPythonContract,
) -> (String, Vec<ApiMockContractLineMap>) {
    let mut out = String::with_capacity(source.len() + 64);
    let mut maps = Vec::new();
    let mut edit_start = 0usize;
    for line in source.split_inclusive('\n') {
        let line_body = line.strip_suffix('\n').unwrap_or(line);
        let has_newline = line.ends_with('\n');
        let hidden = hidden_contract_line(line_body);
        let source_line_start = source_start + out.len();
        out.push_str(&hidden);
        if has_newline {
            out.push('\n');
        }
        if let Some(init) = hidden_contract_init_for_line(&hidden, contract) {
            out.push_str(&init);
        }
        let edit_end = edit_start + line_body.len();
        maps.push(ApiMockContractLineMap {
            edit_start,
            edit_end,
            source_start: source_line_start,
            source_end: source_line_start + hidden.len(),
        });
        edit_start = edit_end.saturating_add(has_newline as usize);
    }
    if source.is_empty() {
        maps.push(ApiMockContractLineMap {
            edit_start: 0,
            edit_end: 0,
            source_start,
            source_end: source_start,
        });
    }
    (out, maps)
}

fn hidden_contract_init_for_line(line: &str, contract: &ApiMockPythonContract) -> Option<String> {
    let class_name = hidden_contract_class_name(line)?;
    let spec = match class_name {
        "Query" => &contract.query,
        "Body" => &contract.body,
        "Response" => &contract.response,
        _ => return None,
    };
    Some(hidden_contract_init_for_class(spec))
}

fn hidden_contract_class_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("class ")?;
    rest.split(['(', ':']).next().map(str::trim)
}

fn hidden_contract_init_for_class(spec: &ApiMockClassSpec) -> String {
    let mut out = String::from("    def __init__(self");
    for required in [true, false] {
        for field in enabled_fields(spec).filter(|field| field.required == required) {
            out.push_str(", ");
            out.push_str(&field.python_name);
            out.push_str(": ");
            out.push_str(&api_mock_python_type(field));
            if !field.required {
                out.push_str(" = ");
                let default_literal = api_mock_default_literal(field);
                let default_text = default_literal.as_deref().unwrap_or(if field.nullable {
                    "None"
                } else {
                    "..."
                });
                out.push_str(default_text);
            }
        }
    }
    out.push_str(") -> None: ...\n");
    out
}

fn hidden_contract_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent_len = line.len().saturating_sub(trimmed.len());
    let Some(rest) = trimmed.strip_prefix("class Response") else {
        return line.to_string();
    };
    let Some(colon_idx) = rest.rfind(':') else {
        return line.to_string();
    };
    if rest[..colon_idx].contains("BaseModel") {
        return line.to_string();
    }
    let indent = &line[..indent_len];
    let header = rest[..colon_idx].trim();
    let tail = &rest[colon_idx..];
    if header.is_empty() {
        format!("{indent}class Response(BaseModel){tail}")
    } else if let Some(args) = header
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
    {
        if args.trim().is_empty() {
            format!("{indent}class Response(BaseModel){tail}")
        } else {
            format!("{indent}class Response(BaseModel, {args}){tail}")
        }
    } else {
        line.to_string()
    }
}

#[cfg(test)]
pub fn build_api_mock_ty_source(
    method: ApiMethod,
    path: &str,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    script: &ApiMockPythonScript,
) -> String {
    build_api_mock_virtual_source(method, path, route, model, script).source
}

pub fn build_api_mock_virtual_source(
    method: ApiMethod,
    path: &str,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    script: &ApiMockPythonScript,
) -> ApiMockVirtualSource {
    let base_contract = api_mock_effective_contract(script, route, model);
    let contract_source = api_mock_contract_source_text(script, route, model);
    let contract = if script.contract_source.trim().is_empty() {
        base_contract
    } else {
        api_mock_contract_from_state_text(&base_contract, &contract_source)
    };
    let mut out = String::with_capacity(
        script.prelude.len() + script.body.len() + contract_source.len() + 2048,
    );
    out.push_str(&api_mock_type_source_prefix(method, path));
    out.push_str("# Editable contract classes\n");
    let contract_start = out.len();
    let (hidden_contract, contract_lines) =
        hidden_contract_source(&contract_source, contract_start, &contract);
    out.push_str(&hidden_contract);
    let contract_end = out.len();
    if !contract_source.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&api_mock_type_source_suffix(&contract_source));
    out.push_str("# User prelude\n");
    let prelude_start = out.len();
    out.push_str(&script.prelude);
    let prelude_end = out.len();
    if !script.prelude.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    let signature_start = out.len();
    out.push_str(&api_mock_handler_signature_text(&contract));
    let signature_end = out.len();
    out.push('\n');
    let mut body_lines = Vec::new();
    if script.body.trim().is_empty() {
        out.push_str("\n    return Response(ok=True)\n");
    } else {
        let mut edit_start = 0usize;
        for line in script.body.lines() {
            let add_indent = !line.is_empty() && !line.starts_with(char::is_whitespace);
            if add_indent {
                out.push_str("    ");
            }
            let source_start = out.len();
            out.push_str(line);
            let source_end = out.len();
            let edit_end = edit_start + line.len();
            body_lines.push(ApiMockBodyLineMap {
                edit_start,
                edit_end,
                source_start,
                source_end,
            });
            out.push('\n');
            edit_start = edit_end.saturating_add(1);
        }
        if script.body.ends_with('\n') {
            let source_start = out.len() + 4;
            out.push_str("    ");
            body_lines.push(ApiMockBodyLineMap {
                edit_start,
                edit_end: edit_start,
                source_start,
                source_end: source_start,
            });
            out.push('\n');
        }
    }
    ApiMockVirtualSource {
        source: out,
        contract_text: contract_source,
        contract_start,
        contract_end,
        contract_lines,
        prelude_start,
        prelude_end,
        signature_start,
        signature_end,
        body_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::{ApiParam, ApiParamLocation, ApiPrimitiveType};
    use crate::app::api_mock::types::ApiMockFieldConstraints;

    #[test]
    fn ty_source_contains_locked_signature_and_models() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Get,
            path: "/users/{id}".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: vec![ApiParam {
                name: "page".to_string(),
                location: ApiParamLocation::Query,
                required: false,
                primitive_type: ApiPrimitiveType::Integer,
                item_type: None,
                enum_values: Vec::new(),
                default_value: None,
                example: None,
                examples: Vec::new(),
                description: String::new(),
                constraints: ApiMockFieldConstraints::default(),
            }],
            request_body: None,
            responses: Vec::new(),
        };
        let model = ApiSpecModel::default();
        let source = build_api_mock_ty_source(
            ApiMethod::Get,
            "/users/{id}",
            &route,
            &model,
            &ApiMockPythonScript {
                enabled: true,
                contract: Default::default(),
                contract_source: String::new(),
                prelude: String::new(),
                body: "return json_response({\"id\": id})".to_string(),
                timeout_ms: 1000,
            },
        );

        assert!(source.contains("class Query"));
        assert!(source.contains("page: int | None = None"));
        assert!(source.contains("def __init__(self, page: int | None = None) -> None: ..."));
        assert!(source.contains("class Response(BaseModel):"));
        assert!(!source.contains("@dataclass\nclass Response(BaseModel):"));
        assert!(source.contains("def handler(\n    req: Request,\n    id: str,"));
    }

    #[test]
    fn ty_source_keeps_response_model_pydantic_style_for_mixed_defaults() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Get,
            path: "/users/me".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        };
        let model = ApiSpecModel::default();
        let source = build_api_mock_ty_source(
            ApiMethod::Get,
            "/users/me",
            &route,
            &model,
            &ApiMockPythonScript {
                enabled: true,
                contract: Default::default(),
                contract_source:
                    "class Response:\n    image_link: str | None = None\n    id: int\n".to_string(),
                prelude: String::new(),
                body: "return Response(image_link=None, id=1)".to_string(),
                timeout_ms: 1000,
            },
        );
        let response_start = source.find("class Response(BaseModel):").unwrap();
        let response_source = &source[response_start..];
        let image_pos = response_source
            .find("\n    image_link: str | None = None")
            .unwrap();
        let id_pos = response_source.find("\n    id: int").unwrap();

        assert!(image_pos < id_pos);
        assert!(
            response_source.contains(
                "def __init__(self, id: int, image_link: str | None = None) -> None: ..."
            )
        );
        assert!(!source.contains("@dataclass\nclass Response"));
    }

    #[test]
    fn ty_source_adds_body_constructor_args_for_completion() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Post,
            path: "/items".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        };
        let model = ApiSpecModel::default();
        let source = build_api_mock_ty_source(
            ApiMethod::Post,
            "/items",
            &route,
            &model,
            &ApiMockPythonScript {
                enabled: true,
                contract: Default::default(),
                contract_source: "class Body:\n    title: str\n    count: int | None = None\n"
                    .to_string(),
                prelude: String::new(),
                body: "return Response(ok=True)".to_string(),
                timeout_ms: 1000,
            },
        );

        assert!(
            source
                .contains("def __init__(self, title: str, count: int | None = None) -> None: ...")
        );
    }

    #[test]
    fn virtual_source_maps_body_offsets_over_indent() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Post,
            path: "/items".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        };
        let script = ApiMockPythonScript {
            enabled: true,
            contract: Default::default(),
            contract_source: String::new(),
            prelude: "import math".to_string(),
            body: "value = 1\nreturn json_response({\"value\": value})".to_string(),
            timeout_ms: 1000,
        };
        let model = ApiSpecModel::default();
        let virtual_source =
            build_api_mock_virtual_source(ApiMethod::Post, "/items", &route, &model, &script);
        let edit_offset = script.body.find("json_response").unwrap();
        let source_offset = virtual_source.edit_offset_to_source(
            ApiMockSourcePart::Body,
            &script.body,
            edit_offset,
        );

        assert_eq!(
            &virtual_source.source[source_offset..source_offset + "json_response".len()],
            "json_response"
        );
        assert_eq!(
            virtual_source.source_offset_to_edit(ApiMockSourcePart::Body, source_offset),
            Some(edit_offset)
        );
        assert!(
            virtual_source.source[virtual_source.prelude_start..virtual_source.prelude_end]
                .contains("import math")
        );
    }

    #[test]
    fn virtual_source_places_body_directly_after_signature() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Get,
            path: "/items".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        };
        let script = ApiMockPythonScript {
            enabled: true,
            contract: Default::default(),
            contract_source: String::new(),
            prelude: String::new(),
            body: "    return json_response({\"ok\": True})".to_string(),
            timeout_ms: 1000,
        };
        let model = ApiSpecModel::default();
        let virtual_source =
            build_api_mock_virtual_source(ApiMethod::Get, "/items", &route, &model, &script);

        assert!(
            virtual_source
                .source
                .contains(") -> Response | dict[str, Any]:\n    return")
        );
        assert!(
            !virtual_source
                .source
                .contains(") -> Response | dict[str, Any]:\n\n    return")
        );
    }

    #[test]
    fn ty_diagnostics_map_to_body_hover_message_range() {
        let route = ApiRouteRow {
            tag: String::new(),
            method: ApiMethod::Get,
            path: "/items".to_string(),
            summary: String::new(),
            description: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        };
        let script = ApiMockPythonScript {
            enabled: true,
            contract: Default::default(),
            contract_source: String::new(),
            prelude: String::new(),
            body: "\n    missing_name\n    return json_response({})".to_string(),
            timeout_ms: 1000,
        };
        let model = ApiSpecModel::default();
        let virtual_source =
            build_api_mock_virtual_source(ApiMethod::Get, "/items", &route, &model, &script);
        let source_offset = virtual_source
            .source
            .find("missing_name")
            .expect("missing name in source");
        let before = &virtual_source.source[..source_offset];
        let line_one = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let col_one = before
            .rsplit('\n')
            .next()
            .map(|line| line.chars().count() + 1)
            .unwrap_or(1);
        let message = format!(
            "error: Name `missing_name` is not defined\n  --> mock_route_0.py:{line_one}:{col_one}"
        );

        let diagnostics = parse_api_mock_ty_diagnostics(&message, &virtual_source, &script);

        assert_eq!(diagnostics.len(), 1);
        let diag = &diagnostics[0];
        assert_eq!(diag.part, ApiMockSourcePart::Body);
        assert_eq!(diag.line, 1);
        assert_eq!(diag.start_col, 4);
        assert_eq!(diag.end_col, "    missing_name".chars().count());
        assert!(diag.message.contains("missing_name"));
    }

    #[test]
    fn ty_check_command_uses_selected_uv_python() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::UvManaged,
            uv_path: Some(PathBuf::from(r"C:\Program Files\uv\uv.exe")),
            custom_python_path: None,
            python_version: "3.12".to_string(),
        };
        let command = api_mock_ty_command(
            &runtime,
            &PathBuf::from(r"C:\Users\Reyan\Mock Project\mock_route_0.py"),
        )
        .unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(command.get_program(), r"C:\Program Files\uv\uv.exe");
        assert_eq!(
            args,
            vec![
                "run",
                "--no-project",
                "--python",
                "3.12",
                "--no-python-downloads",
                "ty",
                "check",
                r"C:\Users\Reyan\Mock Project\mock_route_0.py"
            ]
        );
    }

    #[test]
    fn ty_check_command_uses_custom_python_module() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::CustomPython,
            uv_path: None,
            custom_python_path: Some(PathBuf::from("/opt/python/bin/python")),
            python_version: "3.13".to_string(),
        };
        let command =
            api_mock_ty_command(&runtime, &PathBuf::from("/tmp/mock_route_0.py")).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(command.get_program(), "/opt/python/bin/python");
        assert_eq!(args, vec!["-m", "ty", "check", "/tmp/mock_route_0.py"]);
    }

    #[test]
    fn ty_check_command_supports_windows_python_launcher() {
        let runtime = ApiPythonRuntimeConfig {
            mode: ApiPythonRuntimeMode::CustomPython,
            uv_path: None,
            custom_python_path: Some(PathBuf::from(r"C:\Windows\py.exe")),
            python_version: "3.13".to_string(),
        };
        let command = api_mock_ty_command(
            &runtime,
            &PathBuf::from(r"C:\work\project\mock_route_0.py"),
        )
        .unwrap();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_program(), r"C:\Windows\py.exe");
        assert_eq!(
            args,
            vec![
                "-3",
                "-m",
                "ty",
                "check",
                r"C:\work\project\mock_route_0.py"
            ]
        );
    }

    #[test]
    fn ty_location_parser_ignores_windows_drive_colon() {
        assert_eq!(
            parse_ty_line_col(r"  --> C:\work\project\mock_route_0.py:12:34"),
            Some((12, 34))
        );
        assert_eq!(
            parse_ty_line_col(r"  --> \\server\share\mock_route_0.py:7:9"),
            Some((7, 9))
        );
    }
}
