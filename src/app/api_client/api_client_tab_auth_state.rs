#[derive(Clone, Debug)]
pub struct ApiClientTabState {
    pub route_idx: Option<usize>,
    pub auth_view: bool,
    pub server_idx: usize,
    pub path_values: Vec<ApiInputValue>,
    pub query_values: Vec<ApiInputValue>,
    pub body_values: Vec<ApiInputValue>,
    pub body_file_paths: FxHashMap<String, Vec<PathBuf>>,
    pub body_json: String,
    pub response: Option<ApiJobResponse>,
    pub response_view: ApiResponseView,
    pub input_doc_view: ApiInputDocView,
    pub output_doc_view: ApiOutputDocView,
    pub input_schema_idx: usize,
    pub input_schema_menu_open: bool,
    pub output_status_idx: usize,
    pub output_example_idx: usize,
    pub output_schema_idx: usize,
    pub output_schema_menu_open: bool,
    pub output_schema_menu_anim: f32,
    pub output_schema_menu_scroll: ScrollState,
    pub input_schema_collapsed: FxHashSet<String>,
    pub output_schema_collapsed: FxHashSet<String>,
    pub pending: bool,
    pub pending_request_id: Option<u64>,
    pub tab_scroll: ScrollState,
    pub body_scroll: ScrollState,
    pub body_scroll_x: ScrollState,
    pub output_scroll: ScrollState,
    pub output_scroll_x: ScrollState,
    pub mock_static_response_scroll: ScrollState,
    pub mock_static_response_scroll_x: ScrollState,
    pub response_scroll: ScrollState,
    pub response_scroll_x: ScrollState,
    pub focused_schema_pane: Option<ApiSchemaPaneFocus>,
    pub(crate) route_text_selection: Option<ApiRouteTextSelection>,
    pub view_scrolls: Vec<ApiViewScrollMemory>,
    pub route_states: Vec<ApiRouteStateMemory>,
}

#[derive(Clone, Debug)]
pub struct ApiViewScrollMemory {
    pub auth_view: bool,
    pub route_idx: Option<usize>,
    pub current: f32,
    pub target: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRouteStateMemory {
    pub route_idx: usize,
    pub path_values: Vec<ApiInputValue>,
    pub query_values: Vec<ApiInputValue>,
    pub body_values: Vec<ApiInputValue>,
    pub body_file_paths: FxHashMap<String, Vec<PathBuf>>,
    pub body_json: String,
    pub response: Option<ApiJobResponse>,
    pub response_view: ApiResponseView,
    pub input_doc_view: ApiInputDocView,
    pub output_doc_view: ApiOutputDocView,
    pub input_schema_idx: usize,
    pub input_schema_menu_open: bool,
    pub output_status_idx: usize,
    pub output_example_idx: usize,
    pub output_schema_idx: usize,
    pub output_schema_menu_open: bool,
    pub input_schema_collapsed: FxHashSet<String>,
    pub output_schema_collapsed: FxHashSet<String>,
    pub pending: bool,
    pub pending_request_id: Option<u64>,
}

impl Default for ApiClientTabState {
    fn default() -> Self {
        Self {
            route_idx: None,
            auth_view: false,
            server_idx: 0,
            path_values: Vec::new(),
            query_values: Vec::new(),
            body_values: Vec::new(),
            body_file_paths: FxHashMap::default(),
            body_json: "{\n  \n}".to_string(),
            response: None,
            response_view: ApiResponseView::Body,
            input_doc_view: ApiInputDocView::Input,
            output_doc_view: ApiOutputDocView::Example,
            input_schema_idx: 0,
            input_schema_menu_open: false,
            output_status_idx: 0,
            output_example_idx: 0,
            output_schema_idx: 0,
            output_schema_menu_open: false,
            output_schema_menu_anim: 0.0,
            output_schema_menu_scroll: ScrollState::new(7.0),
            input_schema_collapsed: FxHashSet::default(),
            output_schema_collapsed: FxHashSet::default(),
            pending: false,
            pending_request_id: None,
            tab_scroll: ScrollState::new(7.0),
            body_scroll: ScrollState::new(7.0),
            body_scroll_x: ScrollState::new(7.0),
            output_scroll: ScrollState::new(7.0),
            output_scroll_x: ScrollState::new(7.0),
            mock_static_response_scroll: ScrollState::new(7.0),
            mock_static_response_scroll_x: ScrollState::new(7.0),
            response_scroll: ScrollState::new(7.0),
            response_scroll_x: ScrollState::new(7.0),
            focused_schema_pane: None,
            route_text_selection: None,
            view_scrolls: Vec::new(),
            route_states: Vec::new(),
        }
    }
}

impl ApiClientTabState {
    pub fn reset_route_content(&mut self, route_idx: Option<usize>) {
        self.route_idx = route_idx;
        self.path_values.clear();
        self.query_values.clear();
        self.body_values.clear();
        self.body_file_paths.clear();
        self.body_json = "{\n  \n}".to_string();
        self.response = None;
        self.response_view = ApiResponseView::Body;
        self.input_doc_view = ApiInputDocView::Input;
        self.output_doc_view = ApiOutputDocView::Example;
        self.input_schema_idx = 0;
        self.input_schema_menu_open = false;
        self.output_status_idx = 0;
        self.output_example_idx = 0;
        self.output_schema_idx = 0;
        self.output_schema_menu_open = false;
        self.output_schema_menu_anim = 0.0;
        self.output_schema_menu_scroll.reset();
        self.input_schema_collapsed.clear();
        self.output_schema_collapsed.clear();
        self.pending = false;
        self.pending_request_id = None;
        self.body_scroll.reset();
        self.body_scroll_x.reset();
        self.output_scroll.reset();
        self.output_scroll_x.reset();
        self.mock_static_response_scroll.reset();
        self.mock_static_response_scroll_x.reset();
        self.response_scroll.reset();
        self.response_scroll_x.reset();
        self.focused_schema_pane = None;
        self.route_text_selection = None;
    }

