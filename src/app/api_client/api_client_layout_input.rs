pub fn grouped_route_ranges(
    routes: &[ApiRouteRow],
    collapsed: &FxHashSet<(ApiSpecId, String)>,
    spec_id: ApiSpecId,
) -> Vec<(String, usize, usize, bool)> {
    let mut groups = Vec::new();
    let mut start = 0usize;
    while start < routes.len() {
        let tag = routes[start].tag.clone();
        let mut end = start + 1;
        while end < routes.len() && routes[end].tag == tag {
            end += 1;
        }
        let is_collapsed = collapsed.contains(&(spec_id, tag.clone()));
        groups.push((tag, start, end - start, is_collapsed));
        start = end;
    }
    groups
}

pub fn api_panel_max_scroll(api: &ApiClientState, visible_h: f32, scale: f32) -> f32 {
    let pad = 10.0 * scale;
    let mut content_h = pad + 40.0 * scale;
    if api.import_menu_open {
        content_h += 28.0 * scale * 2.0 + 12.0 * scale;
    }
    if api.import_url_open {
        content_h += 42.0 * scale;
    }
    if api.import_error.is_some() {
        content_h += 24.0 * scale;
    }
    content_h += 392.0 * scale + api.mock.manual_routes.len().min(8) as f32 * 34.0 * scale;
    if api.specs.is_empty() {
        content_h += 34.0 * scale;
    }
    content_h += api.specs.len() as f32 * 122.0 * scale;
    if let Some(model) = api.selected_model() {
        content_h += 28.0 * scale;
        content_h += 34.0 * scale;
        if !api.collapsed_route_roots.contains(&model.id) {
            for (_, _, len, collapsed) in
                grouped_route_ranges(&model.routes, &api.collapsed_tags, model.id)
            {
                content_h += 28.0 * scale;
                if !collapsed {
                    content_h += len as f32 * 30.0 * scale;
                }
            }
        }
    }
    (content_h + pad + 36.0 * scale - visible_h).max(0.0)
}

