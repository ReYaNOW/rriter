//! Persistent, UI-independent settings for Dart language support.
//!
//! The settings UI and closing-label runtime share this persisted source of truth. Keeping
//! it outside either subsystem prevents the settings UI from becoming the
//! source of truth and gives the LSP/renderer agents a stable integration
//! contract.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DartClosingLabelsMode {
    Off,
    DartServer,
    #[default]
    DartServerAndBlocks,
}

impl DartClosingLabelsMode {
    pub const fn config_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::DartServer => "dart_server",
            Self::DartServerAndBlocks => "dart_server_and_blocks",
        }
    }

    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Self::Off,
            "dart_server" | "server" => Self::DartServer,
            "dart_server_and_blocks" | "server_and_blocks" | "all" => Self::DartServerAndBlocks,
            _ => Self::default(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Closing labels: выкл.",
            Self::DartServer => "Closing labels: Dart server",
            Self::DartServerAndBlocks => "Closing labels: server + блоки",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::DartServer,
            Self::DartServer => Self::DartServerAndBlocks,
            Self::DartServerAndBlocks => Self::Off,
        }
    }

    pub const fn uses_server_labels(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub const fn uses_syntax_blocks(self) -> bool {
        matches!(self, Self::DartServerAndBlocks)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DartSettings {
    pub enabled: bool,
    pub workspace_analysis: bool,
    pub closing_labels: DartClosingLabelsMode,
    pub minimum_nesting_depth: u8,
    pub minimum_block_lines: u16,
}

impl Default for DartSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            workspace_analysis: true,
            closing_labels: DartClosingLabelsMode::DartServerAndBlocks,
            minimum_nesting_depth: 2,
            minimum_block_lines: 3,
        }
    }
}

impl DartSettings {
    pub const MIN_NESTING_DEPTH: u8 = 1;
    pub const MAX_NESTING_DEPTH: u8 = 16;
    pub const MIN_BLOCK_LINES: u16 = 1;
    pub const MAX_BLOCK_LINES: u16 = 1_000;

    pub fn normalize(&mut self) {
        self.minimum_nesting_depth = self
            .minimum_nesting_depth
            .clamp(Self::MIN_NESTING_DEPTH, Self::MAX_NESTING_DEPTH);
        self.minimum_block_lines = self
            .minimum_block_lines
            .clamp(Self::MIN_BLOCK_LINES, Self::MAX_BLOCK_LINES);
    }

    pub fn adjust_minimum_nesting_depth(&mut self, delta: i8) {
        self.minimum_nesting_depth = self
            .minimum_nesting_depth
            .saturating_add_signed(delta)
            .clamp(Self::MIN_NESTING_DEPTH, Self::MAX_NESTING_DEPTH);
    }

    pub fn adjust_minimum_block_lines(&mut self, delta: i16) {
        self.minimum_block_lines = self
            .minimum_block_lines
            .saturating_add_signed(delta)
            .clamp(Self::MIN_BLOCK_LINES, Self::MAX_BLOCK_LINES);
    }

