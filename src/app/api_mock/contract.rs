use super::types::{
    ApiMockClassSpec, ApiMockContractField, ApiMockContractFieldKind, ApiMockFieldConstraints,
    ApiMockPythonContract, ApiMockPythonScript, api_mock_effective_contract,
    api_mock_sanitize_python_param,
};
use crate::app::api_client::{ApiMethod, ApiRouteRow, ApiSpecModel};
use rustc_hash::FxHashMap;
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiMockWorkerArgPlan {
    pub path_args: Vec<ApiMockWorkerPathArg>,
    pub query: bool,
    pub body: bool,
    pub fields: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiMockWorkerPathArg {
    pub name: String,
    pub python_name: String,
}

pub fn api_mock_worker_arg_plan(contract: &ApiMockPythonContract) -> ApiMockWorkerArgPlan {
    ApiMockWorkerArgPlan {
        path_args: enabled_fields(&contract.path_params)
            .map(|field| ApiMockWorkerPathArg {
                name: field.name.clone(),
                python_name: field.python_name.clone(),
            })
            .collect(),
        query: contract.query.enabled,
        body: contract.body.enabled,
        fields: contract.body.enabled,
    }
}

pub fn api_mock_handler_signature_lines(contract: &ApiMockPythonContract) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("def handler(".to_string());
    lines.push("    req: Request,".to_string());
    for field in enabled_fields(&contract.path_params) {
        lines.push(format!(
            "    {}: {},",
            field.python_name,
            api_mock_python_type(field)
        ));
    }
    if contract.query.enabled {
        lines.push("    query: Query,".to_string());
    }
    if contract.body.enabled {
        lines.push("    body: Body,".to_string());
        lines.push("    fields: Fields,".to_string());
    }
    lines.push(") -> Response | dict[str, Any]:".to_string());
    lines
}

pub fn api_mock_handler_signature_text(contract: &ApiMockPythonContract) -> String {
    api_mock_handler_signature_lines(contract).join("\n")
}

pub fn api_mock_contract_state_text(contract: &ApiMockPythonContract) -> String {
    let mut out = String::new();
    let mut enum_names = FxHashMap::default();
    push_contract_enums(&mut out, "Query", &contract.query, &mut enum_names);
    push_contract_enums(&mut out, "Body", &contract.body, &mut enum_names);
    push_contract_enums(&mut out, "Response", &contract.response, &mut enum_names);
    if contract.query.enabled {
        push_contract_class(&mut out, "Query", &contract.query);
    }
    if contract.body.enabled {
        if !out.is_empty() {
            out.push('\n');
        }
        push_contract_class(&mut out, "Body", &contract.body);
    }
    if contract.response.enabled {
        if !out.is_empty() {
            out.push('\n');
        }
        push_contract_class(&mut out, "Response", &contract.response);
    }
    out
}

pub fn api_mock_contract_source_text(
    script: &ApiMockPythonScript,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
) -> String {
    if script.contract_source.trim().is_empty() {
        api_mock_contract_state_text(&api_mock_effective_contract(script, route, model))
    } else {
        script.contract_source.clone()
    }
}

pub fn api_mock_contract_from_state_text(
    base: &ApiMockPythonContract,
    text: &str,
) -> ApiMockPythonContract {
    let mut contract = base.clone();
    let enum_defs = parse_str_enum_defs(text);
    let query = parse_contract_class(text, "Query", &base.query, &enum_defs);
    let body = parse_contract_class(text, "Body", &base.body, &enum_defs);
    let response = parse_contract_class(text, "Response", &base.response, &enum_defs);
    contract.query = query.unwrap_or_else(|| {
        let mut spec = base.query.clone();
        spec.enabled = false;
        spec
    });
    contract.body = body.unwrap_or_else(|| {
        let mut spec = base.body.clone();
        spec.enabled = false;
        spec
    });
    contract.response = response.unwrap_or_else(|| {
        let mut spec = base.response.clone();
        spec.enabled = false;
        spec
    });
    contract
}

