use super::contract::api_mock_openapi_schema_for_field;
use super::types::{
    ApiMockClassSpec, ApiMockResponse, ApiMockState, api_mock_effective_contract,
    api_mock_source_key, default_contract_for_manual_route,
};
use crate::app::api_client::{
    ApiMethod, ApiParam, ApiParamLocation, ApiRequestBody, ApiRouteRow, ApiSchema, ApiSchemaKind,
    ApiSpecEntry, ApiSpecModel,
};
use serde_json::{Map, Value, json};

pub fn export_openapi_value(
    entry: &ApiSpecEntry,
    model: &ApiSpecModel,
    state: &ApiMockState,
    raw_json: Option<&str>,
) -> Value {
    let mut root = raw_json
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| synthesize_model_openapi(model));
    ensure_root_shape(&mut root, model);
    let source_key = api_mock_source_key(entry);
    for route in &model.routes {
        let override_route = state.route_overrides.iter().find(|item| {
            item.source_key == source_key && item.method == route.method && item.path == route.path
        });
        let operation = ensure_operation_mut(&mut root, &route.path, route.method);
        if operation.get("responses").is_none() {
            operation.insert("responses".to_string(), default_response_value());
        }
        if let Some(script) = override_route
            .and_then(|item| item.python.as_ref())
            .filter(|script| script.enabled)
        {
            let contract = api_mock_effective_contract(script, route, model);
            patch_operation_contract(operation, &contract.path_params, &contract.query, &contract.body);
            operation.insert("responses".to_string(), python_response_value(&contract.response));
        } else if let Some(override_route) = override_route {
            patch_operation_response(operation, &override_route.response);
        }
    }
    for route in &state.manual_routes {
        if !route.enabled {
            continue;
        }
        let contract = route
            .python
            .as_ref()
            .map(|script| {
                if script.contract.is_empty() {
                    default_contract_for_manual_route(&route.path)
                } else {
                    script.contract.clone()
                }
            })
            .unwrap_or_else(|| default_contract_for_manual_route(&route.path));
        let operation = ensure_operation_mut(&mut root, &route.path, route.method);
        operation.clear();
        operation.insert("summary".to_string(), Value::String("RRiter mock".to_string()));
        patch_operation_contract(operation, &contract.path_params, &contract.query, &contract.body);
        patch_operation_response(operation, &route.response);
    }
    root
}

pub fn export_mock_server_openapi_value(
    specs: &[(ApiSpecEntry, ApiSpecModel)],
    state: &ApiMockState,
) -> Value {
    let mut root = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "RRiter Mock Server",
            "version": "1",
        },
        "paths": {},
    });
    for (entry, model) in specs {
        let source_key = api_mock_source_key(entry);
        for route in &model.routes {
            write_route_operation(&mut root, route, model);
            let override_route = state.route_overrides.iter().find(|item| {
                item.source_key == source_key && item.method == route.method && item.path == route.path
            });
            let operation = ensure_operation_mut(&mut root, &route.path, route.method);
            if let Some(script) = override_route
                .and_then(|item| item.python.as_ref())
                .filter(|script| script.enabled)
            {
                let contract = api_mock_effective_contract(script, route, model);
                patch_operation_contract(
                    operation,
                    &contract.path_params,
                    &contract.query,
                    &contract.body,
                );
                operation.insert("responses".to_string(), python_response_value(&contract.response));
            } else if let Some(override_route) = override_route {
                patch_operation_response(operation, &override_route.response);
            }
        }
    }
    append_manual_routes(&mut root, state);
    root
}

