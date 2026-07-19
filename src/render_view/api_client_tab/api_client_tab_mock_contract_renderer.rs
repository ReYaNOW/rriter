impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_controls(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        contract: &crate::app::api_mock::types::ApiMockPythonContract,
        focused: Option<&ApiFocus>,
        constraint_menu: Option<crate::app::api_client::ApiMockContractConstraintMenu>,
        input_editor: &crate::editor::Editor,
        input_scroll_x: f32,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let mut cy = y.round();
        self.draw_string_scaled_stable(
            "Контракт Python",
            x,
            api_mock_contract_row_text_y(cy, 28.0 * s, s),
            [0.68, 0.70, 0.78, 1.0],
            0.78,
        );
        cy += 38.0 * s;
        cy = self.draw_api_mock_contract_toggle_group(
            "path",
            x,
            cy,
            w,
            s,
            route_idx,
            &contract.path_params,
            ApiMockContractGroup::Path,
            focused,
            constraint_menu,
            input_editor,
            input_scroll_x,
            blink_alpha,
            ui_registry,
            mx,
            my,
        );
        cy = self.draw_api_mock_contract_toggle_group(
            "query",
            x,
            cy,
            w,
            s,
            route_idx,
            &contract.query,
            ApiMockContractGroup::Query,
            focused,
            constraint_menu,
            input_editor,
            input_scroll_x,
            blink_alpha,
            ui_registry,
            mx,
            my,
        );
        cy = self.draw_api_mock_contract_toggle_group(
            "body",
            x,
            cy,
            w,
            s,
            route_idx,
            &contract.body,
            ApiMockContractGroup::Body,
            focused,
            constraint_menu,
            input_editor,
            input_scroll_x,
            blink_alpha,
            ui_registry,
            mx,
            my,
        );
        cy + 8.0 * s
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_toggle_group(
        &mut self,
        label: &str,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        spec: &crate::app::api_mock::types::ApiMockClassSpec,
        group: ApiMockContractGroup,
        focused: Option<&ApiFocus>,
        constraint_menu: Option<crate::app::api_client::ApiMockContractConstraintMenu>,
        input_editor: &crate::editor::Editor,
        input_scroll_x: f32,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let mut cy = y.round();
        let toggle_id = match group {
            ApiMockContractGroup::Path => {
                crate::ui_system::UiId::ApiMockContractPathToggle(route_idx)
            }
            ApiMockContractGroup::Query => {
                crate::ui_system::UiId::ApiMockContractQueryToggle(route_idx)
            }
            ApiMockContractGroup::Body => crate::ui_system::UiId::ApiMockContractBodyToggle(route_idx),
        };
        let toggle_w = 116.0 * s;
        let toggle_h = 30.0 * s;
        let chips_x = (x + 128.0 * s).round();
        let chips_w = w - 128.0 * s;
        let toggle_text = if spec.enabled {
            format!("{label} вкл")
        } else {
            format!("{label} выкл")
        };
        self.draw_api_mock_contract_button(
            ui_registry,
            toggle_id,
            x,
            cy,
            toggle_w,
            toggle_h,
            &toggle_text,
            Some(if spec.enabled {
                IconType::Check
            } else {
                IconType::Close
            }),
            0.84,
            17.0 * s,
            mx,
            my,
            s,
        );
        if spec.enabled {
            self.draw_api_mock_contract_field_chips(
                chips_x,
                cy,
                chips_w,
                s,
                route_idx,
                spec,
                group,
                ui_registry,
                mx,
                my,
            );
        } else {
            let text = "Не передается в handler";
            self.draw_string_scaled(
                text,
                chips_x,
                api_mock_contract_status_text_y(cy, toggle_h, s),
                [0.52, 0.54, 0.62, 1.0],
                1.0,
            );
        }
        let chip_rows = self.api_mock_contract_chip_rows(
            chips_w,
            spec,
            s,
            spec.enabled,
        );
        cy += chip_rows * 30.0 * s;
        if spec.enabled {
            cy = self.draw_api_mock_contract_field_editors(
                chips_x,
                cy + 2.0 * s,
                chips_w,
                s,
                route_idx,
                spec,
                group,
                focused,
                constraint_menu,
                input_editor,
                input_scroll_x,
                blink_alpha,
                ui_registry,
                mx,
                my,
            );
        }
        cy + 6.0 * s
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_field_chips(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        spec: &crate::app::api_mock::types::ApiMockClassSpec,
        group: ApiMockContractGroup,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let origin_x = x.round();
        let max_x = origin_x + w;
        let mut cx = origin_x;
        let mut cy = y.round();
        let row_h = 28.0 * s;
        let row_step = 30.0 * s;
        let text_scale = 0.74;
        for (field_idx, field) in spec.fields.iter().enumerate() {
            let label = api_mock_contract_field_label(field);
            let chip_w = (self.measure_mono_width(&label, text_scale) + 48.0 * s)
                .max(76.0 * s)
                .min(w.max(76.0 * s));
            if cx > origin_x && cx + chip_w > max_x {
                cx = origin_x;
                cy = (cy + row_step).round();
            }
            let id = match group {
                ApiMockContractGroup::Path => {
                    crate::ui_system::UiId::ApiMockContractPathFieldToggle(route_idx, field_idx)
                }
                ApiMockContractGroup::Query => {
                    crate::ui_system::UiId::ApiMockContractQueryFieldToggle(route_idx, field_idx)
                }
                ApiMockContractGroup::Body => {
                    crate::ui_system::UiId::ApiMockContractBodyFieldToggle(route_idx, field_idx)
                }
            };
            let hovered = ui_registry.register_rect(id, cx, cy, chip_w, row_h, mx, my);
            self.push_rounded_rect(
                cx,
                cy,
                chip_w,
                row_h,
                5.0 * s,
                if field.enabled {
                    if hovered {
                        [0.33, 0.36, 0.42, 1.0]
                    } else {
                        [0.24, 0.26, 0.31, 1.0]
                    }
                } else if hovered {
                    [0.24, 0.24, 0.28, 1.0]
                } else {
                    [0.17, 0.18, 0.22, 1.0]
                },
            );
            let mark = if field.enabled { "on" } else { "off" };
            let mark_gap = 6.0 * s;
            let mark_w = self.measure_mono_width(mark, text_scale);
            let label_w = self.measure_mono_width(&label, text_scale);
            let content_w = mark_w + mark_gap + label_w;
            let content_x = cx + ((chip_w - content_w).max(0.0) * 0.5).round();
            let mark_x = content_x.max(cx + 8.0 * s);
            let label_x = mark_x + mark_w + mark_gap;
            let text_y = api_mock_contract_row_text_y(cy, row_h, s);
            self.draw_string_mono_scaled(
                mark,
                mark_x,
                text_y,
                if field.enabled {
                    [0.50, 0.90, 0.55, 1.0]
                } else {
                    [0.58, 0.60, 0.66, 1.0]
                },
                text_scale,
            );
            self.draw_string_mono_scaled(
                &label,
                label_x,
                text_y,
                if field.enabled {
                    self.theme.fg
                } else {
                    [0.55, 0.57, 0.64, 1.0]
                },
                text_scale,
            );
            cx += chip_w + 8.0 * s;
        }
        if spec.fields.is_empty() {
            let text = "полей нет";
            self.draw_string_scaled(
                text,
                x.round(),
                api_mock_contract_status_text_y(y, row_h, s),
                [0.52, 0.54, 0.62, 1.0],
                1.0,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_button(
        &mut self,
        ui_registry: &mut crate::ui_system::UiRegistry,
        id: crate::ui_system::UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        icon: Option<IconType>,
        text_scale: f32,
        icon_size: f32,
        mx: f32,
        my: f32,
        s: f32,
    ) -> bool {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let hovered = ui_registry.register_rect(id, x, y, w, h, mx, my);
        let bg = if hovered {
            [0.28, 0.30, 0.33, 1.0]
        } else {
            [0.22, 0.24, 0.26, 1.0]
        };
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            4.0 * s,
            (1.0 * s).round().max(1.0),
            self.theme.sel,
            bg,
        );

        let text_w = self.measure_ui_width(label, text_scale);
        let gap_w = if icon.is_some() && !label.is_empty() {
            8.0 * s
        } else {
            0.0
        };
        let icon_w = if icon.is_some() { icon_size } else { 0.0 };
        let content_w = text_w + icon_w + gap_w;
        let mut content_x = x + ((w - content_w).max(0.0) * 0.5).round();
        if let Some(icon_type) = icon {
            self.draw_atlas_icon(
                icon_type,
                content_x,
                y + ((h - icon_size).max(0.0) * 0.5).round(),
                icon_size,
                [1.0, 1.0, 1.0, 1.0],
            );
            content_x += icon_size + gap_w;
        }
        if !label.is_empty() {
            self.draw_string_scaled_stable(
                label,
                content_x,
                api_mock_contract_button_text_y(y, h, text_scale, s),
                self.theme.fg,
                text_scale,
            );
        }
        hovered
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_field_editors(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        spec: &crate::app::api_mock::types::ApiMockClassSpec,
        group: ApiMockContractGroup,
        focused: Option<&ApiFocus>,
        constraint_menu: Option<crate::app::api_client::ApiMockContractConstraintMenu>,
        input_editor: &crate::editor::Editor,
        input_scroll_x: f32,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let ui_group = group.ui_group();
        let mut cy = y.round();
        for (field_idx, field) in spec.fields.iter().enumerate().filter(|(_, field)| field.enabled) {
            self.push_rect(
                x,
                cy,
                w,
                (1.0 * s).max(1.0),
                [1.0, 1.0, 1.0, 0.08],
            );
            let title = api_mock_contract_field_label(field);
            let title_scale = 0.82;
            let title_y = api_mock_contract_row_text_y(cy + 2.0 * s, 24.0 * s, s);
            self.draw_string_mono_scaled(
                &title,
                x,
                title_y,
                self.theme.fg,
                title_scale,
            );
            let type_text = api_mock_contract_field_type_text(field);
            let type_x = x + self.measure_mono_width(&title, title_scale);
            self.draw_string_mono_scaled(
                &type_text,
                type_x,
                title_y,
                [0.58, 0.61, 0.70, 1.0],
                title_scale,
            );
            let add_w = 146.0 * s;
            self.draw_api_mock_contract_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockContractFieldAddConstraint(
                    route_idx, ui_group, field_idx,
                ),
                x + w - add_w,
                cy + 4.0 * s,
                add_w,
                26.0 * s,
                "constraint",
                Some(IconType::Plus),
                0.78,
                15.0 * s,
                mx,
                my,
                s,
            );
            self.draw_api_mock_contract_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockContractFieldRemove(route_idx, ui_group, field_idx),
                x + w - add_w,
                cy + 34.0 * s,
                add_w,
                26.0 * s,
                "Удалить",
                Some(IconType::Discard),
                0.84,
                15.0 * s,
                mx,
                my,
                s,
            );
            let menu_open = constraint_menu.is_some_and(|menu| {
                menu.route_idx == route_idx && menu.group == ui_group && menu.field_idx == field_idx
            });
            let mut row_y = cy + 62.0 * s;
            if field.required {
                self.draw_api_mock_contract_button(
                    ui_registry,
                    crate::ui_system::UiId::ApiMockContractFieldRequired(route_idx, ui_group, field_idx),
                    x,
                    row_y,
                    112.0 * s,
                    26.0 * s,
                    "required",
                    Some(IconType::Check),
                    0.78,
                    15.0 * s,
                    mx,
                    my,
                    s,
                );
            }
            if field.nullable {
                self.draw_api_mock_contract_button(
                    ui_registry,
                    crate::ui_system::UiId::ApiMockContractFieldNullable(route_idx, ui_group, field_idx),
                    x + 120.0 * s,
                    row_y,
                    112.0 * s,
                    26.0 * s,
                    "nullable",
                    Some(IconType::Check),
                    0.78,
                    15.0 * s,
                    mx,
                    my,
                    s,
                );
            }
            if field.required || field.nullable {
                row_y += 30.0 * s;
            }
            for (label, prop) in api_mock_contract_text_props() {
                if !api_mock_contract_prop_active(field, focused, route_idx, ui_group, field_idx, prop) {
                    continue;
                }
                self.draw_api_mock_contract_prop_input(
                    label,
                    x,
                    row_y,
                    w,
                    s,
                    route_idx,
                    ui_group,
                    field_idx,
                    prop,
                    field,
                    focused,
                    input_editor,
                    input_scroll_x,
                    blink_alpha,
                    ui_registry,
                    mx,
                    my,
                );
                row_y += if matches!(prop, crate::ui_system::ApiMockContractFieldProp::Enum) {
                    42.0 * s
                } else {
                    30.0 * s
                };
            }
            if menu_open {
                row_y = self.draw_api_mock_contract_constraint_menu(
                    x + w - add_w,
                    row_y,
                    add_w,
                    s,
                    route_idx,
                    ui_group,
                    field_idx,
                    ui_registry,
                    mx,
                    my,
                );
            }
            cy = row_y + 8.0 * s;
        }
        cy
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_constraint_menu(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let item_h = 26.0 * s;
        let items = api_mock_contract_constraint_options();
        let h = items.len() as f32 * item_h + 6.0 * s;
        self.push_rounded_rect(x, y, w, h, 5.0 * s, [0.10, 0.11, 0.14, 0.98]);
        for (idx, (label, prop)) in items.into_iter().enumerate() {
            let row_y = y + 3.0 * s + idx as f32 * item_h;
            let hovered = ui_registry.register_rect(
                crate::ui_system::UiId::ApiMockContractFieldAddConstraintOption(
                    route_idx, group, field_idx, prop,
                ),
                x,
                row_y,
                w,
                item_h,
                mx,
                my,
            );
            if hovered {
                self.push_rect(x + 2.0 * s, row_y, w - 4.0 * s, item_h, [1.0, 1.0, 1.0, 0.10]);
            }
            self.draw_string_scaled_stable(
                label,
                x + 8.0 * s,
                api_mock_contract_row_text_y(row_y, item_h, s),
                self.theme.fg,
                0.76,
            );
        }
        y + h
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_contract_prop_input(
        &mut self,
        label: &str,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
        prop: crate::ui_system::ApiMockContractFieldProp,
        field: &crate::app::api_mock::types::ApiMockContractField,
        focused: Option<&ApiFocus>,
        input_editor: &crate::editor::Editor,
        input_scroll_x: f32,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let id = crate::ui_system::UiId::ApiMockContractFieldPropInput(
            route_idx, group, field_idx, prop,
        );
        let is_focused = focused.is_some_and(|focus| {
            matches!(
                focus,
                ApiFocus::MockContractField {
                    route_idx: f_route,
                    group: f_group,
                    field_idx: f_field,
                    prop: f_prop,
                } if *f_route == route_idx && *f_group == group && *f_field == field_idx && *f_prop == prop
            )
        });
        let input_x = x + 74.0 * s;
        let input_w = (w - 74.0 * s).max(42.0 * s);
        let input_h = if matches!(prop, crate::ui_system::ApiMockContractFieldProp::Enum) {
            36.0 * s
        } else {
            24.0 * s
        };
        self.draw_string_scaled_stable(
            label,
            x,
            api_mock_contract_prop_label_text_y(y, input_h, s),
            [0.58, 0.61, 0.70, 1.0],
            API_MOCK_CONTRACT_PROP_LABEL_SCALE,
        );
        let value = if is_focused {
            input_editor.get_full_text()
        } else {
            crate::app::api_client::api_mock_contract_field_prop_value(field, prop)
        };
        if matches!(prop, crate::ui_system::ApiMockContractFieldProp::Enum) {
            self.push_rounded_rect_border(
                input_x,
                y,
                input_w,
                input_h,
                5.0 * s,
                (1.0 * s).max(1.0),
                if is_focused {
                    [0.60, 0.35, 0.85, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                [0.13, 0.14, 0.18, 1.0],
            );
            ui_registry.register_text_input(id, input_x, y, input_w, input_h, mx, my);
            let field_w = input_w - 16.0 * s;
            self.draw_api_array_value_chips(
                &value,
                input_x + 8.0 * s,
                y,
                field_w,
                input_h,
                s,
                is_focused,
            );
            if is_focused && blink_alpha > 0.5 {
                let (cursor_w, cursor_row) = self.api_array_visual_cursor(&value, field_w, s);
                self.push_rect(
                    input_x + 8.0 * s + cursor_w.clamp(0.0, field_w),
                    y + cursor_row as f32 * 32.0 * s + (32.0 * s - 22.0 * s) * 0.5,
                    1.5 * s,
                    22.0 * s,
                    self.theme.fg,
                );
            }
            return;
        }
        self.draw_api_auth_input(
            input_x,
            y,
            input_w,
            24.0 * s,
            s,
            &value,
            is_focused,
            input_scroll_x,
            false,
            input_editor,
            blink_alpha,
            id,
            ui_registry,
            mx,
            my,
        );
    }

    fn api_mock_contract_chip_rows(
        &mut self,
        w: f32,
        spec: &crate::app::api_mock::types::ApiMockClassSpec,
        s: f32,
        enabled: bool,
    ) -> f32 {
        if !enabled || spec.fields.is_empty() {
            return 1.0;
        }
        let mut rows = 1usize;
        let mut cx = 0.0;
        for field in &spec.fields {
            let label = api_mock_contract_field_label(field);
            let chip_w = (self.measure_mono_width(&label, 0.74) + 48.0 * s)
                .max(76.0 * s)
                .min(w.max(76.0 * s));
            if cx > 0.0 && cx + chip_w > w {
                rows += 1;
                cx = 0.0;
            }
            cx += chip_w + 8.0 * s;
        }
        rows as f32
    }
}

#[derive(Clone, Copy)]
enum ApiMockContractGroup {
    Path,
    Query,
    Body,
}

impl ApiMockContractGroup {
    fn ui_group(self) -> crate::ui_system::ApiMockContractFieldGroup {
        match self {
            Self::Path => crate::ui_system::ApiMockContractFieldGroup::Path,
            Self::Query => crate::ui_system::ApiMockContractFieldGroup::Query,
            Self::Body => crate::ui_system::ApiMockContractFieldGroup::Body,
        }
    }
}

fn api_mock_contract_field_label(field: &crate::app::api_mock::types::ApiMockContractField) -> String {
    let mut label = field.name.clone();
    if field.required {
        label.push('*');
    }
    if let Some(default) = &field.default_value {
        label.push('=');
        label.push_str(default);
    }
    label
}

fn api_mock_contract_field_type_text(
    field: &crate::app::api_mock::types::ApiMockContractField,
) -> String {
    let mut text = String::from(": ");
    text.push_str(&api_mock_contract_field_type_label(field));
    text
}

fn api_mock_contract_field_type_label(
    field: &crate::app::api_mock::types::ApiMockContractField,
) -> String {
    use crate::app::api_mock::types::ApiMockContractFieldKind;

    match field.kind {
        ApiMockContractFieldKind::String => "str".to_string(),
        ApiMockContractFieldKind::Integer => "int".to_string(),
        ApiMockContractFieldKind::Number => "float".to_string(),
        ApiMockContractFieldKind::Boolean => "bool".to_string(),
        ApiMockContractFieldKind::Array => {
            let item = field
                .item_kind
                .map(crate::app::api_mock::types::api_mock_contract_kind_label)
                .unwrap_or("Any");
            format!("list[{item}]")
        }
        ApiMockContractFieldKind::Object => "dict".to_string(),
        ApiMockContractFieldKind::Bytes => "bytes".to_string(),
        ApiMockContractFieldKind::File => "file".to_string(),
        ApiMockContractFieldKind::Any => "Any".to_string(),
    }
}

fn api_mock_locked_text_line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.split('\n').count()
    }
}

fn api_mock_locked_text_block_height(text: &str, s: f32) -> f32 {
    let line_count = api_mock_locked_text_line_count(text);
    if line_count == 0 {
        0.0
    } else {
        line_count as f32 * api_text_area_line_height(s) + 12.0 * s
    }
}

fn api_mock_contract_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + (4.5 * scale).round()
}

fn api_mock_contract_status_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + 5.0 * scale
}

fn api_mock_contract_button_text_y(row_y: f32, row_h: f32, text_scale: f32, scale: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + (4.5 * text_scale * scale).round()
}

const API_MOCK_CONTRACT_PROP_LABEL_SCALE: f32 = 0.82;

fn api_mock_contract_prop_label_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    api_centered_text_y(row_y.round(), row_h.round(), scale)
}

fn api_mock_contract_text_props(
) -> [(&'static str, crate::ui_system::ApiMockContractFieldProp); 9] {
    [
        ("default", crate::ui_system::ApiMockContractFieldProp::Default),
        ("enum", crate::ui_system::ApiMockContractFieldProp::Enum),
        ("min len", crate::ui_system::ApiMockContractFieldProp::MinLength),
        ("max len", crate::ui_system::ApiMockContractFieldProp::MaxLength),
        ("pattern", crate::ui_system::ApiMockContractFieldProp::Pattern),
        ("min", crate::ui_system::ApiMockContractFieldProp::Minimum),
        ("max", crate::ui_system::ApiMockContractFieldProp::Maximum),
        ("min items", crate::ui_system::ApiMockContractFieldProp::MinItems),
        ("max items", crate::ui_system::ApiMockContractFieldProp::MaxItems),
    ]
}

fn api_mock_contract_constraint_options(
) -> [(&'static str, crate::ui_system::ApiMockContractFieldProp); 11] {
    [
        ("required", crate::ui_system::ApiMockContractFieldProp::Required),
        ("nullable", crate::ui_system::ApiMockContractFieldProp::Nullable),
        ("default", crate::ui_system::ApiMockContractFieldProp::Default),
        ("enum", crate::ui_system::ApiMockContractFieldProp::Enum),
        ("min length", crate::ui_system::ApiMockContractFieldProp::MinLength),
        ("max length", crate::ui_system::ApiMockContractFieldProp::MaxLength),
        ("pattern", crate::ui_system::ApiMockContractFieldProp::Pattern),
        ("minimum", crate::ui_system::ApiMockContractFieldProp::Minimum),
        ("maximum", crate::ui_system::ApiMockContractFieldProp::Maximum),
        ("min items", crate::ui_system::ApiMockContractFieldProp::MinItems),
        ("max items", crate::ui_system::ApiMockContractFieldProp::MaxItems),
    ]
}

fn api_mock_contract_prop_active(
    field: &crate::app::api_mock::types::ApiMockContractField,
    focused: Option<&ApiFocus>,
    route_idx: usize,
    group: crate::ui_system::ApiMockContractFieldGroup,
    field_idx: usize,
    prop: crate::ui_system::ApiMockContractFieldProp,
) -> bool {
    focused.is_some_and(|focus| {
        matches!(
            focus,
            ApiFocus::MockContractField {
                route_idx: f_route,
                group: f_group,
                field_idx: f_field,
                prop: f_prop,
            } if *f_route == route_idx && *f_group == group && *f_field == field_idx && *f_prop == prop
        )
    }) || match prop {
        crate::ui_system::ApiMockContractFieldProp::Required
        | crate::ui_system::ApiMockContractFieldProp::Nullable => false,
        crate::ui_system::ApiMockContractFieldProp::Default => field.default_value.is_some(),
        crate::ui_system::ApiMockContractFieldProp::Enum => !field.enum_values.is_empty(),
        crate::ui_system::ApiMockContractFieldProp::MinLength => field.constraints.min_length.is_some(),
        crate::ui_system::ApiMockContractFieldProp::MaxLength => field.constraints.max_length.is_some(),
        crate::ui_system::ApiMockContractFieldProp::Pattern => field.constraints.pattern.is_some(),
        crate::ui_system::ApiMockContractFieldProp::Minimum => field.constraints.minimum.is_some(),
        crate::ui_system::ApiMockContractFieldProp::Maximum => field.constraints.maximum.is_some(),
        crate::ui_system::ApiMockContractFieldProp::MinItems => field.constraints.min_items.is_some(),
        crate::ui_system::ApiMockContractFieldProp::MaxItems => field.constraints.max_items.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_contract_prop_label_uses_centered_field_baseline_and_larger_scale() {
        assert_eq!(API_MOCK_CONTRACT_PROP_LABEL_SCALE, 0.82);
        assert_eq!(
            api_mock_contract_prop_label_text_y(10.2, 24.4, 1.0),
            api_centered_text_y(10.0, 24.0, 1.0)
        );
    }
}
