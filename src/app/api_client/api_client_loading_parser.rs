pub fn validate_api_url(input: &str) -> Result<Url, ApiLoadError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL пустой",
        ));
    }
    let parsed = Url::parse(raw)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidUrl, "URL не распознан"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL должен быть http или https",
        ));
    }
    if parsed.fragment().is_some() {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidUrl,
            "URL должен указывать на openapi.json без #fragment",
        ));
    }
    let Some(host) = parsed.host() else {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::InvalidHost,
            "host обязателен",
        ));
    };
    match host {
        Host::Domain(domain) => {
            if !valid_domain(domain) {
                return Err(ApiLoadError::new(
                    ApiLoadErrorKind::InvalidDomain,
                    "домен невалиден",
                ));
            }
        }
        Host::Ipv4(_) | Host::Ipv6(_) => {}
    }
    Ok(parsed)
}

fn valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    let domain = domain.trim_end_matches('.');
    if domain.is_empty() {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

fn api_client_key(resolved: Option<&ApiResolvedHost>) -> ApiHttpClientKey {
    ApiHttpClientKey {
        host: resolved.map(|r| r.host.clone()),
        ip: resolved.map(|r| r.ip),
        port: resolved.map(|r| r.port),
    }
}

fn api_http_client(resolved: Option<&ApiResolvedHost>) -> reqwest::blocking::Client {
    let key = api_client_key(resolved);
    if let Ok(mut clients) = API_HTTP_CLIENTS.lock() {
        if let Some(client) = clients.get(&key) {
            return client.clone();
        }
        let mut builder = reqwest::blocking::Client::builder()
            .timeout(API_FETCH_TIMEOUT)
            .pool_idle_timeout(API_POOL_IDLE_TIMEOUT)
            .use_rustls_tls();
        if let Some(resolved) = resolved {
            builder = builder.resolve(&resolved.host, SocketAddr::new(resolved.ip, resolved.port));
        }
        if let Ok(client) = builder.build() {
            clients.insert(key, client.clone());
            return client;
        }
    }
    reqwest::blocking::Client::new()
}

fn resolve_api_url_host(url: &str) -> Option<ApiResolvedHost> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default()?;
    let ip = match parsed.host()? {
        Host::Ipv4(ip) => IpAddr::V4(ip),
        Host::Ipv6(ip) => IpAddr::V6(ip),
        Host::Domain(_) => (host.as_str(), port).to_socket_addrs().ok()?.next()?.ip(),
    };
    Some(ApiResolvedHost { host, ip, port })
}

fn spawn_api_preconnect(resolved: ApiResolvedHost) {
    std::thread::spawn(move || {
        let client = api_http_client(Some(&resolved));
        let url = format!("https://{}/", resolved.host);
        let _ = client.head(url).send();
    });
}

pub fn spawn_load_local(id: ApiSpecId, path: PathBuf) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = load_local_spec(id, &path);
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

pub fn spawn_load_url(id: ApiSpecId, url: String) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = load_url_spec(id, &url);
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

pub fn spawn_load_cached_url(id: ApiSpecId, url: String) -> Receiver<ApiLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match read_url_cache(id) {
            Some(raw) => parse_openapi_payload(id, ApiSpecSource::Url(url), raw, None, None),
            None => Err(ApiLoadError::new(ApiLoadErrorKind::Io, "URL cache пустой")),
        };
        let _ = tx.send(ApiLoadResult { id, result });
    });
    rx
}

fn load_local_spec(id: ApiSpecId, path: &Path) -> Result<ApiLoadPayload, ApiLoadError> {
    let bytes = std::fs::read(path).map_err(|err| {
        ApiLoadError::new(ApiLoadErrorKind::Io, format!("файл не прочитан: {}", err))
    })?;
    if bytes.len() > API_MAX_SPEC_BYTES {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "openapi.json слишком большой",
        ));
    }
    let raw = String::from_utf8(bytes)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidJson, "JSON не UTF-8"))?;
    parse_openapi_payload(
        id,
        ApiSpecSource::Local(path.to_path_buf()),
        raw,
        None,
        None,
    )
}