fn ensure_root_shape(root: &mut Value, model: &ApiSpecModel) {
    if !root.is_object() {
        *root = synthesize_model_openapi(model);
        return;
    }
    let obj = root.as_object_mut().expect("object checked");
    obj.entry("openapi".to_string())
        .or_insert_with(|| Value::String(model.openapi_version.clone()));
    obj.entry("info".to_string()).or_insert_with(|| {
        json!({
            "title": model.title.clone(),
            "version": model.version.clone(),
        })
    });
    obj.entry("paths".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
}

fn synthesize_model_openapi(model: &ApiSpecModel) -> Value {
    let mut root = json!({
        "openapi": if model.openapi_version.is_empty() { "3.1.0" } else { &model.openapi_version },
        "info": {
            "title": if model.title.is_empty() { "RRiter API" } else { &model.title },
            "version": model.version.clone(),
        },
        "paths": {},
    });
    for route in &model.routes {
        write_route_operation(&mut root, route, model);
    }
    root
}

fn write_route_operation(root: &mut Value, route: &ApiRouteRow, model: &ApiSpecModel) {
    let operation = ensure_operation_mut(root, &route.path, route.method);
    operation.clear();
    operation.insert("summary".to_string(), Value::String(route.summary.clone()));
    operation.insert("operationId".to_string(), Value::String(route.operation_id.clone()));
    let mut params = Vec::new();
    params.extend(route.path_params.iter().map(param_to_openapi));
    params.extend(route.query_params.iter().map(param_to_openapi));
    if !params.is_empty() {
        operation.insert("parameters".to_string(), Value::Array(params));
    }
    if let Some(body) = &route.request_body {
        operation.insert("requestBody".to_string(), body_to_openapi(body, model));
    }
    operation.insert("responses".to_string(), route_responses_to_openapi(route, model));
}

fn append_manual_routes(root: &mut Value, state: &ApiMockState) {
    for route in &state.manual_routes {
        if !route.enabled {
            continue;
        }
        let contract = route
            .python
            .as_ref()
            .map(|script| {
                if script.contract.is_empty() {
                    default_contract_for_manual_route(&route.path)
                } else {
                    script.contract.clone()
                }
            })
            .unwrap_or_else(|| default_contract_for_manual_route(&route.path));
        let operation = ensure_operation_mut(root, &route.path, route.method);
        operation.clear();
        operation.insert("summary".to_string(), Value::String("RRiter mock".to_string()));
        patch_operation_contract(operation, &contract.path_params, &contract.query, &contract.body);
        patch_operation_response(operation, &route.response);
    }
}

fn ensure_operation_mut<'a>(
    root: &'a mut Value,
    path: &str,
    method: ApiMethod,
) -> &'a mut Map<String, Value> {
    let root_obj = root.as_object_mut().expect("root object");
    let paths = root_obj
        .entry("paths".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !paths.is_object() {
        *paths = Value::Object(Map::new());
    }
    let paths_obj = paths.as_object_mut().expect("paths object");
    let path_item = paths_obj
        .entry(path.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !path_item.is_object() {
        *path_item = Value::Object(Map::new());
    }
    let path_obj = path_item.as_object_mut().expect("path object");
    let method_key = method_key(method).to_string();
    let operation = path_obj
        .entry(method_key)
        .or_insert_with(|| Value::Object(Map::new()));
    if !operation.is_object() {
        *operation = Value::Object(Map::new());
    }
    operation.as_object_mut().expect("operation object")
}

fn method_key(method: ApiMethod) -> &'static str {
    match method {
        ApiMethod::Get => "get",
        ApiMethod::Post => "post",
        ApiMethod::Put => "put",
        ApiMethod::Patch => "patch",
        ApiMethod::Delete => "delete",
        ApiMethod::Head => "head",
        ApiMethod::Options => "options",
        ApiMethod::Trace => "trace",
    }
}

fn patch_operation_contract(
    operation: &mut Map<String, Value>,
    path_spec: &ApiMockClassSpec,
    query_spec: &ApiMockClassSpec,
    body_spec: &ApiMockClassSpec,
) {
    let mut params = operation
        .remove("parameters")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    params.retain(|param| {
        let location = param.get("in").and_then(Value::as_str).unwrap_or("");
        let name = param.get("name").and_then(Value::as_str).unwrap_or("");
        match location {
            "path" => path_spec.enabled && field_enabled(path_spec, name),
            "query" => query_spec.enabled && field_enabled(query_spec, name),
            _ => true,
        }
    });
    upsert_contract_params(&mut params, "path", path_spec);
    upsert_contract_params(&mut params, "query", query_spec);
    if params.is_empty() {
        operation.remove("parameters");
    } else {
        operation.insert("parameters".to_string(), Value::Array(params));
    }
    if body_spec.enabled {
        operation.insert("requestBody".to_string(), request_body_from_contract(body_spec));
    } else {
        operation.remove("requestBody");
    }
}

fn field_enabled(spec: &ApiMockClassSpec, name: &str) -> bool {
    spec.fields
        .iter()
        .any(|field| field.enabled && (field.name == name || field.python_name == name))
}

