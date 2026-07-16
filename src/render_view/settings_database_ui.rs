use crate::app::database::{DatabaseConnectionColor, DatabaseSettings};
use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::ButtonView;

const DATABASE_SETTINGS_ROW_COUNT: usize = 10;
const DATABASE_SETTINGS_ROW_HEIGHT: f32 = 43.0;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DatabaseSettingsRow {
    label: &'static str,
    value: String,
}

fn connection_color_label(color: DatabaseConnectionColor) -> &'static str {
    match color {
        DatabaseConnectionColor::Blue => "Синий",
        DatabaseConnectionColor::Green => "Зелёный",
        DatabaseConnectionColor::Yellow => "Жёлтый",
        DatabaseConnectionColor::Orange => "Оранжевый",
        DatabaseConnectionColor::Red => "Красный",
        DatabaseConnectionColor::Purple => "Фиолетовый",
        DatabaseConnectionColor::Cyan => "Бирюзовый",
        DatabaseConnectionColor::Gray => "Серый",
    }
}

fn database_settings_rows(
    settings: &DatabaseSettings,
) -> [DatabaseSettingsRow; DATABASE_SETTINGS_ROW_COUNT] {
    [
        DatabaseSettingsRow {
            label: "Ожидание подтверждения транзакции",
            value: format!("{} с", settings.transaction_review_timeout_seconds),
        },
        DatabaseSettingsRow {
            label: "Таймаут SQL-запроса",
            value: format!("{} с", settings.statement_timeout_seconds),
        },
        DatabaseSettingsRow {
            label: "Таймаут блокировки",
            value: format!("{} с", settings.lock_timeout_seconds),
        },
        DatabaseSettingsRow {
            label: "Таймаут подключения",
            value: format!("{} с", settings.connect_timeout_seconds),
        },
        DatabaseSettingsRow {
            label: "Таймаут запуска SSH",
            value: format!("{} с", settings.ssh_startup_timeout_seconds),
        },
        DatabaseSettingsRow {
            label: "Лимит строк таблицы по умолчанию",
            value: settings.default_table_limit.to_string(),
        },
        DatabaseSettingsRow {
            label: "Максимум строк результата",
            value: settings.result_row_limit.to_string(),
        },
        DatabaseSettingsRow {
            label: "Максимум памяти результата",
            value: format!(
                "{} MiB",
                settings.result_memory_limit_bytes / (1024 * 1024)
            ),
        },
        DatabaseSettingsRow {
            label: "Записей в истории SQL",
            value: settings.sql_history_limit.to_string(),
        },
        DatabaseSettingsRow {
            label: "Цвет нового подключения",
            value: connection_color_label(settings.default_connection_color).to_string(),
        },
    ]
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_database_settings_tab(
        &mut self,
        settings: &DatabaseSettings,
        content_x: f32,
        mut content_y: f32,
        ui_registry: &mut UiRegistry,
    ) {
        let s = self.scale_factor;
        let content_x = content_x.round();
        content_y = content_y.round();
        self.draw_string_scaled_pixel_snapped(
            "Все ограничения применяются к PostgreSQL и SQL-консолям без перезапуска RRiter.",
            content_x,
            content_y,
            [0.55, 0.57, 0.65, 1.0],
            0.82,
        );
        content_y += 34.0 * s;

        for (index, row) in database_settings_rows(settings).iter().enumerate() {
            let row_y = content_y.round();
            self.draw_string_scaled_pixel_snapped(
                row.label,
                content_x,
                Self::tree_row_text_y(row_y, (30.0 * s).round(), s),
                [0.82, 0.82, 0.86, 1.0],
                0.86,
            );

            let minus_x = (content_x + 300.0 * s).round();
            let value_x = (minus_x + 38.0 * s).round();
            let plus_x = (value_x + 116.0 * s).round();
            ui_registry.register_rect(
                UiId::SettingsDatabaseAdjust(index, -1),
                minus_x,
                row_y,
                30.0 * s,
                30.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            ui_registry.register_rect(
                UiId::SettingsDatabaseAdjust(index, 1),
                plus_x,
                row_y,
                30.0 * s,
                30.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );

            ButtonView {
                x: minus_x,
                y: row_y,
                w: 30.0 * s,
                h: 30.0 * s,
                text: "−",
                icon: None,
                text_scale: 0.82,
                icon_size: 0.0,
            }
            .render(
                self,
                self.last_mouse_x,
                self.last_mouse_y,
                s,
                false,
            );

            self.push_rounded_rect(
                value_x,
                row_y,
                108.0 * s,
                30.0 * s,
                5.0 * s,
                [0.20, 0.21, 0.26, 1.0],
            );
            let value_w = self.measure_ui_width(&row.value, 0.78);
            self.draw_string_scaled_pixel_snapped(
                &row.value,
                (value_x + (108.0 * s - value_w) * 0.5).round(),
                Self::tree_row_text_y(row_y, (30.0 * s).round(), s),
                [0.92, 0.92, 0.95, 1.0],
                0.78,
            );

            ButtonView {
                x: plus_x,
                y: row_y,
                w: 30.0 * s,
                h: 30.0 * s,
                text: "+",
                icon: None,
                text_scale: 0.82,
                icon_size: 0.0,
            }
            .render(
                self,
                self.last_mouse_x,
                self.last_mouse_y,
                s,
                false,
            );
            content_y += DATABASE_SETTINGS_ROW_HEIGHT * s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_settings_rows_cover_every_adjustable_setting_in_ui_order() {
        let rows = database_settings_rows(&DatabaseSettings::default());
        assert_eq!(rows.len(), DATABASE_SETTINGS_ROW_COUNT);
        assert_eq!(rows[0].label, "Ожидание подтверждения транзакции");
        assert_eq!(rows[1].label, "Таймаут SQL-запроса");
        assert_eq!(rows[2].label, "Таймаут блокировки");
        assert_eq!(rows[3].label, "Таймаут подключения");
        assert_eq!(rows[4].label, "Таймаут запуска SSH");
        assert_eq!(rows[5].label, "Лимит строк таблицы по умолчанию");
        assert_eq!(rows[6].label, "Максимум строк результата");
        assert_eq!(rows[7].label, "Максимум памяти результата");
        assert_eq!(rows[8].label, "Записей в истории SQL");
        assert_eq!(rows[9].label, "Цвет нового подключения");
    }

    #[test]
    fn database_settings_rows_render_normalized_values() {
        let mut settings = DatabaseSettings {
            transaction_review_timeout_seconds: 1,
            statement_timeout_seconds: 0,
            lock_timeout_seconds: 0,
            connect_timeout_seconds: 0,
            ssh_startup_timeout_seconds: 0,
            default_table_limit: 0,
            result_row_limit: 0,
            result_memory_limit_bytes: 0,
            sql_history_limit: 0,
            default_connection_color: DatabaseConnectionColor::Purple,
        };
        settings.normalize();
        let rows = database_settings_rows(&settings);
        assert_eq!(rows[0].value, "30 с");
        assert_eq!(rows[1].value, "1 с");
        assert_eq!(rows[5].value, "1");
        assert_eq!(rows[7].value, "1 MiB");
        assert_eq!(rows[9].value, "Фиолетовый");
    }

    #[test]
    fn every_connection_color_has_a_stable_russian_label() {
        let colors = [
            DatabaseConnectionColor::Blue,
            DatabaseConnectionColor::Green,
            DatabaseConnectionColor::Yellow,
            DatabaseConnectionColor::Orange,
            DatabaseConnectionColor::Red,
            DatabaseConnectionColor::Purple,
            DatabaseConnectionColor::Cyan,
            DatabaseConnectionColor::Gray,
        ];
        let mut labels = colors
            .into_iter()
            .map(connection_color_label)
            .collect::<Vec<_>>();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), 8);
        assert!(labels.iter().all(|label| !label.is_empty()));
    }
}
