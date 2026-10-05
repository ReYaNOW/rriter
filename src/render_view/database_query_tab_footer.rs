fn ensure_database_query_review_message_layout(
    state: &crate::app::database::DatabaseQueryTabState,
    ui: &crate::theme::UiPalette,
    max_text_width: f32,
    scale: f32,
    mut char_advance: impl FnMut(char) -> f32,
) -> bool {
    let revision = state.result_view.review_message_layout_revision();
    let source_count = state.messages.len();
    let needs_rebuild = {
        let cache = state.result_view.review_message_layout_cache.borrow();
        !cache.matches(revision, source_count, max_text_width, scale)
    };
    if !needs_rebuild {
        return false;
    }

    let line_height = (20.0 * scale).round().max(16.0);
    let item_gap = (8.0 * scale).round();
    let pad = (10.0 * scale).round();
    let source_items = database_query_review_message_items(state, ui);
    let mut items = Vec::with_capacity(source_items.len());
    let mut total_height = pad;
    for (text, color) in source_items {
        let ranges = crate::render_view::core_text::wrapped_text_ranges(
            &text,
            max_text_width,
            |ch| char_advance(ch) * 0.72,
        );
        let offset_y = total_height;
        total_height += ranges.len() as f32 * line_height + item_gap;
        items.push(crate::app::database::DatabaseQueryReviewMessageLayoutItem {
            text,
            color,
            ranges,
            offset_y,
        });
    }
    total_height += pad;
    state.result_view.review_message_layout_cache.borrow_mut().replace(
        revision,
        source_count,
        max_text_width,
        scale,
        line_height,
        item_gap,
        total_height,
        items,
    );
    true
}

fn database_query_review_message_items(
    state: &crate::app::database::DatabaseQueryTabState,
    ui: &crate::theme::UiPalette,
) -> Vec<(String, [f32; 4])> {
    let mut items = Vec::with_capacity(
        state
            .analysis
            .diagnostics
            .len()
            .saturating_add(state.messages.len())
            .saturating_add(1),
    );
    items.push((
        "Изменения ещё не подтверждены. Примените транзакцию или отмените её.".to_string(),
        ui.pick(UiRole::Warning, [0.95, 0.72, 0.30, 1.0]),
    ));
    items.extend(
        database_query_notice_items(state, ui)
            .into_iter()
            .map(|(text, color, _)| (text, color)),
    );
    items
}

fn database_query_execution_summary(
    state: &crate::app::database::DatabaseQueryTabState,
    result: &crate::app::database::DatabaseQueryResultSet,
) -> String {
    let mut parts = vec![format!("Выполнено за {} мс", state.last_duration_ms)];
    let reports_returned = result.returned_rows > 0
        || !result.columns.is_empty()
        || matches!(result.command_kind.as_str(), "SELECT" | "EXPLAIN");
    if reports_returned {
        parts.push(format!("Получено строк: {}", result.returned_rows));
    }
    if result.affected_rows > 0 {
        parts.push(format!("Изменено строк: {}", result.affected_rows));
    }
    if !result.command_kind.is_empty() {
        parts.push(result.command_kind.clone());
    }
    if !state.messages.is_empty() {
        parts.push(format!("Уведомлений: {}", state.messages.len()));
    }
    if result.truncated {
        parts.push(format!("Показаны первые {}", result.rows.len()));
    }
    parts.join(" · ")
}