    pub fn remember_route_state(&mut self) {
        let Some(route_idx) = self.route_idx else {
            return;
        };
        let saved = ApiRouteStateMemory {
            route_idx,
            path_values: self.path_values.clone(),
            query_values: self.query_values.clone(),
            body_values: self.body_values.clone(),
            body_file_paths: self.body_file_paths.clone(),
            body_json: self.body_json.clone(),
            response: self.response.clone(),
            response_view: self.response_view,
            input_doc_view: self.input_doc_view,
            output_doc_view: self.output_doc_view,
            input_schema_idx: self.input_schema_idx,
            input_schema_menu_open: self.input_schema_menu_open,
            output_status_idx: self.output_status_idx,
            output_example_idx: self.output_example_idx,
            output_schema_idx: self.output_schema_idx,
            output_schema_menu_open: self.output_schema_menu_open,
            input_schema_collapsed: self.input_schema_collapsed.clone(),
            output_schema_collapsed: self.output_schema_collapsed.clone(),
            pending: self.pending,
            pending_request_id: self.pending_request_id,
        };
        if let Some(slot) = self
            .route_states
            .iter_mut()
            .find(|saved| saved.route_idx == route_idx)
        {
            *slot = saved;
        } else {
            self.route_states.push(saved);
        }
    }

    pub fn restore_route_state(&mut self, route_idx: usize) -> bool {
        let Some(saved) = self
            .route_states
            .iter()
            .find(|saved| saved.route_idx == route_idx)
            .cloned()
        else {
            return false;
        };
        self.route_idx = Some(route_idx);
        self.path_values = saved.path_values;
        self.query_values = saved.query_values;
        self.body_values = saved.body_values;
        self.body_file_paths = saved.body_file_paths;
        self.body_json = saved.body_json;
        self.response = saved.response;
        self.response_view = saved.response_view;
        self.input_doc_view = saved.input_doc_view;
        self.output_doc_view = saved.output_doc_view;
        self.input_schema_idx = saved.input_schema_idx;
        self.input_schema_menu_open = saved.input_schema_menu_open;
        self.output_status_idx = saved.output_status_idx;
        self.output_example_idx = saved.output_example_idx;
        self.output_schema_idx = saved.output_schema_idx;
        self.output_schema_menu_open = saved.output_schema_menu_open;
        self.output_schema_menu_anim = if self.output_schema_menu_open {
            1.0
        } else {
            0.0
        };
        self.output_schema_menu_scroll.reset();
        self.input_schema_collapsed = saved.input_schema_collapsed;
        self.output_schema_collapsed = saved.output_schema_collapsed;
        self.pending = saved.pending;
        self.pending_request_id = saved.pending_request_id;
        self.body_scroll.reset();
        self.body_scroll_x.reset();
        self.output_scroll.reset();
        self.output_scroll_x.reset();
        self.mock_static_response_scroll.reset();
        self.mock_static_response_scroll_x.reset();
        self.response_scroll.reset();
        self.response_scroll_x.reset();
        self.focused_schema_pane = None;
        self.route_text_selection = None;
        true
    }