    pub(crate) fn closing_hint_settings(&self) -> crate::languages::dart::ClosingHintSettings {
        let mode = match self.closing_labels {
            DartClosingLabelsMode::Off => crate::languages::dart::ClosingHintMode::Off,
            DartClosingLabelsMode::DartServer => {
                crate::languages::dart::ClosingHintMode::DartServer
            }
            DartClosingLabelsMode::DartServerAndBlocks => {
                crate::languages::dart::ClosingHintMode::DartServerAndBlocks
            }
        };
        crate::languages::dart::ClosingHintSettings {
            mode,
            minimum_nesting_depth: usize::from(self.minimum_nesting_depth),
            minimum_block_lines: usize::from(self.minimum_block_lines),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DartClosingLabelsMode, DartSettings};

    #[test]
    fn dart_settings_defaults_match_closing_label_contract() {
        let settings = DartSettings::default();

        assert!(settings.enabled);
        assert!(settings.workspace_analysis);
        assert_eq!(
            settings.closing_labels,
            DartClosingLabelsMode::DartServerAndBlocks
        );
        assert_eq!(settings.minimum_nesting_depth, 2);
        assert_eq!(settings.minimum_block_lines, 3);
    }

    #[test]
    fn closing_label_modes_round_trip_through_config_values() {
        for mode in [
            DartClosingLabelsMode::Off,
            DartClosingLabelsMode::DartServer,
            DartClosingLabelsMode::DartServerAndBlocks,
        ] {
            assert_eq!(
                DartClosingLabelsMode::from_config_value(mode.config_value()),
                mode
            );
        }
    }

    #[test]
    fn unknown_closing_label_mode_uses_safe_default() {
        assert_eq!(
            DartClosingLabelsMode::from_config_value("future-mode"),
            DartClosingLabelsMode::DartServerAndBlocks
        );
        assert_eq!(
            DartClosingLabelsMode::from_config_value(""),
            DartClosingLabelsMode::DartServerAndBlocks
        );
    }

    #[test]
    fn closing_label_mode_accepts_legacy_aliases() {
        assert_eq!(
            DartClosingLabelsMode::from_config_value("server"),
            DartClosingLabelsMode::DartServer
        );
        assert_eq!(
            DartClosingLabelsMode::from_config_value("ALL"),
            DartClosingLabelsMode::DartServerAndBlocks
        );
    }

    #[test]
    fn closing_label_mode_cycles_without_skipping_states() {
        let off = DartClosingLabelsMode::Off;
        let server = off.next();
        let all = server.next();

        assert_eq!(server, DartClosingLabelsMode::DartServer);
        assert_eq!(all, DartClosingLabelsMode::DartServerAndBlocks);
        assert_eq!(all.next(), DartClosingLabelsMode::Off);
    }

    #[test]
    fn closing_label_capability_helpers_are_precise() {
        assert!(!DartClosingLabelsMode::Off.uses_server_labels());
        assert!(!DartClosingLabelsMode::Off.uses_syntax_blocks());
        assert!(DartClosingLabelsMode::DartServer.uses_server_labels());
        assert!(!DartClosingLabelsMode::DartServer.uses_syntax_blocks());
        assert!(DartClosingLabelsMode::DartServerAndBlocks.uses_server_labels());
        assert!(DartClosingLabelsMode::DartServerAndBlocks.uses_syntax_blocks());
    }

    #[test]
    fn normalization_clamps_values_loaded_from_config() {
        let mut below = DartSettings {
            minimum_nesting_depth: 0,
            minimum_block_lines: 0,
            ..DartSettings::default()
        };
        below.normalize();
        assert_eq!(below.minimum_nesting_depth, DartSettings::MIN_NESTING_DEPTH);
        assert_eq!(below.minimum_block_lines, DartSettings::MIN_BLOCK_LINES);

        let mut above = DartSettings {
            minimum_nesting_depth: u8::MAX,
            minimum_block_lines: u16::MAX,
            ..DartSettings::default()
        };
        above.normalize();
        assert_eq!(above.minimum_nesting_depth, DartSettings::MAX_NESTING_DEPTH);
        assert_eq!(above.minimum_block_lines, DartSettings::MAX_BLOCK_LINES);
    }

    #[test]
    fn nesting_adjustment_saturates_at_both_bounds() {
        let mut settings = DartSettings::default();
        settings.adjust_minimum_nesting_depth(i8::MIN);
        assert_eq!(
            settings.minimum_nesting_depth,
            DartSettings::MIN_NESTING_DEPTH
        );

        settings.adjust_minimum_nesting_depth(i8::MAX);
        assert_eq!(
            settings.minimum_nesting_depth,
            DartSettings::MAX_NESTING_DEPTH
        );
    }

    #[test]
    fn block_line_adjustment_saturates_at_both_bounds() {
        let mut settings = DartSettings::default();
        settings.adjust_minimum_block_lines(i16::MIN);
        assert_eq!(settings.minimum_block_lines, DartSettings::MIN_BLOCK_LINES);

        settings.adjust_minimum_block_lines(i16::MAX);
        assert_eq!(settings.minimum_block_lines, DartSettings::MAX_BLOCK_LINES);
    }

    #[test]
    fn adjustments_apply_small_deltas_without_resetting_other_fields() {
        let mut settings = DartSettings {
            enabled: false,
            workspace_analysis: false,
            closing_labels: DartClosingLabelsMode::DartServer,
            minimum_nesting_depth: 4,
            minimum_block_lines: 12,
        };

        settings.adjust_minimum_nesting_depth(-1);
        settings.adjust_minimum_block_lines(5);

        assert!(!settings.enabled);
        assert!(!settings.workspace_analysis);
        assert_eq!(settings.closing_labels, DartClosingLabelsMode::DartServer);
        assert_eq!(settings.minimum_nesting_depth, 3);
        assert_eq!(settings.minimum_block_lines, 17);
    }

    #[test]
    fn every_persisted_closing_mode_maps_to_the_matching_runtime_mode() {
        let cases = [
            (
                DartClosingLabelsMode::Off,
                crate::languages::dart::ClosingHintMode::Off,
            ),
            (
                DartClosingLabelsMode::DartServer,
                crate::languages::dart::ClosingHintMode::DartServer,
            ),
            (
                DartClosingLabelsMode::DartServerAndBlocks,
                crate::languages::dart::ClosingHintMode::DartServerAndBlocks,
            ),
        ];

        for (persisted, expected) in cases {
            let settings = DartSettings {
                closing_labels: persisted,
                ..DartSettings::default()
            };
            assert_eq!(settings.closing_hint_settings().mode, expected);
        }
    }

    #[test]
    fn persisted_closing_label_settings_drive_runtime_renderer_contract() {
        let settings = DartSettings {
            enabled: true,
            workspace_analysis: true,
            closing_labels: DartClosingLabelsMode::DartServer,
            minimum_nesting_depth: 5,
            minimum_block_lines: 11,
        };

        let runtime = settings.closing_hint_settings();
        assert_eq!(
            runtime.mode,
            crate::languages::dart::ClosingHintMode::DartServer
        );
        assert_eq!(runtime.minimum_nesting_depth, 5);
        assert_eq!(runtime.minimum_block_lines, 11);
    }
}