pub fn api_mock_default_handler_body(contract: &ApiMockPythonContract) -> String {
    const INDENT: &str = "    ";
    const LEADING_BLANK: &str = "\n";
    if !contract.response.enabled {
        return format!("{LEADING_BLANK}{INDENT}return Response()");
    }
    let fields: Vec<_> = enabled_fields(&contract.response).collect();
    if fields.is_empty() {
        return format!("{LEADING_BLANK}{INDENT}return Response()");
    }
    if fields.len() == 1 {
        let field = fields[0];
        return format!(
            "{LEADING_BLANK}{INDENT}return Response({}={})",
            field.python_name,
            api_mock_response_sample_literal(field)
        );
    }
    let mut out = String::from(LEADING_BLANK);
    out.push_str(INDENT);
    out.push_str("return Response(\n");
    for field in fields {
        out.push_str(INDENT);
        out.push_str("    ");
        out.push_str(&field.python_name);
        out.push('=');
        out.push_str(&api_mock_response_sample_literal(field));
        out.push_str(",\n");
    }
    out.push_str(INDENT);
    out.push(')');
    out
}

pub fn api_mock_type_source_prefix(method: ApiMethod, path: &str) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n");
    out.push_str("from dataclasses import dataclass\n");
    out.push_str("from enum import StrEnum\n");
    out.push_str("from typing import Any, Annotated, Literal\n\n");
    out.push_str("# Generated by RRiter. Locked route contract.\n");
    out.push_str(&format!("# {} {}\n\n", method.as_str(), path));
    push_constraint_markers(&mut out);
    out.push_str("@dataclass(init=False)\n");
    out.push_str("class BaseModel:\n");
    out.push_str("    def __init__(self, **values: Any) -> None: ...\n\n");
    out.push_str("class UploadFile(BaseModel):\n");
    out.push_str("    filename: str\n");
    out.push_str("    content_type: str | None = None\n");
    out.push_str("    content: bytes = b\"\"\n");
    out.push_str("    size: int = 0\n\n");
    out.push_str("class Request:\n");
    out.push_str("    method: str\n    path: str\n    headers: dict[str, str]\n\n");
    out
}

pub fn api_mock_type_source_suffix(contract_source: &str) -> String {
    let mut out = String::new();
    if !contract_source_contains_class(contract_source, "Query") {
        out.push_str("class Query:\n    pass\n\n");
    }
    if !contract_source_contains_class(contract_source, "Body") {
        out.push_str("class Body:\n    pass\n\n");
    }
    if !contract_source_contains_class(contract_source, "Response") {
        out.push_str("class Response(BaseModel):\n    ok: bool = True\n\n");
    }
    out.push_str("class Fields:\n    values: dict[str, Any]\n\n");
    out.push_str("class Output:\n");
    out.push_str("    status: int = 200\n");
    out.push_str("    headers: dict[str, str] | None = None\n");
    out.push_str("    json: Any | None = None\n");
    out.push_str("    text: str | None = None\n\n");
    out.push_str("def json_response(data: Any, status: int = 200, headers: dict[str, str] | None = None) -> dict[str, Any]: ...\n");
    out.push_str("def text_response(text: str, status: int = 200, headers: dict[str, str] | None = None) -> dict[str, Any]: ...\n");
    out.push_str("def error_response(message: str, status: int = 500) -> dict[str, Any]: ...\n\n");
    out
}

pub fn push_contract_class(out: &mut String, name: &str, spec: &ApiMockClassSpec) {
    out.push_str("class ");
    out.push_str(name);
    out.push_str(":\n");
    let mut any = false;
    for field in enabled_fields(spec) {
        any = true;
        out.push_str("    ");
        out.push_str(&field.python_name);
        out.push_str(": ");
        out.push_str(&api_mock_python_type(field));
        if let Some(default) = api_mock_default_literal(field) {
            out.push_str(" = ");
            out.push_str(&default);
        }
        out.push('\n');
    }
    if !any {
        out.push_str("    pass\n");
    }
    out.push('\n');
}

fn push_contract_enums(
    out: &mut String,
    group_name: &str,
    spec: &ApiMockClassSpec,
    names: &mut FxHashMap<String, String>,
) {
    if !spec.enabled {
        return;
    }
    for field in enabled_fields(spec).filter(|field| !field.enum_values.is_empty()) {
        let enum_name = api_mock_enum_class_name(group_name, field, names);
        out.push_str("class ");
        out.push_str(&enum_name);
        out.push_str("(StrEnum):\n");
        for (idx, value) in field.enum_values.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&api_mock_enum_member_name(value, idx));
            out.push_str(" = ");
            out.push_str(&serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()));
            out.push('\n');
        }
        out.push('\n');
    }
}

fn contract_source_contains_class(text: &str, class_name: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("class ")
            .and_then(|rest| rest.split(['(', ':']).next())
            .is_some_and(|name| name.trim() == class_name)
    })
}