fn load_url_spec(id: ApiSpecId, url: &str) -> Result<ApiLoadPayload, ApiLoadError> {
    validate_api_url(url)?;
    let resolved = resolve_api_url_host(url);
    let fetch_started = std::time::Instant::now();
    let raw = fetch_json(url, resolved.as_ref())?;
    let fetch_secs = fetch_started.elapsed().as_secs_f64();
    let mut payload = parse_openapi_payload(
        id,
        ApiSpecSource::Url(url.to_string()),
        raw,
        Some(ApiUrlStatus::Ok(200)),
        Some(fetch_secs),
    )?;
    payload.resolved_host = resolved;
    Ok(payload)
}

fn fetch_json(url: &str, resolved: Option<&ApiResolvedHost>) -> Result<String, ApiLoadError> {
    let client = api_http_client(resolved);
    let mut response = client
        .get(url)
        .header("Accept", "application/json, */*")
        .send()
        .map_err(classify_reqwest_error)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::HttpStatus(status),
            format!("HTTP {}", status),
        ));
    }
    if let Some(content_len) = response.content_length()
        && content_len > API_MAX_SPEC_BYTES as u64
    {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "ответ больше лимита",
        ));
    }
    read_limited_text(&mut response, API_MAX_SPEC_BYTES)
}

fn read_limited_text(
    response: &mut reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<String, ApiLoadError> {
    let mut raw = Vec::new();
    let mut limited = response.take(max_bytes as u64 + 1);
    limited.read_to_end(&mut raw).map_err(classify_io_error)?;
    if raw.len() > max_bytes {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::TooLarge,
            "ответ больше лимита",
        ));
    }
    String::from_utf8(raw)
        .map_err(|_| ApiLoadError::new(ApiLoadErrorKind::InvalidJson, "ответ не UTF-8"))
}

fn classify_reqwest_error(err: reqwest::Error) -> ApiLoadError {
    if err.is_timeout() {
        return ApiLoadError::new(ApiLoadErrorKind::Timeout, "таймаут запроса");
    }
    if let Some(status) = err.status() {
        return ApiLoadError::new(
            ApiLoadErrorKind::HttpStatus(status.as_u16()),
            format!("HTTP {}", status.as_u16()),
        );
    }
    if err.is_decode() {
        return ApiLoadError::new(ApiLoadErrorKind::InvalidJson, err.to_string());
    }
    if err.is_connect() {
        let text = err.to_string();
        let kind = if text.contains("dns") || text.contains("Name or service not known") {
            ApiLoadErrorKind::Dns
        } else if text.contains("tls") || text.contains("certificate") {
            ApiLoadErrorKind::Tls
        } else {
            ApiLoadErrorKind::NoInternet
        };
        return ApiLoadError::new(kind, text);
    }
    ApiLoadError::new(ApiLoadErrorKind::Other, err.to_string())
}

fn classify_io_error(err: std::io::Error) -> ApiLoadError {
    let kind = match err.kind() {
        std::io::ErrorKind::ConnectionRefused => ApiLoadErrorKind::ConnectRefused,
        std::io::ErrorKind::TimedOut => ApiLoadErrorKind::Timeout,
        std::io::ErrorKind::NotConnected
        | std::io::ErrorKind::NetworkDown
        | std::io::ErrorKind::NetworkUnreachable => ApiLoadErrorKind::NoInternet,
        _ => ApiLoadErrorKind::Io,
    };
    ApiLoadError::new(kind, err.to_string())
}