pub fn api_tab_max_scroll(
    model: Option<&ApiSpecModel>,
    tab_state: &ApiClientTabState,
    visible_h: f32,
    scale: f32,
) -> f32 {
    let Some(model) = model else {
        return 0.0;
    };
    if tab_state.auth_view {
        let pad = 28.0 * scale;
        let content_h = pad
            + 38.0 * scale
            + if model.security_schemes.is_empty() {
                28.0 * scale
            } else {
                model
                    .security_schemes
                    .iter()
                    .map(|scheme| api_auth_scheme_row_height(scheme, scale))
                    .sum::<f32>()
            }
            + {
                let route_count = api_auth_related_route_count(model).min(12);
                if route_count == 0 {
                    0.0
                } else {
                    44.0 * scale + route_count as f32 * 34.0 * scale
                }
            };
        return (content_h + pad + 36.0 * scale - visible_h).max(0.0);
    }
    let Some(route_idx) = tab_state
        .route_idx
        .or_else(|| (!model.routes.is_empty()).then_some(0))
    else {
        return 0.0;
    };
    let Some(route) = model.routes.get(route_idx) else {
        return 0.0;
    };
    let pad = 28.0 * scale;
    let mut content_h = pad + 42.0 * scale;
    if !route.summary.is_empty() {
        content_h += 30.0 * scale;
    }
    content_h += 558.0 * scale;
    content_h += 28.0 * scale;
    content_h += model.servers.len().max(1) as f32 * 34.0 * scale + 42.0 * scale;
    let auth_scheme_indices = api_route_auth_scheme_indices(model, route);
    if !auth_scheme_indices.is_empty() {
        content_h += 28.0 * scale
            + auth_scheme_indices
                .iter()
                .filter_map(|idx| model.security_schemes.get(*idx))
                .map(|scheme| api_auth_scheme_row_height(scheme, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if !route.path_params.is_empty() {
        content_h += 28.0 * scale
            + route
                .path_params
                .iter()
                .map(|param| api_param_row_height(param, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if !route.query_params.is_empty() {
        content_h += 28.0 * scale
            + route
                .query_params
                .iter()
                .map(|param| api_param_row_height(param, scale))
                .sum::<f32>()
            + 8.0 * scale;
    }
    if let Some(body) = &route.request_body {
        content_h += 28.0 * scale;
        if body.is_multipart || body.is_form_urlencoded {
            content_h += body
                .schema
                .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
                .map(|schema| {
                    schema
                        .properties
                        .iter()
                        .filter_map(|prop| model.schema_arena.get(prop.schema.0))
                        .map(|schema| api_body_prop_row_height(schema, model, scale))
                        .sum::<f32>()
                })
                .unwrap_or(0.0);
        } else {
            content_h += api_body_text_area_height(&tab_state.body_json, scale) + 16.0 * scale;
        }
    }
    content_h += 84.0 * scale;
    if let Some(response) = &tab_state.response {
        let response_text = api_response_text(response, tab_state.response_view);
        content_h += 62.0 * scale + api_response_text_area_height(response_text, scale);
        if api_response_has_auth_tokens(response) {
            content_h += model
                .security_schemes
                .iter()
                .filter(|scheme| scheme.token_capable())
                .count() as f32
                * 30.0
                * scale;
        }
    } else if tab_state.pending {
        content_h += 24.0 * scale;
    }
    (content_h + pad + 36.0 * scale - visible_h).max(0.0)
}

pub fn api_auth_scheme_row_height(scheme: &ApiSecurityScheme, scale: f32) -> f32 {
    if matches!(
        scheme.kind,
        ApiSecuritySchemeKind::Http { ref scheme, .. } if scheme.eq_ignore_ascii_case("basic")
    ) {
        92.0 * scale
    } else if scheme.token_capable() {
        72.0 * scale
    } else {
        58.0 * scale
    }
}

pub fn api_param_row_height(param: &ApiParam, scale: f32) -> f32 {
    let mut meta_lines = 0usize;
    if param.default_value.is_some() {
        meta_lines += 1;
    }
    if !param.enum_values.is_empty() || !param.examples.is_empty() {
        meta_lines += 1;
    } else if param.example.is_some() {
        meta_lines += 1;
    }
    let meta_h = if meta_lines == 0 {
        0.0
    } else {
        32.0 + meta_lines.saturating_sub(1) as f32 * 20.0
    };
    (46.0 * scale).max((meta_h + 14.0) * scale)
}

pub fn api_body_prop_row_height(schema: &ApiSchema, model: &ApiSpecModel, scale: f32) -> f32 {
    let mut meta_lines = usize::from(schema.max_chars.is_some());
    if schema.default_value.is_some() {
        meta_lines += 1;
    }
    if !api_schema_allowed_values(schema, model).is_empty() || !schema.examples.is_empty() {
        meta_lines += 1;
    }
    if !api_schema_allowed_values(schema, model).is_empty() {
        meta_lines += schema.examples.len().min(3);
    }
    let meta_h = if meta_lines == 0 {
        0.0
    } else {
        32.0 + meta_lines.saturating_sub(1) as f32 * 20.0
    };
    (46.0 * scale).max((meta_h + 14.0) * scale)
}

pub fn api_auth_route_rank(route: &ApiRouteRow) -> Option<u8> {
    if api_route_has_auth_word(route, "login")
        || api_route_has_auth_word(route, "signin")
        || api_route_has_auth_word(route, "sign-in")
        || api_route_has_auth_word(route, "token")
    {
        Some(0)
    } else if api_route_has_auth_word(route, "refresh") {
        Some(1)
    } else if api_route_has_auth_word(route, "logout")
        || api_route_has_auth_word(route, "session")
        || api_route_has_auth_word(route, "oauth")
        || api_route_has_auth_word(route, "jwt")
        || api_route_has_auth_word(route, "auth")
    {
        Some(2)
    } else {
        None
    }
}

pub fn api_auth_related_route_count(model: &ApiSpecModel) -> usize {
    model
        .routes
        .iter()
        .filter(|route| api_auth_route_rank(route).is_some())
        .count()
}

pub fn api_response_has_auth_tokens(response: &ApiJobResponse) -> bool {
    serde_json::from_str::<Value>(&response.body)
        .ok()
        .is_some_and(|json| {
            json.get("access_token").and_then(Value::as_str).is_some()
                || json.get("refresh_token").and_then(Value::as_str).is_some()
        })
}

fn api_route_has_auth_word(route: &ApiRouteRow, needle: &str) -> bool {
    contains_ascii_ci(&route.path, needle)
        || contains_ascii_ci(&route.operation_id, needle)
        || contains_ascii_ci(&route.summary, needle)
        || contains_ascii_ci(&route.tag, needle)
}

fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return false;
    }
    h.windows(n.len())
        .any(|window| window.iter().zip(n).all(|(a, b)| a.eq_ignore_ascii_case(b)))
}

pub fn api_schema_allowed_values<'a>(
    schema: &'a ApiSchema,
    model: &'a ApiSpecModel,
) -> &'a [String] {
    if !schema.enum_values.is_empty() {
        &schema.enum_values
    } else if matches!(schema.kind, ApiSchemaKind::Array) {
        schema
            .item
            .and_then(|item| model.schema_arena.get(item.0))
            .map(|item| item.enum_values.as_slice())
            .unwrap_or(&[])
    } else {
        &[]
    }
}

pub fn api_schema_is_array_input(schema: &ApiSchema) -> bool {
    matches!(schema.kind, ApiSchemaKind::Array)
}

pub fn split_api_array_values(value: &str) -> Vec<String> {
    api_array_value_parts(value)
        .map(ToString::to_string)
        .collect()
}

pub fn api_array_value_parts(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(['\n', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

pub fn api_array_edit_parts(value: &str) -> (Vec<&str>, &str) {
    if let Some(split_idx) = value.rfind(['\n', ',']) {
        let draft_start = split_idx.saturating_add(1);
        (
            api_array_value_parts(&value[..split_idx]).collect(),
            value[draft_start..].trim_start(),
        )
    } else {
        (Vec::new(), value.trim_start())
    }
}

fn api_array_editor_text(value: &str) -> String {
    let mut out = split_api_array_values(value).join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn finish_api_array_editor_draft(editor: &mut Editor) {
    let text = api_array_editor_text(&editor.get_full_text());
    editor.set_text_clean(&text);
    editor.cursor = editor.len();
    editor.selection_anchor = Some(editor.cursor);
}

fn backspace_api_array_editor(editor: &mut Editor) {
    if editor.selection_anchor.is_some() && editor.selection_anchor != Some(editor.cursor) {
        editor.delete_selection();
        return;
    }
    let text = editor.get_full_text();
    if editor.cursor == text.len() && (text.ends_with('\n') || text.ends_with(',')) {
        let trimmed = text.trim_end_matches(['\n', ',']);
        if let Some(prev_split) = trimmed.rfind(['\n', ',']) {
            let mut next = trimmed[..prev_split].to_string();
            if !next.trim().is_empty() {
                next.push('\n');
            }
            editor.set_text_clean(&next);
        } else {
            editor.set_text_clean("");
        }
        editor.cursor = editor.len();
        editor.selection_anchor = Some(editor.cursor);
    } else {
        editor.backspace();
    }
}

fn push_api_array_value(value: &mut String, item: &str) {
    let item = item.trim();
    if item.is_empty() {
        return;
    }
    if api_array_value_parts(value).any(|part| part == item) {
        return;
    }
    if !value.trim().is_empty() {
        value.push('\n');
    }
    value.push_str(item);
}

pub fn api_schema_is_file_input(schema: &ApiSchema, model: &ApiSpecModel) -> bool {
    matches!(schema.kind, ApiSchemaKind::Bytes) || api_schema_is_multi_file_input(schema, model)
}

pub fn api_schema_is_multi_file_input(schema: &ApiSchema, model: &ApiSpecModel) -> bool {
    matches!(schema.kind, ApiSchemaKind::Array)
        && schema
            .item
            .and_then(|item| model.schema_arena.get(item.0))
            .is_some_and(|item| matches!(item.kind, ApiSchemaKind::Bytes))
}

pub fn build_request_url(
    server: &ApiServer,
    path_template: &str,
    path_values: &[ApiInputValue],
    query_values: &[ApiInputValue],
) -> Result<String, ApiLoadError> {
    let mut server_url = server.url.clone();
    for var in &server.variables {
        let needle = format!("{{{}}}", var.name);
        server_url = server_url.replace(&needle, &var.default_value);
    }
    if server_url == "/" {
        server_url = "http://localhost".to_string();
    }
    let mut path = path_template.to_string();
    for item in path_values {
        let needle = format!("{{{}}}", item.name);
        path = path.replace(&needle, &percent_encode_path_param(&item.value));
    }
    let base = server_url.trim_end_matches('/');
    let full = if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    };
    let mut url = validate_api_url(&full)?;
    {
        let mut pairs = url.query_pairs_mut();
        for item in query_values {
            if item.value.contains('\n') {
                for value in item
                    .value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    pairs.append_pair(&item.name, value);
                }
            } else if !item.value.is_empty() {
                pairs.append_pair(&item.name, &item.value);
            }
        }
    }
    Ok(url.to_string())
}

fn route_security_requirements<'a>(
    route: &'a ApiRouteRow,
    model: &'a ApiSpecModel,
) -> &'a [ApiSecurityRequirement] {
    route
        .security
        .as_deref()
        .unwrap_or(model.root_security.as_slice())
}

pub fn api_route_auth_scheme_indices(model: &ApiSpecModel, route: &ApiRouteRow) -> Vec<usize> {
    let mut out = Vec::new();
    for requirement in route_security_requirements(route, model) {
        for req_scheme in &requirement.schemes {
            if let Some(idx) = model
                .security_schemes
                .iter()
                .position(|scheme| scheme.name == req_scheme.name)
                && !out.contains(&idx)
            {
                out.push(idx);
            }
        }
    }
    out
}

pub fn api_route_auth_missing(
    model: &ApiSpecModel,
    route: &ApiRouteRow,
    auth: &ApiAuthStore,
) -> bool {
    let requirements = route_security_requirements(route, model);
    requirements
        .iter()
        .any(|requirement| !requirement.schemes.is_empty())
        && !requirements
            .iter()
            .any(|requirement| auth_requirement_satisfied(model, requirement, auth))
}

fn prepared_auth_for_route(
    model: &ApiSpecModel,
    route: &ApiRouteRow,
    auth: &ApiAuthStore,
) -> Vec<ApiPreparedAuthPart> {
    for requirement in route_security_requirements(route, model) {
        let mut parts = Vec::new();
        for req_scheme in &requirement.schemes {
            let Some(scheme) = model
                .security_schemes
                .iter()
                .find(|scheme| scheme.name == req_scheme.name)
            else {
                parts.clear();
                break;
            };
            let Some(entry) = auth.entry(model.id, &scheme.name) else {
                parts.clear();
                break;
            };
            let Some(part) = prepared_auth_part(scheme, entry) else {
                parts.clear();
                break;
            };
            parts.push(part);
        }
        if parts.len() == requirement.schemes.len() {
            return parts;
        }
    }
    Vec::new()
}

fn auth_requirement_satisfied(
    model: &ApiSpecModel,
    requirement: &ApiSecurityRequirement,
    auth: &ApiAuthStore,
) -> bool {
    requirement.schemes.iter().all(|req_scheme| {
        let Some(scheme) = model
            .security_schemes
            .iter()
            .find(|scheme| scheme.name == req_scheme.name)
        else {
            return false;
        };
        auth.entry(model.id, &scheme.name)
            .and_then(|entry| prepared_auth_part(scheme, entry))
            .is_some()
    })
}

fn prepared_auth_part(
    scheme: &ApiSecurityScheme,
    entry: &ApiAuthEntry,
) -> Option<ApiPreparedAuthPart> {
    match &scheme.kind {
        ApiSecuritySchemeKind::ApiKey { name, location } => {
            let value = non_empty_auth_value(entry)?;
            Some(match location {
                ApiSecurityApiKeyLocation::Header => ApiPreparedAuthPart::Header {
                    name: name.clone(),
                    value,
                },
                ApiSecurityApiKeyLocation::Query => ApiPreparedAuthPart::Query {
                    name: name.clone(),
                    value,
                },
                ApiSecurityApiKeyLocation::Cookie => ApiPreparedAuthPart::Cookie {
                    name: name.clone(),
                    value,
                },
            })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
            (!entry.username.is_empty() && !entry.password.is_empty()).then(|| {
                ApiPreparedAuthPart::Basic {
                    username: entry.username.clone(),
                    password: entry.password.clone(),
                }
            })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("bearer") => {
            bearer_token(entry).map(|token| ApiPreparedAuthPart::Bearer { token })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("digest") => {
            non_empty_auth_value(entry).map(|value| ApiPreparedAuthPart::Digest { value })
        }
        ApiSecuritySchemeKind::OAuth2 { .. } | ApiSecuritySchemeKind::OpenIdConnect { .. } => {
            bearer_token(entry).map(|token| ApiPreparedAuthPart::Bearer { token })
        }
        ApiSecuritySchemeKind::Http { scheme, .. } => {
            non_empty_auth_value(entry).map(|value| ApiPreparedAuthPart::Header {
                name: "Authorization".to_string(),
                value: format!("{scheme} {value}"),
            })
        }
    }
}

fn non_empty_auth_value(entry: &ApiAuthEntry) -> Option<String> {
    (!entry.value.is_empty()).then(|| entry.value.clone())
}

fn bearer_token(entry: &ApiAuthEntry) -> Option<String> {
    if !entry.value.is_empty() {
        Some(entry.value.clone())
    } else if !entry.access_token.is_empty() {
        Some(entry.access_token.clone())
    } else {
        None
    }
}

fn append_auth_query(url: &mut String, auth_parts: &[ApiPreparedAuthPart]) {
    if !auth_parts
        .iter()
        .any(|part| matches!(part, ApiPreparedAuthPart::Query { .. }))
    {
        return;
    }
    let Ok(mut parsed) = Url::parse(url) else {
        return;
    };
    {
        let mut pairs = parsed.query_pairs_mut();
        for part in auth_parts {
            if let ApiPreparedAuthPart::Query { name, value } = part {
                pairs.append_pair(name, value);
            }
        }
    }
    *url = parsed.to_string();
}

fn percent_encode_path_param(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

pub fn json_body_is_valid(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok()
}

pub const API_BODY_TEXT_SCALE: f32 = 1.0;
pub(crate) const API_MOCK_TY_POPUP_BYTE: usize = usize::MAX;

pub fn api_text_area_line_height(scale: f32) -> f32 {
    26.0 * scale
}

pub fn api_body_text_area_height(text: &str, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h + line_h * 3.0 + 16.0 * scale).clamp(260.0 * scale, 620.0 * scale)
}

pub fn api_response_text_area_height(text: &str, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h + 16.0 * scale).clamp(300.0 * scale, 620.0 * scale)
}

pub fn api_text_area_max_scroll(text: &str, visible_h: f32, scale: f32) -> f32 {
    let line_h = api_text_area_line_height(scale);
    let lines = text.split('\n').count().max(1) as f32;
    (lines * line_h - visible_h.max(line_h)).max(0.0)
}

pub fn api_text_area_max_scroll_x<F>(text: &str, visible_w: f32, mut measure: F) -> f32
where
    F: FnMut(&str) -> f32,
{
    let longest = text.split('\n').map(&mut measure).fold(0.0, f32::max);
    (longest - visible_w.max(1.0) + 20.0).max(0.0)
}
