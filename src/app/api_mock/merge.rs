use super::types::{
    ApiMockMode, ApiMockRouteDecision, ApiMockRouteOrigin, ApiMockRuntimeRoute, ApiMockState,
    api_mock_route_key, api_mock_source_key,
};
use crate::app::api_client::{
    ApiMethod, ApiSpecEntry, ApiSpecModel, api_generated_response_for_route,
};
use std::collections::BTreeMap;

pub fn build_api_mock_routes<'a>(
    specs: impl IntoIterator<Item = (&'a ApiSpecEntry, &'a ApiSpecModel)>,
    state: &ApiMockState,
) -> Vec<ApiMockRuntimeRoute> {
    let mut out = Vec::new();

    for route in &state.manual_routes {
        out.push(ApiMockRuntimeRoute {
            id: format!("manual:{}", route.stable_id),
            source_key: "manual".to_string(),
            method: route.method,
            path: route.path.clone(),
            enabled: route.enabled,
            response: route.response.clone(),
            generated_status: 200,
            generated_content_type: "application/json",
            generated_body: "{}".to_string(),
            python: route.python.clone(),
            input_fields: route.input_fields.clone(),
            output_fields: route.output_fields.clone(),
            origin: ApiMockRouteOrigin::Manual,
        });
    }

    for (entry, model) in specs {
        let source_key = api_mock_source_key(entry);
        for route in &model.routes {
            let override_route = state.route_overrides.iter().find(|override_route| {
                override_route.source_key == source_key
                    && override_route.method == route.method
                    && override_route.path == route.path
            });
            let enabled = override_route.is_some_and(|override_route| override_route.enabled);
            let response = override_route
                .map(|override_route| override_route.response.clone())
                .unwrap_or(super::types::ApiMockResponse::Generated);
            let python = override_route.and_then(|override_route| override_route.python.clone());
            let input_fields = override_route
                .map(|override_route| override_route.extra_input_fields.clone())
                .unwrap_or_default();
            let output_fields = override_route
                .map(|override_route| override_route.extra_output_fields.clone())
                .unwrap_or_default();
            let (generated_status, generated_content_type, generated_body) =
                api_generated_response_for_route(route, model);

            out.push(ApiMockRuntimeRoute {
                id: api_mock_route_key(&source_key, route.method, &route.path),
                source_key: source_key.clone(),
                method: route.method,
                path: route.path.clone(),
                enabled,
                response,
                generated_status,
                generated_content_type,
                generated_body,
                python,
                input_fields,
                output_fields,
                origin: ApiMockRouteOrigin::OpenApi,
            });
        }
    }

    out
}

pub fn resolve_api_mock_route<'a>(
    routes: &'a [ApiMockRuntimeRoute],
    mode: ApiMockMode,
    method: ApiMethod,
    path: &str,
) -> ApiMockRouteDecision<'a> {
    if let Some(route) = routes.iter().find(|route| {
        route.origin == ApiMockRouteOrigin::Manual
            && route.enabled
            && route.method == method
            && route.path == path
    }) {
        return ApiMockRouteDecision::Mock(route);
    }

    if let Some(route) = routes.iter().find(|route| {
        route.origin == ApiMockRouteOrigin::OpenApi
            && route.enabled
            && route.method == method
            && api_mock_path_matches(&route.path, path)
    }) {
        return ApiMockRouteDecision::Mock(route);
    }

    if mode == ApiMockMode::MockAll {
        if let Some(route) = routes.iter().find(|route| {
            route.origin == ApiMockRouteOrigin::OpenApi
                && route.method == method
                && api_mock_path_matches(&route.path, path)
        }) {
            return ApiMockRouteDecision::Mock(route);
        }
    }

    if mode == ApiMockMode::MockSelectedProxyRest {
        ApiMockRouteDecision::Proxy
    } else {
        ApiMockRouteDecision::NotFound
    }
}

fn api_mock_path_matches(pattern: &str, path: &str) -> bool {
    api_mock_path_params(pattern, path).is_some()
}