fn parse_openapi_payload(
    id: ApiSpecId,
    source: ApiSpecSource,
    raw: String,
    url_status: Option<ApiUrlStatus>,
    fetch_secs: Option<f64>,
) -> Result<ApiLoadPayload, ApiLoadError> {
    let parse_started = std::time::Instant::now();
    let root: Value = serde_json::from_str(&raw).map_err(|err| {
        let message = match source {
            ApiSpecSource::Url(_) => "URL не ведет на валидный openapi.json".to_string(),
            ApiSpecSource::Local(_) => err.to_string(),
        };
        ApiLoadError::new(ApiLoadErrorKind::InvalidJson, message)
    })?;
    let model = parse_openapi_model(id, &root)?;
    let parse_secs = parse_started.elapsed().as_secs_f64();
    let entry = ApiSpecEntry {
        id,
        title: model.title.clone(),
        version: model.version.clone(),
        openapi_version: model.openapi_version.clone(),
        source,
        last_loaded: Some(now_epoch_secs()),
        last_fetch_secs: fetch_secs,
        last_parse_secs: Some(parse_secs),
        last_url_status: url_status,
        selected: true,
        error: None,
    };
    Ok(ApiLoadPayload {
        entry,
        model,
        raw_json: Some(raw),
        resolved_host: None,
    })
}

pub fn parse_openapi_model(id: ApiSpecId, root: &Value) -> Result<ApiSpecModel, ApiLoadError> {
    let Some(openapi_version) = root.get("openapi").and_then(Value::as_str) else {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::UnsupportedOpenApi,
            "нет поля openapi",
        ));
    };
    if !openapi_version.starts_with("3.") {
        return Err(ApiLoadError::new(
            ApiLoadErrorKind::UnsupportedOpenApi,
            "поддерживается OpenAPI 3.x",
        ));
    }
    let info = root.get("info").unwrap_or(&Value::Null);
    let title = info
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("OpenAPI")
        .to_string();
    let version = info
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut model = ApiSpecModel {
        id,
        title,
        version,
        openapi_version: openapi_version.to_string(),
        servers: parse_servers(root.get("servers")),
        routes: Vec::new(),
        security_schemes: parse_security_schemes(root),
        root_security: parse_security_requirements(root.get("security")).unwrap_or_default(),
        schema_arena: Vec::new(),
    };
    if model.servers.is_empty() {
        model.servers.push(ApiServer {
            url: "/".to_string(),
            description: String::new(),
            variables: Vec::new(),
        });
    }

    let mut schema_cache = FxHashMap::default();

    let tag_order = parse_tag_order(root.get("tags"));
    if let Some(paths) = root.get("paths").and_then(Value::as_object) {
        for (path, path_item) in paths {
            let path_params = parse_parameters(path_item.get("parameters"), root);
            if let Some(path_obj) = path_item.as_object() {
                for (method_key, op) in path_obj {
                    let Some(method) = ApiMethod::from_key(method_key.as_str()) else {
                        continue;
                    };
                    let mut params = path_params.clone();
                    params.extend(parse_parameters(op.get("parameters"), root));
                    let mut path_params = Vec::new();
                    let mut query_params = Vec::new();
                    for param in params {
                        match param.location {
                            ApiParamLocation::Path => path_params.push(param),
                            ApiParamLocation::Query => query_params.push(param),
                        }
                    }
                    path_params.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    path_params.dedup_by(|a, b| a.name == b.name);
                    query_params.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    query_params.dedup_by(|a, b| a.name == b.name);
                    let tag = op
                        .get("tags")
                        .and_then(Value::as_array)
                        .and_then(|tags| tags.first())
                        .and_then(Value::as_str)
                        .filter(|tag| !tag.is_empty())
                        .unwrap_or(API_UNTAGGED_GROUP)
                        .to_string();
                    let summary = op
                        .get("summary")
                        .or_else(|| op.get("description"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let operation_id = op
                        .get("operationId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let request_body = parse_request_body(
                        op.get("requestBody"),
                        root,
                        &mut model.schema_arena,
                        &mut schema_cache,
                    );
                    let responses = parse_responses(
                        op.get("responses"),
                        root,
                        &mut model.schema_arena,
                        &mut schema_cache,
                    );
                    model.routes.push(ApiRouteRow {
                        tag,
                        method,
                        path: path.to_string(),
                        summary,
                        operation_id,
                        security: parse_security_requirements(op.get("security")),
                        path_params,
                        query_params,
                        request_body,
                        responses,
                    });
                }
            }
        }
    }
    model.routes.sort_unstable_by(|a, b| {
        api_route_tag_rank(&a.tag, &tag_order)
            .cmp(&api_route_tag_rank(&b.tag, &tag_order))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.method.sort_rank().cmp(&b.method.sort_rank()))
    });
    model
        .routes
        .dedup_by(|a, b| a.tag == b.tag && a.path == b.path && a.method == b.method);
    Ok(model)
}

