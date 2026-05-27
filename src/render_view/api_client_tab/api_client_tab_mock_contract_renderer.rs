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
            "query",
            x,
            cy,
            w,
            s,
            route_idx,
            &contract.query,
            ApiMockContractGroup::Query,
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
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let mut cy = y;
        let toggle_id = match group {
            ApiMockContractGroup::Query => {
                crate::ui_system::UiId::ApiMockContractQueryToggle(route_idx)
            }
            ApiMockContractGroup::Body => crate::ui_system::UiId::ApiMockContractBodyToggle(route_idx),
        };
        let btn = Button {
            x,
            y: cy,
            w: 96.0 * s,
            h: 28.0 * s,
            text: if spec.enabled {
                format!("{label} вкл")
            } else {
                format!("{label} выкл")
            },
            icon: Some(if spec.enabled {
                IconType::Check
            } else {
                IconType::Close
            }),
            text_scale: 0.76,
            icon_size: 15.0 * s,
        };
        ui_registry.register_button(toggle_id, &btn, self, mx, my, s, false);
        if spec.enabled {
            self.draw_api_mock_contract_field_chips(
                x + 108.0 * s,
                cy,
                w - 108.0 * s,
                s,
                route_idx,
                spec,
                group,
                ui_registry,
                mx,
                my,
            );
        } else {
            self.draw_string_scaled_stable(
                "Не передается в handler",
                x + 108.0 * s,
                api_mock_contract_row_text_y(cy, 28.0 * s, s),
                [0.52, 0.54, 0.62, 1.0],
                0.74,
            );
        }
        cy += self.api_mock_contract_chip_rows(
            w - 108.0 * s,
            spec.fields.len(),
            s,
            spec.enabled,
        ) * 30.0
            * s;
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
        let mut cx = x;
        let mut cy = y;
        let row_h = 28.0 * s;
        for (field_idx, field) in spec.fields.iter().enumerate() {
            let label = api_mock_contract_field_label(field);
            let chip_w = (self.measure_ui_width(&label, 0.72) + 48.0 * s)
                .max(76.0 * s)
                .min(w.max(76.0 * s));
            if cx > x && cx + chip_w > x + w {
                cx = x;
                cy += 30.0 * s;
            }
            let id = match group {
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
            self.draw_string_scaled_stable(
                mark,
                cx + 8.0 * s,
                api_mock_contract_row_text_y(cy, row_h, s),
                if field.enabled {
                    [0.50, 0.90, 0.55, 1.0]
                } else {
                    [0.58, 0.60, 0.66, 1.0]
                },
                0.64,
            );
            self.draw_string_scaled_stable(
                &label,
                cx + 30.0 * s,
                api_mock_contract_row_text_y(cy, row_h, s),
                if field.enabled {
                    self.theme.fg
                } else {
                    [0.55, 0.57, 0.64, 1.0]
                },
                0.72,
            );
            cx += chip_w + 8.0 * s;
        }
        if spec.fields.is_empty() {
            self.draw_string_scaled_stable(
                "полей нет",
                x,
                api_mock_contract_row_text_y(y, row_h, s),
                [0.52, 0.54, 0.62, 1.0],
                0.74,
            );
        }
    }

    fn api_mock_contract_chip_rows(
        &mut self,
        w: f32,
        count: usize,
        s: f32,
        enabled: bool,
    ) -> f32 {
        if !enabled || count == 0 {
            return 1.0;
        }
        let avg_chip = 112.0 * s;
        let per_row = (w / avg_chip).floor().max(1.0) as usize;
        count.div_ceil(per_row).max(1) as f32
    }
}

#[derive(Clone, Copy)]
enum ApiMockContractGroup {
    Query,
    Body,
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