fn upsert_contract_params(params: &mut Vec<Value>, location: &str, spec: &ApiMockClassSpec) {
    if !spec.enabled {
        return;
    }
    for field in spec.fields.iter().filter(|field| field.enabled) {
        let mut found = false;
        for param in params.iter_mut() {
            let same = param.get("in").and_then(Value::as_str) == Some(location)
                && param.get("name").and_then(Value::as_str) == Some(field.name.as_str());
            if same {
                if let Some(obj) = param.as_object_mut() {
                    obj.insert("schema".to_string(), api_mock_openapi_schema_for_field(field));
                    obj.insert("required".to_string(), Value::Bool(field.required || location == "path"));
                }
                found = true;
                break;
            }
        }
        if !found {
            params.push(json!({
                "name": field.name.clone(),
                "in": location,
                "required": field.required || location == "path",
                "schema": api_mock_openapi_schema_for_field(field),
            }));
        }
    }
}

fn request_body_from_contract(spec: &ApiMockClassSpec) -> Value {
    json!({
        "required": true,
        "content": {
            "application/json": {
                "schema": object_schema_from_contract(spec),
            }
        }
    })
}

fn object_schema_from_contract(spec: &ApiMockClassSpec) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in spec.fields.iter().filter(|field| field.enabled) {
        properties.insert(field.name.clone(), api_mock_openapi_schema_for_field(field));
        if field.required {
            required.push(Value::String(field.name.clone()));
        }
    }
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".to_string(), Value::Array(required));
    }
    Value::Object(schema)
}

fn patch_operation_response(operation: &mut Map<String, Value>, response: &ApiMockResponse) {
    match response {
        ApiMockResponse::Generated => {}
        ApiMockResponse::Json(text) => {
            let example = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
            operation.insert(
                "responses".to_string(),
                json!({
                    "200": {
                        "description": "RRiter mock response",
                        "content": {
                            "application/json": {
                                "example": example,
                            }
                        }
                    }
                }),
            );
        }
        ApiMockResponse::Text(text) => {
            operation.insert(
                "responses".to_string(),
                json!({
                    "200": {
                        "description": "RRiter mock response",
                        "content": {
                            "text/plain": {
                                "example": text,
                                "schema": {"type": "string"}
                            }
                        }
                    }
                }),
            );
        }
    }
}

fn python_response_value(response_spec: &ApiMockClassSpec) -> Value {
    let schema = if response_spec.enabled {
        object_schema_from_contract(response_spec)
    } else {
        Value::Object(Map::new())
    };
    json!({
        "200": {
            "description": "RRiter Python mock response",
            "content": {
                "application/json": {
                    "schema": schema
                }
            }
        }
    })
}

fn default_response_value() -> Value {
    json!({
        "200": {
            "description": "OK"
        }
    })
}

fn route_responses_to_openapi(route: &ApiRouteRow, model: &ApiSpecModel) -> Value {
    let mut responses = Map::new();
    for response in &route.responses {
        let mut item = Map::new();
        item.insert(
            "description".to_string(),
            Value::String(response.description.clone()),
        );
        if !response.content_type.is_empty() {
            let mut media = Map::new();
            if let Some(schema_ref) = response.schema
                && let Some(schema) = model.schema_arena.get(schema_ref.0)
            {
                media.insert("schema".to_string(), schema_to_openapi(schema, model));
            }
            if let Some(example) = &response.example {
                media.insert(
                    "example".to_string(),
                    serde_json::from_str(example).unwrap_or_else(|_| Value::String(example.clone())),
                );
            }
            let mut content = Map::new();
            content.insert(response.content_type.clone(), Value::Object(media));
            item.insert("content".to_string(), Value::Object(content));
        }
        responses.insert(response.status.clone(), Value::Object(item));
    }
    if responses.is_empty() {
        default_response_value()
    } else {
        Value::Object(responses)
    }
}

fn param_to_openapi(param: &ApiParam) -> Value {
    let field = super::types::ApiMockContractField {
        name: param.name.clone(),
        python_name: super::types::api_mock_sanitize_python_param(&param.name),
        enabled: true,
        kind: super::types::ApiMockContractFieldKind::from_primitive(param.primitive_type),
        item_kind: param
            .item_type
            .map(super::types::ApiMockContractFieldKind::from_primitive),
        required: param.required,
        nullable: param.constraints.nullable,
        enum_values: param.enum_values.clone(),
        default_value: param.default_value.clone(),
        examples: param.examples.clone(),
        constraints: param.constraints.clone(),
    };
    json!({
        "name": param.name.clone(),
        "in": match param.location { ApiParamLocation::Path => "path", ApiParamLocation::Query => "query" },
        "required": param.required,
        "schema": api_mock_openapi_schema_for_field(&field),
    })
}