fn parse_tag_order(value: Option<&Value>) -> FxHashMap<String, usize> {
    let mut out = FxHashMap::default();
    if let Some(tags) = value.and_then(Value::as_array) {
        for tag in tags {
            let Some(name) = tag.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !name.is_empty() && !out.contains_key(name) {
                out.insert(name.to_string(), out.len());
            }
        }
    }
    out
}

fn api_route_tag_rank<'a>(
    tag: &'a str,
    tag_order: &FxHashMap<String, usize>,
) -> (u8, usize, &'a str) {
    if tag == API_UNTAGGED_GROUP {
        return (2, usize::MAX, tag);
    }
    if let Some(rank) = tag_order.get(tag) {
        (0, *rank, tag)
    } else {
        (1, usize::MAX, tag)
    }
}

fn parse_servers(value: Option<&Value>) -> Vec<ApiServer> {
    let mut servers = Vec::new();
    if let Some(items) = value.and_then(Value::as_array) {
        for item in items {
            let Some(url) = item.get("url").and_then(Value::as_str) else {
                continue;
            };
            let mut variables = Vec::new();
            if let Some(vars) = item.get("variables").and_then(Value::as_object) {
                for (name, var) in vars {
                    if let Some(default_value) = var.get("default").and_then(Value::as_str) {
                        variables.push(ApiServerVariable {
                            name: name.to_string(),
                            default_value: default_value.to_string(),
                        });
                    }
                }
            }
            variables.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            servers.push(ApiServer {
                url: url.to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                variables,
            });
        }
    }
    servers
}

fn parse_security_schemes(root: &Value) -> Vec<ApiSecurityScheme> {
    let mut schemes = Vec::new();
    let Some(items) = root
        .get("components")
        .and_then(|v| v.get("securitySchemes"))
        .and_then(Value::as_object)
    else {
        return schemes;
    };
    for (name, value) in items {
        let Some(kind) = parse_security_scheme_kind(value) else {
            continue;
        };
        schemes.push(ApiSecurityScheme {
            name: name.to_string(),
            kind,
        });
    }
    schemes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    schemes
}

