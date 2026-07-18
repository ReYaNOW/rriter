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
    state.body_file_paths.clear();
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
    file_paths: &FxHashMap<String, Vec<PathBuf>>,
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
            if let Some(paths) = file_paths.get(&prop.name) {
                out.extend(paths.iter().cloned().map(|path| ApiMultipartPart::File {
                    name: prop.name.clone(),
                    path,
                }));
                continue;
            }
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
    if let Some(response) = response {
        (
            status,
            "text/plain; charset=utf-8",
            format!(
                "Response {} schema/example not described in OpenAPI.",
                response.status
            ),
        )
    } else if is_json {
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

pub(crate) fn api_manual_route_model(
    route: &crate::app::api_mock::types::ApiManualRoute,
) -> ApiSpecModel {
    let mut model = ApiSpecModel {
        id: API_MANUAL_MOCK_SPEC_ID,
        title: "Manual Mock".to_string(),
        version: String::new(),
        openapi_version: "manual".to_string(),
        servers: vec![ApiServer {
            url: "/".to_string(),
            description: String::new(),
            variables: Vec::new(),
        }],
        routes: vec![api_manual_route_row(route)],
        route_groups: Vec::new(),
        route_display_paths: Vec::new(),
        security_schemes: Vec::new(),
        root_security: Vec::new(),
        schema_arena: Vec::new(),
    };
    model.rebuild_route_layout_cache();
    model
}

pub(crate) fn api_manual_route_row(
    route: &crate::app::api_mock::types::ApiManualRoute,
) -> ApiRouteRow {
    ApiRouteRow {
        tag: "Manual".to_string(),
        method: route.method,
        path: route.path.clone(),
        summary: "Manual mock route".to_string(),
        description: String::new(),
        operation_id: route.stable_id.clone(),
        security: None,
        path_params: crate::app::api_mock::types::api_mock_path_param_names(&route.path)
            .into_iter()
            .map(|name| ApiParam {
                name,
                location: ApiParamLocation::Path,
                required: true,
                primitive_type: ApiPrimitiveType::String,
                item_type: None,
                enum_values: Vec::new(),
                default_value: None,
                example: None,
                examples: Vec::new(),
                description: String::new(),
                constraints: ApiMockFieldConstraints::default(),
            })
            .collect(),
        query_params: Vec::new(),
        request_body: None,
        responses: Vec::new(),
    }
}

pub(crate) fn api_route_input_schema_text(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
) -> String {
    let mut out = String::new();
    {
        let mut sink = ApiSchemaTextSink::text(&mut out);
        append_input_schema_document(&mut sink, route, model, media_idx, collapsed);
    }
    out
}

pub(crate) fn api_mock_input_schema_text(
    contract: &crate::app::api_mock::types::ApiMockPythonContract,
) -> String {
    let mut out = String::new();
    {
        let mut sink = ApiSchemaTextSink::text(&mut out);
        append_mock_contract_input_schema_document(&mut sink, contract);
    }
    out
}

pub(crate) fn api_mock_input_schema_summary(
    contract: &crate::app::api_mock::types::ApiMockPythonContract,
) -> String {
    let path_count = enabled_mock_contract_fields(&contract.path_params).count();
    let query_count = enabled_mock_contract_fields(&contract.query).count();
    let body_count = enabled_mock_contract_fields(&contract.body).count();
    if path_count == 0 && query_count == 0 && body_count == 0 {
        return "Mock contract input not described".to_string();
    }
    format!("Mock contract · path {path_count} · query {query_count} · body {body_count}")
}

pub(crate) fn api_route_input_schema_fold_key_at_line(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
    line_idx: usize,
) -> Option<String> {
    let mut sink = ApiSchemaTextSink::locator(line_idx);
    append_input_schema_document(&mut sink, route, model, media_idx, collapsed);
    sink.hit
}

fn append_mock_contract_input_schema_document(
    sink: &mut ApiSchemaTextSink<'_>,
    contract: &crate::app::api_mock::types::ApiMockPythonContract,
) {
    let mut wrote_input = append_mock_contract_schema_fields(sink, &contract.path_params);
    wrote_input |= append_mock_contract_schema_fields(sink, &contract.query);
    wrote_input |= append_mock_contract_schema_fields(sink, &contract.body);
    if !wrote_input {
        sink.push_line(None, |out| {
            out.push_str("Input schema not described in mock contract.")
        });
    }
}

fn append_mock_contract_schema_fields(
    sink: &mut ApiSchemaTextSink<'_>,
    spec: &crate::app::api_mock::types::ApiMockClassSpec,
) -> bool {
    let fields = enabled_mock_contract_fields(spec).collect::<Vec<_>>();
    if fields.is_empty() {
        return false;
    }
    for field in fields {
        sink.push_line(None, |out| {
            let quoted = schema_key_literal(&field.name);
            let required = if field.required { "*" } else { "" };
            let _ = write!(
                out,
                "{quoted}{required}: {}",
                api_mock_contract_value_placeholder(field)
            );
            append_mock_contract_inline_meta(out, field);
        });
    }
    true
}

fn enabled_mock_contract_fields(
    spec: &crate::app::api_mock::types::ApiMockClassSpec,
) -> impl Iterator<Item = &crate::app::api_mock::types::ApiMockContractField> {
    spec.fields
        .iter()
        .filter(|field| spec.enabled && field.enabled)
}

fn append_mock_contract_inline_meta(
    out: &mut String,
    field: &crate::app::api_mock::types::ApiMockContractField,
) {
    let mut has_meta = false;
    append_inline_piece(
        out,
        &mut has_meta,
        api_mock_contract_kind_label(field.kind),
    );
    append_inline_opt_piece(out, &mut has_meta, "default", field.default_value.as_deref());
    append_inline_values_piece(out, &mut has_meta, "enum", &field.enum_values);
    append_inline_values_piece(out, &mut has_meta, "examples", &field.examples);
    append_inline_constraints(out, &mut has_meta, &field.constraints);
}

fn api_mock_contract_value_placeholder(
    field: &crate::app::api_mock::types::ApiMockContractField,
) -> &'static str {
    use crate::app::api_mock::types::ApiMockContractFieldKind;

    match field.kind {
        ApiMockContractFieldKind::String => "\"string\"",
        ApiMockContractFieldKind::Integer => "0",
        ApiMockContractFieldKind::Number => "0.0",
        ApiMockContractFieldKind::Boolean => "false",
        ApiMockContractFieldKind::Array => "[]",
        ApiMockContractFieldKind::Object => "{}",
        ApiMockContractFieldKind::Bytes => "\"base64\"",
        ApiMockContractFieldKind::File => "file",
        ApiMockContractFieldKind::Any => "null",
    }
}

