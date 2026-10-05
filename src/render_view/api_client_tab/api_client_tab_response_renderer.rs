#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    /// Output docs of a route tab: status chips, Example/Schema tabs and the output pane.
    fn draw_api_client_tab_output(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, route, model, tab_meta,
            tab_state, ide_panel,
            ..
        } = ctx;
        if !route.responses.is_empty() {
            self.draw_api_section_title("Output", x + pad, cy + 18.0 * s, s);
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            let mut status_x = x + pad;
            let selected_status_idx = tab_state
                .output_status_idx
                .min(route.responses.len().saturating_sub(1));
            for (response_idx, response) in route.responses.iter().enumerate() {
                let label = response.status.as_str();
                let chip_w = (self.measure_ui_width(label, 0.86) + 22.0 * s).max(54.0 * s);
                if status_x + chip_w > x + pad + content_w {
                    status_x = x + pad;
                    cy += API_ROUTE_OUTPUT_STATUS_ROW_ADVANCE * s;
                }
                self.draw_api_response_tab(
                    label,
                    response_idx == selected_status_idx,
                    status_x,
                    cy,
                    chip_w,
                    28.0 * s,
                    s,
                    crate::ui_system::UiId::ApiOutputStatusTab(route_idx, response_idx),
                    ui_registry,
                    mx,
                    my,
                );
                status_x += chip_w + 8.0 * s;
            }
            cy += API_ROUTE_OUTPUT_TABS_ROW_ADVANCE * s;
            let output_tab_y = cy;
            let output_tab_h = 28.0 * s;
            let example_w = self.measure_ui_width("Example", 0.86) + 22.0 * s;
            let schema_w = self.measure_ui_width("Schema", 0.86) + 22.0 * s;
            let example_count = api_route_output_example_count(route, selected_status_idx).max(1);
            let schema_media_count =
                api_route_output_media_count(route, selected_status_idx).max(1);
            let selected_example_idx = tab_state
                .output_example_idx
                .min(example_count.saturating_sub(1));
            let selected_schema_media_idx = tab_state
                .output_schema_idx
                .min(schema_media_count.saturating_sub(1));
            let show_example_label = tab_state.output_doc_view == ApiOutputDocView::Example;
            let show_example_menu = show_example_label && example_count > 1;
            let menu_label = api_route_output_example_menu_label(
                route,
                selected_status_idx,
                selected_example_idx,
            );
            let mut menu_label_w = self.measure_ui_width(&menu_label, 0.82);
            if show_example_menu {
                for option_idx in 0..example_count {
                    let label =
                        api_route_output_example_menu_label(route, selected_status_idx, option_idx);
                    menu_label_w = menu_label_w.max(self.measure_ui_width(&label, 0.82));
                }
            }
            let menu_extra_w = if show_example_menu {
                48.0 * s
            } else {
                20.0 * s
            };
            let menu_w = api_popup_width(menu_label_w + menu_extra_w, 132.0 * s, content_w);
            let tab_x = x + pad;
            self.draw_api_response_tab(
                "Example",
                tab_state.output_doc_view == ApiOutputDocView::Example,
                tab_x,
                output_tab_y,
                example_w,
                output_tab_h,
                s,
                crate::ui_system::UiId::ApiOutputExampleTab(route_idx),
                ui_registry,
                mx,
                my,
            );
            self.draw_api_response_tab(
                "Schema",
                tab_state.output_doc_view == ApiOutputDocView::Schema,
                tab_x + example_w + 8.0 * s,
                output_tab_y,
                schema_w,
                output_tab_h,
                s,
                crate::ui_system::UiId::ApiOutputSchemaTab(route_idx),
                ui_registry,
                mx,
                my,
            );
            cy += API_ROUTE_OUTPUT_VIEW_TABS_ADVANCE * s;
            let output_menu_y = cy;
            if show_example_label {
                self.push_rounded_rect(
                    x + pad,
                    output_menu_y,
                    menu_w,
                    output_tab_h,
                    5.0 * s,
                    self.ui.pick(UiRole::BgPanelAlt, [0.18, 0.19, 0.23, 1.0]),
                );
                if show_example_menu {
                    ui_registry.register_blocker(
                        crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx),
                        x + pad,
                        output_menu_y,
                        menu_w,
                        output_tab_h,
                        mx,
                        my,
                    );
                }
                self.draw_string_scaled_stable(
                    &menu_label,
                    x + pad + 10.0 * s,
                    api_centered_text_y(output_menu_y, output_tab_h, s),
                    self.ui.pick(UiRole::TextSecondary, [0.78, 0.80, 0.88, 1.0]),
                    0.82,
                );
                if show_example_menu {
                    self.draw_string_scaled_stable(
                        if tab_state.output_schema_menu_open {
                            "▼"
                        } else {
                            "▶"
                        },
                        x + pad + menu_w - 18.0 * s,
                        api_centered_text_y(output_menu_y, output_tab_h, s),
                        self.ui.pick(UiRole::TextSecondary, [0.78, 0.80, 0.88, 1.0]),
                        0.82,
                    );
                }
            }
            let output_example_text = api_route_output_example_text_for(
                route,
                model,
                selected_status_idx,
                selected_example_idx,
            );
            let output_schema_text = api_route_output_schema_text_for(
                route,
                model,
                selected_status_idx,
                selected_schema_media_idx,
                &tab_state.output_schema_collapsed,
            );
            let output_h = api_response_text_area_height(&output_example_text, s)
                .max(api_response_text_area_height(&output_schema_text, s));
            let output_text = match tab_state.output_doc_view {
                ApiOutputDocView::Example => output_example_text,
                ApiOutputDocView::Schema => output_schema_text,
            };
            let output_summary = if tab_state.output_doc_view == ApiOutputDocView::Schema {
                api_route_output_schema_summary(
                    route,
                    model,
                    selected_status_idx,
                    selected_schema_media_idx,
                )
            } else {
                String::new()
            };
            if tab_state.output_doc_view == ApiOutputDocView::Schema && !output_summary.is_empty() {
                self.draw_api_schema_summary(&output_summary, x + pad, cy + 18.0 * s);
            }
            cy += API_ROUTE_OUTPUT_BODY_GAP_ADVANCE * s;
            let output_schema_focused = tab_state.focused_schema_pane
                == Some(crate::app::api_client::ApiSchemaPaneFocus::Output);
            self.push_rounded_rect_border(
                x + pad,
                cy,
                content_w,
                output_h,
                0.0,
                (1.0 * s).max(1.0),
                if output_schema_focused {
                    self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
                } else {
                    self.ui.ink(0.12)
                },
                self.ui.pick(UiRole::BgInput, [0.12, 0.13, 0.17, 1.0]),
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::ApiOutputSchemaBody(route_idx),
                x + pad,
                cy,
                content_w,
                output_h,
                mx,
                my,
            );
            let output_clip = (
                x + pad + 10.0 * s,
                cy + 8.0 * s,
                content_w - 20.0 * s,
                output_h - 16.0 * s,
            );
            if self.begin_api_text_clip(output_clip, tab_clip) {
                let output_schema_text_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::OutputSchema { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                );
                if output_schema_text_focused {
                    let output_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                    self.draw_api_editor_selection_multiline_ui(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        output_text_top,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        tab_state.output_scroll_x.current.round(),
                    );
                }
                if tab_state.output_doc_view == ApiOutputDocView::Schema {
                    self.draw_api_schema_text_area(
                        &output_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        tab_state.output_scroll_x.current.round(),
                        false,
                        false,
                        route_idx,
                        ui_registry,
                        mx,
                        my,
                    );
                } else {
                    self.draw_json_text_area(
                        &output_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        tab_state.output_scroll_x.current.round(),
                        false,
                    );
                }
                if output_schema_text_focused && blink_alpha > 0.5 {
                    let output_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                    self.draw_api_editor_cursor_multiline_ui(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        output_text_top,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        tab_state.output_scroll_x.current.round(),
                    );
                }
                if tab_state.output_doc_view == ApiOutputDocView::Schema {
                    self.draw_api_schema_scrollbar(
                        &output_text,
                        x + pad + content_w - 15.0 * s,
                        cy + 8.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        crate::ui_system::UiId::ApiOutputScrollY(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                } else {
                    self.draw_api_text_scrollbar(
                        &output_text,
                        x + pad + content_w - 15.0 * s,
                        cy + 8.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.output_scroll.current.round(),
                        crate::ui_system::UiId::ApiOutputScrollY(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                }
                self.restore_api_tab_clip(tab_clip);
                self.draw_api_text_scrollbar_x(
                    &output_text,
                    x + pad + 8.0 * s,
                    cy + output_h - 12.0 * s,
                    content_w - 16.0 * s,
                    content_w - 20.0 * s,
                    tab_state.output_scroll_x.current,
                    crate::ui_system::UiId::ApiOutputScrollX(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
            }
            if show_example_menu && tab_state.output_schema_menu_anim > 0.01 {
                self.draw_api_client_tab_output_example_menu(
                    ctx,
                    menu_w,
                    output_menu_y,
                    output_tab_h,
                    example_count,
                    selected_status_idx,
                    selected_example_idx,
                    ui_registry,
                );
            }
            cy += output_h + API_ROUTE_OUTPUT_TAIL_ADVANCE * s;
        }
        cy
    }

    /// Animated popup listing the output examples of the selected status.
    fn draw_api_client_tab_output_example_menu(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        menu_w: f32,
        output_menu_y: f32,
        output_tab_h: f32,
        example_count: usize,
        selected_status_idx: usize,
        selected_example_idx: usize,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let ApiTabRouteCtx {
            x, pad, s, mx, my, tab_clip, route_idx, route, tab_state,
            ..
        } = ctx;
        let menu_x = x + pad;
        let row_h = 30.0 * s;
        let row_inset = 5.0 * s;
        let row_top_pad = 6.0 * s;
        let track_w = (4.0 * s).max(3.0);
        let track_gap = 8.0 * s;
        let (menu_h, max_scroll) =
            crate::app::api_client::api_output_schema_menu_scroll_metrics(example_count, s);
        let anim_h = (menu_h * tab_state.output_schema_menu_anim)
            .round()
            .max(1.0);
        let popup_y = output_menu_y + output_tab_h + 6.0 * s;
        let scrollbar_visible = max_scroll > 0.5;
        let list_scrolling = tab_state.output_schema_menu_scroll.is_dragging
            || (tab_state.output_schema_menu_scroll.current
                - tab_state.output_schema_menu_scroll.target)
                .abs()
                >= 0.5;
        for i in 1..=5 {
            let offset = i as f32 * s;
            let alpha = (0.15 - (i as f32 * 0.03))
                * tab_state.output_schema_menu_anim.clamp(0.0, 1.0);
            self.push_rounded_rect(
                menu_x - offset,
                popup_y - offset,
                menu_w + offset * 2.0,
                anim_h + offset * 2.0,
                6.0 * s,
                self.ui.shadow_alpha(alpha),
            );
        }
        self.push_rounded_rect_border(
            menu_x - 2.0 * s,
            popup_y - 2.0 * s,
            menu_w + 4.0 * s,
            anim_h + 4.0 * s,
            6.0 * s,
            (2.0 * s).max(1.0),
            self.ui.pick(UiRole::Selection, [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0]),
            self.ui.pick(UiRole::BgPanelAlt, [0.15, 0.16, 0.20, 1.0]),
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx),
            menu_x,
            popup_y,
            menu_w,
            anim_h,
            mx,
            my,
        );
        let clip_top_pad = (4.0 * s).max(3.0);
        let clip_bottom_pad = clip_top_pad;
        let menu_clip = (
            menu_x,
            popup_y + clip_top_pad,
            menu_w,
            (anim_h - clip_top_pad - clip_bottom_pad).max(1.0),
        );
        if self.begin_api_text_clip(menu_clip, tab_clip) {
            let scroll_y = tab_state.output_schema_menu_scroll.current.round();
            let first = ((scroll_y - row_top_pad).max(0.0) / row_h).floor() as usize;
            let max_visible = (menu_h / row_h).ceil() as usize + 1;
            for option_idx in first..example_count.min(first + max_visible) {
                let item_y = popup_y + row_top_pad + option_idx as f32 * row_h - scroll_y;
                if item_y >= popup_y + anim_h || item_y + 28.0 * s <= popup_y {
                    continue;
                }
                let label = api_route_output_example_menu_label(
                    route,
                    selected_status_idx,
                    option_idx,
                );
                let row_x = menu_x + row_inset;
                let gutter_w = if scrollbar_visible {
                    track_w + track_gap
                } else {
                    0.0
                };
                let row_w = (menu_w - 2.0 * row_inset - gutter_w).max(0.0);
                let hovered = !list_scrolling
                    && mx >= row_x
                    && mx <= row_x + row_w
                    && my >= item_y
                    && my <= item_y + 28.0 * s;
                self.push_rounded_rect(
                    row_x,
                    item_y,
                    row_w,
                    28.0 * s,
                    4.0 * s,
                    if option_idx == selected_example_idx {
                        self.ui.ink(0.12)
                    } else if hovered {
                        self.ui.pick(UiRole::RowHover, [0.20, 0.21, 0.28, 1.0])
                    } else {
                        self.ui.pick(UiRole::BgPanelAlt, [0.15, 0.16, 0.20, 1.0])
                    },
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::ApiOutputSchemaMenuItem(route_idx, option_idx),
                    row_x,
                    item_y,
                    row_w,
                    28.0 * s,
                    mx,
                    my,
                );
                self.draw_string_scaled_stable(
                    &label,
                    row_x + 10.0 * s,
                    api_centered_text_y(item_y, 28.0 * s, s),
                    self.ui.pick(UiRole::TextPrimary, self.theme.fg),
                    0.80,
                );
            }
            if scrollbar_visible {
                let track_x = menu_x + menu_w - track_w - 4.0 * s;
                let track_y = popup_y + clip_top_pad;
                let track_h = (anim_h - clip_top_pad - clip_bottom_pad).max(1.0);
                self.push_rect(
                    track_x,
                    track_y,
                    track_w,
                    track_h,
                    self.ui.pick(UiRole::ScrollbarThumb, [0.52, 0.54, 0.60, 0.36]),
                );
                let id = crate::ui_system::UiId::ApiOutputSchemaMenuScrollY(route_idx);
                let lane = (
                    menu_x + menu_w - track_w - track_gap,
                    track_y,
                    track_w + track_gap,
                    track_h,
                );
                let scrollbar = crate::app::api_client::api_output_schema_menu_scrollbar(
                    lane,
                    example_count,
                    scroll_y,
                    s,
                );
                let _ = self.draw_scrollbar(
                    &scrollbar,
                    s,
                    1.0,
                    Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                        ui: ui_registry,
                        id,
                        mx,
                        my,
                        blocker: false,
                    }),
                );
            }
            self.restore_api_tab_clip(tab_clip);
        }
    }

    /// Response of the last request: status, Body/Headers/Curl tabs, token actions and text pane.
    fn draw_api_client_tab_response(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, model, tab_meta,
            tab_state, ide_panel,
            ..
        } = ctx;
        self.draw_api_section_title("Ответ", x + pad, cy + 18.0 * s, s);
        cy += API_ROUTE_RESPONSE_TITLE_ADVANCE * s;
        if let Some(response) = &tab_state.response {
            if let Some(err) = &response.error {
                self.draw_string_scaled_stable(
                    &err.message,
                    x + pad,
                    cy + 18.0 * s,
                    self.ui.pick(UiRole::Error, [1.0, 0.42, 0.42, 1.0]),
                    0.88,
                );
                cy += API_ROUTE_RESPONSE_ERROR_ROW_ADVANCE * s;
            }
            if response.error.is_none() || !response.body.is_empty() {
                let status_text = response
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string());
                self.draw_string_scaled_stable(
                    &status_text,
                    x + pad,
                    cy + 18.0 * s,
                    api_status_color(response.status),
                    0.92,
                );
                self.draw_string_scaled_stable(
                    &response.timing_text,
                    x + pad + 62.0 * s,
                    cy + 18.0 * s,
                    self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
                    0.88,
                );
                cy += API_ROUTE_RESPONSE_STATUS_ROW_ADVANCE * s;
                let tab_y = cy;
                let tab_h = 28.0 * s;
                let body_w = self.measure_ui_width("Body", 0.86) + 22.0 * s;
                let headers_w = self.measure_ui_width("Headers", 0.86) + 22.0 * s;
                let curl_w = self.measure_ui_width("Curl", 0.86) + 22.0 * s;
                self.draw_api_response_tab(
                    "Body",
                    tab_state.response_view == ApiResponseView::Body,
                    x + pad,
                    tab_y,
                    body_w,
                    tab_h,
                    s,
                    crate::ui_system::UiId::ApiResponseBodyTab(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                self.draw_api_response_tab(
                    "Headers",
                    tab_state.response_view == ApiResponseView::Headers,
                    x + pad + body_w + 8.0 * s,
                    tab_y,
                    headers_w,
                    tab_h,
                    s,
                    crate::ui_system::UiId::ApiResponseHeadersTab(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                self.draw_api_response_tab(
                    "Curl",
                    tab_state.response_view == ApiResponseView::Curl,
                    x + pad + body_w + headers_w + 16.0 * s,
                    tab_y,
                    curl_w,
                    tab_h,
                    s,
                    crate::ui_system::UiId::ApiResponseCurlTab(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                cy += API_ROUTE_RESPONSE_TABS_ADVANCE * s;
                let (has_access, has_refresh) = response_auth_token_flags(response);
                if has_access || has_refresh {
                    let row_h = API_ROUTE_RESPONSE_TOKEN_ROW_ADVANCE * s;
                    let btn_h = 24.0 * s;
                    let access_w = self.measure_ui_width("Сохранить access", 0.78) + 18.0 * s;
                    let refresh_w = self.measure_ui_width("Сохранить refresh", 0.78) + 18.0 * s;
                    for (scheme_idx, scheme) in model
                        .security_schemes
                        .iter()
                        .enumerate()
                        .filter(|(_, scheme)| scheme.token_capable())
                    {
                        self.draw_string_scaled_stable(
                            &scheme.name,
                            x + pad,
                            cy + 20.0 * s,
                            self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
                            0.78,
                        );
                        let mut bx = x + pad + (content_w * 0.34).max(130.0 * s);
                        if has_access {
                            let use_btn = Button {
                                x: bx,
                                y: cy + 2.0 * s,
                                w: access_w,
                                h: btn_h,
                                text: "Сохранить access".to_string(),
                                icon: None,
                                text_scale: 0.84,
                                icon_size: 0.0,
                            };
                            ui_registry.register_button(
                                crate::ui_system::UiId::ApiResponseUseAccessToken(
                                    route_idx, scheme_idx,
                                ),
                                &use_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                            bx += access_w + 8.0 * s;
                        }
                        if has_refresh {
                            let save_btn = Button {
                                x: bx,
                                y: cy + 2.0 * s,
                                w: refresh_w,
                                h: btn_h,
                                text: "Сохранить refresh".to_string(),
                                icon: None,
                                text_scale: 0.84,
                                icon_size: 0.0,
                            };
                            ui_registry.register_button(
                                crate::ui_system::UiId::ApiResponseSaveRefreshToken(
                                    route_idx, scheme_idx,
                                ),
                                &save_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                        }
                        cy += row_h;
                    }
                }
                let response_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::Response { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                );
                let response_text = if response_focused {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    api_response_text(response, tab_state.response_view).to_string()
                };
                let resp_h = api_response_text_area_height(&response_text, s);
                self.push_rounded_rect_border(
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    0.0,
                    (1.0 * s).max(1.0),
                    if response_focused {
                        self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
                    } else {
                        self.ui.ink(0.12)
                    },
                    self.ui.pick(UiRole::BgInput, [0.12, 0.13, 0.17, 1.0]),
                );
                ui_registry.register_text_input(
                    crate::ui_system::UiId::ApiResponseBody(route_idx),
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    mx,
                    my,
                );
                let resp_clip = (
                    x + pad + 10.0 * s,
                    cy + 8.0 * s,
                    content_w - 20.0 * s,
                    resp_h - 16.0 * s,
                );
                if self.begin_api_text_clip(resp_clip, tab_clip) {
                    if response_focused {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_selection_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    }
                    if tab_state.response_view == ApiResponseView::Curl {
                        self.draw_curl_text_area(
                            &response_text,
                            x + pad + 10.0 * s,
                            cy + 29.0 * s,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    } else {
                        self.draw_json_text_area(
                            &response_text,
                            x + pad + 10.0 * s,
                            cy + 29.0 * s,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                            tab_state.response_view == ApiResponseView::Headers,
                        );
                    }
                    self.draw_api_text_scrollbar(
                        &response_text,
                        x + pad + content_w - 8.0 * s,
                        cy + 8.0 * s,
                        resp_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current,
                        crate::ui_system::UiId::ApiResponseScrollY(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                    if response_focused && blink_alpha > 0.5 {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_cursor_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                    self.draw_api_text_scrollbar_x(
                        &response_text,
                        x + pad + 8.0 * s,
                        cy + resp_h - 12.0 * s,
                        content_w - 16.0 * s,
                        content_w - 20.0 * s,
                        tab_state.response_scroll_x.current,
                        crate::ui_system::UiId::ApiResponseScrollX(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                }
                if response.truncated {
                    self.draw_string_scaled_stable(
                        "обрезано",
                        x + pad + content_w - 86.0 * s,
                        cy + 18.0 * s,
                        self.ui.pick(UiRole::HttpPut, [1.0, 0.76, 0.32, 1.0]),
                        0.78,
                    );
                }
            }
        } else if tab_state.pending {
            self.draw_string_scaled_stable(
                "Запрос выполняется",
                x + pad,
                cy + 18.0 * s,
                self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
                0.88,
            );
        }
    }
}
