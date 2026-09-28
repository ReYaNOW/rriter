pub struct ApiClientState {
    pub specs: Vec<ApiSpecEntry>,
    pub models: FxHashMap<ApiSpecId, ApiSpecModel>,
    pub auth: ApiAuthStore,
    pub mock: ApiMockState,
    pub selected_spec: Option<ApiSpecId>,
    pub next_id: u64,
    pub import_menu_open: bool,
    pub import_url_open: bool,
    pub import_error: Option<String>,
    pub import_error_at: Option<u64>,
    pub timing_label_deadline: Option<u64>,
    pub spec_remove_dialog: Option<ApiSpecRemoveDialog>,
    pub loading: FxHashSet<ApiSpecId>,
    pub load_tickets: FxHashMap<ApiSpecId, ApiLoadTicket>,
    pub next_load_generation: u64,
    pub persistence_error: Option<String>,
    pub collapsed_tags: FxHashMap<ApiSpecId, FxHashSet<String>>,
    pub collapsed_route_roots: FxHashSet<ApiSpecId>,
    pub route_filter: String,
    pub expanded_mock_routes: FxHashSet<(ApiSpecId, usize)>,
    pub panel_scroll: ScrollState,
    pub route_scroll: ScrollState,
    pub next_request_id: u64,
    pub input_editor: Editor,
    pub input_scroll_x: ScrollState,
    pub focused: Option<ApiFocus>,
    pub mock_guide_open: bool,
    pub mock_guide_scroll: ScrollState,
    pub mock_server_detail_open: bool,
    pub mock_server_url_copied_at: Option<Instant>,
    pub mock_server_logs: Vec<ApiMockServerLogLine>,
    pub mock_server_log_scroll: ScrollState,
    pub mock_python_runtime_open: bool,
    pub mock_python_version_picker_open: bool,
    pub mock_python_versions_loading: bool,
    pub mock_python_versions: Vec<ApiPythonVersionRow>,
    pub mock_python_versions_scroll: ScrollState,
    pub mock_python_install_running: bool,
    pub mock_python_install_log: Vec<ApiPythonInstallLogLine>,
    pub mock_python_install_log_scroll: ScrollState,
    pub mock_route_reset_dialog: Option<ApiMockRouteResetDialog>,
    pub mock_contract_field_delete_dialog: Option<ApiMockContractFieldDeleteDialog>,
    pub mock_contract_constraint_menu: Option<ApiMockContractConstraintMenu>,
    pub mock_ty_due: Option<Instant>,
    pub mock_ty_pending: Option<(usize, u64)>,
    pub mock_ty_diagnostics: Vec<ApiMockTyDiagnostic>,
    pub(crate) mock_hover_target: Option<ApiMockHoverTarget>,
    pub(crate) mock_hover_request: Option<ApiMockHoverRequest>,
    mock_lsp_opened: FxHashMap<PathBuf, i32>,
    pub mock_highlighter: Highlighter,
    pub mock_highlight_target: Option<(usize, ApiMockSourcePart, u64)>,
    pub mock_highlight_spans: Vec<ColorSpan>,
    pub mock_highlight_cache: FxHashMap<(usize, ApiMockSourcePart), Vec<ColorSpan>>,
    pub mock_python_scrolls: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    pub mock_python_scrolls_x: FxHashMap<(usize, ApiMockSourcePart), ScrollState>,
    pub(crate) mock_python_editors: FxHashMap<(usize, ApiMockSourcePart), Editor>,
    pub last_resolved_host: Option<ApiResolvedHost>,
    body_json_validation: Option<ApiJsonValidationState>,
    body_json_validation_pending: Option<(ApiSpecId, usize, u64)>,
    body_json_validation_rx: Option<Receiver<ApiJsonValidationResult>>,
    python_version_list_rx: Option<Receiver<ApiPythonVersionListResult>>,
    python_version_list_cancel: Option<Arc<AtomicBool>>,
    python_install_rx: Option<Receiver<ApiPythonInstallEvent>>,
    python_install_cancel: Option<Arc<AtomicBool>>,
    python_path_pick_rx: Option<Receiver<ApiPythonPathPickResult>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiMockContractConstraintMenu {
    pub route_idx: usize,
    pub group: crate::ui_system::ApiMockContractFieldGroup,
    pub field_idx: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiSpecRemoveDialog {
    pub spec_id: ApiSpecId,
    pub title: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockRouteResetDialog {
    pub route_idx: usize,
    pub route_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMockContractFieldDeleteDialog {
    pub route_idx: usize,
    pub group: crate::ui_system::ApiMockContractFieldGroup,
    pub field_idx: usize,
    pub field_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApiMockHoverTarget {
    pub route_idx: usize,
    pub part: ApiMockSourcePart,
    pub edit_byte: usize,
    pub version: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ApiMockHoverRequest {
    pub request_id: i32,
    pub target: ApiMockHoverTarget,
    pub source: String,
    pub source_cursor: usize,
    pub anchor: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct ApiMockServerLogLine {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ApiPythonVersionRow {
    pub version: String,
    pub installed: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct ApiPythonInstallLogLine {
    pub text: String,
    pub kind: ApiPythonInstallLogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiPythonInstallLogKind {
    Info,
    Ok,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ApiPythonRuntimeDialogLayout {
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub pad: f32,
    pub content_w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApiPythonPathPickKind {
    Uv,
    CustomPython,
}

struct ApiPythonPathPickResult {
    kind: ApiPythonPathPickKind,
    path: Option<PathBuf>,
}

struct ApiPythonVersionListResult {
    rows: Vec<ApiPythonVersionRow>,
    error: Option<String>,
}

enum ApiPythonInstallEvent {
    Line(ApiPythonInstallLogLine),
    Done(Result<(), String>),
}

#[derive(Clone, Copy)]
struct ApiJsonValidationState {
    spec_id: ApiSpecId,
    route_idx: usize,
    version: u64,
    valid: bool,
}

struct ApiJsonValidationResult {
    spec_id: ApiSpecId,
    route_idx: usize,
    version: u64,
    valid: bool,
}

impl Default for ApiClientState {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            models: FxHashMap::default(),
            auth: ApiAuthStore::default(),
            mock: ApiMockState::default(),
            selected_spec: None,
            next_id: 1,
            import_menu_open: false,
            import_url_open: false,
            import_error: None,
            import_error_at: None,
            timing_label_deadline: None,
            spec_remove_dialog: None,
            loading: FxHashSet::default(),
            load_tickets: FxHashMap::default(),
            next_load_generation: 1,
            persistence_error: None,
            collapsed_tags: FxHashMap::default(),
            collapsed_route_roots: FxHashSet::default(),
            route_filter: String::new(),
            expanded_mock_routes: FxHashSet::default(),
            panel_scroll: ScrollState::new(7.0),
            route_scroll: ScrollState::new(7.0),
            next_request_id: 1,
            input_editor: Editor::new(512),
            input_scroll_x: ScrollState::new(7.0),
            focused: None,
            mock_guide_open: false,
            mock_guide_scroll: ScrollState::new(7.0),
            mock_server_detail_open: false,
            mock_server_url_copied_at: None,
            mock_server_logs: Vec::new(),
            mock_server_log_scroll: ScrollState::new(7.0),
            mock_python_runtime_open: false,
            mock_python_version_picker_open: false,
            mock_python_versions_loading: false,
            mock_python_versions: Vec::new(),
            mock_python_versions_scroll: ScrollState::new(7.0),
            mock_python_install_running: false,
            mock_python_install_log: Vec::new(),
            mock_python_install_log_scroll: ScrollState::new(7.0),
            mock_route_reset_dialog: None,
            mock_contract_field_delete_dialog: None,
            mock_contract_constraint_menu: None,
            mock_ty_due: None,
            mock_ty_pending: None,
            mock_ty_diagnostics: Vec::new(),
            mock_hover_target: None,
            mock_hover_request: None,
            mock_lsp_opened: FxHashMap::default(),
            mock_highlighter: Highlighter::new(),
            mock_highlight_target: None,
            mock_highlight_spans: Vec::new(),
            mock_highlight_cache: FxHashMap::default(),
            mock_python_scrolls: FxHashMap::default(),
            mock_python_scrolls_x: FxHashMap::default(),
            mock_python_editors: FxHashMap::default(),
            last_resolved_host: None,
            body_json_validation: None,
            body_json_validation_pending: None,
            body_json_validation_rx: None,
            python_version_list_rx: None,
            python_version_list_cancel: None,
            python_install_rx: None,
            python_install_cancel: None,
            python_path_pick_rx: None,
        }
    }
}

impl ApiClientState {
    #[cfg(test)]
    pub(crate) fn seed_body_json_validation(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        version: u64,
        valid: bool,
    ) {
        self.body_json_validation = Some(ApiJsonValidationState {
            spec_id,
            route_idx,
            version,
            valid,
        });
        self.body_json_validation_pending = Some((spec_id, route_idx, version));
    }

    pub(crate) fn handle_load_disconnect(&mut self, id: ApiSpecId, generation: u64) -> bool {
        if self.finish_load(id, generation).is_none() {
            return false;
        }
        self.mark_load_error(
            id,
            ApiLoadError::new(
                ApiLoadErrorKind::Other,
                "Загрузка OpenAPI неожиданно завершилась",
            ),
        );
        true
    }

    pub fn load_persisted() -> Self {
        let mut state = Self::default();
        let mut load_errors = Vec::new();
        match load_api_auth_checked() {
            Ok(auth) => state.auth = auth,
            Err(error) => load_errors.push(error),
        }
        match load_api_mocks_checked() {
            Ok(mock) => state.mock = mock,
            Err(error) => load_errors.push(error),
        }
        clear_legacy_api_python_runtime_message(&mut state);
        match load_api_specs_checked() {
            Ok(Some(saved)) => {
                state.specs = saved.specs;
                state.selected_spec = saved.selected_spec;
                state.next_id = saved.next_id.max(1);
                state.last_resolved_host = saved.last_resolved_host;
                if let Some(resolved) = state.last_resolved_host.clone() {
                    spawn_api_preconnect(resolved);
                }
                for spec in &mut state.specs {
                    spec.selected = Some(spec.id) == state.selected_spec;
                }
                for entry in &state.specs {
                    if let ApiSpecSource::Url(url) = &entry.source
                        && let Some(raw) = read_url_cache(entry.id)
                        && let Ok(payload) = parse_openapi_payload(
                            entry.id,
                            ApiSpecSource::Url(url.clone()),
                            raw,
                            entry.last_url_status.clone(),
                            None,
                        )
                    {
                        state.models.insert(entry.id, payload.model);
                    }
                }
            }
            Ok(None) => {}
            Err(error) => load_errors.push(error),
        }
        if !load_errors.is_empty() {
            state.persistence_error = Some(load_errors.join("; "));
        }
        state
    }

    pub fn shutdown_background_tasks(&mut self) {
        if let Some(cancel) = &self.python_version_list_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(cancel) = &self.python_install_cancel {
            cancel.store(true, Ordering::Release);
        }

        let deadline = Instant::now() + API_PYTHON_SHUTDOWN_TIMEOUT;
        while Instant::now() < deadline
            && (self.python_version_list_cancel.is_some() || self.python_install_cancel.is_some())
        {
            if let Some(rx) = &self.python_version_list_rx {
                match rx.try_recv() {
                    Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.python_version_list_cancel = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            } else {
                self.python_version_list_cancel = None;
            }

            if let Some(rx) = &self.python_install_rx {
                loop {
                    match rx.try_recv() {
                        Ok(ApiPythonInstallEvent::Done(_))
                        | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.python_install_cancel = None;
                            break;
                        }
                        Ok(ApiPythonInstallEvent::Line(_)) => continue,
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    }
                }
            } else {
                self.python_install_cancel = None;
            }
            if self.python_version_list_cancel.is_some() || self.python_install_cancel.is_some() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        self.python_version_list_cancel = None;
        self.python_install_cancel = None;
        self.python_version_list_rx = None;
        self.python_install_rx = None;
        self.mock_python_versions_loading = false;
        self.mock_python_install_running = false;
    }

    pub fn persist(&mut self) {
        let saved = ApiSpecsPersist {
            specs: self.specs.clone(),
            selected_spec: self.selected_spec,
            last_resolved_host: self.last_resolved_host.clone(),
            next_id: self.next_id.max(
                self.specs
                    .iter()
                    .map(|entry| entry.id.0.saturating_add(1))
                    .max()
                    .unwrap_or(1),
            ),
        };
        let result = (|| -> Result<(), String> {
            let content = serde_json::to_vec_pretty(&saved)
                .map_err(|err| format!("API specifications не сериализованы: {err}"))?;
            crate::platform::atomic_write(&api_specs_path(), &content)
                .map_err(|err| format!("API specifications не сохранены: {err}"))?;
            save_api_auth(&self.auth)
                .map_err(|err| format!("API credentials не сохранены: {err}"))?;
            save_api_mocks(&self.mock)
                .map_err(|err| format!("API mock configuration не сохранена: {err}"))?;
            Ok(())
        })();
        self.persistence_error = result.err();
        if let Some(error) = self.persistence_error.as_deref() {
            eprintln!("RRiter: {error}");
        }
    }

    pub fn body_json_valid_for(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
        version: u64,
    ) -> Option<bool> {
        self.body_json_validation
            .filter(|state| {
                state.spec_id == spec_id && state.route_idx == route_idx && state.version == version
            })
            .map(|state| state.valid)
    }

    pub fn alloc_spec_id(&mut self) -> ApiSpecId {
        let mut candidate = self.next_id.max(1);
        loop {
            let id = ApiSpecId(candidate);
            let used = self.specs.iter().any(|entry| entry.id == id)
                || self.models.contains_key(&id)
                || self.loading.contains(&id)
                || self.load_tickets.contains_key(&id);
            candidate = candidate.wrapping_add(1).max(1);
            if !used {
                self.next_id = candidate;
                return id;
            }
        }
    }

    pub fn begin_load(&mut self, id: ApiSpecId, select_on_success: bool) -> u64 {
        let generation = self.next_load_generation.max(1);
        self.next_load_generation = generation.wrapping_add(1).max(1);
        self.loading.insert(id);
        self.load_tickets.insert(
            id,
            ApiLoadTicket {
                generation,
                select_on_success,
            },
        );
        generation
    }

    pub fn is_current_load(&self, id: ApiSpecId, generation: u64) -> bool {
        self.load_tickets
            .get(&id)
            .is_some_and(|ticket| ticket.generation == generation)
    }

    pub fn finish_load(&mut self, id: ApiSpecId, generation: u64) -> Option<ApiLoadTicket> {
        let ticket = self.load_tickets.get(&id).copied()?;
        if ticket.generation != generation {
            return None;
        }
        self.load_tickets.remove(&id);
        self.loading.remove(&id);
        Some(ticket)
    }

    pub fn selected_model(&self) -> Option<&ApiSpecModel> {
        let id = self.selected_spec?;
        self.models.get(&id)
    }

    pub fn tag_collapsed(&self, spec_id: ApiSpecId, tag: &str) -> bool {
        self.collapsed_tags
            .get(&spec_id)
            .is_some_and(|tags| tags.contains(tag))
    }

    pub fn toggle_tag_collapsed(&mut self, spec_id: ApiSpecId, tag: &str) {
        let remove_spec_entry = {
            let tags = self.collapsed_tags.entry(spec_id).or_default();
            if !tags.remove(tag) {
                tags.insert(tag.to_string());
            }
            tags.is_empty()
        };
        if remove_spec_entry {
            self.collapsed_tags.remove(&spec_id);
        }
    }

    pub fn clear_collapsed_tags_for_spec(&mut self, spec_id: ApiSpecId) {
        self.collapsed_tags.remove(&spec_id);
    }

    pub fn selected_entry(&self) -> Option<&ApiSpecEntry> {
        let id = self.selected_spec?;
        self.specs.iter().find(|entry| entry.id == id)
    }

    pub fn mock_server_snapshot(&self) -> ApiMockServerSnapshot {
        let specs = self.selected_spec.into_iter().filter_map(|id| {
            let entry = self.specs.iter().find(|entry| entry.id == id)?;
            let model = self.models.get(&id)?;
            Some((entry, model))
        });
        ApiMockServerSnapshot {
            bind_host: self.mock.bind_host.clone(),
            port: self.mock.port,
            mode: self.mock.mode,
            proxy_base_url: self.mock.proxy_base_url.clone(),
            python_runtime: self.mock.uv.runtime_config(),
            routes: build_api_mock_routes(specs, &self.mock),
        }
    }

    pub fn select_spec(&mut self, id: ApiSpecId) {
        self.selected_spec = Some(id);
        for entry in &mut self.specs {
            entry.selected = entry.id == id;
        }
        self.route_scroll.reset();
    }

    pub fn upsert_loaded(&mut self, payload: ApiLoadPayload, select_on_success: bool) {
        let id = payload.entry.id;
        if let Some(existing) = self.specs.iter_mut().find(|entry| entry.id == id) {
            *existing = payload.entry.clone();
        } else {
            self.specs.push(payload.entry.clone());
        }
        self.models.insert(id, payload.model);
        if payload.resolved_host.is_some() {
            self.last_resolved_host = payload.resolved_host;
        }
        if select_on_success || self.selected_spec.is_none() {
            self.select_spec(id);
        }
        let cache_error = payload
            .raw_json
            .as_deref()
            .and_then(|raw| save_url_cache(id, raw).err());
        self.persist();
        if let Some(error) = cache_error {
            self.persistence_error = Some(match self.persistence_error.take() {
                Some(existing) => format!("{existing}; {error}"),
                None => error,
            });
        }
    }

    pub fn mark_load_error(&mut self, id: ApiSpecId, err: ApiLoadError) {
        if let Some(entry) = self.specs.iter_mut().find(|entry| entry.id == id) {
            entry.error = Some(err.message.clone());
            if matches!(entry.source, ApiSpecSource::Url(_)) {
                entry.last_url_status = Some(ApiUrlStatus::Failed(err.kind.clone()));
            }
        } else {
            self.import_error = Some(err.message.clone());
            self.import_error_at = Some(now_epoch_secs());
        }
        self.persist();
    }

    pub fn remove_spec(&mut self, idx: usize) -> Option<ApiSpecId> {
        if idx >= self.specs.len() {
            return None;
        }
        let id = self.specs[idx].id;
        self.specs.remove(idx);
        self.models.remove(&id);
        self.auth.retain_spec(id);
        self.loading.remove(&id);
        self.load_tickets.remove(&id);
        self.clear_collapsed_tags_for_spec(id);
        self.collapsed_route_roots.remove(&id);
        self.expanded_mock_routes
            .retain(|(spec_id, _)| *spec_id != id);
        if self.selected_spec == Some(id) {
            self.selected_spec = self.specs.first().map(|entry| entry.id);
            for entry in &mut self.specs {
                entry.selected = Some(entry.id) == self.selected_spec;
            }
        }
        self.persist();
        Some(id)
    }

    fn clear_stale_keyboard_focus(&mut self, active: Option<(ApiSpecId, Option<usize>)>) -> bool {
        let Some(focus) = self.focused.as_ref() else {
            return false;
        };
        if api_focus_targets_active_tab(focus, active) {
            return true;
        }
        self.focused = None;
        false
    }
}

impl Drop for ApiClientState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.python_version_list_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(cancel) = &self.python_install_cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ApiSpecsPersist {
    specs: Vec<ApiSpecEntry>,
    selected_spec: Option<ApiSpecId>,
    #[serde(default)]
    last_resolved_host: Option<ApiResolvedHost>,
    next_id: u64,
}

fn load_api_specs_checked() -> Result<Option<ApiSpecsPersist>, String> {
    load_api_specs_from_checked(&api_specs_path())
}

fn load_api_specs_from_checked(path: &std::path::Path) -> Result<Option<ApiSpecsPersist>, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("API specifications не прочитаны: {error}")),
    };
    serde_json::from_str::<ApiSpecsPersist>(&content)
        .map(Some)
        .map_err(|error| {
            let backup_note = crate::platform::corrupt_file_backup_note(path);
            format!("API specifications повреждены: {error}{backup_note}")
        })
}

fn api_focus_targets_active_tab(
    focus: &ApiFocus,
    active: Option<(ApiSpecId, Option<usize>)>,
) -> bool {
    match focus {
        ApiFocus::ImportUrl => true,
        ApiFocus::RouteFilter => true,
        ApiFocus::MockProxyBase => true,
        ApiFocus::MockPythonUvPath => true,
        ApiFocus::MockPythonVersion => true,
        ApiFocus::MockPythonCustomPath => true,
        ApiFocus::MockManualPath { .. } => true,
        ApiFocus::MockContract { .. }
        | ApiFocus::MockPrelude { .. }
        | ApiFocus::MockBody { .. }
        | ApiFocus::MockSignature { .. }
        | ApiFocus::MockStaticResponse { .. }
        | ApiFocus::MockContractField { .. } => true,
        ApiFocus::AuthValue { spec_id, .. }
        | ApiFocus::AuthRefreshToken { spec_id, .. }
        | ApiFocus::AuthUsername { spec_id, .. }
        | ApiFocus::AuthPassword { spec_id, .. } => {
            active.is_some_and(|(active_spec, _)| active_spec == *spec_id)
        }
        ApiFocus::PathParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::QueryParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::BodyField {
            spec_id, route_idx, ..
        }
        | ApiFocus::Body { spec_id, route_idx }
        | ApiFocus::InputSchema { spec_id, route_idx }
        | ApiFocus::OutputSchema { spec_id, route_idx }
        | ApiFocus::Response { spec_id, route_idx } => {
            active.is_some_and(|(active_spec, active_route)| {
                active_spec == *spec_id && active_route == Some(*route_idx)
            })
        }
    }
}

fn api_focus_order_for_view(
    spec_id: ApiSpecId,
    model: &ApiSpecModel,
    state: &ApiClientTabState,
) -> Vec<ApiFocus> {
    let mut out = Vec::new();
    if state.auth_view {
        for scheme in &model.security_schemes {
            if matches!(
                scheme.kind,
                ApiSecuritySchemeKind::Http { ref scheme, .. } if scheme.eq_ignore_ascii_case("basic")
            ) {
                out.push(ApiFocus::AuthUsername {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
                out.push(ApiFocus::AuthPassword {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
            } else {
                out.push(ApiFocus::AuthValue {
                    spec_id,
                    scheme: scheme.name.clone(),
                });
            }
        }
        return out;
    }

    let Some(route_idx) = state
        .route_idx
        .or_else(|| (!model.routes.is_empty()).then_some(0))
    else {
        return out;
    };
    let Some(route) = model.routes.get(route_idx) else {
        return out;
    };
    for param in &route.path_params {
        out.push(ApiFocus::PathParam {
            spec_id,
            route_idx,
            name: param.name.clone(),
        });
    }
    for param in &route.query_params {
        out.push(ApiFocus::QueryParam {
            spec_id,
            route_idx,
            name: param.name.clone(),
        });
    }
    if let Some(body) = &route.request_body {
        if body.is_multipart || body.is_form_urlencoded {
            if let Some(schema) = body
                .schema
                .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            {
                for prop in &schema.properties {
                    out.push(ApiFocus::BodyField {
                        spec_id,
                        route_idx,
                        name: prop.name.clone(),
                    });
                }
            }
        } else {
            out.push(ApiFocus::Body { spec_id, route_idx });
        }
    }
    out
}