fn api_mock_contract_kind_label(
    kind: crate::app::api_mock::types::ApiMockContractFieldKind,
) -> &'static str {
    use crate::app::api_mock::types::ApiMockContractFieldKind;

    match kind {
        ApiMockContractFieldKind::String => "str",
        ApiMockContractFieldKind::Integer => "int",
        ApiMockContractFieldKind::Number => "float",
        ApiMockContractFieldKind::Boolean => "bool",
        ApiMockContractFieldKind::Array => "list",
        ApiMockContractFieldKind::Object => "dict",
        ApiMockContractFieldKind::Bytes => "bytes",
        ApiMockContractFieldKind::File => "file",
        ApiMockContractFieldKind::Any => "Any",
    }
}

fn append_input_schema_document(
    sink: &mut ApiSchemaTextSink<'_>,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
) {
    let mut wrote_input = append_param_schema_group(sink, "path", &route.path_params);
    wrote_input |= append_param_schema_group(sink, "query", &route.query_params);
    let Some(body) = &route.request_body else {
        if wrote_input {
            return;
        }
        sink.push_line(None, |out| out.push_str("Input body schema not described in OpenAPI."));
        return;
    };
    let media = api_route_input_media_at(body, media_idx);
    if let Some(schema_ref) = media.and_then(|media| media.schema).or(body.schema) {
        append_schema_document_without_root_object(sink, schema_ref, model, "input.body", collapsed);
        return;
    }
    sink.push_line(None, |out| {
        let required = if body.required { "*" } else { "" };
        let _ = write!(out, "\"body\"{required}: null  · schema missing");
    });
}

fn append_schema_document_without_root_object(
    sink: &mut ApiSchemaTextSink<'_>,
    schema_ref: ApiSchemaRef,
    model: &ApiSpecModel,
    key_prefix: &str,
    collapsed: &FxHashSet<String>,
) {
    let Some(schema) = model.schema_arena.get(schema_ref.0) else {
        sink.push_line(None, |out| out.push_str("null  · schema missing"));
        return;
    };
    if schema.kind != ApiSchemaKind::Object {
        append_schema_entry(
            sink,
            "",
            false,
            schema_ref,
            model,
            0,
            0,
            key_prefix,
            collapsed,
            None,
        );
        return;
    }
    if schema.properties.is_empty() {
        sink.push_line(None, |out| out.push_str("{}"));
        return;
    }
    for child in schema.properties.iter().take(80) {
        append_schema_entry(
            sink,
            &child.name,
            child.required,
            child.schema,
            model,
            0,
            0,
            &schema_child_key(key_prefix, &child.name),
            collapsed,
            None,
        );
    }
    append_schema_overflow_line(sink, schema.properties.len(), 0);
}

