impl crate::app::api_client::ApiClientState {
    pub(crate) fn api_mock_constraint_menu_contains_ui_id(
        &self,
        id: Option<crate::ui_system::UiId>,
    ) -> bool {
        api_mock_constraint_menu_contains_ui_id(self.mock_contract_constraint_menu, id)
    }

    pub(crate) fn close_api_mock_constraint_menu(&mut self) -> bool {
        self.mock_contract_constraint_menu.take().is_some()
    }

    pub(crate) fn start_api_python_runtime_scroll_drag(
        &mut self,
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        pointer_y: f32,
        scale: f32,
    ) -> bool {
        let (scroll, max_scroll) = match id {
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY
                if self.mock_python_version_picker_open =>
            {
                let max_scroll = crate::app::api_client::api_python_version_list_max_scroll(
                    self.mock_python_versions.len(),
                    rect.3 + 12.0 * scale,
                    scale,
                );
                (&mut self.mock_python_versions_scroll, max_scroll)
            }
            crate::ui_system::UiId::ApiMockPythonInstallLogScrollY
                if crate::app::api_client::api_python_install_log_visible(self) =>
            {
                let max_scroll = crate::app::api_client::api_python_install_log_max_scroll(
                    self.mock_python_install_log.len(),
                    rect.3 + 12.0 * scale,
                    scale,
                );
                (&mut self.mock_python_install_log_scroll, max_scroll)
            }
            _ => return false,
        };
        let Some((drag_offset, target)) = crate::app::api_client::api_python_scrollbar_drag_target(
            (rect.0, rect.1 - 6.0 * scale, rect.2, rect.3 + 12.0 * scale),
            scroll.current,
            max_scroll,
            pointer_y,
            scale,
            None,
        ) else {
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
    }

    pub(crate) fn update_api_python_runtime_scroll_drag(
        &mut self,
        id: crate::ui_system::UiId,
        rect: Option<(f32, f32, f32, f32)>,
        pointer_y: f32,
        scale: f32,
    ) -> bool {
        let Some(rect) = rect else {
            match id {
                crate::ui_system::UiId::ApiMockPythonVersionsScrollY => {
                    self.mock_python_versions_scroll.end_drag();
                }
                crate::ui_system::UiId::ApiMockPythonInstallLogScrollY => {
                    self.mock_python_install_log_scroll.end_drag();
                }
                _ => {}
            }
            return false;
        };
        let source_rect = (rect.0, rect.1 - 6.0 * scale, rect.2, rect.3 + 12.0 * scale);
        let (scroll, max_scroll) = match id {
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY => {
                let max_scroll = crate::app::api_client::api_python_version_list_max_scroll(
                    self.mock_python_versions.len(),
                    source_rect.3,
                    scale,
                );
                (&mut self.mock_python_versions_scroll, max_scroll)
            }
            crate::ui_system::UiId::ApiMockPythonInstallLogScrollY => {
                let max_scroll = crate::app::api_client::api_python_install_log_max_scroll(
                    self.mock_python_install_log.len(),
                    source_rect.3,
                    scale,
                );
                (&mut self.mock_python_install_log_scroll, max_scroll)
            }
            _ => return false,
        };
        let Some((drag_offset, target)) = crate::app::api_client::api_python_scrollbar_drag_target(
            source_rect,
            scroll.current,
            max_scroll,
            pointer_y,
            scale,
            Some(scroll.drag_offset),
        ) else {
            scroll.end_drag();
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
    }

    fn api_mock_generated_preview(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(active, route_idx)?;
        Some(api_generated_response_for_route(&route, &model).2)
    }

    fn api_focus_is_array_input(&self, focus: &ApiFocus) -> bool {
        match focus {
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.path_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => self
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.query_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => self
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx).map(|route| (model, route)))
                .and_then(|(model, route)| {
                    let root = route.request_body.as_ref()?.schema?;
                    let prop = model
                        .schema_arena
                        .get(root.0)?
                        .properties
                        .iter()
                        .find(|prop| prop.name == *name)?;
                    model.schema_arena.get(prop.schema.0)
                })
                .is_some_and(api_schema_is_array_input),
            ApiFocus::MockContractField { prop, .. } => {
                matches!(prop, crate::ui_system::ApiMockContractFieldProp::Enum)
            }
            _ => false,
        }
    }

    fn apply_response_token_to_auth(
        &mut self,
        spec_id: ApiSpecId,
        response_body: &str,
        scheme_idx: usize,
        save_access: bool,
        save_refresh: bool,
    ) {
        let Ok(json) = serde_json::from_str::<Value>(response_body) else {
            return;
        };
        let access_token = json.get("access_token").and_then(Value::as_str);
        let refresh_token = json.get("refresh_token").and_then(Value::as_str);
        if (!save_access || access_token.is_none()) && (!save_refresh || refresh_token.is_none()) {
            return;
        }
        let token_type = json
            .get("token_type")
            .and_then(Value::as_str)
            .unwrap_or("Bearer")
            .to_string();
        let expires_at = json
            .get("expires_in")
            .and_then(Value::as_u64)
            .map(|secs| now_epoch_secs().saturating_add(secs));
        let Some(scheme_name) = self
            .models
            .get(&spec_id)
            .and_then(|model| model.security_schemes.get(scheme_idx))
            .filter(|scheme| scheme.token_capable())
            .map(|scheme| scheme.name.clone())
        else {
            return;
        };
        let entry = self.auth.entry_mut(spec_id, &scheme_name);
        if save_access && let Some(token) = access_token {
            entry.access_token = token.to_string();
            entry.value = token.to_string();
        }
        if save_refresh && let Some(token) = refresh_token {
            entry.refresh_token = token.to_string();
            entry.value = token.to_string();
        }
        entry.token_type = token_type;
        entry.expires_at = expires_at;
        self.persist();
    }
}