fn body_to_openapi(body: &ApiRequestBody, model: &ApiSpecModel) -> Value {
    let schema = body
        .schema
        .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
        .map(|schema| schema_to_openapi(schema, model))
        .unwrap_or_else(|| json!({}));
    let mut media = Map::new();
    media.insert("schema".to_string(), schema);
    let mut content = Map::new();
    content.insert(body.content_type.clone(), Value::Object(media));
    json!({
        "required": body.required,
        "content": Value::Object(content),
    })
}

fn schema_to_openapi(schema: &ApiSchema, model: &ApiSpecModel) -> Value {
    let mut out = Map::new();
    match schema.kind {
        ApiSchemaKind::Object => {
            out.insert("type".to_string(), Value::String("object".to_string()));
            let mut props = Map::new();
            let mut required = Vec::new();
            for prop in &schema.properties {
                if let Some(prop_schema) = model.schema_arena.get(prop.schema.0) {
                    props.insert(prop.name.clone(), schema_to_openapi(prop_schema, model));
                    if prop.required {
                        required.push(Value::String(prop.name.clone()));
                    }
                }
            }
            out.insert("properties".to_string(), Value::Object(props));
            if !required.is_empty() {
                out.insert("required".to_string(), Value::Array(required));
            }
        }
        ApiSchemaKind::Array => {
            out.insert("type".to_string(), Value::String("array".to_string()));
            if let Some(item) = schema.item.and_then(|item_ref| model.schema_arena.get(item_ref.0)) {
                out.insert("items".to_string(), schema_to_openapi(item, model));
            }
        }
        ApiSchemaKind::String | ApiSchemaKind::Date | ApiSchemaKind::DateTime => {
            out.insert("type".to_string(), Value::String("string".to_string()));
        }
        ApiSchemaKind::Integer => {
            out.insert("type".to_string(), Value::String("integer".to_string()));
        }
        ApiSchemaKind::Number => {
            out.insert("type".to_string(), Value::String("number".to_string()));
        }
        ApiSchemaKind::Boolean => {
            out.insert("type".to_string(), Value::String("boolean".to_string()));
        }
        ApiSchemaKind::Bytes => {
            out.insert("type".to_string(), Value::String("string".to_string()));
            out.insert("format".to_string(), Value::String("binary".to_string()));
        }
        ApiSchemaKind::Unknown => {}
    }
    if !schema.enum_values.is_empty() {
        out.insert(
            "enum".to_string(),
            Value::Array(
                schema
                    .enum_values
                    .iter()
                    .map(|value| Value::String(value.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(default) = &schema.default_value {
        out.insert(
            "default".to_string(),
            serde_json::from_str(default).unwrap_or_else(|_| Value::String(default.clone())),
        );
    }
    super::contract::apply_constraints_to_schema(&mut out, &schema.constraints);
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::{ApiSpecId, ApiSpecSource, parse_openapi_model};
    use crate::app::api_mock::types::{
        ApiMockRouteOverride, default_api_mock_python_script, default_contract_from_route,
    };

    #[test]
    fn export_openapi_keeps_constraints_and_drops_disabled_query() {
        let spec = json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "paths": {
                "/users/{id}": {
                    "get": {
                        "parameters": [
                            {"name": "id", "in": "path", "required": true, "schema": {"type": "string", "maxLength": 12}},
                            {"name": "page", "in": "query", "schema": {"type": "integer", "default": 1}}
                        ],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let entry = ApiSpecEntry {
            id: ApiSpecId(3),
            title: "Demo".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.test/openapi.json".to_string()),
            last_loaded: None,
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: true,
            error: None,
        };
        let model = parse_openapi_model(entry.id, &spec).expect("parse");
        let mut script = default_api_mock_python_script();
        script.contract = default_contract_from_route(&model.routes[0], &model);
        script.contract.query.enabled = false;
        let mut state = ApiMockState::default();
        state.route_overrides.push(ApiMockRouteOverride {
            source_key: api_mock_source_key(&entry),
            method: ApiMethod::Get,
            path: "/users/{id}".to_string(),
            enabled: true,
            response: ApiMockResponse::Generated,
            python: Some(script),
            extra_input_fields: Vec::new(),
            extra_output_fields: Vec::new(),
        });

        let exported = export_openapi_value(&entry, &model, &state, None);
        let params = exported
            .pointer("/paths/~1users~1{id}/get/parameters")
            .and_then(Value::as_array)
            .expect("params");

        assert_eq!(params.len(), 1);
        assert_eq!(params[0].get("name").and_then(Value::as_str), Some("id"));
        assert_eq!(
            exported.pointer("/paths/~1users~1{id}/get/parameters/0/schema/maxLength"),
            Some(&json!(12))
        );
    }
}