fn append_param_schema_group(
    sink: &mut ApiSchemaTextSink<'_>,
    group_name: &str,
    params: &[ApiParam],
) -> bool {
    if params.is_empty() {
        return false;
    }
    sink.push_line(None, |out| {
        let _ = write!(out, "\"{group_name}\": {{");
    });
    for param in params {
        sink.push_line(None, |out| {
            append_indent(out, 2);
            let quoted = schema_key_literal(&param.name);
            let required = if param.required { "*" } else { "" };
            let _ = write!(
                out,
                "{quoted}{required}: {}",
                api_param_value_placeholder(param)
            );
            append_param_inline_meta(out, param);
        });
    }
    sink.push_line(None, |out| out.push('}'));
    true
}

fn append_param_inline_meta(out: &mut String, param: &ApiParam) {
    let mut has_meta = false;
    append_inline_piece(
        out,
        &mut has_meta,
        api_param_kind_label(param.primitive_type),
    );
    append_inline_trimmed_piece(out, &mut has_meta, "desc", &param.description, 72);
    append_inline_opt_piece(out, &mut has_meta, "default", param.default_value.as_deref());
    append_inline_values_piece(out, &mut has_meta, "enum", &param.enum_values);
    append_inline_values_piece(out, &mut has_meta, "examples", &param.examples);
    if let Some(example) = param.example.as_ref() {
        if !has_meta {
            out.push_str("  · ");
            has_meta = true;
        } else {
            out.push_str(" | ");
        }
        let _ = write!(out, "example={example}");
    }
    append_inline_constraints(out, &mut has_meta, &param.constraints);
}

pub(crate) fn api_route_output_example_text_for(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    status_idx: usize,
    example_idx: usize,
) -> String {
    let mut out = String::new();
    let Some(response) = api_route_response_at(route, status_idx) else {
        out.push_str("Output schema not described in OpenAPI.");
        return out;
    };
    let example = api_response_example_at(response, example_idx)
        .map(|example| example.value.clone())
        .or_else(|| {
            api_response_media_at(response, 0)
                .and_then(|media| media.schema)
                .map(|schema| schema_example_json(schema, model, 0))
        })
        .or_else(|| response.example.clone())
        .or_else(|| response.schema.map(|schema| schema_example_json(schema, model, 0)));
    if let Some(example) = example {
        let formatted = api_format_example_json(&example);
        out.push_str(&formatted);
        if !formatted.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push_str("schema/example not described\n");
    }
    out
}

fn api_format_example_json(example: &str) -> String {
    serde_json::from_str::<Value>(example)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
        .unwrap_or_else(|| example.to_string())
}

