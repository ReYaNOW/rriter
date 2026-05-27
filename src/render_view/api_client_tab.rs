use crate::app::api_client::{
    API_BODY_TEXT_SCALE, API_MOCK_TY_POPUP_BYTE, ApiFocus, ApiParam, ApiResponseView, ApiSchema,
    ApiSchemaKind, ApiSecuritySchemeKind, api_array_edit_parts, api_array_value_parts,
    api_auth_related_route_count, api_auth_route_rank, api_auth_scheme_row_height,
    api_body_text_area_height, api_response_text, api_response_text_area_height,
    api_route_auth_missing, api_route_auth_scheme_indices, api_schema_allowed_values,
    api_generated_response_for_route, api_mock_body_editor_text, api_mock_lan_url,
    api_schema_is_array_input,
    api_schema_is_file_input, api_schema_is_multi_file_input, api_text_area_line_height,
    api_text_area_max_scroll_x, json_body_is_valid, write_api_path_display,
};
use crate::app::api_mock::ty_check::ApiMockSourcePart;
use crate::app::api_mock::types::{api_mock_path_param_names, api_mock_sanitize_python_param};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton, IconType};
use glow::HasContext;

const API_SECTION_TITLE_SCALE: f32 = 0.92;
const API_FIELD_NAME_SCALE: f32 = 0.94;
const API_FIELD_TYPE_SCALE: f32 = 0.84;
const API_FIELD_VALUE_SCALE: f32 = 0.88;
const API_FIELD_META_SCALE: f32 = 0.78;
#[derive(Clone, Copy)]
struct ApiFieldRowLayout {
    row_h: f32,
    input_x: f32,
    input_w: f32,
    input_h: f32,
    right_x: f32,
    right_w: f32,
}

fn api_mock_signature_lines(path: &str) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("def handler(".to_string());
    lines.push("    req: Request,".to_string());
    for name in api_mock_path_param_names(path) {
        lines.push(format!(
            "    {}: str,",
            api_mock_sanitize_python_param(&name)
        ));
    }
    lines.push("    query: Query,".to_string());
    lines.push("    body: Body | None,".to_string());
    lines.push("    fields: Fields,".to_string());
    lines.push(") -> dict[str, Any]:".to_string());
    lines
}

fn api_mock_signature_text(path: &str) -> String {
    api_mock_signature_lines(path).join("\n")
}

fn api_mock_path_param_count(path: &str) -> usize {
    let mut count = 0usize;
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        if !after[..end].trim().is_empty() {
            count += 1;
        }
        rest = &after[end + 1..];
    }
    count
}

fn api_mock_signature_block_height(path: &str, s: f32) -> f32 {
    let line_h = api_text_area_line_height(s);
    let line_count = 6 + api_mock_path_param_count(path);
    line_count as f32 * line_h + 12.0 * s
}

fn editor_line_number_text<'a>(line_no: usize, buf: &'a mut [u8; 20]) -> Option<&'a str> {
    let mut n = line_no;
    let mut idx = 20;
    if n == 0 {
        idx -= 1;
        buf[idx] = b'0';
    } else {
        while n > 0 {
            idx -= 1;
            buf[idx] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    std::str::from_utf8(&buf[idx..]).ok()
}


include!("api_client_tab/api_client_tab_main_renderer.rs");
include!("api_client_tab/api_client_tab_auth_renderer.rs");
include!("api_client_tab/api_client_tab_field_renderer.rs");
include!("api_client_tab/api_client_tab_python_renderer.rs");

fn api_rect_intersection(
    a: (f32, f32, f32, f32),
    b: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32)> {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    (x2 > x1 && y2 > y1).then_some((x1, y1, x2 - x1, y2 - y1))
}

fn api_centered_text_y(y: f32, h: f32, scale: f32) -> f32 {
    y + h * 0.5 + 4.5 * scale
}

fn api_split_label_text_y(y: f32, h: f32, scale: f32, bottom: bool) -> f32 {
    y + h * if bottom { 0.74 } else { 0.30 } + 4.5 * scale
}

fn response_auth_token_flags(response: &crate::app::api_client::ApiJobResponse) -> (bool, bool) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&response.body) else {
        return (false, false);
    };
    (
        json.get("access_token")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        json.get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .is_some(),
    )
}

fn byte_offset_for_char_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(idx, _)| idx)
        .unwrap_or(line.len())
}

fn json_string_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut idx = start.saturating_add(1);
    let mut escaped = false;
    while idx < bytes.len() {
        let b = bytes[idx];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            return idx + 1;
        }
        idx += 1;
    }
    line.len()
}

