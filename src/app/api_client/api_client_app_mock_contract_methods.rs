impl crate::app::App {
    pub fn trigger_api_mock_export_openapi(&mut self) {
        self.commit_api_focus();
        if self.api_openapi_export_rx.is_some() {
            self.ide_panel.api.persistence_error =
                Some("Экспорт OpenAPI уже выполняется".to_string());
            return;
        }
        let (specs, mock) = self.ide_panel.api.api_mock_openapi_export_snapshot();
        if crate::platform::native_dialog_requires_main_thread() {
            if let Err(error) =
                export_api_mock_openapi_file(self.external_requests.sink(), &specs, &mock)
            {
                self.ide_panel.api.persistence_error = Some(error);
            }
            return;
        }
        let requests = self.external_requests.sink().clone();
        match self.ui_waker.spawn_one_shot("rriter-api-openapi-export", move || {
            export_api_mock_openapi_file(&requests, &specs, &mock)
        }) {
            Ok(job) => self.api_openapi_export_rx = Some(job),
            Err(err) => {
                self.ide_panel.api.persistence_error =
                    Some(format!("Не удалось запустить экспорт OpenAPI: {err}"));
            }
        }
    }

    /// Runs a contract edit on `ApiClientState` with the API focus committed first
    /// and the route's ty tools restarted when the contract changed.
    pub(crate) fn edit_api_mock_contract(
        &mut self,
        route_idx: usize,
        edit: impl FnOnce(&mut ApiClientState, Option<&ApiActiveRoute>) -> bool,
    ) -> bool {
        self.commit_api_focus();
        let active = self.api_active_route();
        let changed = edit(&mut self.ide_panel.api, active.as_ref());
        if changed {
            self.start_api_mock_contract_tools(route_idx);
        }
        changed
    }

    /// Starts the route's ty tools after a contract edit, or schedules them.
    fn start_api_mock_contract_tools(&mut self, route_idx: usize) {
        if !self.start_api_mock_route_tools_now(route_idx) {
            self.ide_panel.api.mock_ty_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(450));
        }
    }

    pub fn open_api_mock_route_reset_dialog(&mut self, route_idx: usize) {
        self.commit_api_focus();
        let active = self.api_active_route();
        self.ide_panel
            .api
            .open_api_mock_route_reset_dialog(active.as_ref(), route_idx);
    }

    pub fn confirm_api_mock_route_reset(&mut self) {
        if self.ide_panel.api.mock_route_reset_dialog.is_none() {
            return;
        }
        self.commit_api_focus();
        let active = self.api_active_route();
        self.ide_panel.api.confirm_api_mock_route_reset(active.as_ref());
    }

    pub fn open_api_mock_contract_field_delete_dialog(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) {
        self.commit_api_focus();
        let active = self.api_active_route();
        self.ide_panel.api.open_api_mock_contract_field_delete_dialog(
            active.as_ref(),
            route_idx,
            group,
            field_idx,
        );
    }

    pub fn confirm_api_mock_contract_field_delete(&mut self) {
        if self.ide_panel.api.mock_contract_field_delete_dialog.is_none() {
            return;
        }
        let active = self.api_active_route();
        if !self
            .ide_panel
            .api
            .api_mock_contract_field_delete_target_exists(active.as_ref())
        {
            self.ide_panel
                .api
                .confirm_api_mock_contract_field_delete(active.as_ref());
            return;
        }
        self.commit_api_focus();
        let active = self.api_active_route();
        if let Some(route_idx) = self
            .ide_panel
            .api
            .confirm_api_mock_contract_field_delete(active.as_ref())
        {
            self.start_api_mock_contract_tools(route_idx);
        }
    }

    pub(crate) fn add_api_mock_contract_field_constraint(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
    ) -> bool {
        if matches!(
            prop,
            crate::ui_system::ApiMockContractFieldProp::Required
                | crate::ui_system::ApiMockContractFieldProp::Nullable
        ) {
            self.ide_panel.api.close_api_mock_contract_constraint_menu();
            return self.edit_api_mock_contract(route_idx, |api, active| {
                api.add_api_mock_contract_field_constraint(active, route_idx, group, field_idx, prop)
            });
        }
        self.ide_panel.api.mock_contract_constraint_menu = None;
        self.focus_api_input(crate::app::api_client::ApiFocus::MockContractField {
            route_idx,
            group,
            field_idx,
            prop,
        });
        true
    }

    pub(crate) fn api_mock_contract_source_for_route(&self, route_idx: usize) -> Option<String> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_mock_contract_source_for_route(active.as_ref(), route_idx)
    }

    fn api_mock_signature_for_route(&self, route_idx: usize) -> Option<String> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_mock_signature_for_route(active.as_ref(), route_idx)
    }
}

fn export_api_mock_openapi_file(
    requests: &crate::platform::ExternalRequestSink,
    specs: &[(crate::app::api_client::ApiSpecEntry, crate::app::api_client::ApiSpecModel)],
    mock: &crate::app::api_mock::types::ApiMockState,
) -> Result<Option<std::path::PathBuf>, String> {
    let value = crate::app::api_mock::openapi_export::export_mock_server_openapi_value(specs, mock);
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("Не удалось сериализовать OpenAPI: {error}"))?;
    let Some(path) = crate::platform::save_file_with_filter(
        requests,
        "Экспорт openapi.json",
        "openapi.json",
        "OpenAPI JSON",
        &["json"],
    ) else {
        return Ok(None);
    };
    crate::platform::atomic_write(&path, text.as_bytes())
        .map_err(|error| format!("Не удалось сохранить экспорт OpenAPI: {error}"))?;
    Ok(Some(path))
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