pub(crate) fn api_route_output_example_count(route: &ApiRouteRow, status_idx: usize) -> usize {
    api_route_response_at(route, status_idx)
        .map(|response| {
            response
                .media
                .iter()
                .map(|media| media.examples.len())
                .sum::<usize>()
                .max(1)
        })
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn api_route_output_example_label(
    route: &ApiRouteRow,
    status_idx: usize,
    example_idx: usize,
) -> String {
    let Some(response) = api_route_response_at(route, status_idx) else {
        return "example".to_string();
    };
    api_response_example_at(response, example_idx)
        .map(|example| example.label.clone())
        .unwrap_or_else(|| "example".to_string())
}

pub(crate) fn api_route_output_example_menu_label(
    route: &ApiRouteRow,
    status_idx: usize,
    example_idx: usize,
) -> String {
    let Some(response) = api_route_response_at(route, status_idx) else {
        return "example".to_string();
    };
    let mut idx = 0usize;
    for media in &response.media {
        for example in &media.examples {
            if idx == example_idx {
                return example.label.clone();
            }
            idx += 1;
        }
    }
    "example".to_string()
}

pub(crate) fn api_route_output_schema_text_for(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    status_idx: usize,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
) -> String {
    let mut out = String::new();
    {
        let mut sink = ApiSchemaTextSink::text(&mut out);
        append_output_schema_document(&mut sink, route, model, status_idx, media_idx, collapsed);
    }
    out
}

pub(crate) fn api_route_output_schema_fold_key_at_line(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    status_idx: usize,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
    line_idx: usize,
) -> Option<String> {
    let mut sink = ApiSchemaTextSink::locator(line_idx);
    append_output_schema_document(&mut sink, route, model, status_idx, media_idx, collapsed);
    sink.hit
}

fn append_output_schema_document(
    sink: &mut ApiSchemaTextSink<'_>,
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    status_idx: usize,
    media_idx: usize,
    collapsed: &FxHashSet<String>,
) {
    let Some(response) = api_route_response_at(route, status_idx) else {
        sink.push_line(None, |out| out.push_str("Output schema not described in OpenAPI."));
        return;
    };
    let media = api_response_media_at(response, media_idx);
    if let Some(schema_ref) = media.and_then(|media| media.schema).or(response.schema) {
        append_schema_document_without_root_object(
            sink,
            schema_ref,
            model,
            "output.schema",
            collapsed,
        );
    } else {
        sink.push_line(None, |out| out.push_str("null  · not described"));
    }
}

pub(crate) fn api_route_output_schema_summary(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    status_idx: usize,
    media_idx: usize,
) -> String {
    let Some(response) = api_route_response_at(route, status_idx) else {
        return "Output schema not described".to_string();
    };
    let media = api_response_media_at(response, media_idx);
    let mut out = String::new();
    let _ = write!(out, "{}", response.status);
    let content_type = media
        .map(|media| media.content_type.as_str())
        .filter(|kind| !kind.is_empty())
        .unwrap_or(response.content_type.as_str());
    if !content_type.is_empty() {
        let _ = write!(out, " · {content_type}");
    }
    if !response.description.is_empty() {
        let _ = write!(out, " · {}", response.description);
    }
    if let Some(schema) = media
        .and_then(|media| media.schema)
        .or(response.schema)
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
    {
        if !schema.name.is_empty() {
            let _ = write!(out, " · {}", schema.name);
        }
        let _ = write!(out, " · {}", schema_kind_label(schema.kind));
    }
    out
}

pub(crate) fn api_route_output_media_count(route: &ApiRouteRow, status_idx: usize) -> usize {
    api_route_response_at(route, status_idx)
        .map(|response| response.media.len().max(1))
        .unwrap_or(0)
}

pub(crate) fn api_route_input_media_count(route: &ApiRouteRow) -> usize {
    route
        .request_body
        .as_ref()
        .map(|body| body.media.len().max(1))
        .unwrap_or(0)
}

pub(crate) fn api_route_input_media_label(route: &ApiRouteRow, media_idx: usize) -> String {
    let Some(body) = &route.request_body else {
        return "schema".to_string();
    };
    api_route_input_media_at(body, media_idx)
        .map(|media| {
            if media.content_type.is_empty() {
                "schema".to_string()
            } else {
                media.content_type.clone()
            }
        })
        .unwrap_or_else(|| {
            if body.content_type.is_empty() {
                "schema".to_string()
            } else {
                body.content_type.clone()
            }
        })
}

pub(crate) fn api_route_input_schema_summary(
    route: &ApiRouteRow,
    model: &ApiSpecModel,
    media_idx: usize,
) -> String {
    let Some(body) = &route.request_body else {
        return "Body schema not described".to_string();
    };
    let media = api_route_input_media_at(body, media_idx);
    let schema = media
        .and_then(|media| media.schema)
        .or(body.schema)
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0));
    let mut out = String::from("Body");
    if body.required {
        out.push_str(" required");
    }
    let content_type = media
        .map(|media| media.content_type.as_str())
        .filter(|content_type| !content_type.is_empty())
        .unwrap_or(body.content_type.as_str());
    if !content_type.is_empty() {
        let _ = write!(out, " · {content_type}");
    }
    if let Some(schema) = schema {
        let _ = write!(out, " · {}", schema_kind_label(schema.kind));
    }
    out
}

fn api_route_input_media_at(
    body: &ApiRequestBody,
    media_idx: usize,
) -> Option<&ApiRequestBodyMedia> {
    body.media
        .get(media_idx)
        .or_else(|| body.media.first())
}

fn api_route_response_at(route: &ApiRouteRow, status_idx: usize) -> Option<&ApiResponseSummary> {
    route
        .responses
        .get(status_idx)
        .or_else(|| route.responses.first())
}

fn api_response_media_at(
    response: &ApiResponseSummary,
    media_idx: usize,
) -> Option<&ApiResponseMedia> {
    response
        .media
        .get(media_idx)
        .or_else(|| response.media.first())
}

fn api_response_example_at(
    response: &ApiResponseSummary,
    example_idx: usize,
) -> Option<&ApiResponseExample> {
    let mut remaining = example_idx;
    for media in &response.media {
        if remaining < media.examples.len() {
            return media.examples.get(remaining);
        }
        remaining = remaining.saturating_sub(media.examples.len());
    }
    None
}

