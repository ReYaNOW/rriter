impl crate::app::api_client::ApiClientState {
    pub(crate) fn api_mock_openapi_export_snapshot(
        &self,
    ) -> (
        Vec<(crate::app::api_client::ApiSpecEntry, crate::app::api_client::ApiSpecModel)>,
        crate::app::api_mock::types::ApiMockState,
    ) {
        let specs = self
            .specs
            .iter()
            .filter_map(|entry| {
                let model = self.models.get(&entry.id)?;
                Some((entry.clone(), model.clone()))
            })
            .collect::<Vec<_>>();
        (specs, self.mock.clone())
    }

    pub fn open_api_mock_route_reset_dialog(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) {
        let route_label = self
            .api_mock_route_context(active, route_idx)
            .map(|(method, path, _, _)| format!("{} {}", method.as_str(), path))
            .unwrap_or_else(|| format!("route {}", route_idx.saturating_add(1)));
        self.mock_contract_constraint_menu = None;
        self.mock_contract_field_delete_dialog = None;
        self.mock_route_reset_dialog =
            Some(crate::app::api_client::ApiMockRouteResetDialog {
                route_idx,
                route_label,
            });
    }

    pub fn confirm_api_mock_route_reset(&mut self, active: Option<&ApiActiveRoute>) {
        let Some(dialog) = self.mock_route_reset_dialog.take() else {
            return;
        };
        self.reset_api_route_mock(active, dialog.route_idx);
    }

    pub fn open_api_mock_contract_field_delete_dialog(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) {
        let Some((_, _, route, model)) = self.api_mock_route_context(active, route_idx) else {
            return;
        };
        let Some(script) = self.api_route_python_script(active, route_idx) else {
            return;
        };
        let contract =
            crate::app::api_mock::types::api_mock_effective_contract(script, &route, &model);
        let Some(field_label) = Self::api_mock_contract_class(&contract, group)
            .fields
            .get(field_idx)
            .map(|field| field.name.clone())
        else {
            return;
        };
        self.mock_contract_constraint_menu = None;
        self.mock_route_reset_dialog = None;
        self.mock_contract_field_delete_dialog =
            Some(crate::app::api_client::ApiMockContractFieldDeleteDialog {
                route_idx,
                group,
                field_idx,
                field_label,
            });
    }

    /// Returns the route whose contract changed, for the App-side tools restart.
    pub fn confirm_api_mock_contract_field_delete(
        &mut self,
        active: Option<&ApiActiveRoute>,
    ) -> Option<usize> {
        let dialog = self.mock_contract_field_delete_dialog.take()?;
        self.remove_api_mock_contract_field(active, dialog.route_idx, dialog.group, dialog.field_idx)
            .then_some(dialog.route_idx)
    }

    pub(crate) fn api_mock_contract_field_delete_target_exists(
        &self,
        active: Option<&ApiActiveRoute>,
    ) -> bool {
        let Some(dialog) = self.mock_contract_field_delete_dialog.as_ref() else {
            return false;
        };
        self.api_mock_contract_field_exists(active, dialog.route_idx, dialog.group, dialog.field_idx)
    }

    fn api_mock_contract_field_exists(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        let Some((_, _, route, model)) = self.api_mock_route_context(active, route_idx) else {
            return false;
        };
        let Some(script) = self.api_route_python_script(active, route_idx) else {
            return false;
        };
        let contract = crate::app::api_mock::types::api_mock_effective_contract(
            script, &route, &model,
        );
        Self::api_mock_contract_class(&contract, group)
            .fields
            .get(field_idx)
            .is_some()
    }

    pub(crate) fn api_mock_contract_source_for_route(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(active, route_idx)?;
        let script = self.api_route_python_script(active, route_idx)?;
        Some(crate::app::api_mock::contract::api_mock_contract_source_text(
            script, &route, &model,
        ))
    }

    fn api_mock_signature_for_route(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(active, route_idx)?;
        let script = self.api_route_python_script(active, route_idx)?;
        let contract = crate::app::api_mock::types::api_mock_effective_contract(
            script, &route, &model,
        );
        Some(crate::app::api_mock::contract::api_mock_handler_signature_text(
            &contract,
        ))
    }

    /// Applies a contract edit and commits the mock config. The API focus commit
    /// before and the ty tools restart after (on `true`) are run by the App router
    /// `App::edit_api_mock_contract`.
    fn mutate_api_mock_contract_no_commit<F>(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        mut apply: F,
    ) -> bool
    where
        F: FnMut(&mut crate::app::api_mock::types::ApiMockPythonContract),
    {
        let Some((_, _, route, model)) = self.api_mock_route_context(active, route_idx) else {
            return false;
        };
        let default_contract =
            crate::app::api_mock::types::default_contract_from_route(&route, &model);
        let Some(script) = self.api_route_python_script_mut(active, route_idx) else {
            return false;
        };
        if script.contract.is_empty() {
            script.contract = default_contract;
        }
        apply(&mut script.contract);
        script.contract_source =
            crate::app::api_mock::contract::api_mock_contract_state_text(&script.contract);
        self.invalidate_api_mock_contract_tools(route_idx);
        self.commit_mock_config();
        true
    }

    fn api_mock_contract_class_mut(
        contract: &mut crate::app::api_mock::types::ApiMockPythonContract,
        group: crate::ui_system::ApiMockContractFieldGroup,
    ) -> &mut crate::app::api_mock::types::ApiMockClassSpec {
        match group {
            crate::ui_system::ApiMockContractFieldGroup::Path => &mut contract.path_params,
            crate::ui_system::ApiMockContractFieldGroup::Query => &mut contract.query,
            crate::ui_system::ApiMockContractFieldGroup::Body => &mut contract.body,
        }
    }

    fn api_mock_contract_class(
        contract: &crate::app::api_mock::types::ApiMockPythonContract,
        group: crate::ui_system::ApiMockContractFieldGroup,
    ) -> &crate::app::api_mock::types::ApiMockClassSpec {
        match group {
            crate::ui_system::ApiMockContractFieldGroup::Path => &contract.path_params,
            crate::ui_system::ApiMockContractFieldGroup::Query => &contract.query,
            crate::ui_system::ApiMockContractFieldGroup::Body => &contract.body,
        }
    }

    pub(crate) fn api_mock_contract_field_prop_text(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    ) -> String {
        let (_, _, route, model) = match self.api_mock_route_context(active, route_idx) {
            Some(ctx) => ctx,
            None => return String::new(),
        };
        let Some(script) = self.api_route_python_script(active, route_idx) else {
            return String::new();
        };
        let contract = crate::app::api_mock::types::api_mock_effective_contract(
            script, &route, &model,
        );
        let Some(field) = Self::api_mock_contract_class(&contract, group)
            .fields
            .get(field_idx)
        else {
            return String::new();
        };
        crate::app::api_client::api_mock_contract_field_prop_value(field, prop)
    }

    pub(crate) fn commit_api_mock_contract_field_prop(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
        text: &str,
    ) -> bool {
        let value = text.trim();
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                .fields
                .get_mut(field_idx)
            else {
                return;
            };
            match prop {
                crate::ui_system::ApiMockContractFieldProp::Required => {
                    field.required = true;
                }
                crate::ui_system::ApiMockContractFieldProp::Nullable => {
                    field.nullable = true;
                    field.constraints.nullable = true;
                }
                crate::ui_system::ApiMockContractFieldProp::Default => {
                    field.default_value = (!value.is_empty()).then(|| value.to_string());
                }
                crate::ui_system::ApiMockContractFieldProp::Enum => {
                    field.enum_values = value
                        .split([',', '\n'])
                        .map(|item| item.trim().trim_matches(['"', '\'']))
                        .filter(|item| !item.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                crate::ui_system::ApiMockContractFieldProp::MinLength => {
                    field.constraints.min_length = parse_optional_usize(value);
                }
                crate::ui_system::ApiMockContractFieldProp::MaxLength => {
                    field.constraints.max_length = parse_optional_usize(value);
                }
                crate::ui_system::ApiMockContractFieldProp::Pattern => {
                    field.constraints.pattern = (!value.is_empty()).then(|| value.to_string());
                }
                crate::ui_system::ApiMockContractFieldProp::Minimum => {
                    field.constraints.minimum = (!value.is_empty()).then(|| value.to_string());
                    field.constraints.exclusive_minimum = false;
                }
                crate::ui_system::ApiMockContractFieldProp::Maximum => {
                    field.constraints.maximum = (!value.is_empty()).then(|| value.to_string());
                    field.constraints.exclusive_maximum = false;
                }
                crate::ui_system::ApiMockContractFieldProp::MinItems => {
                    field.constraints.min_items = parse_optional_usize(value);
                }
                crate::ui_system::ApiMockContractFieldProp::MaxItems => {
                    field.constraints.max_items = parse_optional_usize(value);
                }
            }
        })
    }

    pub(crate) fn add_api_mock_contract_field_constraint(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    ) -> bool {
        match prop {
            crate::ui_system::ApiMockContractFieldProp::Required => {
                self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
                    if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                        .fields
                        .get_mut(field_idx)
                    {
                        field.required = true;
                    }
                })
            }
            crate::ui_system::ApiMockContractFieldProp::Nullable => {
                self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
                    if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                        .fields
                        .get_mut(field_idx)
                    {
                        field.nullable = true;
                        field.constraints.nullable = true;
                    }
                })
            }
            // Value props are edited through an input the App router focuses.
            _ => false,
        }
    }

    pub(crate) fn close_api_mock_contract_constraint_menu(&mut self) {
        self.mock_contract_constraint_menu = None;
    }

    fn invalidate_api_mock_contract_tools(&mut self, route_idx: usize) {
        for part in [
            crate::app::api_mock::ty_check::ApiMockSourcePart::Contract,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Signature,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        ] {
            self.mock_highlight_cache.remove(&(route_idx, part));
        }
        self.mock_python_editors
            .remove(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Contract));
        self.mock_python_editors
            .remove(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Signature));
        if matches!(
            self.focused,
            Some(crate::app::api_client::ApiFocus::MockContract { route_idx: focused })
                if focused == route_idx
        ) || matches!(
            self.focused,
            Some(crate::app::api_client::ApiFocus::MockSignature { route_idx: focused })
                if focused == route_idx
        ) {
            self.focused = None;
            self.input_editor = crate::editor::Editor::new(512);
        }
        self.mock_highlight_target = None;
        self.mock_highlight_spans.clear();
        self.mock_ty_diagnostics.clear();
        self.mock.check_status =
            crate::app::api_mock::types::ApiMockCheckStatus::Idle;
        self.reset_api_mock_hover_tracking();
    }

    pub fn toggle_api_mock_contract_query(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            contract.query.enabled = !contract.query.enabled;
        })
    }

    pub fn toggle_api_mock_contract_path(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            contract.path_params.enabled = !contract.path_params.enabled;
        })
    }

    pub fn toggle_api_mock_contract_body(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            contract.body.enabled = !contract.body.enabled;
        })
    }

    pub fn toggle_api_mock_contract_path_field(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            if let Some(field) = contract.path_params.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_query_field(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            if let Some(field) = contract.query.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_body_field(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            if let Some(field) = contract.body.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_field_required(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                .fields
                .get_mut(field_idx)
            {
                field.required = !field.required;
            }
        })
    }

    pub fn toggle_api_mock_contract_field_nullable(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                .fields
                .get_mut(field_idx)
            {
                field.nullable = !field.nullable;
                field.constraints.nullable = field.nullable;
            }
        })
    }

    pub fn remove_api_mock_contract_field(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        if !self.api_mock_contract_field_exists(active, route_idx, group, field_idx) {
            return false;
        }
        self.mock_contract_constraint_menu = None;
        let clear_focus = matches!(
            self.focused,
            Some(crate::app::api_client::ApiFocus::MockContractField {
                route_idx: focused_route,
                group: focused_group,
                field_idx: focused_field,
                ..
            }) if focused_route == route_idx && focused_group == group && focused_field >= field_idx
        );
        let changed = self.mutate_api_mock_contract_no_commit(active, route_idx, |contract| {
            let fields = &mut Self::api_mock_contract_class_mut(contract, group).fields;
            if field_idx < fields.len() {
                fields.remove(field_idx);
            }
        });
        if changed && clear_focus {
            self.focused = None;
            self.input_editor = crate::editor::Editor::new(512);
        }
        changed
    }

}

fn parse_optional_usize(value: &str) -> Option<usize> {
    if value.is_empty() {
        None
    } else {
        value.parse().ok()
    }
}