    pub fn remember_view_scroll(&mut self) {
        let key = (self.auth_view, self.route_idx);
        if let Some(saved) = self
            .view_scrolls
            .iter_mut()
            .find(|saved| (saved.auth_view, saved.route_idx) == key)
        {
            saved.current = self.tab_scroll.current;
            saved.target = self.tab_scroll.target;
        } else {
            self.view_scrolls.push(ApiViewScrollMemory {
                auth_view: self.auth_view,
                route_idx: self.route_idx,
                current: self.tab_scroll.current,
                target: self.tab_scroll.target,
            });
        }
    }

    pub fn restore_view_scroll(&mut self, auth_view: bool, route_idx: Option<usize>) {
        if let Some(saved) = self
            .view_scrolls
            .iter()
            .find(|saved| saved.auth_view == auth_view && saved.route_idx == route_idx)
        {
            self.tab_scroll.jump_to(saved.current);
            self.tab_scroll.animate_to(saved.target);
        } else {
            self.tab_scroll.reset();
        }
    }
}

impl PartialEq for ApiClientTabState {
    fn eq(&self, other: &Self) -> bool {
        self.route_idx == other.route_idx
            && self.auth_view == other.auth_view
            && self.server_idx == other.server_idx
            && self.path_values == other.path_values
            && self.query_values == other.query_values
            && self.body_values == other.body_values
            && self.body_file_paths == other.body_file_paths
            && self.body_json == other.body_json
            && self.response == other.response
            && self.response_view == other.response_view
            && self.input_doc_view == other.input_doc_view
            && self.output_doc_view == other.output_doc_view
            && self.input_schema_idx == other.input_schema_idx
            && self.input_schema_menu_open == other.input_schema_menu_open
            && self.output_status_idx == other.output_status_idx
            && self.output_example_idx == other.output_example_idx
            && self.output_schema_idx == other.output_schema_idx
            && self.output_schema_menu_open == other.output_schema_menu_open
            && self.input_schema_collapsed == other.input_schema_collapsed
            && self.output_schema_collapsed == other.output_schema_collapsed
            && self.pending == other.pending
            && self.pending_request_id == other.pending_request_id
            && self.route_states == other.route_states
    }
}

impl Eq for ApiClientTabState {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiFocus {
    ImportUrl,
    RouteFilter,
    MockProxyBase,
    MockPythonUvPath,
    MockPythonVersion,
    MockPythonCustomPath,
    MockManualPath {
        manual_idx: usize,
    },
    MockContract {
        route_idx: usize,
    },
    MockPrelude {
        route_idx: usize,
    },
    MockBody {
        route_idx: usize,
    },
    MockSignature {
        route_idx: usize,
    },
    MockStaticResponse {
        route_idx: usize,
    },
    MockContractField {
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    },
    AuthValue {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthRefreshToken {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthUsername {
        spec_id: ApiSpecId,
        scheme: String,
    },
    AuthPassword {
        spec_id: ApiSpecId,
        scheme: String,
    },
    PathParam {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    QueryParam {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    BodyField {
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
    },
    Body {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    InputSchema {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    OutputSchema {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
    Response {
        spec_id: ApiSpecId,
        route_idx: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiAuthStore {
    #[serde(default)]
    pub entries: Vec<ApiAuthEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiAuthEntry {
    pub spec_id: ApiSpecId,
    pub scheme: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl ApiAuthStore {
    pub(crate) fn entry(&self, spec_id: ApiSpecId, scheme: &str) -> Option<&ApiAuthEntry> {
        self.entries
            .iter()
            .find(|entry| entry.spec_id == spec_id && entry.scheme == scheme)
    }

    fn entry_mut(&mut self, spec_id: ApiSpecId, scheme: &str) -> &mut ApiAuthEntry {
        if let Some(idx) = self
            .entries
            .iter()
            .position(|entry| entry.spec_id == spec_id && entry.scheme == scheme)
        {
            return &mut self.entries[idx];
        }
        self.entries.push(ApiAuthEntry {
            spec_id,
            scheme: scheme.to_string(),
            ..Default::default()
        });
        let idx = self.entries.len().saturating_sub(1);
        &mut self.entries[idx]
    }

    pub(crate) fn set_value(&mut self, spec_id: ApiSpecId, scheme: &str, value: String) {
        let entry = self.entry_mut(spec_id, scheme);
        entry.value = value;
        if !entry.value.is_empty() && entry.token_type.is_empty() {
            entry.token_type = "Bearer".to_string();
        }
    }

    fn remove(&mut self, spec_id: ApiSpecId, scheme: &str) {
        self.entries
            .retain(|entry| !(entry.spec_id == spec_id && entry.scheme == scheme));
    }

    fn retain_spec(&mut self, spec_id: ApiSpecId) {
        self.entries.retain(|entry| entry.spec_id != spec_id);
    }
}