struct ApiSchemaTextSink<'a> {
    out: Option<&'a mut String>,
    target_line: Option<usize>,
    line_idx: usize,
    hit: Option<String>,
}

impl<'a> ApiSchemaTextSink<'a> {
    fn text(out: &'a mut String) -> Self {
        Self {
            out: Some(out),
            target_line: None,
            line_idx: 0,
            hit: None,
        }
    }

    fn locator(target_line: usize) -> Self {
        Self {
            out: None,
            target_line: Some(target_line),
            line_idx: 0,
            hit: None,
        }
    }

    fn push_line<F>(&mut self, fold_key: Option<&str>, write_line: F)
    where
        F: FnOnce(&mut String),
    {
        if self.target_line == Some(self.line_idx) && self.hit.is_none() {
            self.hit = fold_key.map(str::to_string);
        }
        if let Some(out) = self.out.as_deref_mut() {
            write_line(out);
            out.push('\n');
        }
        self.line_idx = self.line_idx.saturating_add(1);
    }
}

#[allow(clippy::too_many_arguments)]
fn append_schema_entry(
    sink: &mut ApiSchemaTextSink<'_>,
    label: &str,
    required: bool,
    schema_ref: ApiSchemaRef,
    model: &ApiSpecModel,
    depth: usize,
    indent: usize,
    key: &str,
    collapsed: &FxHashSet<String>,
    media_hint: Option<&str>,
) {
    if depth > 6 {
        sink.push_line(None, |out| {
            append_indent(out, indent);
            out.push_str("...");
        });
        return;
    }
    let Some(schema) = model.schema_arena.get(schema_ref.0) else {
        sink.push_line(None, |out| {
            append_indent(out, indent);
            let quoted = schema_key_literal(label);
            let required = if required { "*" } else { "" };
            let _ = write!(out, "{quoted}{required}: null  · schema missing");
        });
        return;
    };
    match schema.kind {
        ApiSchemaKind::Object => {
            let is_collapsed = collapsed.contains(key);
            sink.push_line(Some(key), |out| {
                append_indent(out, indent);
                let marker = if is_collapsed { "+" } else { "-" };
                let required = if required { "*" } else { "" };
                if label.is_empty() {
                    let _ = write!(out, "{marker} {{");
                } else {
                    let quoted = schema_key_literal(label);
                    let _ = write!(out, "{marker} {quoted}{required}: {{");
                }
                append_schema_inline_meta(out, schema, model, required == "*", media_hint);
            });
            if is_collapsed {
                return;
            }
            for child in schema.properties.iter().take(80) {
                let child_key = schema_child_key(key, &child.name);
                append_schema_entry(
                    sink,
                    &child.name,
                    child.required,
                    child.schema,
                    model,
                    depth + 1,
                    indent + 2,
                    &child_key,
                    collapsed,
                    None,
                );
            }
            append_schema_overflow_line(sink, schema.properties.len(), indent + 2);
            sink.push_line(None, |out| {
                append_indent(out, indent);
                out.push_str("},");
            });
        }
        ApiSchemaKind::Array => {
            if let Some(item) = schema.item.and_then(|item| model.schema_arena.get(item.0))
                && !matches!(item.kind, ApiSchemaKind::Object | ApiSchemaKind::Array)
            {
                sink.push_line(None, |out| {
                    append_indent(out, indent);
                    let quoted = schema_key_literal(label);
                    let required = if required { "*" } else { "" };
                    let _ = write!(out, "{quoted}{required}: [],");
                    let kind = format!("array<{}>", schema_kind_label(item.kind));
                    append_schema_inline_meta_with_kind(
                        out,
                        schema,
                        model,
                        required == "*",
                        media_hint,
                        &kind,
                    );
                });
                return;
            }
            let is_collapsed = collapsed.contains(key);
            sink.push_line(Some(key), |out| {
                append_indent(out, indent);
                let marker = if is_collapsed { "+" } else { "-" };
                let required = if required { "*" } else { "" };
                if label.is_empty() {
                    let _ = write!(out, "{marker} [");
                } else {
                    let quoted = schema_key_literal(label);
                    let _ = write!(out, "{marker} {quoted}{required}: [");
                }
                append_schema_inline_meta(out, schema, model, required == "*", media_hint);
            });
            if is_collapsed {
                return;
            }
            if let Some(item) = schema.item {
                let item_key = format!("{key}[]");
                append_schema_entry(
                    sink,
                    "",
                    false,
                    item,
                    model,
                    depth + 1,
                    indent + 2,
                    &item_key,
                    collapsed,
                    None,
                );
            } else {
                sink.push_line(None, |out| {
                    append_indent(out, indent + 2);
                    out.push_str("null  · items missing");
                });
            }
            sink.push_line(None, |out| {
                append_indent(out, indent);
                out.push_str("],");
            });
        }
        _ => {
            sink.push_line(None, |out| {
                append_indent(out, indent);
                let required = if required { "*" } else { "" };
                if label.is_empty() {
                    let _ = write!(out, "{},", schema_value_placeholder(schema.kind));
                } else {
                    let quoted = schema_key_literal(label);
                    let _ = write!(
                        out,
                        "{quoted}{required}: {},",
                        schema_value_placeholder(schema.kind)
                    );
                }
                append_schema_inline_meta(out, schema, model, required == "*", media_hint);
            });
        }
    }
}

