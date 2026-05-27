impl crate::app::App {
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
        self.start_api_mock_route_tools_now(route_idx);
        true
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

    pub fn toggle_api_mock_contract_body(&mut self, route_idx: usize) -> bool {
        self.mutate_api_mock_contract(route_idx, |contract| {
            contract.body.enabled = !contract.body.enabled;
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