pub fn api_mock_path_params(pattern: &str, path: &str) -> Option<BTreeMap<String, String>> {
    if pattern == path {
        return Some(BTreeMap::new());
    }
    let mut params = BTreeMap::new();

    let mut pattern_parts = pattern.trim_matches('/').split('/');
    let mut path_parts = path.trim_matches('/').split('/');
    loop {
        match (pattern_parts.next(), path_parts.next()) {
            (None, None) => return Some(params),
            (Some(pattern_part), Some(path_part))
                if pattern_part.starts_with('{') && pattern_part.ends_with('}') =>
            {
                if path_part.is_empty() {
                    return None;
                }
                let name = &pattern_part[1..pattern_part.len().saturating_sub(1)];
                params.insert(name.to_string(), path_part.to_string());
            }
            (Some(pattern_part), Some(path_part)) if pattern_part == path_part => {}
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::{ApiRouteRow, ApiSpecId, ApiSpecSource, parse_openapi_model};

    fn entry() -> ApiSpecEntry {
        ApiSpecEntry {
            id: ApiSpecId(7),
            title: "Demo".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.test/openapi.json#frag".to_string()),
            last_loaded: None,
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: true,
            error: None,
        }
    }

    fn model(route: ApiRouteRow) -> ApiSpecModel {
        ApiSpecModel {
            id: ApiSpecId(7),
            title: "Demo".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            servers: Vec::new(),
            routes: vec![route],
            security_schemes: Vec::new(),
            root_security: Vec::new(),
            schema_arena: Vec::new(),
        }
    }

    fn route(method: ApiMethod, path: &str) -> ApiRouteRow {
        ApiRouteRow {
            tag: String::new(),
            method,
            path: path.to_string(),
            summary: String::new(),
            operation_id: String::new(),
            security: None,
            path_params: Vec::new(),
            query_params: Vec::new(),
            request_body: None,
            responses: Vec::new(),
        }
    }

    #[test]
    fn mock_all_uses_generated_openapi_route() {
        let state = ApiMockState::default();
        let entry = entry();
        let model = model(route(ApiMethod::Get, "/users/{id}"));
        let routes = build_api_mock_routes([(&entry, &model)], &state);

        let decision =
            resolve_api_mock_route(&routes, ApiMockMode::MockAll, ApiMethod::Get, "/users/42");

        assert!(matches!(decision, ApiMockRouteDecision::Mock(_)));
    }

    #[test]
    fn selected_only_returns_404_when_route_not_enabled() {
        let state = ApiMockState {
            mode: ApiMockMode::MockSelectedOnly,
            ..Default::default()
        };
        let entry = entry();
        let model = model(route(ApiMethod::Get, "/users"));
        let routes = build_api_mock_routes([(&entry, &model)], &state);

        let decision = resolve_api_mock_route(
            &routes,
            ApiMockMode::MockSelectedOnly,
            ApiMethod::Get,
            "/users",
        );

        assert_eq!(decision, ApiMockRouteDecision::NotFound);
    }

    #[test]
    fn selected_proxy_mode_falls_back_to_proxy() {
        let state = ApiMockState {
            mode: ApiMockMode::MockSelectedProxyRest,
            ..Default::default()
        };

        let decision = resolve_api_mock_route(&[], state.mode, ApiMethod::Post, "/missing");

        assert_eq!(decision, ApiMockRouteDecision::Proxy);
    }

    #[test]
    fn path_params_extract_named_segments() {
        let params = api_mock_path_params("/users/{id}/posts/{post_id}", "/users/42/posts/7")
            .expect("match");

        assert_eq!(params.get("id").map(String::as_str), Some("42"));
        assert_eq!(params.get("post_id").map(String::as_str), Some("7"));
    }

    #[test]
    fn generated_response_uses_openapi_schema_without_mock_marker() {
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
                                            "properties": {
                                                "name": {"type": "string"},
                                                "active": {"type": "boolean"}
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
        let entry = entry();
        let model = parse_openapi_model(entry.id, &spec).expect("parse");
        let routes = build_api_mock_routes([(&entry, &model)], &ApiMockState::default());
        let (_, content_type, body) = routes[0].static_response_text();

        assert_eq!(content_type, "application/json");
        assert!(body.contains("\"name\""));
        assert!(!body.contains("\"mock\""));
    }

    #[test]
    fn manual_generated_response_falls_back_to_empty_json() {
        let mut state = ApiMockState::default();
        state
            .manual_routes
            .push(super::super::types::ApiManualRoute {
                stable_id: "manual-1".to_string(),
                method: ApiMethod::Get,
                path: "/manual".to_string(),
                enabled: true,
                response: super::super::types::ApiMockResponse::Generated,
                python: None,
                input_fields: Vec::new(),
                output_fields: Vec::new(),
            });
        let routes = build_api_mock_routes([], &state);
        let (_, content_type, body) = routes[0].static_response_text();

        assert_eq!(content_type, "application/json");
        assert_eq!(body, "{}");
    }
}