fn parse_security_scheme_kind(value: &Value) -> Option<ApiSecuritySchemeKind> {
    match value.get("type").and_then(Value::as_str)? {
        "apiKey" => {
            let name = value.get("name").and_then(Value::as_str)?;
            let location = match value.get("in").and_then(Value::as_str)? {
                "header" => ApiSecurityApiKeyLocation::Header,
                "query" => ApiSecurityApiKeyLocation::Query,
                "cookie" => ApiSecurityApiKeyLocation::Cookie,
                _ => return None,
            };
            Some(ApiSecuritySchemeKind::ApiKey {
                name: name.to_string(),
                location,
            })
        }
        "http" => Some(ApiSecuritySchemeKind::Http {
            scheme: value
                .get("scheme")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase(),
            bearer_format: value
                .get("bearerFormat")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "oauth2" => Some(ApiSecuritySchemeKind::OAuth2 {
            flows: parse_oauth_flows(value.get("flows")),
        }),
        "openIdConnect" => Some(ApiSecuritySchemeKind::OpenIdConnect {
            open_id_connect_url: value
                .get("openIdConnectUrl")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn parse_oauth_flows(value: Option<&Value>) -> Vec<ApiOAuthFlow> {
    let Some(flows) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, flow) in [
        ("implicit", ApiOAuthFlow::Implicit),
        ("password", ApiOAuthFlow::Password),
        ("clientCredentials", ApiOAuthFlow::ClientCredentials),
        ("authorizationCode", ApiOAuthFlow::AuthorizationCode),
    ] {
        if flows.contains_key(key) {
            out.push(flow);
        }
    }
    out
}

fn parse_security_requirements(value: Option<&Value>) -> Option<Vec<ApiSecurityRequirement>> {
    let items = value?.as_array()?;
    let mut requirements = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let mut schemes = Vec::new();
        for (name, scopes) in obj {
            let scopes = scopes
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            schemes.push(ApiSecurityRequirementScheme {
                name: name.to_string(),
                scopes,
            });
        }
        schemes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        requirements.push(ApiSecurityRequirement { schemes });
    }
    Some(requirements)
}

fn parse_parameters(value: Option<&Value>, root: &Value) -> Vec<ApiParam> {
    let mut out = Vec::new();
    if let Some(items) = value.and_then(Value::as_array) {
        for item in items {
            let item = resolve_parameter_ref(item, root).unwrap_or(item);
            let Some(location) = item.get("in").and_then(Value::as_str) else {
                continue;
            };
            let location = match location {
                "path" => ApiParamLocation::Path,
                "query" => ApiParamLocation::Query,
                _ => continue,
            };
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let schema = item.get("schema");
            let item_schema = schema.and_then(|schema| schema.get("items"));
            let resolved_schema =
                schema.and_then(|schema| resolve_schema_ref(schema, root).or(Some(schema)));
            let resolved_item_schema =
                item_schema.and_then(|schema| resolve_schema_ref(schema, root).or(Some(schema)));
            let enum_values = schema_enum_values(resolved_schema)
                .or_else(|| schema_enum_values(resolved_item_schema))
                .unwrap_or_default();
            let default_value = schema
                .and_then(|schema| schema.get("default"))
                .or_else(|| resolved_schema.and_then(|schema| schema.get("default")))
                .or_else(|| resolved_item_schema.and_then(|schema| schema.get("default")))
                .and_then(value_to_string);
            let examples = parameter_examples(item, resolved_schema, resolved_item_schema);
            let example = examples.first().cloned();
            let constraints = parse_schema_constraints(resolved_schema.or(schema));
            out.push(ApiParam {
                name: name.to_string(),
                location,
                required: item
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(matches!(location, ApiParamLocation::Path)),
                primitive_type: ApiPrimitiveType::from_schema(resolved_schema.or(schema)),
                item_type: resolved_item_schema
                    .or(item_schema)
                    .map(|schema| ApiPrimitiveType::from_schema(Some(schema))),
                enum_values,
                default_value,
                example,
                examples,
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                constraints,
            });
        }
    }
    out
}

fn resolve_schema_ref<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let ref_s = schema.get("$ref").and_then(Value::as_str)?;
    root.pointer(ref_s.strip_prefix('#')?)
}

fn parameter_examples(
    item: &Value,
    schema: Option<&Value>,
    item_schema: Option<&Value>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(example) = item.get("example").and_then(value_to_string) {
        out.push(example);
    }
    if let Some(items) = item.get("examples").and_then(Value::as_object) {
        for item in items.values() {
            let value = item
                .get("value")
                .and_then(value_to_string)
                .or_else(|| value_to_string(item));
            if let Some(value) = value {
                out.push(value);
            }
        }
    }
    if let Some(schema) = schema {
        out.extend(schema_examples(schema));
    }
    if let Some(item_schema) = item_schema {
        out.extend(schema_examples(item_schema));
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn schema_enum_values(schema: Option<&Value>) -> Option<Vec<String>> {
    schema?
        .get("enum")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(value_to_string).collect())
}

fn parse_schema_constraints(schema: Option<&Value>) -> ApiMockFieldConstraints {
    let Some(schema) = schema else {
        return ApiMockFieldConstraints::default();
    };
    let mut constraints = ApiMockFieldConstraints {
        min_length: schema
            .get("minLength")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()),
        max_length: schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()),
        pattern: schema
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::to_string),
        minimum: schema.get("minimum").and_then(numberish_to_string),
        maximum: schema.get("maximum").and_then(numberish_to_string),
        exclusive_minimum: schema
            .get("exclusiveMinimum")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        exclusive_maximum: schema
            .get("exclusiveMaximum")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        min_items: schema
            .get("minItems")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()),
        max_items: schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()),
        nullable: schema
            .get("nullable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || schema.get("type").and_then(Value::as_array).is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.as_str().is_some_and(|kind| kind == "null"))
            }),
    };
    if let Some(value) = schema.get("exclusiveMinimum").and_then(numberish_to_string) {
        constraints.minimum = Some(value);
        constraints.exclusive_minimum = true;
    }
    if let Some(value) = schema.get("exclusiveMaximum").and_then(numberish_to_string) {
        constraints.maximum = Some(value);
        constraints.exclusive_maximum = true;
    }
    constraints
}