fn append_schema_overflow_line(
    sink: &mut ApiSchemaTextSink<'_>,
    count: usize,
    indent: usize,
) {
    if count <= 80 {
        return;
    }
    sink.push_line(None, |out| {
        append_indent(out, indent);
        out.push_str("...");
    });
}

fn append_schema_inline_meta(
    out: &mut String,
    schema: &ApiSchema,
    model: &ApiSpecModel,
    required: bool,
    media_hint: Option<&str>,
) {
    append_schema_inline_meta_with_kind(
        out,
        schema,
        model,
        required,
        media_hint,
        schema_kind_label(schema.kind),
    );
}

fn append_schema_inline_meta_with_kind(
    out: &mut String,
    schema: &ApiSchema,
    model: &ApiSpecModel,
    _required: bool,
    media_hint: Option<&str>,
    kind_label: &str,
) {
    let mut has_meta = false;
    append_inline_piece(out, &mut has_meta, kind_label);
    append_inline_opt_piece(out, &mut has_meta, "media", media_hint);
    append_inline_opt_piece(
        out,
        &mut has_meta,
        "name",
        (!schema.name.is_empty()).then_some(schema.name.as_str()),
    );
    append_inline_trimmed_piece(out, &mut has_meta, "desc", &schema.description, 72);
    append_inline_opt_piece(out, &mut has_meta, "default", schema.default_value.as_deref());
    append_inline_values_piece(out, &mut has_meta, "enum", api_schema_allowed_values(schema, model));
    append_inline_values_piece(out, &mut has_meta, "examples", &schema.examples);
    append_inline_constraints(out, &mut has_meta, &schema.constraints);
}

fn append_inline_constraints(
    out: &mut String,
    has_meta: &mut bool,
    constraints: &ApiMockFieldConstraints,
) {
    append_inline_usize_piece(out, has_meta, "minLength", constraints.min_length);
    append_inline_usize_piece(out, has_meta, "maxLength", constraints.max_length);
    append_inline_opt_piece(out, has_meta, "pattern", constraints.pattern.as_deref());
    append_inline_opt_piece(
        out,
        has_meta,
        if constraints.exclusive_minimum {
            "exclusiveMinimum"
        } else {
            "minimum"
        },
        constraints.minimum.as_deref(),
    );
    append_inline_opt_piece(
        out,
        has_meta,
        if constraints.exclusive_maximum {
            "exclusiveMaximum"
        } else {
            "maximum"
        },
        constraints.maximum.as_deref(),
    );
    append_inline_usize_piece(out, has_meta, "minItems", constraints.min_items);
    append_inline_usize_piece(out, has_meta, "maxItems", constraints.max_items);
    if constraints.nullable {
        append_inline_piece(out, has_meta, "nullable");
    }
}

fn append_inline_piece(out: &mut String, has_meta: &mut bool, value: &str) {
    if value.is_empty() {
        return;
    }
    if *has_meta {
        out.push_str(", ");
    } else {
        out.push_str("  · ");
        *has_meta = true;
    }
    out.push_str(value);
}

fn append_inline_opt_piece(
    out: &mut String,
    has_meta: &mut bool,
    label: &str,
    value: Option<&str>,
) {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return;
    };
    if *has_meta {
        out.push_str(", ");
    } else {
        out.push_str("  · ");
        *has_meta = true;
    }
    let _ = write!(out, "{label}={value}");
}

fn append_inline_trimmed_piece(
    out: &mut String,
    has_meta: &mut bool,
    label: &str,
    value: &str,
    max_chars: usize,
) {
    let value = value.lines().next().unwrap_or("").trim();
    if value.is_empty() {
        return;
    }
    if *has_meta {
        out.push_str(", ");
    } else {
        out.push_str("  · ");
        *has_meta = true;
    }
    let _ = write!(out, "{label}=");
    for ch in value.chars().take(max_chars) {
        out.push(ch);
    }
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
}