fn parse_contract_class(
    text: &str,
    class_name: &str,
    base: &ApiMockClassSpec,
    enum_defs: &FxHashMap<String, Vec<String>>,
) -> Option<ApiMockClassSpec> {
    let mut fields = Vec::new();
    let mut in_class = false;
    let mut seen = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") {
            in_class = trimmed
                .strip_prefix("class ")
                .and_then(|rest| rest.split(['(', ':']).next())
                .is_some_and(|name| name.trim() == class_name);
            if in_class {
                seen = true;
            }
            continue;
        }
        if !in_class {
            continue;
        }
        if line.chars().next().is_some_and(|ch| !ch.is_whitespace()) && !trimmed.is_empty() {
            in_class = false;
            continue;
        }
        if trimmed.is_empty()
            || trimmed == "pass"
            || trimmed.starts_with('#')
            || trimmed.starts_with("def ")
            || !trimmed.contains(':')
        {
            continue;
        }
        if let Some(field) = parse_contract_field_line(trimmed, base, enum_defs) {
            fields.push(field);
        }
    }
    if !seen {
        return None;
    }
    for base_field in &base.fields {
        let exists = fields.iter().any(|field| {
            field.name == base_field.name || field.python_name == base_field.python_name
        });
        if !exists {
            let mut field = base_field.clone();
            field.enabled = false;
            fields.push(field);
        }
    }
    Some(ApiMockClassSpec {
        enabled: true,
        fields,
    })
}

fn parse_contract_field_line(
    line: &str,
    base: &ApiMockClassSpec,
    enum_defs: &FxHashMap<String, Vec<String>>,
) -> Option<ApiMockContractField> {
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let (type_text, default_text) = split_type_and_default(rest.trim());
    let mut field = base
        .fields
        .iter()
        .find(|field| field.name == name || field.python_name == name)
        .cloned()
        .unwrap_or_else(|| {
            ApiMockContractField::new(name.to_string(), ApiMockContractFieldKind::Any, true)
        });
    field.name = name.to_string();
    field.python_name = api_mock_sanitize_python_param(name);
    field.enabled = true;
    let base_type = annotated_base_type(type_text)
        .split('|')
        .next()
        .map(str::trim)
        .unwrap_or(type_text);
    field.enum_values = parse_literal_values(type_text)
        .into_iter()
        .chain(enum_defs.get(base_type).cloned().unwrap_or_default())
        .collect();
    field.constraints = parse_contract_constraints(type_text);
    field.kind = parse_contract_field_kind(type_text);
    field.item_kind = parse_contract_item_kind(type_text);
    field.required = default_text.is_none();
    let default_is_none = default_text == Some("None");
    field.nullable = type_text.contains("None") || default_is_none;
    field.default_value = default_text.and_then(|default| {
        enum_default_to_contract_default(default, &field.enum_values)
            .or_else(|| python_default_to_contract_default(default))
    });
    field.required = !field.nullable && field.default_value.is_none();
    Some(field)
}

fn split_type_and_default(text: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    let mut quote = None;
    let bytes = text.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        if let Some(quoted) = quote {
            if ch == quoted && bytes.get(idx.wrapping_sub(1)) != Some(&b'\\') {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            '=' if depth == 0 => {
                return (text[..idx].trim(), Some(text[idx + 1..].trim()));
            }
            _ => {}
        }
        idx += 1;
    }
    (text.trim(), None)
}

fn parse_contract_field_kind(type_text: &str) -> ApiMockContractFieldKind {
    let base = annotated_base_type(type_text);
    if !parse_literal_values(base).is_empty() {
        return ApiMockContractFieldKind::String;
    }
    if base
        .split('|')
        .next()
        .map(str::trim)
        .is_some_and(|item| item.ends_with("Enum"))
    {
        return ApiMockContractFieldKind::String;
    }
    if base.contains("list[") {
        return ApiMockContractFieldKind::Array;
    }
    if base.contains("dict[") {
        return ApiMockContractFieldKind::Object;
    }
    if base.contains("int") {
        return ApiMockContractFieldKind::Integer;
    }
    if base.contains("float") {
        return ApiMockContractFieldKind::Number;
    }
    if base.contains("bool") {
        return ApiMockContractFieldKind::Boolean;
    }
    if base.contains("UploadFile") || base.contains("MultipartFile") {
        return ApiMockContractFieldKind::File;
    }
    if base.contains("bytes") {
        return ApiMockContractFieldKind::Bytes;
    }
    if base.contains("str") {
        return ApiMockContractFieldKind::String;
    }
    if base
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return ApiMockContractFieldKind::Object;
    }
    ApiMockContractFieldKind::Any
}

