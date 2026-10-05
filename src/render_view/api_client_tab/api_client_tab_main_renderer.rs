use crate::theme::UiRole;
#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_editor_line_number(
        &mut self,
        line_no: usize,
        right_x: f32,
        right_pad: f32,
        baseline_y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let mut buf = [0u8; 20];
        if let Some(num_str) = editor_line_number_text(line_no, &mut buf) {
            let num_w = self.measure_mono_width(num_str, scale);
            let draw_x = right_x - right_pad - num_w;
            self.draw_string_mono_scaled(num_str, draw_x, baseline_y, color, scale);
        }
    }

    pub(crate) fn draw_editor_line_number_centered(
        &mut self,
        line_no: usize,
        x: f32,
        w: f32,
        baseline_y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let mut buf = [0u8; 20];
        if let Some(num_str) = editor_line_number_text(line_no, &mut buf) {
            let num_w = self.measure_mono_width(num_str, scale);
            let draw_x = x + ((w - num_w) * 0.5).round();
            self.draw_string_mono_scaled(num_str, draw_x, baseline_y, color, scale);
        }
    }

    fn api_mono_width(&mut self, text: &str) -> f32 {
        text.chars().map(|ch| self.char_advance(ch)).sum()
    }

    fn api_route_glyph(
        &mut self,
        ch: char,
        force_emoji: bool,
    ) -> Option<crate::renderer::GlyphInfo> {
        if force_emoji {
            self.get_glyph_for_color_preference(ch, Some(true))
                .or_else(|| self.get_ui_glyph(ch))
        } else {
            self.get_ui_glyph(ch)
        }
    }

    fn api_route_text_width(&mut self, text: &str, scale: f32) -> f32 {
        let mut width = 0.0;
        let mut chars = text.char_indices().peekable();
        while let Some((_, ch)) = chars.next() {
            if matches!(ch, '\n' | '\r' | '\u{FE0F}' | '\u{200D}') {
                continue;
            }
            let force_emoji = api_route_force_emoji_presentation(chars.peek().map(|(_, next)| *next));
            if let Some(glyph) = self.api_route_glyph(ch, force_emoji) {
                width += Self::snapped_text_advance(glyph.advance, scale);
            }
        }
        width
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_route_text_run(
        &mut self,
        text: &str,
        x: f32,
        baseline_y: f32,
        line_top: f32,
        line_h: f32,
        color: [f32; 4],
        scale: f32,
        bold: bool,
        source_base: usize,
        selection: Option<(usize, usize)>,
    ) -> f32 {
        let mut draw_x = x.round();
        let baseline_y = baseline_y.round();
        let mut chars = text.char_indices().peekable();
        while let Some((byte_idx, ch)) = chars.next() {
            if matches!(ch, '\n' | '\r' | '\u{FE0F}' | '\u{200D}') {
                continue;
            }
            let force_emoji = api_route_force_emoji_presentation(chars.peek().map(|(_, next)| *next));
            let mut source_end = source_base + byte_idx + ch.len_utf8();
            if force_emoji {
                source_end = source_end.saturating_add('\u{FE0F}'.len_utf8());
            }
            let source_start = source_base + byte_idx;
            let Some(glyph) = self.api_route_glyph(ch, force_emoji) else {
                continue;
            };
            let advance = Self::snapped_text_advance(glyph.advance, scale);
            if selection.is_some_and(|(start, end)| source_start < end && source_end > start) {
                self.push_rect(
                    draw_x,
                    line_top.round(),
                    advance.max(1.0),
                    line_h.round(),
                    self.ui_theme.sel,
                );
            }
            self.push_ui_glyph_at_scale(ch, draw_x, baseline_y, scale, color, bold);
            draw_x += advance;
        }
        draw_x - x.round()
    }

    fn api_route_markdown_next_nonspace(text: &str, from: usize) -> usize {
        for span in api_description_inline_spans(text) {
            if span.source_end <= from {
                continue;
            }
            let start = span.source_start.max(from);
            for (relative, ch) in text[start..span.source_end].char_indices() {
                let source = start + relative;
                if !ch.is_whitespace() {
                    return source;
                }
            }
        }
        text.len()
    }

    fn api_route_markdown_wrap_range(
        &mut self,
        text: &str,
        start: usize,
        available_w: f32,
        scale: f32,
    ) -> (usize, usize) {
        let mut width = 0.0;
        let mut last_soft = None;
        let mut saw_visible = false;

        for span in api_description_inline_spans(text) {
            if span.source_end <= start {
                continue;
            }
            let span_start = span.source_start.max(start);
            let mut chars = text[span_start..span.source_end].char_indices().peekable();
            while let Some((relative, ch)) = chars.next() {
                if matches!(ch, '\r' | '\u{FE0F}' | '\u{200D}') {
                    continue;
                }
                let source_start = span_start + relative;
                let force_emoji =
                    api_route_force_emoji_presentation(chars.peek().map(|(_, next)| *next));
                let mut source_end = source_start + ch.len_utf8();
                if force_emoji {
                    source_end = source_end.saturating_add('\u{FE0F}'.len_utf8());
                }
                let advance = self
                    .api_route_glyph(ch, force_emoji)
                    .map(|glyph| Self::snapped_text_advance(glyph.advance, scale))
                    .unwrap_or(8.0);
                if saw_visible && width + advance > available_w {
                    let end = last_soft.unwrap_or(source_start);
                    let next = Self::api_route_markdown_next_nonspace(text, end);
                    return (end, next.max(end));
                }
                width += advance;
                saw_visible = true;
                if matches!(ch, ' ' | ',' | '·' | '|') {
                    last_soft = Some(source_end.min(text.len()));
                }
            }
        }
        (text.len(), text.len())
    }

    fn api_route_markdown_byte_at_range(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
        x: f32,
        mouse_x: f32,
        scale: f32,
    ) -> usize {
        let target = (mouse_x - x).max(0.0);
        let mut width = 0.0;
        let mut last_boundary = start.min(text.len());

        for span in api_description_inline_spans(text) {
            let span_start = span.source_start.max(start);
            let span_end = span.source_end.min(end);
            if span_start >= span_end {
                continue;
            }
            let mut chars = text[span_start..span_end].char_indices().peekable();
            while let Some((relative, ch)) = chars.next() {
                if matches!(ch, '\r' | '\u{FE0F}' | '\u{200D}') {
                    continue;
                }
                let source_start = span_start + relative;
                let force_emoji =
                    api_route_force_emoji_presentation(chars.peek().map(|(_, next)| *next));
                let mut source_end = source_start + ch.len_utf8();
                if force_emoji {
                    source_end = source_end.saturating_add('\u{FE0F}'.len_utf8());
                }
                let advance = self
                    .api_route_glyph(ch, force_emoji)
                    .map(|glyph| Self::snapped_text_advance(glyph.advance, scale))
                    .unwrap_or(8.0);
                if target < width + advance * 0.5 {
                    return source_start;
                }
                width += advance;
                last_boundary = source_end.min(text.len());
                if target < width {
                    return last_boundary;
                }
            }
        }
        last_boundary
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_route_markdown_range(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
        x: f32,
        baseline_y: f32,
        line_top: f32,
        line_h: f32,
        color: [f32; 4],
        scale: f32,
        source_base: usize,
        selection: Option<(usize, usize)>,
        heading: bool,
    ) -> f32 {
        let mut draw_x = x.round();
        for span in api_description_inline_spans(text) {
            let span_start = span.source_start.max(start);
            let span_end = span.source_end.min(end);
            if span_start >= span_end {
                continue;
            }
            let visible = &text[span_start..span_end];
            let width = self.api_route_text_width(visible, scale);
            if span.kind == ApiDescriptionInlineKind::Code && width > 0.0 {
                self.push_rounded_rect(
                    draw_x - 2.0,
                    line_top.round() + 2.0,
                    width + 4.0,
                    (line_h.round() - 4.0).max(1.0),
                    3.0,
                    [self.ui_theme.sel[0], self.ui_theme.sel[1], self.ui_theme.sel[2], 0.34],
                );
            }
            let advanced = self.draw_api_route_text_run(
                visible,
                draw_x,
                baseline_y,
                line_top,
                line_h,
                color,
                scale,
                heading || span.kind == ApiDescriptionInlineKind::Bold,
                source_base + span_start,
                selection,
            );
            draw_x += advanced;
        }
        draw_x - x.round()
    }

    fn api_route_one_line_byte_at(
        &mut self,
        text: &str,
        x: f32,
        mouse_x: f32,
        scale: f32,
    ) -> usize {
        let target = (mouse_x - x).max(0.0);
        let mut width = 0.0;
        let mut chars = text.char_indices().peekable();
        while let Some((byte_idx, ch)) = chars.next() {
            if matches!(ch, '\n' | '\r' | '\u{FE0F}' | '\u{200D}') {
                continue;
            }
            let force_emoji = api_route_force_emoji_presentation(chars.peek().map(|(_, next)| *next));
            let advance = self
                .api_route_glyph(ch, force_emoji)
                .map(|glyph| Self::snapped_text_advance(glyph.advance, scale))
                .unwrap_or(8.0);
            if target < width + advance * 0.5 {
                return byte_idx;
            }
            width += advance;
            let mut boundary = byte_idx + ch.len_utf8();
            if force_emoji {
                boundary = boundary.saturating_add('\u{FE0F}'.len_utf8());
            }
            if target < width {
                return boundary.min(text.len());
            }
        }
        text.len()
    }

    pub(crate) fn api_route_text_byte_at(
        &mut self,
        field: ApiRouteTextField,
        text: &str,
        rect: (f32, f32, f32, f32),
        mouse_x: f32,
        mouse_y: f32,
        scale: f32,
    ) -> usize {
        match field {
            ApiRouteTextField::Path => {
                self.api_route_one_line_byte_at(text, rect.0, mouse_x, 1.14)
            }
            ApiRouteTextField::Summary => {
                self.api_route_one_line_byte_at(text, rect.0, mouse_x, 0.92)
            }
            ApiRouteTextField::Description => {
                self.api_route_description_byte_at(text, rect, mouse_x, mouse_y, scale)
            }
        }
    }

    fn api_route_description_byte_at(
        &mut self,
        text: &str,
        rect: (f32, f32, f32, f32),
        mouse_x: f32,
        mouse_y: f32,
        scale: f32,
    ) -> usize {
        if mouse_y <= rect.1 {
            return 0;
        }
        if mouse_y >= rect.1 + rect.3 {
            return text.len();
        }

        let mut top = rect.1;
        let mut source_offset = 0usize;
        for source_line in text.split('\n') {
            let line = source_line.trim_end_matches('\r');
            let (kind, content_start, content) = api_description_line_parts(line);
            let (text_scale, line_h, content_x, available_w) = match kind {
                ApiDescriptionLineKind::Heading => (1.02, 25.0 * scale, rect.0, rect.2),
                ApiDescriptionLineKind::ListItem => (
                    0.84,
                    20.0 * scale,
                    rect.0 + API_DESCRIPTION_LIST_CONTENT_INDENT * scale,
                    (rect.2 - API_DESCRIPTION_LIST_CONTENT_INDENT * scale).max(1.0),
                ),
                ApiDescriptionLineKind::Text => (0.82, 19.0 * scale, rect.0, rect.2),
            };
            if content.trim().is_empty() {
                if mouse_y < top + line_h {
                    return (source_offset + content_start).min(text.len());
                }
                top += line_h;
                source_offset = source_offset.saturating_add(source_line.len() + 1);
                continue;
            }

            let mut visual_start = 0usize;
            loop {
                let (visual_end, next_start) = self.api_route_markdown_wrap_range(
                    content,
                    visual_start,
                    available_w,
                    text_scale,
                );
                if mouse_y < top + line_h {
                    return source_offset
                        + content_start
                        + self.api_route_markdown_byte_at_range(
                            content,
                            visual_start,
                            visual_end,
                            content_x,
                            mouse_x,
                            text_scale,
                        );
                }
                top += line_h;
                if next_start >= content.len() {
                    break;
                }
                if next_start <= visual_start {
                    break;
                }
                visual_start = next_start;
            }
            source_offset = source_offset.saturating_add(source_line.len() + 1);
        }
        text.len()
    }

    fn draw_api_route_description(
        &mut self,
        text: &str,
        x: f32,
        top_y: f32,
        w: f32,
        s: f32,
        selection: Option<ApiRouteTextSelection>,
    ) -> f32 {
        let selected = selection
            .filter(|selection| selection.field == ApiRouteTextField::Description)
            .and_then(|selection| selection.range(text));
        let mut top = top_y;
        let mut source_offset = 0usize;

        for source_line in text.split('\n') {
            let line = source_line.trim_end_matches('\r');
            let (kind, content_start, content) = api_description_line_parts(line);
            let color = api_description_line_color(kind, self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg));
            let (text_scale, line_h, baseline_offset, content_x, available_w) = match kind {
                ApiDescriptionLineKind::Heading => (1.02, 25.0 * s, 19.0 * s, x, w),
                ApiDescriptionLineKind::ListItem => (
                    0.84,
                    20.0 * s,
                    16.0 * s,
                    x + API_DESCRIPTION_LIST_CONTENT_INDENT * s,
                    (w - API_DESCRIPTION_LIST_CONTENT_INDENT * s).max(1.0),
                ),
                ApiDescriptionLineKind::Text => (0.82, 19.0 * s, 15.0 * s, x, w),
            };
            if content.trim().is_empty() {
                top += line_h;
                source_offset = source_offset.saturating_add(source_line.len() + 1);
                continue;
            }

            let mut visual_start = 0usize;
            let mut first_visual = true;
            loop {
                let (visual_end, next_start) = self.api_route_markdown_wrap_range(
                    content,
                    visual_start,
                    available_w,
                    text_scale,
                );
                if kind == ApiDescriptionLineKind::ListItem && first_visual {
                    self.draw_api_route_text_run(
                        API_DESCRIPTION_LIST_MARKER,
                        x + API_DESCRIPTION_LIST_MARKER_INDENT * s,
                        top + baseline_offset,
                        top,
                        line_h,
                        color,
                        0.84,
                        false,
                        usize::MAX / 2,
                        None,
                    );
                }
                self.draw_api_route_markdown_range(
                    content,
                    visual_start,
                    visual_end,
                    content_x,
                    top + baseline_offset,
                    top,
                    line_h,
                    color,
                    text_scale,
                    source_offset + content_start,
                    selected,
                    kind == ApiDescriptionLineKind::Heading,
                );
                top += line_h;
                first_visual = false;
                if next_start >= content.len() {
                    break;
                }
                if next_start <= visual_start {
                    break;
                }
                visual_start = next_start;
            }
            source_offset = source_offset.saturating_add(source_line.len() + 1);
        }

        top - top_y
    }

    pub(crate) fn draw_api_client_tab(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        _editor: &crate::editor::Editor,
        ide_panel: &crate::app::IdePanelState,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        self.push_rect(x, y, w, h, self.ui.pick(UiRole::BgPanel, self.ui_theme.bg));
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, x, y, w, h, mx, my);
        let manual_route = match &tab_meta.route_identity {
            Some(crate::app::api_client::ApiClientRouteIdentity::Manual { stable_id }) => ide_panel
                .api
                .mock
                .manual_routes
                .iter()
                .enumerate()
                .find(|(_, route)| route.stable_id == *stable_id),
            _ => None,
        };
        let Some(model) = crate::app::api_client::api_route_model_for_identity(
            &ide_panel.api,
            tab_meta.spec_id,
            tab_meta.route_identity.as_ref(),
        ) else {
            self.draw_string_scaled_stable(
                "Спецификация загружается или кэш пустой",
                x + 28.0 * s,
                y + 46.0 * s,
                self.ui.pick(UiRole::TextSecondary, [0.72, 0.74, 0.82, 1.0]),
                0.95,
            );
            return;
        };
        let model = model.as_ref();
        if tab_state.auth_view {
            self.draw_api_client_tab_auth_view(
                x,
                y,
                w,
                h,
                s,
                model,
                tab_meta,
                tab_state,
                ide_panel,
                ui_registry,
                mx,
                my,
                blink_alpha,
            );
            return;
        }
        let Some(route_idx) = tab_state
            .route_idx
            .or_else(|| (!model.routes.is_empty()).then_some(0))
        else {
            self.draw_string_scaled_stable(
                "В спецификации нет routes",
                x + 28.0 * s,
                y + 46.0 * s,
                self.ui.pick(UiRole::TextSecondary, [0.72, 0.74, 0.82, 1.0]),
                0.95,
            );
            return;
        };
        let Some(route) = (if manual_route.is_some() {
            model.routes.first()
        } else {
            model.routes.get(route_idx)
        }) else {
            return;
        };
        let manual_mock = manual_route.map(|(_, route)| route);
        let mock_override = if manual_mock.is_none() {
            ide_panel
                .api
                .mock
                .route_overrides
                .iter()
                .find(|item| item.method == route.method && item.path == route.path)
        } else {
            None
        };

        self.flush();
        ui_registry.push_clip(crate::ui_system::UiClipRect::new(x, y, w, h));
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
        }

        let tab_clip = (x, y, w, h);
        let pad = 28.0 * s;
        let content_w = (w - pad * 2.0).max(1.0);
        let scroll = tab_state.tab_scroll.current.round();
        let mut cy = y + pad - scroll;

        let ctx = ApiTabRouteCtx {
            x,
            pad,
            content_w,
            s,
            mx,
            my,
            blink_alpha,
            tab_clip,
            route_idx,
            route,
            model,
            tab_meta,
            tab_state,
            ide_panel,
        };
        cy = self.draw_api_client_tab_route_header(ctx, cy, ui_registry);
        cy = self.draw_api_client_tab_mock(ctx, cy, manual_mock, mock_override, ui_registry, hover);
        cy = self.draw_api_client_tab_route_auth(ctx, cy, ui_registry);
        cy = self.draw_api_client_tab_input_tabs(ctx, cy, ui_registry);
        if tab_state.input_doc_view == ApiInputDocView::Schema {
            cy = self.draw_api_client_tab_input_schema(
                ctx,
                cy,
                manual_mock,
                mock_override,
                ui_registry,
            );
        } else {
            cy = self.draw_api_client_tab_path_query_params(ctx, cy, ui_registry);
            cy = self.draw_api_client_tab_request_body(ctx, cy, ui_registry);
        }
        cy = self.draw_api_client_tab_output(ctx, cy, ui_registry);
        cy = self.draw_api_client_tab_servers_and_send(ctx, cy, ui_registry);
        self.draw_api_client_tab_response(ctx, cy, ui_registry);

        ui_registry.pop_clip();
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    /// Authorization page of an API tab (`ApiClientTabState::auth_view`): all schemes
    /// of the spec and the routes that issue or refresh tokens.
    fn draw_api_client_tab_auth_view(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        model: &crate::app::api_client::ApiSpecModel,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        ui_registry.push_clip(crate::ui_system::UiClipRect::new(x, y, w, h));
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
        }
        let tab_clip = (x, y, w, h);
        let pad = 28.0 * s;
        let content_w = (w - pad * 2.0).max(1.0);
        let scroll = tab_state.tab_scroll.current.round();
        let mut cy = y + pad - scroll;

        self.draw_string_scaled_stable(
            "Авторизация",
            x + pad,
            cy + 24.0 * s,
            self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
            1.18,
        );
        cy += 38.0 * s;
        if model.security_schemes.is_empty() {
            self.draw_string_scaled_stable(
                "Схем авторизации нет",
                x + pad,
                cy + 20.0 * s,
                self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
                0.90,
            );
        } else {
            self.draw_api_dynamic_table_frame(
                x + pad,
                cy,
                content_w,
                model
                    .security_schemes
                    .iter()
                    .map(|scheme| api_auth_scheme_row_height(scheme, s))
                    .sum::<f32>(),
                s,
            );
            for (scheme_idx, scheme) in model.security_schemes.iter().enumerate() {
                cy = self.draw_api_auth_scheme_row(
                    x + pad,
                    cy,
                    content_w,
                    s,
                    tab_meta.spec_id,
                    scheme_idx,
                    scheme,
                    ide_panel,
                    blink_alpha,
                    ui_registry,
                    mx,
                    my,
                );
            }
        }
        let auth_route_count = api_auth_related_route_count(model);
        if auth_route_count > 0 {
            cy += 28.0 * s;
            self.draw_api_section_title("Роуты авторизации", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            self.draw_api_dynamic_table_frame(
                x + pad,
                cy,
                content_w,
                auth_route_count as f32 * 34.0 * s,
                s,
            );
            let mut drawn = 0usize;
            for rank in 0..=2 {
                for (route_idx, route) in model.routes.iter().enumerate() {
                    if drawn >= auth_route_count {
                        break;
                    }
                    if api_auth_route_rank(route) != Some(rank) {
                        continue;
                    }
                    let row_y = cy + drawn as f32 * 34.0 * s;
                    let method_w = 56.0 * s;
                    self.draw_api_method_chip(
                        route.method,
                        x + pad + 8.0 * s,
                        row_y + 5.0 * s,
                        method_w,
                        24.0 * s,
                        s,
                        0.72,
                    );
                    let mut display_path = String::new();
                    write_api_path_display(&route.path, &mut display_path);
                    self.draw_string_scaled_stable(
                        &display_path,
                        x + pad + method_w + 20.0 * s,
                        row_y + 22.0 * s,
                        self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
                        0.86,
                    );
                    if !route.summary.is_empty() {
                        let path_w = self.measure_ui_width(&display_path, 0.86);
                        self.draw_string_scaled_stable(
                            &route.summary,
                            x + pad + method_w + path_w + 32.0 * s,
                            row_y + 22.0 * s,
                            self.ui.pick(UiRole::TextSecondary, [0.62, 0.64, 0.72, 1.0]),
                            0.78,
                        );
                    }
                    ui_registry.register_rect(
                        crate::ui_system::UiId::ApiRouteRow(route_idx),
                        x + pad,
                        row_y,
                        content_w,
                        34.0 * s,
                        mx,
                        my,
                    );
                    self.push_rect(
                        x + pad,
                        row_y + 34.0 * s,
                        content_w,
                        1.0,
                        self.ui.ink(0.08),
                    );
                    drawn += 1;
                }
            }
        }
        self.restore_api_tab_clip(tab_clip);
        ui_registry.pop_clip();
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}

/// Layout values and read-only state shared by the `draw_api_client_tab_*` section methods
/// (`draw_api_client_tab` builds it once the route is resolved).
#[derive(Clone, Copy)]
struct ApiTabRouteCtx<'a> {
    x: f32,
    pad: f32,
    content_w: f32,
    s: f32,
    mx: f32,
    my: f32,
    blink_alpha: f32,
    tab_clip: (f32, f32, f32, f32),
    route_idx: usize,
    route: &'a crate::app::api_client::ApiRouteRow,
    model: &'a crate::app::api_client::ApiSpecModel,
    tab_meta: &'a crate::app::api_client::ApiClientTabMeta,
    tab_state: &'a crate::app::api_client::ApiClientTabState,
    ide_panel: &'a crate::app::IdePanelState,
}

include!("api_client_tab_request_renderer.rs");
include!("api_client_tab_mock_renderer.rs");
include!("api_client_tab_response_renderer.rs");
