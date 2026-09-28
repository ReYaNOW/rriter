// Spec import and tab opening: App-level entry points that create tabs, start
// loads and forward to the state objects (`ApiClientState`) that own the data.
// App routes here; the load/spawn effects stay in `App`.

impl crate::app::App {
    fn queue_api_load(
        &mut self,
        id: ApiSpecId,
        generation: u64,
        receiver: std::io::Result<crate::ui_waker::OneShot<ApiLoadResult>>,
    ) {
        match receiver {
            Ok(rx) => self.api_load_rx.push(crate::app::api_client::ApiLoadReceiver {
                id,
                generation,
                rx,
            }),
            Err(err) => {
                self.ide_panel.api.mark_load_error(
                    id,
                    ApiLoadError::new(
                        ApiLoadErrorKind::Io,
                        format!("Не удалось запустить загрузку OpenAPI: {err}"),
                    ),
                );
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_api_file_picker(&mut self) {
        if !native_picker_can_start(&self.api_import_file_rx) {
            self.ide_panel.api.import_error = Some("Окно выбора OpenAPI уже открыто".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            if let Some(path) = crate::platform::pick_file_with_filter(
                self.external_requests.sink(),
                "Импорт openapi.json",
                "OpenAPI JSON",
                &["json"],
            ) {
                self.start_api_local_import(path);
            }
            return;
        }
        let requests = self.external_requests.sink().clone();
        match self.ui_waker.spawn_one_shot("rriter-api-file-picker", move || {
            crate::platform::pick_file_with_filter(
                &requests, "Импорт openapi.json", "OpenAPI JSON", &["json"],
            )
        }) {
            Ok(job) => self.api_import_file_rx = Some(job),
            Err(err) => self.ide_panel.api.import_error = Some(format!("Не удалось открыть выбор OpenAPI: {err}")),
        }
    }

    fn trigger_api_body_file_picker(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
        multi: bool,
    ) {
        if !native_picker_can_start(&self.api_body_file_rx) {
            self.ide_panel.api.import_error = Some("Окно выбора body-файла уже открыто".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            let requests = self.external_requests.sink();
            let paths = if multi {
                crate::platform::pick_files(requests, "Выбрать файл")
            } else {
                crate::platform::pick_file(requests, "Выбрать файл")
                    .into_iter()
                    .collect()
            };
            self.apply_api_body_file_pick(ApiBodyFilePickResult {
                spec_id,
                route_idx,
                name,
                paths,
            });
            return;
        }
        let requests = self.external_requests.sink().clone();
        match self.ui_waker.spawn_one_shot("rriter-api-body-file-picker", move || {
            let paths = if multi {
                crate::platform::pick_files(&requests, "Выбрать файл")
            } else {
                crate::platform::pick_file(&requests, "Выбрать файл").into_iter().collect()
            };
            ApiBodyFilePickResult {
                spec_id, route_idx, name, paths,
            }
        }) {
            Ok(job) => self.api_body_file_rx = Some(job),
            Err(err) => self.ide_panel.api.import_error = Some(format!("Не удалось открыть выбор body-файла: {err}")),
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_path_picker(&mut self, kind: ApiPythonPathPickKind) {
        if !native_picker_can_start(&self.ide_panel.api.python_path_pick_rx) {
            self.ide_panel.api.mock.uv.last_error = "Окно выбора Python/uv уже открыто".to_string();
            return;
        }
        let title = match kind {
            ApiPythonPathPickKind::Uv => "Выбрать исполняемый файл uv",
            ApiPythonPathPickKind::CustomPython => "Выбрать исполняемый файл Python",
        };
        if crate::platform::native_dialog_requires_main_thread() {
            let path = crate::platform::pick_file(self.external_requests.sink(), title);
            self.ide_panel.api.apply_python_path_pick(ApiPythonPathPickResult { kind, path });
            return;
        }
        let requests = self.external_requests.sink().clone();
        match self.ui_waker.spawn_one_shot("rriter-api-python-path-picker", move || {
            let path = crate::platform::pick_file(&requests, title);
            ApiPythonPathPickResult { kind, path }
        }) {
            Ok(job) => self.ide_panel.api.python_path_pick_rx = Some(job),
            Err(err) => self.ide_panel.api.mock.uv.last_error = format!("Не удалось открыть выбор пути: {err}"),
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_version_list(&mut self) {
        self.ide_panel.api.trigger_api_python_version_list(&self.ui_waker);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_install(&mut self) {
        self.ide_panel.api.trigger_api_python_install(&self.ui_waker);
    }

    fn apply_api_body_file_pick(&mut self, result: ApiBodyFilePickResult) {
        let (tabs, active_tab, api) = (
            &mut self.tabs,
            self.active_tab,
            &mut self.ide_panel.api,
        );
        let state = tabs.get_mut(active_tab).and_then(|tab| match &mut tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) if meta.spec_id == result.spec_id => {
                Some(state)
            }
            _ => None,
        });
        api.apply_api_body_file_pick(state, result);
    }

    pub fn start_api_local_import(&mut self, path: PathBuf) {
        let id = self.ide_panel.api.alloc_spec_id();
        let generation = self.ide_panel.api.begin_load(id, true);
        let receiver = spawn_load_local(id, generation, path, &self.ui_waker);
        self.queue_api_load(id, generation, receiver);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn start_api_url_import_from_input(&mut self) {
        let raw = self.ide_panel.api.input_editor.get_full_text();
        let url = match validate_api_url(&raw) {
            Ok(url) => url.to_string(),
            Err(err) => {
                self.ide_panel.api.import_error = Some(err.message);
                self.ide_panel.api.import_error_at = Some(now_epoch_secs());
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        // Re-importing a known URL reloads that spec instead of adding a duplicate.
        let existing = self.ide_panel.api.specs.iter().find_map(|entry| match &entry.source {
            ApiSpecSource::Url(known) if *known == url => Some(entry.id),
            _ => None,
        });
        let id = existing.unwrap_or_else(|| self.ide_panel.api.alloc_spec_id());
        self.ide_panel.api.import_error = None;
        self.ide_panel.api.import_error_at = None;
        self.ide_panel.api.import_url_open = false;
        self.ide_panel.api.focused = None;
        let generation = self.ide_panel.api.begin_load(id, true);
        let receiver = spawn_load_url(id, generation, url, &self.ui_waker);
        self.queue_api_load(id, generation, receiver);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn refresh_api_spec(&mut self, id: ApiSpecId) {
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        let generation = self.ide_panel.api.begin_load(id, false);
        match entry.source {
            ApiSpecSource::Local(path) => {
                let receiver = spawn_load_local(id, generation, path, &self.ui_waker);
                self.queue_api_load(id, generation, receiver);
            }
            ApiSpecSource::Url(url) => {
                let receiver = spawn_load_url(id, generation, url, &self.ui_waker);
                self.queue_api_load(id, generation, receiver);
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn ensure_api_model_loaded(&mut self, id: ApiSpecId) {
        if self.ide_panel.api.models.contains_key(&id) || self.ide_panel.api.loading.contains(&id) {
            return;
        }
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        let generation = self.ide_panel.api.begin_load(id, false);
        match entry.source {
            ApiSpecSource::Local(path) => {
                let receiver = spawn_load_local(id, generation, path, &self.ui_waker);
                self.queue_api_load(id, generation, receiver);
            }
            ApiSpecSource::Url(url) => {
                let receiver = spawn_load_cached_url(id, generation, url, &self.ui_waker);
                self.queue_api_load(id, generation, receiver);
            }
        }
    }

    fn push_api_client_tab(&mut self, tab: crate::app::EditorTab, clear_highlighter_rx: bool) {
        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        if clear_highlighter_rx {
            while self.highlighter.rx.try_recv().is_ok() {}
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    fn last_api_route_tab_idx(&self, id: ApiSpecId) -> Option<usize> {
        self.tabs.iter().rposition(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && !state.auth_view
            )
        })
    }

    fn open_new_api_spec_tab(&mut self, id: ApiSpecId) {
        let title = self.ide_panel.api.api_spec_title(id);
        let mut api_state = ApiClientTabState::default();
        let mut route_method = None;
        let mut route_path = String::new();
        if let Some(model) = self.ide_panel.api.models.get(&id)
            && let Some(route) = model.routes.first()
        {
            api_state.route_idx = Some(0);
            route_method = Some(route.method);
            route_path = route.path.clone();
            fill_api_tab_inputs(&mut api_state, route, model);
        }

        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: title.clone(),
            file_extension: String::new(),
            markdown: Default::default(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            closing_hints: Default::default(),
            deleted: false,
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: api_state.route_idx.map(|route_idx| {
                        ApiClientRouteIdentity::OpenApi {
                            spec_id: id,
                            route_idx,
                        }
                    }),
                    route_method,
                    route_path,
                },
                api_state,
            ),
        };

        self.push_api_client_tab(tab, true);
    }

    pub fn open_api_spec_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        self.ide_panel.api.refresh_mock_server();

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && !state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        self.open_new_api_spec_tab(id);
    }

    pub fn open_api_auth_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        self.ide_panel.api.refresh_mock_server();
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| format!("Auth · {}", entry.title))
            .unwrap_or_else(|| "API Auth".to_string());

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        let api_state = ApiClientTabState {
            auth_view: true,
            ..Default::default()
        };
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: title.clone(),
            file_extension: String::new(),
            markdown: Default::default(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            closing_hints: Default::default(),
            deleted: false,
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: None,
                    route_method: None,
                    route_path: String::new(),
                },
                api_state,
            ),
        };

        self.push_api_client_tab(tab, false);
    }

    pub fn open_api_route(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        self.open_api_route_with_new_tab(spec_id, route_idx, false);
    }

    pub fn open_api_route_with_new_tab(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        force_new_tab: bool,
    ) {
        self.ide_panel.api.select_spec(spec_id);
        self.ensure_api_model_loaded(spec_id);
        self.ide_panel.api.refresh_mock_server();
        if force_new_tab {
            self.open_new_api_spec_tab(spec_id);
        } else if let Some(idx) = self.last_api_route_tab_idx(spec_id) {
            self.switch_to_tab(idx);
        } else {
            self.open_new_api_spec_tab(spec_id);
        }
        let mut needs_input_sync = false;
        let route_header = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .map(|route| (route.method, route.path.clone()));
        if let Some((meta, state)) = self.active_api_tab_mut_for(spec_id) {
            state.remember_view_scroll();
            state.remember_route_state();
            state.auth_view = false;
            meta.route_identity = Some(ApiClientRouteIdentity::OpenApi { spec_id, route_idx });
            if let Some((method, path)) = route_header {
                meta.route_method = Some(method);
                meta.route_path = path;
            }
            if !state.restore_route_state(route_idx) {
                state.reset_route_content(Some(route_idx));
                needs_input_sync = true;
            }
            state.restore_view_scroll(false, Some(route_idx));
        }
        if needs_input_sync {
            self.sync_api_tab_inputs(spec_id, route_idx);
        }
        self.save_tabs_state();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn open_api_manual_route(&mut self, manual_idx: usize) {
        self.commit_api_focus();
        let Some(route) = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .get(manual_idx)
            .cloned()
        else {
            return;
        };
        let stable_id = route.stable_id.clone();
        let title = api_manual_route_title(route.method, &route.path);
        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(
                    ApiClientTabMeta {
                        route_identity:
                            Some(ApiClientRouteIdentity::Manual { stable_id: tab_id }),
                        ..
                    },
                    _
                ) if tab_id == &stable_id
            )
        }) {
            self.switch_to_tab(idx);
            if let Some((meta, state)) = self.active_api_tab_mut_for(API_MANUAL_MOCK_SPEC_ID) {
                meta.title = "Mock".to_string();
                meta.route_method = Some(route.method);
                meta.route_path = route.path.clone();
                state.route_idx = Some(manual_idx);
                state.route_text_selection = None;
            }
            self.ide_panel
                .api
                .expanded_mock_routes
                .insert((API_MANUAL_MOCK_SPEC_ID, manual_idx));
            self.base_title = title;
            if let Some(window) = self.window.as_ref() {
                crate::app::App::update_window_title(window, &self.base_title, false);
                window.request_redraw();
            }
            return;
        }

        let api_state = ApiClientTabState {
            route_idx: Some(manual_idx),
            ..Default::default()
        };
        self.ide_panel
            .api
            .expanded_mock_routes
            .insert((API_MANUAL_MOCK_SPEC_ID, manual_idx));
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: title.clone(),
            file_extension: String::new(),
            markdown: Default::default(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            closing_hints: Default::default(),
            deleted: false,
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: API_MANUAL_MOCK_SPEC_ID,
                    title: "Mock".to_string(),
                    route_identity: Some(ApiClientRouteIdentity::Manual { stable_id }),
                    route_method: Some(route.method),
                    route_path: route.path.clone(),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    fn sync_api_manual_route_tabs(&mut self) {
        let routes = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .map(|(idx, route)| {
                (
                    idx,
                    route.stable_id.clone(),
                    route.method,
                    route.path.clone(),
                    api_manual_route_title(route.method, &route.path),
                )
            })
            .collect::<Vec<_>>();
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
            let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind else {
                continue;
            };
            let Some(ApiClientRouteIdentity::Manual { stable_id }) = &meta.route_identity else {
                continue;
            };
            if let Some((manual_idx, _, method, path, title)) =
                routes.iter().find(|(_, id, _, _, _)| id == stable_id)
            {
                meta.title = "Mock".to_string();
                meta.route_method = Some(*method);
                meta.route_path = path.clone();
                if state.route_idx != Some(*manual_idx) {
                    state.route_text_selection = None;
                }
                state.route_idx = Some(*manual_idx);
                tab.base_title = title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = title.clone();
                }
            } else {
                meta.title = "Mock removed".to_string();
                meta.route_method = None;
                meta.route_path.clear();
                state.route_idx = None;
                state.route_text_selection = None;
                tab.base_title = meta.title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = meta.title.clone();
                }
            }
        }
    }
}