fn parse_contract_item_kind(type_text: &str) -> Option<ApiMockContractFieldKind> {
    let base = annotated_base_type(type_text);
    let start = base.find("list[")? + "list[".len();
    let inner = &base[start..];
    let end = inner.find(']')?;
    Some(parse_contract_field_kind(&inner[..end]))
}

fn annotated_base_type(type_text: &str) -> &str {
    let trimmed = type_text.trim();
    let Some(rest) = trimmed.strip_prefix("Annotated[") else {
        return trimmed;
    };
    let mut depth = 0i32;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => return rest[..idx].trim(),
            _ => {}
        }
    }
    trimmed
}

fn parse_literal_values(type_text: &str) -> Vec<String> {
    let Some(start) = type_text.find("Literal[") else {
        return Vec::new();
    };
    let rest = &type_text[start + "Literal[".len()..];
    let mut values = Vec::new();
    let mut quote_start = None;
    let mut quote = 0u8;
    let mut escaped = false;
    let mut depth = 1usize;
    for (idx, byte) in rest.bytes().enumerate() {
        if quote != 0 {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                if let Some(start) = quote_start.take()
                    && let Some(value) =
                        crate::languages::decode_python_string_literal(&rest[start..=idx])
                {
                    values.push(value);
                }
                quote = 0;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => {
                quote = byte;
                quote_start = Some(idx);
            }
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    values
}

fn parse_str_enum_defs(text: &str) -> FxHashMap<String, Vec<String>> {
    let mut defs: FxHashMap<String, Vec<String>> = FxHashMap::default();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") {
            current = trimmed
                .strip_prefix("class ")
                .and_then(|rest| rest.split(['(', ':']).next())
                .map(str::trim)
                .filter(|name| !name.is_empty() && trimmed.contains("StrEnum"))
                .map(str::to_string);
            if let Some(name) = &current {
                defs.entry(name.clone()).or_default();
            }
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        if line.chars().next().is_some_and(|ch| !ch.is_whitespace()) && !trimmed.is_empty() {
            current = None;
            continue;
        }
        let Some((_, value)) = trimmed.split_once('=') else {
            continue;
        };
        if let Some(value) = crate::languages::decode_python_string_literal(value) {
            defs.entry(name.clone()).or_default().push(value);
        }
    }
    defs
}

fn api_mock_enum_class_name_for_field(field: &ApiMockContractField) -> String {
    let mut out = String::new();
    for part in field.python_name.split('_').filter(|part| !part.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        out.push_str("Field");
    }
    out.push_str("Enum");
    out
}

fn api_mock_enum_class_name(
    _group_name: &str,
    field: &ApiMockContractField,
    _names: &mut FxHashMap<String, String>,
) -> String {
    api_mock_enum_class_name_for_field(field)
}

fn api_mock_enum_member_name(value: &str, idx: usize) -> String {
    let mut out = String::new();
    let mut prev_sep = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.is_empty() && ch.is_ascii_digit() {
                out.push_str("VALUE_");
            }
            out.push(ch.to_ascii_uppercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("VALUE");
        out.push_str(&idx.to_string());
    }
    out
}

fn parse_contract_constraints(type_text: &str) -> ApiMockFieldConstraints {
    let mut constraints = ApiMockFieldConstraints::default();
    constraints.min_length = parse_usize_marker(type_text, "MinLen");
    constraints.max_length = parse_usize_marker(type_text, "MaxLen");
    constraints.min_items = parse_usize_marker(type_text, "MinItems");
    constraints.max_items = parse_usize_marker(type_text, "MaxItems");
    constraints.pattern = parse_string_marker(type_text, "Pattern");
    if let Some(value) = parse_string_or_number_marker(type_text, "Ge") {
        constraints.minimum = Some(value);
    }
    if let Some(value) = parse_string_or_number_marker(type_text, "Gt") {
        constraints.minimum = Some(value);
        constraints.exclusive_minimum = true;
    }
    if let Some(value) = parse_string_or_number_marker(type_text, "Le") {
        constraints.maximum = Some(value);
    }
    if let Some(value) = parse_string_or_number_marker(type_text, "Lt") {
        constraints.maximum = Some(value);
        constraints.exclusive_maximum = true;
    }
    constraints.nullable = type_text.contains("None");
    constraints
}

fn parse_usize_marker(text: &str, marker: &str) -> Option<usize> {
    parse_marker_arg(text, marker)?.parse().ok()
}

fn parse_string_marker(text: &str, marker: &str) -> Option<String> {
    let arg = parse_marker_arg(text, marker)?;
    crate::languages::decode_python_string_literal(&arg)
}

fn parse_string_or_number_marker(text: &str, marker: &str) -> Option<String> {
    let arg = parse_marker_arg(text, marker)?;
    Some(crate::languages::decode_python_string_literal(&arg).unwrap_or(arg))
}

fn parse_marker_arg(text: &str, marker: &str) -> Option<String> {
    crate::languages::python_call_argument(text, marker).map(str::to_string)
}

fn python_default_to_contract_default(default: &str) -> Option<String> {
    if default == "None" {
        return None;
    }
    if default == "True" {
        return Some("true".to_string());
    }
    if default == "False" {
        return Some("false".to_string());
    }
    crate::languages::decode_python_string_literal(default).or_else(|| Some(default.to_string()))
}

fn enum_default_to_contract_default(default: &str, enum_values: &[String]) -> Option<String> {
    if enum_values.is_empty() || !default.contains('.') {
        return None;
    }
    let member = default.rsplit('.').next()?.trim();
    enum_values
        .iter()
        .enumerate()
        .find(|(idx, value)| api_mock_enum_member_name(value, *idx) == member)
        .map(|(_, value)| value.clone())
}

pub fn enabled_fields(spec: &ApiMockClassSpec) -> impl Iterator<Item = &ApiMockContractField> {
    spec.fields.iter().filter(|field| field.enabled)
}

pub fn api_mock_python_type(field: &ApiMockContractField) -> String {
    let mut base = if !field.enum_values.is_empty() {
        api_mock_enum_class_name_for_field(field)
    } else {
        match field.kind {
            ApiMockContractFieldKind::String => "str".to_string(),
            ApiMockContractFieldKind::Integer => "int".to_string(),
            ApiMockContractFieldKind::Number => "float".to_string(),
            ApiMockContractFieldKind::Boolean => "bool".to_string(),
            ApiMockContractFieldKind::Array => format!(
                "list[{}]",
                field.item_kind.map(base_python_type).unwrap_or("Any")
            ),
            ApiMockContractFieldKind::Object => "dict[str, Any]".to_string(),
            ApiMockContractFieldKind::Bytes => "str".to_string(),
            ApiMockContractFieldKind::File => "UploadFile".to_string(),
            ApiMockContractFieldKind::Any => "Any".to_string(),
        }
    };
    let annotations = constraint_annotations(field);
    if !annotations.is_empty() {
        base = format!("Annotated[{base}, {}]", annotations.join(", "));
    }
    if (field.nullable || !field.required) && field.default_value.is_none() {
        base.push_str(" | None");
    }
    base
}

fn base_python_type(kind: ApiMockContractFieldKind) -> &'static str {
    match kind {
        ApiMockContractFieldKind::String | ApiMockContractFieldKind::Bytes => "str",
        ApiMockContractFieldKind::Integer => "int",
        ApiMockContractFieldKind::Number => "float",
        ApiMockContractFieldKind::Boolean => "bool",
        ApiMockContractFieldKind::Array => "list[Any]",
        ApiMockContractFieldKind::Object => "dict[str, Any]",
        ApiMockContractFieldKind::File => "UploadFile",
        ApiMockContractFieldKind::Any => "Any",
    }
}

