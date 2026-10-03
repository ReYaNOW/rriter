#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustCheckCommand {
    Check,
    Clippy,
}

impl RustCheckCommand {
    pub const fn config_value(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Clippy => "clippy",
        }
    }

    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "check" => Self::Check,
            "clippy" => Self::Clippy,
            _ => Self::default(),
        }
    }
}

impl Default for RustCheckCommand {
    fn default() -> Self {
        Self::Clippy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustSettings {
    pub enabled: bool,
    pub check_command: RustCheckCommand,
}

impl Default for RustSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            check_command: RustCheckCommand::Clippy,
        }
    }
}

impl RustSettings {
    pub fn from_config_value(value: &serde_json::Value) -> Self {
        let Some(settings) = value.as_object() else {
            return Self::default();
        };
        let mut result = Self::default();
        if let Some(enabled) = settings.get("enabled").and_then(serde_json::Value::as_bool) {
            result.enabled = enabled;
        }
        if let Some(command) = settings
            .get("check_command")
            .and_then(serde_json::Value::as_str)
        {
            result.check_command = RustCheckCommand::from_config_value(command);
        }
        result
    }

    pub fn config_value(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "check_command": self.check_command.config_value(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{RustCheckCommand, RustSettings};

    #[test]
    fn defaults_to_clippy_for_invalid_values() {
        assert_eq!(RustCheckCommand::from_config_value(""), RustCheckCommand::Clippy);
        assert_eq!(RustCheckCommand::from_config_value("garbage"), RustCheckCommand::Clippy);
        assert_eq!(RustSettings::from_config_value(&serde_json::Value::Null), RustSettings::default());
        assert_eq!(RustSettings::from_config_value(&serde_json::json!({})), RustSettings::default());
        assert_eq!(
            RustSettings::from_config_value(&serde_json::json!({"enabled": "yes", "check_command": 1})),
            RustSettings::default()
        );
    }

    #[test]
    fn parses_supported_settings() {
        assert_eq!(
            RustSettings::from_config_value(&serde_json::json!({"enabled": false, "check_command": "check"})),
            RustSettings { enabled: false, check_command: RustCheckCommand::Check }
        );
    }
}
