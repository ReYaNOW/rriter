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

pub(crate) fn api_route_model_for_identity<'a>(
    api: &'a ApiClientState,
    spec_id: ApiSpecId,
    route_identity: Option<&ApiClientRouteIdentity>,
) -> Option<std::borrow::Cow<'a, ApiSpecModel>> {
    match route_identity {
        Some(ApiClientRouteIdentity::Manual { stable_id }) => api
            .mock
            .manual_routes
            .iter()
            .find(|route| route.stable_id == *stable_id)
            .map(|route| std::borrow::Cow::Owned(api_manual_route_model(route))),
        _ => api.models.get(&spec_id).map(std::borrow::Cow::Borrowed),
    }
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

include!("api_client_schema_documents.rs");
include!("api_client_persistence_logs.rs");