fn database_query_notice_items(
    state: &crate::app::database::DatabaseQueryTabState,
    ui: &crate::theme::UiPalette,
) -> Vec<(String, [f32; 4], Option<usize>)> {
    state
        .messages
        .iter()
        .map(|message| {
            let mut text = format!("{}: {}", message.severity, message.message);
            if let Some(detail) = message.detail.as_deref() {
                text.push_str(" · ");
                text.push_str(detail);
            }
            if let Some(hint) = message.hint.as_deref() {
                text.push_str(" · Подсказка: ");
                text.push_str(hint);
            }
            (text, ui.pick(UiRole::TextSecondary, [0.82, 0.84, 0.90, 1.0]), None)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn draw_query_button(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    icon: Option<IconType>,
    active: bool,
    primary: bool,
    mx: f32,
    my: f32,
    s: f32,
    clip: Option<UiClipRect>,
) {
    let view = ButtonView {
        x: x.round(),
        y: y.round(),
        w: w.round(),
        h: h.round(),
        text,
        icon,
        text_scale: QUERY_BUTTON_TEXT_SCALE,
        icon_size: (16.0 * s).round(),
    };
    if active {
        if primary {
            let _ = view.render_styled(
                renderer,
                mx,
                my,
                s,
                false,
                ButtonStyle {
                    border: renderer.ui.pick(UiRole::Success, [0.32, 0.76, 0.43, 1.0]),
                    background: renderer.ui.pick(UiRole::Success, [0.16, 0.48, 0.26, 1.0]),
                    hover_background: renderer.ui.pick(UiRole::Success, [0.20, 0.58, 0.31, 1.0]),
                    pressed_background: renderer.ui.pick(UiRole::Success, [0.12, 0.40, 0.22, 1.0]),
                    content: renderer.ui.pick(UiRole::TextPrimary, renderer.ui_theme.fg),
                },
            );
        } else {
            let _ = view.render(renderer, mx, my, s, false);
        }
        if let Some(clip) = clip {
            ui.register_rect_clipped(id, view.x, view.y, view.w, view.h, clip, mx, my);
        } else {
            ui.register_rect(id, view.x, view.y, view.w, view.h, mx, my);
        }
    } else {
        view.render_disabled(renderer, s);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_query_tab(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    active: bool,
    mx: f32,
    my: f32,
    s: f32,
    clip: UiClipRect,
) {
    let x = x.round();
    let y = y.round();
    let w = w.round();
    let h = h.round();
    let visible = clip.intersect(x, y, w, h);
    let hovered = visible.is_some_and(|rect| {
        mx >= rect.x && mx <= rect.x + rect.w && my >= rect.y && my <= rect.y + rect.h
    });
    renderer.push_rounded_rect(
        x,
        y,
        w,
        h,
        (5.0 * s).round(),
        if active {
            renderer.ui.pick(UiRole::RowActive, [0.28, 0.24, 0.38, 1.0])
        } else if hovered {
            renderer.ui.pick(UiRole::RowHover, [0.18, 0.19, 0.24, 1.0])
        } else {
            renderer.ui.pick(UiRole::BgPanelAlt, [0.13, 0.135, 0.17, 1.0])
        },
    );
    let mut scratch = String::new();
    renderer.draw_tree_label_clipped(
        text,
        x + (10.0 * s).round(),
        Renderer::tree_row_text_y(y, h, s),
        (w - 20.0 * s).max(4.0),
        renderer.ui.pick(UiRole::TextPrimary, renderer.ui_theme.fg),
        0.68,
        &mut scratch,
    );
    ui.register_rect_clipped(id, x, y, w, h, clip, mx, my);
}

impl Renderer {
    pub(crate) fn database_query_scrollbar(
        lane: (f32, f32, f32, f32), viewport: f32, max_scroll: f32,
        offset: f32, horizontal: bool,
    ) -> crate::render_view::scrollbar_widget::Scrollbar {
        Self::database_query_scrollbar_with_ui(
            lane,
            viewport,
            max_scroll,
            offset,
            horizontal,
            &crate::theme::UiPalette::for_id(crate::theme::ThemeId::Dracula),
        )
    }

    fn database_query_scrollbar_with_ui(
        lane: (f32, f32, f32, f32), viewport: f32, max_scroll: f32,
        offset: f32, horizontal: bool, ui: &crate::theme::UiPalette,
    ) -> crate::render_view::scrollbar_widget::Scrollbar {
        use crate::render_view::scrollbar_widget::{Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle};
        Scrollbar {
            style: ScrollbarStyle {
                thumb_thickness: 0.0,
                edge_gap: Some(2.0),
                track_pad: 0.0,
                min_thumb: if horizontal { 36.0 } else { 28.0 },
                radius: Some(3.0),
                track_color: Some(ui.pick(UiRole::BgPanel, [0.055, 0.058, 0.075, 1.0])),
                thumb_color: ui.pick(UiRole::ScrollbarThumb, [0.62, 0.38, 0.82, 0.9]),
                thumb_paint: crate::render_view::scrollbar_widget::ScrollbarPaint::Literal,
                track_paint: crate::render_view::scrollbar_widget::ScrollbarPaint::Literal,
            },
            axis: if horizontal { ScrollbarAxis::Horizontal } else { ScrollbarAxis::Vertical },
            lane,
            extent: ScrollbarExtent::with_max(viewport, max_scroll, offset),
        }
    }
}