fn literal_type(values: &[String]) -> String {
    let mut out = String::from("Literal[");
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()));
    }
    out.push(']');
    out
}

fn constraint_annotations(field: &ApiMockContractField) -> Vec<String> {
    let mut out = Vec::new();
    let c = &field.constraints;
    if let Some(value) = c.min_length {
        out.push(format!("MinLen({value})"));
    }
    if let Some(value) = c.max_length {
        out.push(format!("MaxLen({value})"));
    }
    if let Some(pattern) = &c.pattern {
        out.push(format!(
            "Pattern({})",
            serde_json::to_string(pattern).unwrap_or_else(|_| "\"\"".to_string())
        ));
    }
    if let Some(value) = &c.minimum {
        out.push(format!(
            "{}({value})",
            if c.exclusive_minimum { "Gt" } else { "Ge" }
        ));
    }
    if let Some(value) = &c.maximum {
        out.push(format!(
            "{}({value})",
            if c.exclusive_maximum { "Lt" } else { "Le" }
        ));
    }
    if let Some(value) = c.min_items {
        out.push(format!("MinItems({value})"));
    }
    if let Some(value) = c.max_items {
        out.push(format!("MaxItems({value})"));
    }
    out
}

pub fn api_mock_default_literal(field: &ApiMockContractField) -> Option<String> {
    if let Some(default) = &field.default_value {
        if !field.enum_values.is_empty()
            && let Some(idx) = field.enum_values.iter().position(|value| value == default)
        {
            return Some(format!(
                "{}.{}",
                api_mock_enum_class_name_for_field(field),
                api_mock_enum_member_name(default, idx)
            ));
        }
        return Some(default_string_to_python(default, field.kind));
    }
    (field.nullable || !field.required).then(|| "None".to_string())
}