fn json_string_is_property(line: &str, string_end: usize) -> bool {
    let bytes = line.as_bytes();
    let mut idx = string_end;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    bytes.get(idx).is_some_and(|b| *b == b':')
}

fn json_number_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut idx = start;
    while idx < bytes.len()
        && (bytes[idx].is_ascii_digit() || matches!(bytes[idx], b'-' | b'+' | b'.' | b'e' | b'E'))
    {
        idx += 1;
    }
    idx.max(start + 1)
}

fn json_keyword_end(line: &str, start: usize) -> Option<usize> {
    for kw in ["true", "false", "null"] {
        let end = start + kw.len();
        if line.get(start..end) == Some(kw) && json_token_boundary(line, end) {
            return Some(end);
        }
    }
    None
}

fn json_token_boundary(line: &str, idx: usize) -> bool {
    line.as_bytes()
        .get(idx)
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

fn header_value_is_number(value: &str) -> bool {
    let value = value.trim();
    value.bytes().any(|b| b.is_ascii_digit()) && value.parse::<f64>().is_ok()
}

fn api_param_type_text(param: &ApiParam) -> String {
    if matches!(
        param.primitive_type,
        crate::app::api_client::ApiPrimitiveType::Array
    ) {
        let item = param
            .item_type
            .map(api_primitive_type_text)
            .unwrap_or("any");
        return format!("array<{item}>");
    }
    if !param.enum_values.is_empty() {
        "enum".to_string()
    } else {
        api_primitive_type_text(param.primitive_type).to_string()
    }
}

fn api_primitive_type_text(kind: crate::app::api_client::ApiPrimitiveType) -> &'static str {
    api_schema_type_text(match kind {
        crate::app::api_client::ApiPrimitiveType::String => ApiSchemaKind::String,
        crate::app::api_client::ApiPrimitiveType::Date => ApiSchemaKind::Date,
        crate::app::api_client::ApiPrimitiveType::DateTime => ApiSchemaKind::DateTime,
        crate::app::api_client::ApiPrimitiveType::Integer => ApiSchemaKind::Integer,
        crate::app::api_client::ApiPrimitiveType::Number => ApiSchemaKind::Number,
        crate::app::api_client::ApiPrimitiveType::Boolean => ApiSchemaKind::Boolean,
        crate::app::api_client::ApiPrimitiveType::Array => ApiSchemaKind::Array,
        crate::app::api_client::ApiPrimitiveType::Object => ApiSchemaKind::Object,
        crate::app::api_client::ApiPrimitiveType::Bytes => ApiSchemaKind::Bytes,
        crate::app::api_client::ApiPrimitiveType::Unknown => ApiSchemaKind::Unknown,
    })
}

fn api_schema_type_text(kind: ApiSchemaKind) -> &'static str {
    match kind {
        ApiSchemaKind::Object => "object",
        ApiSchemaKind::Array => "array",
        ApiSchemaKind::String => "string",
        ApiSchemaKind::Date => "date",
        ApiSchemaKind::DateTime => "date-time",
        ApiSchemaKind::Integer => "int",
        ApiSchemaKind::Number => "number",
        ApiSchemaKind::Boolean => "bool",
        ApiSchemaKind::Bytes => "bytes",
        ApiSchemaKind::Unknown => "any",
    }
}

fn api_body_schema_type_text(
    schema: &ApiSchema,
    model: &crate::app::api_client::ApiSpecModel,
) -> String {
    if api_schema_is_multi_file_input(schema, model) {
        "files".to_string()
    } else if matches!(schema.kind, ApiSchemaKind::Bytes) {
        "file".to_string()
    } else if matches!(schema.kind, ApiSchemaKind::Array) {
        if let Some(item) = schema.item.and_then(|item| model.schema_arena.get(item.0)) {
            format!("array<{}>", api_schema_type_text(item.kind))
        } else {
            "array<any>".to_string()
        }
    } else if !api_schema_allowed_values(schema, model).is_empty() {
        "enum".to_string()
    } else {
        api_schema_type_text(schema.kind).to_string()
    }
}

fn api_status_color(status: Option<u16>) -> [f32; 4] {
    match status {
        Some(200..=399) => [0.48, 0.86, 0.52, 1.0],
        Some(400..=499) => [0.35, 0.75, 1.0, 1.0],
        Some(500..=599) => [1.0, 0.42, 0.42, 1.0],
        _ => [0.68, 0.70, 0.78, 1.0],
    }
}