fn append_inline_usize_piece(
    out: &mut String,
    has_meta: &mut bool,
    label: &str,
    value: Option<usize>,
) {
    let Some(value) = value else {
        return;
    };
    if *has_meta {
        out.push_str(", ");
    } else {
        out.push_str("  · ");
        *has_meta = true;
    }
    let _ = write!(out, "{label}={value}");
}

fn append_inline_values_piece(
    out: &mut String,
    has_meta: &mut bool,
    label: &str,
    values: &[String],
) {
    if values.is_empty() {
        return;
    }
    if *has_meta {
        out.push_str(", ");
    } else {
        out.push_str("  · ");
        *has_meta = true;
    }
    let _ = write!(out, "{label}=[");
    for (idx, value) in values.iter().take(12).enumerate() {
        if idx > 0 {
            out.push('|');
        }
        out.push_str(value);
    }
    if values.len() > 12 {
        out.push_str("|...");
    }
    out.push(']');
}

fn schema_key_literal(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string())
}

fn schema_child_key(parent: &str, child: &str) -> String {
    let mut out = String::with_capacity(parent.len() + child.len() + 1);
    out.push_str(parent);
    out.push('.');
    out.push_str(child);
    out
}

fn schema_value_placeholder(kind: ApiSchemaKind) -> &'static str {
    match kind {
        ApiSchemaKind::String => "\"string\"",
        ApiSchemaKind::Date => "\"2026-01-01\"",
        ApiSchemaKind::DateTime => "\"2026-01-01T00:00:00Z\"",
        ApiSchemaKind::Time => "\"12:00:00\"",
        ApiSchemaKind::Integer | ApiSchemaKind::Number => "0",
        ApiSchemaKind::Boolean => "false",
        ApiSchemaKind::Bytes => "🖼",
        ApiSchemaKind::Object | ApiSchemaKind::Array | ApiSchemaKind::Unknown => "null",
    }
}

fn append_indent(out: &mut String, spaces: usize) {
    for _ in 0..spaces {
        out.push(' ');
    }
}

fn schema_kind_label(kind: ApiSchemaKind) -> &'static str {
    match kind {
        ApiSchemaKind::Object => "object",
        ApiSchemaKind::Array => "array",
        ApiSchemaKind::String => "string",
        ApiSchemaKind::Date => "date",
        ApiSchemaKind::DateTime => "date-time",
        ApiSchemaKind::Time => "time",
        ApiSchemaKind::Integer => "integer",
        ApiSchemaKind::Number => "number",
        ApiSchemaKind::Boolean => "boolean",
        ApiSchemaKind::Bytes => "bytes",
        ApiSchemaKind::Unknown => "unknown",
    }
}

fn api_param_kind_label(kind: ApiPrimitiveType) -> &'static str {
    match kind {
        ApiPrimitiveType::String => "string",
        ApiPrimitiveType::Date => "date",
        ApiPrimitiveType::DateTime => "date-time",
        ApiPrimitiveType::Time => "time",
        ApiPrimitiveType::Integer => "integer",
        ApiPrimitiveType::Number => "number",
        ApiPrimitiveType::Boolean => "boolean",
        ApiPrimitiveType::Array => "array",
        ApiPrimitiveType::Object => "object",
        ApiPrimitiveType::Bytes => "bytes",
        ApiPrimitiveType::Unknown => "unknown",
    }
}

fn api_param_value_placeholder(param: &ApiParam) -> &'static str {
    match param.primitive_type {
        ApiPrimitiveType::Array => "[]",
        ApiPrimitiveType::Object => "{}",
        ApiPrimitiveType::String => "\"\"",
        ApiPrimitiveType::Date => "\"2026-01-01\"",
        ApiPrimitiveType::DateTime => "\"2026-01-01T00:00:00Z\"",
        ApiPrimitiveType::Time => "\"12:00:00\"",
        ApiPrimitiveType::Bytes => "🖼",
        ApiPrimitiveType::Integer | ApiPrimitiveType::Number => "0",
        ApiPrimitiveType::Boolean => "false",
        ApiPrimitiveType::Unknown => "null",
    }
}

pub(crate) fn schema_example_json(
    schema_ref: ApiSchemaRef,
    model: &ApiSpecModel,
    depth: usize,
) -> String {
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
        ApiSchemaKind::String => "\"\"".to_string(),
        ApiSchemaKind::Date => "\"2026-01-01\"".to_string(),
        ApiSchemaKind::DateTime => "\"2026-01-01T00:00:00Z\"".to_string(),
        ApiSchemaKind::Time => "\"12:00:00\"".to_string(),
        ApiSchemaKind::Integer | ApiSchemaKind::Number => "0".to_string(),
        ApiSchemaKind::Boolean => "false".to_string(),
        ApiSchemaKind::Bytes => "\"🖼\"".to_string(),
        ApiSchemaKind::Unknown => "null".to_string(),
    }
}

