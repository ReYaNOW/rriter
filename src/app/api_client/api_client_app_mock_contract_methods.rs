impl crate::app::App {
    pub fn open_api_mock_route_reset_dialog(&mut self, route_idx: usize) {
        self.commit_api_focus();
        let route_label = self
            .api_mock_route_context(route_idx)
            .map(|(method, path, _, _)| format!("{} {}", method.as_str(), path))
            .unwrap_or_else(|| format!("route {}", route_idx.saturating_add(1)));
        self.ide_panel.api.mock_contract_constraint_menu = None;
        self.ide_panel.api.mock_contract_field_delete_dialog = None;
        self.ide_panel.api.mock_route_reset_dialog =
            Some(crate::app::api_client::ApiMockRouteResetDialog {
                route_idx,
                route_label,
            });
    }

    pub fn confirm_api_mock_route_reset(&mut self) {
        let Some(dialog) = self.ide_panel.api.mock_route_reset_dialog.take() else {
            return;
        };
        self.reset_api_route_mock(dialog.route_idx);
    }

    pub fn open_api_mock_contract_field_delete_dialog(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) {
        self.commit_api_focus();
        let Some((_, _, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_route_python_script(route_idx) else {
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
        self.ide_panel.api.mock_contract_constraint_menu = None;
        self.ide_panel.api.mock_route_reset_dialog = None;
        self.ide_panel.api.mock_contract_field_delete_dialog =
            Some(crate::app::api_client::ApiMockContractFieldDeleteDialog {
                route_idx,
                group,
                field_idx,
                field_label,
            });
    }

    pub fn confirm_api_mock_contract_field_delete(&mut self) {
        let Some(dialog) = self.ide_panel.api.mock_contract_field_delete_dialog.take() else {
            return;
        };
        self.remove_api_mock_contract_field(dialog.route_idx, dialog.group, dialog.field_idx);
    }

    pub(crate) fn api_mock_contract_source_for_route(
        &self,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(route_idx)?;
        let script = self.api_route_python_script(route_idx)?;
        Some(crate::app::api_mock::contract::api_mock_contract_source_text(
            script, &route, &model,
        ))
    }

    fn api_mock_signature_for_route(
        &self,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(route_idx)?;
        let script = self.api_route_python_script(route_idx)?;
        let contract = crate::app::api_mock::types::api_mock_effective_contract(
            script, &route, &model,
        );
        Some(crate::app::api_mock::contract::api_mock_handler_signature_text(
            &contract,
        ))
    }

    fn mutate_api_mock_contract<F>(&mut self, route_idx: usize, mut apply: F) -> bool
    where
        F: FnMut(&mut crate::app::api_mock::types::ApiMockPythonContract),
    {
        self.commit_api_focus();
        self.mutate_api_mock_contract_no_commit(route_idx, |contract| apply(contract))
    }

    fn mutate_api_mock_contract_no_commit<F>(&mut self, route_idx: usize, mut apply: F) -> bool
    where
        F: FnMut(&mut crate::app::api_mock::types::ApiMockPythonContract),
    {
        let Some((_, _, route, model)) = self.api_mock_route_context(route_idx) else {
            return false;
        };
        let default_contract =
            crate::app::api_mock::types::default_contract_from_route(&route, &model);
        let Some(script) = self.api_route_python_script_mut(route_idx) else {
            return false;
        };
        if script.contract.is_empty() {
            script.contract = default_contract;
        }
        apply(&mut script.contract);
        script.contract_source =
            crate::app::api_mock::contract::api_mock_contract_state_text(&script.contract);
        self.invalidate_api_mock_contract_tools(route_idx);
        self.ide_panel.api.persist();
        if !self.start_api_mock_route_tools_now(route_idx) {
            self.ide_panel.api.mock_ty_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(450));
        }
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
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    ) -> String {
        let (_, _, route, model) = match self.api_mock_route_context(route_idx) {
            Some(ctx) => ctx,
            None => return String::new(),
        };
        let Some(script) = self.api_route_python_script(route_idx) else {
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
        match prop {
            crate::ui_system::ApiMockContractFieldProp::Required
            | crate::ui_system::ApiMockContractFieldProp::Nullable => String::new(),
            crate::ui_system::ApiMockContractFieldProp::Default => {
                field.default_value.clone().unwrap_or_default()
            }
            crate::ui_system::ApiMockContractFieldProp::Enum => field.enum_values.join(", "),
            crate::ui_system::ApiMockContractFieldProp::MinLength => field
                .constraints
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            crate::ui_system::ApiMockContractFieldProp::MaxLength => field
                .constraints
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            crate::ui_system::ApiMockContractFieldProp::Pattern => {
                field.constraints.pattern.clone().unwrap_or_default()
            }
            crate::ui_system::ApiMockContractFieldProp::Minimum => {
                field.constraints.minimum.clone().unwrap_or_default()
            }
            crate::ui_system::ApiMockContractFieldProp::Maximum => {
                field.constraints.maximum.clone().unwrap_or_default()
            }
            crate::ui_system::ApiMockContractFieldProp::MinItems => field
                .constraints
                .min_items
                .map(|value| value.to_string())
                .unwrap_or_default(),
            crate::ui_system::ApiMockContractFieldProp::MaxItems => field
                .constraints
                .max_items
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn commit_api_mock_contract_field_prop(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
        text: &str,
    ) -> bool {
        let value = text.trim();
        self.mutate_api_mock_contract_no_commit(route_idx, |contract| {
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
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    ) -> bool {
        self.ide_panel.api.mock_contract_constraint_menu = None;
        match prop {
            crate::ui_system::ApiMockContractFieldProp::Required => {
                self.mutate_api_mock_contract(route_idx, |contract| {
                    if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                        .fields
                        .get_mut(field_idx)
                    {
                        field.required = true;
                    }
                })
            }
            crate::ui_system::ApiMockContractFieldProp::Nullable => {
                self.mutate_api_mock_contract(route_idx, |contract| {
                    if let Some(field) = Self::api_mock_contract_class_mut(contract, group)
                        .fields
                        .get_mut(field_idx)
                    {
                        field.nullable = true;
                        field.constraints.nullable = true;
                    }
                })
            }
            _ => {
                self.focus_api_input(crate::app::api_client::ApiFocus::MockContractField {
                    route_idx,
                    group,
                    field_idx,
                    prop,
                });
                true
            }
        }
    }

    fn invalidate_api_mock_contract_tools(&mut self, route_idx: usize) {
        for part in [
            crate::app::api_mock::ty_check::ApiMockSourcePart::Contract,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Signature,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        ] {
            self.ide_panel
                .api
                .mock_highlight_cache
                .remove(&(route_idx, part));
        }
        self.ide_panel
            .api
            .mock_python_editors
            .remove(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Contract));
        self.ide_panel
            .api
            .mock_python_editors
            .remove(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Signature));
        if matches!(
            self.ide_panel.api.focused,
            Some(crate::app::api_client::ApiFocus::MockContract { route_idx: focused })
                if focused == route_idx
        ) || matches!(
            self.ide_panel.api.focused,
            Some(crate::app::api_client::ApiFocus::MockSignature { route_idx: focused })
                if focused == route_idx
        ) {
            self.ide_panel.api.focused = None;
            self.ide_panel.api.input_editor = crate::editor::Editor::new(512);
        }
        self.ide_panel.api.mock_highlight_target = None;
        self.ide_panel.api.mock_highlight_spans.clear();
        self.ide_panel.api.mock_ty_diagnostics.clear();
        self.ide_panel.api.mock.check_status =
            crate::app::api_mock::types::ApiMockCheckStatus::Idle;
        self.reset_api_mock_hover_tracking();
    }

    pub fn toggle_api_mock_contract_query(&mut self, route_idx: usize) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            contract.query.enabled = !contract.query.enabled;
        })
    }

    pub fn toggle_api_mock_contract_path(&mut self, route_idx: usize) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            contract.path_params.enabled = !contract.path_params.enabled;
        })
    }

    pub fn toggle_api_mock_contract_body(&mut self, route_idx: usize) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            contract.body.enabled = !contract.body.enabled;
        })
    }

    pub fn toggle_api_mock_contract_path_field(
        &mut self,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            if let Some(field) = contract.path_params.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_query_field(
        &mut self,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            if let Some(field) = contract.query.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_body_field(
        &mut self,
        route_idx: usize,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            if let Some(field) = contract.body.fields.get_mut(field_idx) {
                field.enabled = !field.enabled;
            }
        })
    }

    pub fn toggle_api_mock_contract_field_required(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
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
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
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
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) -> bool {
        let Some((_, _, route, model)) = self.api_mock_route_context(route_idx) else {
            return false;
        };
        let Some(script) = self.api_route_python_script(route_idx) else {
            return false;
        };
        let contract =
            crate::app::api_mock::types::api_mock_effective_contract(script, &route, &model);
        if Self::api_mock_contract_class(&contract, group)
            .fields
            .get(field_idx)
            .is_none()
        {
            return false;
        }
        self.ide_panel.api.mock_contract_constraint_menu = None;
        let clear_focus = matches!(
            self.ide_panel.api.focused,
            Some(crate::app::api_client::ApiFocus::MockContractField {
                route_idx: focused_route,
                group: focused_group,
                field_idx: focused_field,
                ..
            }) if focused_route == route_idx && focused_group == group && focused_field >= field_idx
        );
        let changed = self.mutate_api_mock_contract(route_idx, |contract| {
            let fields = &mut Self::api_mock_contract_class_mut(contract, group).fields;
            if field_idx < fields.len() {
                fields.remove(field_idx);
            }
        });
        if changed && clear_focus {
            self.ide_panel.api.focused = None;
            self.ide_panel.api.input_editor = crate::editor::Editor::new(512);
        }
        changed
    }

    pub fn trigger_api_mock_export_openapi(&mut self) {
        self.commit_api_focus();
        let specs = self
            .ide_panel
            .api
            .specs
            .iter()
            .filter_map(|entry| {
                let model = self.ide_panel.api.models.get(&entry.id)?;
                Some((entry.clone(), model.clone()))
            })
            .collect::<Vec<_>>();
        let mock = self.ide_panel.api.mock.clone();
        std::thread::spawn(move || {
            let value = crate::app::api_mock::openapi_export::export_mock_server_openapi_value(
                &specs, &mock,
            );
            let Ok(text) = serde_json::to_string_pretty(&value) else {
                return;
            };
            let Some(path) = rfd::FileDialog::new()
                .set_title("Экспорт openapi.json")
                .set_file_name("openapi.json")
                .add_filter("OpenAPI JSON", &["json"])
                .save_file()
            else {
                return;
            };
            let _ = std::fs::write(path, text);
        });
    }
}