fn api_mock_response_sample_literal(field: &ApiMockContractField) -> String {
    if let Some(default) = api_mock_default_literal(field) {
        return default;
    }
    match field.kind {
        ApiMockContractFieldKind::String | ApiMockContractFieldKind::Bytes => {
            "\"value\"".to_string()
        }
        ApiMockContractFieldKind::File => "None".to_string(),
        ApiMockContractFieldKind::Integer => "1".to_string(),
        ApiMockContractFieldKind::Number => "1.0".to_string(),
        ApiMockContractFieldKind::Boolean => "True".to_string(),
        ApiMockContractFieldKind::Array => "[]".to_string(),
        ApiMockContractFieldKind::Object => "{}".to_string(),
        ApiMockContractFieldKind::Any => "None".to_string(),
    }
}

pub fn api_mock_default_json(field: &ApiMockContractField) -> Option<Value> {
    let default = field.default_value.as_ref()?;
    Some(default_string_to_json(default, field.kind))
}

fn default_string_to_python(default: &str, kind: ApiMockContractFieldKind) -> String {
    match default_string_to_json(default, kind) {
        Value::Null => "None".to_string(),
        Value::Bool(value) => {
            if value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => {
            serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string())
        }
        Value::Array(items) => {
            let mut out = String::from("[");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&json_value_to_python(item));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut out = String::from("{");
            for (idx, (key, value)) in map.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()));
                out.push_str(": ");
                out.push_str(&json_value_to_python(value));
            }
            out.push('}');
            out
        }
    }
}

fn json_value_to_python(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(value) => {
            if *value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "None".to_string())
        }
    }
}

fn default_string_to_json(default: &str, kind: ApiMockContractFieldKind) -> Value {
    if default.eq_ignore_ascii_case("null") || default == "None" {
        return Value::Null;
    }
    match kind {
        ApiMockContractFieldKind::String
        | ApiMockContractFieldKind::Bytes
        | ApiMockContractFieldKind::File => Value::String(default.to_string()),
        ApiMockContractFieldKind::Integer => default
            .parse::<i64>()
            .map(|value| json!(value))
            .unwrap_or_else(|_| Value::String(default.to_string())),
        ApiMockContractFieldKind::Number => default
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(default.to_string())),
        ApiMockContractFieldKind::Boolean => match default.to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(default.to_string()),
        },
        ApiMockContractFieldKind::Array
        | ApiMockContractFieldKind::Object
        | ApiMockContractFieldKind::Any => {
            serde_json::from_str(default).unwrap_or_else(|_| Value::String(default.to_string()))
        }
    }
}

