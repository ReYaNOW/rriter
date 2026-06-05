use super::types::{
    ApiMockMode, ApiMockRouteDecision, ApiMockRouteOrigin, ApiMockRuntimeRoute, ApiMockState,
    api_mock_effective_contract, api_mock_route_key, api_mock_source_key,
    default_contract_for_manual_route, default_contract_from_route,
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
        let python = route.python.clone().map(|mut script| {
            if script.contract.is_empty() {
                script.contract = default_contract_for_manual_route(&route.path);
            } else if !script.contract.response.enabled
                && script.contract.response.fields.is_empty()
            {
                script.contract.response = default_contract_for_manual_route(&route.path).response;
            }
            script
        });
        let contract = python
            .as_ref()
            .map(|script| script.contract.clone())
            .unwrap_or_else(|| default_contract_for_manual_route(&route.path));
        out.push(ApiMockRuntimeRoute {
            id: format!("manual:{}", route.stable_id),
            source_key: "manual".to_string(),
            method: route.method,
            path: route.path.clone(),
            enabled: true,
            proxy_when_disabled: false,
            response: route.response.clone(),
            generated_status: 200,
            generated_content_type: "application/json",
            generated_body: "{}".to_string(),
            python,
            contract,
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
            let python = override_route.and_then(|override_route| {
                override_route.python.clone().map(|mut script| {
                    script.contract = api_mock_effective_contract(&script, route, model);
                    script
                })
            });
            let contract = python
                .as_ref()
                .map(|script| api_mock_effective_contract(script, route, model))
                .unwrap_or_else(|| default_contract_from_route(route, model));
            let input_fields = override_route
                .map(|override_route| override_route.extra_input_fields.clone())
                .unwrap_or_default();
            let output_fields = override_route
                .map(|override_route| override_route.extra_output_fields.clone())
                .unwrap_or_default();
            let (generated_status, generated_content_type, generated_body) =
                api_generated_response_for_route(route, model);
            let proxy_when_disabled = override_route.is_some_and(|override_route| {
                override_route.proxy_when_disabled
                    || (!override_route.enabled
                        && override_route.python.is_none()
                        && matches!(
                            &override_route.response,
                            super::types::ApiMockResponse::Generated
                        ))
            });

            out.push(ApiMockRuntimeRoute {
                id: api_mock_route_key(&source_key, route.method, &route.path),
                source_key: source_key.clone(),
                method: route.method,
                path: route.path.clone(),
                enabled,
                proxy_when_disabled,
                response,
                generated_status,
                generated_content_type,
                generated_body,
                python,
                contract,
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
            && api_mock_path_matches(&route.path, path)
    }) {
        return ApiMockRouteDecision::Mock(route);
    }

    if mode.canonical() != ApiMockMode::MockAll {
        if routes.iter().any(|route| {
            route.origin == ApiMockRouteOrigin::OpenApi
                && route.proxy_when_disabled
                && !route.enabled
                && route.method == method
                && api_mock_path_matches(&route.path, path)
        }) {
            return ApiMockRouteDecision::Proxy;
        }
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

    if mode.canonical() == ApiMockMode::MockSelectedProxyRest {
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
            (Some(pattern_part), Some(path_part)) => {
                match_path_segment(pattern_part, path_part, &mut params)?;
            }
            _ => return None,
        }
    }
}

fn match_path_segment(
    pattern: &str,
    path: &str,
    params: &mut BTreeMap<String, String>,
) -> Option<()> {
    if !pattern.contains('{') {
        return (pattern == path).then_some(());
    }
    let tokens = path_pattern_tokens(pattern)?;
    let mut pos = 0usize;
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            PathPatternToken::Static(text) => {
                if !path[pos..].starts_with(text) {
                    return None;
                }
                pos = pos.saturating_add(text.len());
            }
            PathPatternToken::Param(name) => {
                let next_static = tokens[idx + 1..].iter().find_map(|token| match token {
                    PathPatternToken::Static(text) if !text.is_empty() => Some(text.as_str()),
                    _ => None,
                });
                let end = if let Some(next_static) = next_static {
                    path[pos..].find(next_static).map(|offset| pos + offset)?
                } else {
                    path.len()
                };
                if end <= pos {
                    return None;
                }
                params.insert(name.clone(), path[pos..end].to_string());
                pos = end;
            }
        }
    }
    (pos == path.len()).then_some(())
}

fn path_pattern_tokens(pattern: &str) -> Option<Vec<PathPatternToken>> {
    let mut tokens = Vec::new();
    let mut rest = pattern;
    loop {
        let Some(open) = rest.find('{') else {
            if !rest.is_empty() {
                tokens.push(PathPatternToken::Static(rest.to_string()));
            }
            return Some(tokens);
        };
        if open > 0 {
            tokens.push(PathPatternToken::Static(rest[..open].to_string()));
        }
        rest = &rest[open + 1..];
        let close = rest.find('}')?;
        let name = &rest[..close];
        if name.is_empty() {
            return None;
        }
        tokens.push(PathPatternToken::Param(name.to_string()));
        rest = &rest[close + 1..];
    }
}

enum PathPatternToken {
    Static(String),
    Param(String),
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
        let mut model = ApiSpecModel {
            id: ApiSpecId(7),
            title: "Demo".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            servers: Vec::new(),
            routes: vec![route],
            route_groups: Vec::new(),
            route_display_paths: Vec::new(),
            security_schemes: Vec::new(),
            root_security: Vec::new(),
            schema_arena: Vec::new(),
        };
        model.rebuild_route_layout_cache();
        model
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
    fn mock_all_hard_disabled_openapi_route_still_mocks() {
        let entry = entry();
        let model = model(route(ApiMethod::Get, "/users"));
        let mut state = ApiMockState::default();
        state
            .route_overrides
            .push(super::super::types::ApiMockRouteOverride {
                source_key: super::super::types::api_mock_source_key(&entry),
                method: ApiMethod::Get,
                path: "/users".to_string(),
                enabled: false,
                proxy_when_disabled: true,
                response: super::super::types::ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            });
        let routes = build_api_mock_routes([(&entry, &model)], &state);

        let decision = resolve_api_mock_route(&routes, state.mode, ApiMethod::Get, "/users");

        assert!(matches!(decision, ApiMockRouteDecision::Mock(_)));
    }

    #[test]
    fn mock_all_python_override_still_mocks_without_route_enable() {
        let entry = entry();
        let model = model(route(ApiMethod::Get, "/users"));
        let mut state = ApiMockState::default();
        state
            .route_overrides
            .push(super::super::types::ApiMockRouteOverride {
                source_key: super::super::types::api_mock_source_key(&entry),
                method: ApiMethod::Get,
                path: "/users".to_string(),
                enabled: false,
                proxy_when_disabled: false,
                response: super::super::types::ApiMockResponse::Generated,
                python: Some(super::super::types::default_api_mock_python_script()),
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            });
        let routes = build_api_mock_routes([(&entry, &model)], &state);

        let decision = resolve_api_mock_route(&routes, state.mode, ApiMethod::Get, "/users");

        assert!(matches!(decision, ApiMockRouteDecision::Mock(_)));
    }

    #[test]
    fn legacy_selected_only_falls_back_to_proxy() {
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

        assert_eq!(decision, ApiMockRouteDecision::Proxy);
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