fn schema_json_literal(kind: ApiSchemaKind, value: &str) -> String {
    match kind {
        ApiSchemaKind::String
        | ApiSchemaKind::Date
        | ApiSchemaKind::DateTime
        | ApiSchemaKind::Time
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
        crate::platform::config_dir()
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

const API_AUTH_SECRET_PURPOSE: &str = "RRiter API authentication";

fn load_api_auth_checked() -> Result<ApiAuthStore, String> {
    load_api_auth_from_checked(&api_auth_path())
}

fn load_api_auth_from_checked(path: &std::path::Path) -> Result<ApiAuthStore, String> {
    let record = match std::fs::read(path) {
        Ok(record) => record,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ApiAuthStore::default());
        }
        Err(error) => return Err(format!("API credentials не прочитаны: {error}")),
    };
    let parse_result = crate::platform::open_user_secret(&record, API_AUTH_SECRET_PURPOSE)
        .map_err(|error| format!("API credentials не расшифрованы: {error}"))
        .and_then(|content| {
            serde_json::from_slice::<ApiAuthStore>(&content)
                .map_err(|error| format!("API credentials повреждены: {error}"))
        });
    if let Err(error) = parse_result {
        let backup_note = crate::platform::corrupt_file_backup_note(path);
        return Err(format!("{error}{backup_note}"));
    }
    parse_result
}

fn load_api_auth() -> ApiAuthStore {
    load_api_auth_checked().unwrap_or_default()
}

fn save_api_auth(auth: &ApiAuthStore) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(auth).map_err(|err| err.to_string())?;
    let record = crate::platform::seal_user_secret(&content, API_AUTH_SECRET_PURPOSE)
        .map_err(|err| err.to_string())?;
    crate::platform::atomic_write_secret(&api_auth_path(), &record)
        .map_err(|err| err.to_string())
}

fn save_url_cache(id: ApiSpecId, raw: &str) -> Result<(), String> {
    save_url_cache_to(&api_cache_dir().join(format!("{}.json", id.0)), raw)
}

fn save_url_cache_to(path: &std::path::Path, raw: &str) -> Result<(), String> {
    crate::platform::atomic_write(path, raw.as_bytes())
        .map_err(|error| format!("OpenAPI URL cache не сохранён: {error}"))
}

fn read_url_cache(id: ApiSpecId) -> Option<String> {
    std::fs::read_to_string(api_cache_dir().join(format!("{}.json", id.0))).ok()
}

pub(crate) fn api_python_runtime_dialog_layout(
    width: f32,
    height: f32,
    scale: f32,
) -> ApiPythonRuntimeDialogLayout {
    let available_w = (width - 32.0 * scale).max(0.0);
    let available_h = (height - 32.0 * scale).max(0.0);
    let box_w = (crate::app::file_tree::FILE_TREE_DIALOG_W * scale).min(available_w);
    let box_h = (500.0 * scale).min(available_h);
    let pad = (crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * scale)
        .min(box_w * 0.5);
    let box_x = ((width - box_w) / 2.0).round();
    let box_y = ((height - box_h) / 2.0).round();
    ApiPythonRuntimeDialogLayout {
        box_x,
        box_y,
        box_w,
        box_h,
        pad,
        content_w: (box_w - pad * 2.0).max(0.0),
    }
}

pub(crate) fn api_python_version_list_rect(
    layout: ApiPythonRuntimeDialogLayout,
    scale: f32,
) -> (f32, f32, f32, f32) {
    let y = (layout.box_y + 210.0 * scale).min(layout.box_y + layout.box_h);
    let footer_y = (layout.box_y + layout.box_h - 64.0 * scale).max(layout.box_y);
    (
        layout.box_x + layout.pad,
        y,
        layout.content_w.max(0.0),
        (footer_y - y).max(0.0).min(158.0 * scale),
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
        layout.content_w.max(0.0),
        (btn_y - y - 12.0 * scale).max(0.0),
    )
}

pub(crate) fn api_python_install_log_max_scroll(count: usize, view_h: f32, scale: f32) -> f32 {
    (count as f32 * api_python_install_log_line_height(scale) - view_h).max(0.0)
}

pub(crate) fn api_python_install_log_line_height(scale: f32) -> f32 {
    18.0 * scale
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
