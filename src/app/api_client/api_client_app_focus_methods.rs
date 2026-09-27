impl crate::app::App {
    fn focus_next_api_input(&mut self, reverse: bool) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return false;
        };
        let Some(next) = self
            .ide_panel
            .api
            .next_api_focus(spec_id, model, state, reverse)
        else {
            return false;
        };
        self.focus_api_input(next);
        self.sync_api_one_line_scroll_target(true);
        true
    }

    fn api_focus_text(&self, focus: &ApiFocus) -> String {
        let active = self.api_active_route();
        let tab_state = self.active_api_tab().map(|(_, state)| state);
        self.ide_panel
            .api
            .api_focus_text(focus, active.as_ref(), tab_state)
    }

    fn apply_response_token_to_auth(
        &mut self,
        route_idx: usize,
        scheme_idx: usize,
        save_access: bool,
        save_refresh: bool,
    ) {
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.route_idx != Some(route_idx) {
            return;
        }
        let spec_id = meta.spec_id;
        let Some(response) = state.response.as_ref() else {
            return;
        };
        // Owned copy: the tab borrow must end before `ide_panel.api` is borrowed mutably.
        let response_body = response.body.clone();
        self.ide_panel.api.apply_response_token_to_auth(
            spec_id,
            &response_body,
            scheme_idx,
            save_access,
            save_refresh,
        );
    }

    pub fn commit_api_focus(&mut self) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        let text = self.ide_panel.api.normalized_api_focus_text(&focus);
        let active = self.api_active_route();
        let mut result = self
            .ide_panel
            .api
            .commit_api_focus_state(&focus, text, active.as_ref());
        let text = result.text_for_app.take().unwrap_or_default();

        if let ApiFocus::MockContractField {
            route_idx,
            group,
            field_idx,
            prop,
        } = focus
        {
            if self.ide_panel.api.commit_api_mock_contract_field_prop(
                active.as_ref(),
                route_idx,
                group,
                field_idx,
                prop,
                &text,
            ) {
                self.start_api_mock_contract_tools(route_idx);
            }
        } else {
            if let ApiFocus::MockManualPath { manual_idx } = focus {
                if result.mock_config_changed {
                    self.sync_api_manual_route_tabs();
                }
                if result.invalidate_contract_tools {
                    self.ide_panel
                        .api
                        .invalidate_api_mock_contract_tools(manual_idx);
                }
            } else if let ApiFocus::MockContract { route_idx } = focus
                && result.invalidate_contract_tools
            {
                self.ide_panel
                    .api
                    .invalidate_api_mock_contract_tools(route_idx);
            }

            if let Some((spec_id, route_idx)) = api_focus_tab_route(&focus)
                && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                && state.route_idx == Some(route_idx)
            {
                state.commit_api_tab_focus(focus.clone(), text);
            }
        }

        if result.mock_config_changed {
            self.ide_panel.api.commit_mock_config();
        }
    }

}

fn api_focus_tab_route(focus: &ApiFocus) -> Option<(ApiSpecId, usize)> {
    match focus {
        ApiFocus::PathParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::QueryParam {
            spec_id, route_idx, ..
        }
        | ApiFocus::BodyField {
            spec_id, route_idx, ..
        }
        | ApiFocus::Body { spec_id, route_idx } => Some((*spec_id, *route_idx)),
        _ => None,
    }
}