fn numberish_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) if text.parse::<f64>().is_ok() => Some(text.to_string()),
        _ => None,
    }
}

fn resolve_parameter_ref<'a>(item: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let ref_s = item.get("$ref").and_then(Value::as_str)?;
    root.pointer(ref_s.strip_prefix('#')?)
}

fn parse_request_body(
    value: Option<&Value>,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
) -> Option<ApiRequestBody> {
    let body = value.and_then(|value| resolve_schema_ref(value, root).or(Some(value)))?;
    let content = body.get("content").and_then(Value::as_object)?;
    let mut keys: Vec<&String> = content.keys().collect();
    keys.sort_by(|a, b| {
        api_request_media_rank(a)
            .cmp(&api_request_media_rank(b))
            .then_with(|| a.cmp(b))
    });
    let mut media_items = Vec::new();
    for content_type in keys {
        let Some(media) = content.get(content_type) else {
            continue;
        };
        let schema = media.get("schema").and_then(|schema| {
            normalize_schema(schema, root, arena, schema_cache, 0, &mut Vec::new())
        });
        media_items.push(ApiRequestBodyMedia {
            content_type: content_type.to_string(),
            schema,
        });
    }
    let first = media_items.first()?;
    let content_type = first.content_type.clone();
    let schema = first.schema;
    Some(ApiRequestBody {
        required: body
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        content_type: content_type.clone(),
        schema,
        is_multipart: content_type == "multipart/form-data",
        is_form_urlencoded: content_type == "application/x-www-form-urlencoded",
        media: media_items,
    })
}

fn api_request_media_rank(content_type: &str) -> u8 {
    if content_type == "application/x-www-form-urlencoded" {
        0
    } else if content_type == "application/json" {
        1
    } else if content_type == "multipart/form-data" {
        2
    } else if content_type.contains("json") {
        3
    } else {
        4
    }
}

fn normalize_schema(
    schema: &Value,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
    depth: usize,
    guard: &mut Vec<String>,
) -> Option<ApiSchemaRef> {
    if depth > API_SCHEMA_MAX_DEPTH || arena.len() >= API_SCHEMA_MAX_COUNT {
        return None;
    }
    if let Some(ref_s) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(schema_ref) = schema_cache.get(ref_s).copied() {
            return Some(schema_ref);
        }
        if guard.iter().any(|seen| seen == ref_s) {
            return None;
        }
        let target = resolve_schema_ref(schema, root)?;
        let name = schema_ref_name(ref_s);
        guard.push(ref_s.to_string());
        let out = normalize_schema_named(
            &name,
            target,
            root,
            arena,
            schema_cache,
            depth + 1,
            guard,
            Some(ref_s),
        );
        guard.pop();
        return out;
    }
    normalize_schema_named("", schema, root, arena, schema_cache, depth, guard, None)
}