fn parse_optional_usize(value: &str) -> Option<usize> {
    if value.is_empty() {
        None
    } else {
        value.parse().ok()
    }
}

#[cfg(test)]
mod mock_contract_method_tests {
    use crate::app::api_client::{ApiSpecId, parse_openapi_model};
    use crate::app::api_mock::contract::api_mock_handler_signature_text;
    use crate::app::api_mock::types::default_contract_from_route;

    #[test]
    fn default_contract_from_openapi_route_seeds_path_query_and_constraints() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "paths": {
                "/users/{id}": {
                    "get": {
                        "parameters": [
                            {
                                "name": "id",
                                "in": "path",
                                "required": true,
                                "schema": {"type": "string", "minLength": 2, "maxLength": 24}
                            },
                            {
                                "name": "page",
                                "in": "query",
                                "schema": {"type": "integer", "default": 1, "minimum": 1}
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(1), &spec).expect("parse");
        let contract = default_contract_from_route(&model.routes[0], &model);

        assert!(contract.path_params.enabled);
        assert!(contract.query.enabled);
        assert_eq!(contract.path_params.fields[0].constraints.max_length, Some(24));
        assert_eq!(
            contract.query.fields[0].default_value.as_deref(),
            Some("1")
        );
        assert_eq!(
            contract.query.fields[0].constraints.minimum.as_deref(),
            Some("1")
        );

        let signature = api_mock_handler_signature_text(&contract);
        assert!(signature.contains("id: Annotated[str, MinLen(2), MaxLen(24)]"));
        assert!(signature.contains("query: Query"));
    }
}