pub fn api_mock_openapi_schema_for_field(field: &ApiMockContractField) -> Value {
    let mut schema = Map::new();
    match field.kind {
        ApiMockContractFieldKind::String
        | ApiMockContractFieldKind::Bytes
        | ApiMockContractFieldKind::File => {
            schema.insert("type".to_string(), Value::String("string".to_string()));
            if matches!(
                field.kind,
                ApiMockContractFieldKind::Bytes | ApiMockContractFieldKind::File
            ) {
                schema.insert("format".to_string(), Value::String("binary".to_string()));
            }
        }
        ApiMockContractFieldKind::Integer => {
            schema.insert("type".to_string(), Value::String("integer".to_string()));
        }
        ApiMockContractFieldKind::Number => {
            schema.insert("type".to_string(), Value::String("number".to_string()));
        }
        ApiMockContractFieldKind::Boolean => {
            schema.insert("type".to_string(), Value::String("boolean".to_string()));
        }
        ApiMockContractFieldKind::Array => {
            schema.insert("type".to_string(), Value::String("array".to_string()));
            let item_kind = field.item_kind.unwrap_or(ApiMockContractFieldKind::String);
            let mut item_field = field.clone();
            item_field.kind = item_kind;
            item_field.item_kind = None;
            item_field.constraints = ApiMockFieldConstraints::default();
            schema.insert(
                "items".to_string(),
                api_mock_openapi_schema_for_field(&item_field),
            );
        }
        ApiMockContractFieldKind::Object => {
            schema.insert("type".to_string(), Value::String("object".to_string()));
        }
        ApiMockContractFieldKind::Any => {}
    }
    if field.nullable || field.constraints.nullable {
        schema.insert("nullable".to_string(), Value::Bool(true));
    }
    if !field.enum_values.is_empty() {
        schema.insert(
            "enum".to_string(),
            Value::Array(
                field
                    .enum_values
                    .iter()
                    .map(|value| Value::String(value.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(default) = api_mock_default_json(field) {
        schema.insert("default".to_string(), default);
    }
    apply_constraints_to_schema(&mut schema, &field.constraints);
    Value::Object(schema)
}

pub fn apply_constraints_to_schema(schema: &mut Map<String, Value>, c: &ApiMockFieldConstraints) {
    if let Some(value) = c.min_length {
        schema.insert("minLength".to_string(), json!(value));
    }
    if let Some(value) = c.max_length {
        schema.insert("maxLength".to_string(), json!(value));
    }
    if let Some(value) = &c.pattern {
        schema.insert("pattern".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = &c.minimum
        && let Some(number) = numeric_json(value)
    {
        if c.exclusive_minimum {
            schema.insert("exclusiveMinimum".to_string(), number);
        } else {
            schema.insert("minimum".to_string(), number);
        }
    }
    if let Some(value) = &c.maximum
        && let Some(number) = numeric_json(value)
    {
        if c.exclusive_maximum {
            schema.insert("exclusiveMaximum".to_string(), number);
        } else {
            schema.insert("maximum".to_string(), number);
        }
    }
    if let Some(value) = c.min_items {
        schema.insert("minItems".to_string(), json!(value));
    }
    if let Some(value) = c.max_items {
        schema.insert("maxItems".to_string(), json!(value));
    }
}

fn numeric_json(value: &str) -> Option<Value> {
    if let Ok(value) = value.parse::<i64>() {
        return Some(json!(value));
    }
    value
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
}

fn push_constraint_markers(out: &mut String) {
    for name in [
        "MaxLen", "MinLen", "Pattern", "Ge", "Gt", "Le", "Lt", "MinItems", "MaxItems",
    ] {
        out.push_str("class ");
        out.push_str(name);
        out.push_str(":\n");
        out.push_str("    def __init__(self, value: Any): ...\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{ApiMockClassSpec, ApiMockContractField};
    use super::*;

    #[test]
    fn signature_and_worker_plan_drop_disabled_parts() {
        let mut contract = ApiMockPythonContract::default();
        contract.path_params.enabled = true;
        contract.path_params.fields.push(ApiMockContractField::new(
            "id",
            ApiMockContractFieldKind::String,
            true,
        ));
        contract.query.enabled = true;
        contract.query.fields.push(ApiMockContractField::new(
            "page",
            ApiMockContractFieldKind::Integer,
            false,
        ));
        contract.body.enabled = false;
        contract.body.fields.push(ApiMockContractField::new(
            "name",
            ApiMockContractFieldKind::String,
            true,
        ));

        let signature = api_mock_handler_signature_text(&contract);
        let plan = api_mock_worker_arg_plan(&contract);

        assert!(signature.contains("id: str"));
        assert!(signature.contains("query: Query"));
        assert!(signature.contains(") -> Response | dict[str, Any]:"));
        assert!(!signature.contains("body: Body"));
        assert_eq!(plan.path_args[0].name, "id");
        assert!(plan.query);
        assert!(!plan.body);
    }

    #[test]
    fn class_text_uses_str_enum_constraints_and_defaults() {
        let mut field =
            ApiMockContractField::new("status", ApiMockContractFieldKind::String, false);
        field.enum_values = vec!["new".to_string(), "done".to_string()];
        field.default_value = Some("new".to_string());
        field.constraints.max_length = Some(16);
        let contract = ApiMockPythonContract {
            query: ApiMockClassSpec {
                enabled: true,
                fields: vec![field],
            },
            ..Default::default()
        };
        let out = api_mock_contract_state_text(&contract);

        assert!(out.contains("class StatusEnum(StrEnum):"));
        assert!(out.contains("NEW = \"new\""));
        assert!(out.contains("status: Annotated[StatusEnum, MaxLen(16)] = StatusEnum.NEW"));

        let parsed = api_mock_contract_from_state_text(
            &ApiMockPythonContract {
                query: ApiMockClassSpec {
                    enabled: true,
                    fields: vec![ApiMockContractField::new(
                        "status",
                        ApiMockContractFieldKind::String,
                        true,
                    )],
                },
                ..Default::default()
            },
            &out,
        );
        assert_eq!(
            parsed.query.fields[0].enum_values,
            ["new".to_string(), "done".to_string()]
        );
        assert_eq!(parsed.query.fields[0].default_value.as_deref(), Some("new"));
        assert_eq!(parsed.query.fields[0].constraints.max_length, Some(16));
    }

    #[test]
    fn class_text_uses_annotated_constraints_without_enum() {
        let mut field =
            ApiMockContractField::new("status", ApiMockContractFieldKind::String, false);
        field.default_value = Some("new".to_string());
        field.constraints.max_length = Some(16);
        let spec = ApiMockClassSpec {
            enabled: true,
            fields: vec![field],
        };
        let mut out = String::new();
        push_contract_class(&mut out, "Query", &spec);

        assert!(out.contains("MaxLen(16)"));
        assert!(out.contains("= \"new\""));
    }

    #[test]
    fn multipart_binary_body_uses_upload_file_type() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Upload", "version": "1"},
            "paths": {
                "/users": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "multipart/form-data": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "image": {"type": "string", "format": "binary"},
                                            "photos": {
                                                "type": "array",
                                                "items": {"type": "string", "format": "binary"}
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
        let model = crate::app::api_client::parse_openapi_model(
            crate::app::api_client::ApiSpecId(9),
            &spec,
        )
        .expect("parse");
        let contract =
            crate::app::api_mock::types::default_contract_from_route(&model.routes[0], &model);
        let source = api_mock_contract_state_text(&contract);
        let prefix = api_mock_type_source_prefix(ApiMethod::Post, "/users");
        let parsed = api_mock_contract_from_state_text(&contract, &source);

        assert!(source.contains("image: UploadFile | None = None"));
        assert!(source.contains("photos: list[UploadFile] | None = None"));
        assert!(prefix.contains("class UploadFile(BaseModel):"));
        assert!(prefix.contains("content: bytes = b\"\""));
        assert_eq!(parsed.body.fields[0].kind, ApiMockContractFieldKind::File);
        assert_eq!(
            parsed.body.fields[1].item_kind,
            Some(ApiMockContractFieldKind::File)
        );
    }

    #[test]
    fn response_contract_uses_openapi_schema_and_default_handler_body() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "paths": {
                "/users": {
                    "get": {
                        "responses": {
                            "200": {
                                "description": "ok",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "required": ["id"],
                                            "properties": {
                                                "id": {"type": "integer"},
                                                "name": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let model = crate::app::api_client::parse_openapi_model(
            crate::app::api_client::ApiSpecId(1),
            &spec,
        )
        .expect("parse");
        let contract =
            crate::app::api_mock::types::default_contract_from_route(&model.routes[0], &model);
        let source = api_mock_contract_state_text(&contract);
        let body = api_mock_default_handler_body(&contract);

        assert!(source.contains("class Response"));
        assert!(source.contains("id: int"));
        assert!(source.contains("name: str | None = None"));
        assert!(!source.contains("__init__"));
        assert!(body.starts_with("\n    return Response("));
        assert!(body.contains("    return Response("));
        assert!(body.contains("        id=1"));
        assert!(body.contains("        name=None"));
    }

    #[test]
    fn response_contract_parses_nested_class_references() {
        let mut base = ApiMockPythonContract::default();
        base.response.enabled = true;
        let contract = api_mock_contract_from_state_text(
            &base,
            "class User:\n    id: int\n\nclass Response:\n    user: User\n",
        );

        assert!(contract.response.enabled);
        assert_eq!(contract.response.fields[0].python_name, "user");
        assert_eq!(
            contract.response.fields[0].kind,
            ApiMockContractFieldKind::Object
        );
    }
}