fn normalize_schema_named(
    name: &str,
    schema: &Value,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
    depth: usize,
    guard: &mut Vec<String>,
    cache_key: Option<&str>,
) -> Option<ApiSchemaRef> {
    if depth > API_SCHEMA_MAX_DEPTH || arena.len() >= API_SCHEMA_MAX_COUNT {
        return None;
    }
    let idx = arena.len();
    let constraints = parse_schema_constraints(Some(schema));
    let kind = if schema.get("allOf").and_then(Value::as_array).is_some() {
        ApiSchemaKind::Object
    } else {
        schema_kind(schema)
    };
    arena.push(ApiSchema {
        name: name.to_string(),
        description: schema
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        kind,
        properties: Vec::new(),
        item: None,
        enum_values: schema_enum_values(Some(schema)).unwrap_or_default(),
        default_value: schema.get("default").and_then(value_to_string),
        examples: schema_examples(schema),
        max_chars: constraints.max_length,
        constraints,
    });
    let schema_ref = ApiSchemaRef(idx);
    if let Some(cache_key) = cache_key {
        schema_cache.insert(cache_key.to_string(), schema_ref);
    }
    if let Some(items) = schema.get("allOf").and_then(Value::as_array) {
        for item in items {
            if arena[idx].properties.len() >= API_SCHEMA_MAX_PROPERTIES {
                break;
            }
            if let Some(part_ref) =
                normalize_schema(item, root, arena, schema_cache, depth + 1, guard)
            {
                let properties = arena
                    .get(part_ref.0)
                    .map(|schema| schema.properties.clone())
                    .unwrap_or_default();
                append_schema_properties_dedup(&mut arena[idx].properties, properties);
            }
        }
    }
    if matches!(arena[idx].kind, ApiSchemaKind::Object) {
        append_inline_object_properties(schema, root, arena, schema_cache, depth, guard, idx);
    } else if matches!(arena[idx].kind, ApiSchemaKind::Array)
        && let Some(items) = schema.get("items")
    {
        arena[idx].item = normalize_schema(items, root, arena, schema_cache, depth + 1, guard);
    }
    Some(schema_ref)
}

fn append_inline_object_properties(
    schema: &Value,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
    depth: usize,
    guard: &mut Vec<String>,
    idx: usize,
) {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<FxHashSet<_>>()
        })
        .unwrap_or_default();
    let Some(props) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (prop_name, prop_schema) in props {
        if arena[idx].properties.len() >= API_SCHEMA_MAX_PROPERTIES {
            break;
        }
        if arena[idx]
            .properties
            .iter()
            .any(|prop| prop.name == *prop_name)
        {
            continue;
        }
        if let Some(prop_ref) =
            normalize_schema(prop_schema, root, arena, schema_cache, depth + 1, guard)
        {
            arena[idx].properties.push(ApiSchemaProperty {
                name: prop_name.to_string(),
                required: required.contains(prop_name.as_str()),
                schema: prop_ref,
            });
        }
    }
}

fn append_schema_properties_dedup(
    target: &mut Vec<ApiSchemaProperty>,
    properties: Vec<ApiSchemaProperty>,
) {
    for prop in properties {
        if target.len() >= API_SCHEMA_MAX_PROPERTIES {
            break;
        }
        if target.iter().any(|existing| existing.name == prop.name) {
            continue;
        }
        target.push(prop);
    }
}

fn schema_ref_name(ref_s: &str) -> String {
    let tail = ref_s.rsplit('/').next().unwrap_or("");
    let mut out = String::with_capacity(tail.len());
    let mut chars = tail.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '~' {
            match chars.peek().copied() {
                Some('0') => {
                    chars.next();
                    out.push('~');
                    continue;
                }
                Some('1') => {
                    chars.next();
                    out.push('/');
                    continue;
                }
                _ => {}
            }
        }
        out.push(ch);
    }
    out
}

fn schema_kind(schema: &Value) -> ApiSchemaKind {
    if schema
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|fmt| matches!(fmt, "binary" | "byte"))
    {
        return ApiSchemaKind::Bytes;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => ApiSchemaKind::Object,
        Some("array") => ApiSchemaKind::Array,
        Some("string") => match schema.get("format").and_then(Value::as_str) {
            Some("date") => ApiSchemaKind::Date,
            Some("date-time") => ApiSchemaKind::DateTime,
            Some("time") => ApiSchemaKind::Time,
            _ => ApiSchemaKind::String,
        },
        Some("integer") => ApiSchemaKind::Integer,
        Some("number") => ApiSchemaKind::Number,
        Some("boolean") => ApiSchemaKind::Boolean,
        _ if schema.get("properties").is_some() => ApiSchemaKind::Object,
        _ => ApiSchemaKind::Unknown,
    }
}

fn schema_examples(schema: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(example) = schema.get("example").and_then(value_to_string) {
        out.push(example);
    }
    if let Some(items) = schema.get("examples").and_then(Value::as_array) {
        for item in items {
            if let Some(value) = value_to_string(item) {
                out.push(value);
            }
        }
    } else if let Some(items) = schema.get("examples").and_then(Value::as_object) {
        for item in items.values() {
            let value = item
                .get("value")
                .and_then(value_to_string)
                .or_else(|| value_to_string(item));
            if let Some(value) = value {
                out.push(value);
            }
        }
    }
    let mut deduped = Vec::with_capacity(out.len());
    for value in out {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

fn parse_responses(
    value: Option<&Value>,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
) -> Vec<ApiResponseSummary> {
    let mut out = Vec::new();
    if let Some(map) = value.and_then(Value::as_object) {
        for (status, body) in map {
            let body = resolve_schema_ref(body, root).unwrap_or(body);
            let media = parse_response_media(body, root, arena, schema_cache);
            let first = media.first();
            out.push(ApiResponseSummary {
                status: status.to_string(),
                description: body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                content_type: first
                    .map(|item| item.content_type.clone())
                    .unwrap_or_default(),
                example: first.and_then(|item| item.example.clone()),
                schema: first.and_then(|item| item.schema),
                media,
            });
        }
    }
    out.sort_unstable_by(|a, b| a.status.cmp(&b.status));
    out
}

fn parse_response_media(
    body: &Value,
    root: &Value,
    arena: &mut Vec<ApiSchema>,
    schema_cache: &mut FxHashMap<String, ApiSchemaRef>,
) -> Vec<ApiResponseMedia> {
    let Some(content) = body.get("content").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut keys: Vec<&String> = content.keys().collect();
    keys.sort_by(|a, b| {
        api_response_media_rank(a)
            .cmp(&api_response_media_rank(b))
            .then_with(|| a.cmp(b))
    });
    let mut out = Vec::new();
    for content_type in keys {
        let Some(media) = content.get(content_type) else {
            continue;
        };
        let examples = response_examples(media);
        let example = examples.first().map(|example| example.value.clone());
        let schema = media
            .get("schema")
            .and_then(|schema| normalize_schema(schema, root, arena, schema_cache, 0, &mut Vec::new()));
        out.push(ApiResponseMedia {
            content_type: content_type.to_string(),
            example,
            examples,
            schema,
        });
    }
    out
}

fn response_examples(media: &Value) -> Vec<ApiResponseExample> {
    let mut out = Vec::new();
    if let Some(value) = media.get("example").and_then(value_to_string) {
        out.push(ApiResponseExample {
            label: "example".to_string(),
            value,
        });
    }
    if let Some(examples) = media.get("examples").and_then(Value::as_object) {
        let mut keys: Vec<&String> = examples.keys().collect();
        keys.sort_unstable();
        for key in keys {
            let Some(example) = examples.get(key) else {
                continue;
            };
            let value = example
                .get("value")
                .and_then(value_to_string)
                .or_else(|| value_to_string(example));
            if let Some(value) = value {
                let label = example
                    .get("summary")
                    .and_then(Value::as_str)
                    .filter(|summary| !summary.trim().is_empty())
                    .unwrap_or(key)
                    .to_string();
                out.push(ApiResponseExample {
                    label,
                    value,
                });
            }
        }
    }
    out
}

fn api_response_media_rank(content_type: &str) -> u8 {
    if content_type == "application/json" {
        0
    } else if content_type == "application/problem+json" {
        1
    } else if content_type.contains("json") {
        2
    } else {
        3
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Null => None,
        _ => serde_json::to_string(value).ok(),
    }
}
