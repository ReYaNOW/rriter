use super::types::{
    ApiManualRoute, ApiMockMode, ApiMockRouteOverride, ApiMockServerStatus, ApiMockState,
    ApiUvState,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const API_MOCK_PERSIST_VERSION: u32 = 3;
const API_MOCK_LAN_BIND_HOST: &str = "0.0.0.0";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ApiMockPersist {
    version: u32,
    bind_host: String,
    port: u16,
    mode: ApiMockMode,
    proxy_base_url: String,
    uv: ApiUvState,
    route_overrides: Vec<ApiMockRouteOverride>,
    manual_routes: Vec<ApiManualRoute>,
}

impl From<&ApiMockState> for ApiMockPersist {
    fn from(state: &ApiMockState) -> Self {
        Self {
            version: API_MOCK_PERSIST_VERSION,
            bind_host: lan_bind_host(&state.bind_host).to_string(),
            port: state.port,
            mode: state.mode.canonical(),
            proxy_base_url: state.proxy_base_url.clone(),
            uv: state.uv.clone(),
            route_overrides: state.route_overrides.clone(),
            manual_routes: state.manual_routes.clone(),
        }
    }
}

impl From<ApiMockPersist> for ApiMockState {
    fn from(saved: ApiMockPersist) -> Self {
        let mode = if saved.version < API_MOCK_PERSIST_VERSION
            && saved.mode.canonical() == ApiMockMode::MockAll
            && (!saved.route_overrides.is_empty() || !saved.manual_routes.is_empty())
        {
            ApiMockMode::MockSelectedProxyRest
        } else {
            saved.mode.canonical()
        };
        Self {
            enabled: false,
            bind_host: lan_bind_host(&saved.bind_host).to_string(),
            port: saved.port.max(1),
            mode,
            proxy_base_url: saved.proxy_base_url,
            server_status: ApiMockServerStatus::Stopped,
            check_status: super::types::ApiMockCheckStatus::Idle,
            uv: saved.uv,
            route_overrides: saved.route_overrides,
            manual_routes: saved.manual_routes,
        }
    }
}

fn lan_bind_host(_bind_host: &str) -> &'static str {
    API_MOCK_LAN_BIND_HOST
}

#[allow(dead_code)]
pub fn load_api_mocks() -> ApiMockState {
    load_api_mocks_checked().unwrap_or_default()
}

pub fn load_api_mocks_checked() -> Result<ApiMockState, String> {
    load_api_mocks_from_checked(&api_mocks_path())
}

fn load_api_mocks_from_checked(path: &Path) -> Result<ApiMockState, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ApiMockState::default());
        }
        Err(error) => return Err(format!("API mock configuration не прочитана: {error}")),
    };
    match serde_json::from_str::<ApiMockPersist>(&content) {
        Ok(saved) => Ok(ApiMockState::from(saved)),
        Err(error) => {
            let backup_note = crate::platform::corrupt_file_backup_note(path);
            Err(format!("API mock configuration повреждена: {error}{backup_note}"))
        }
    }
}

#[cfg(test)]
fn load_api_mocks_from(path: &Path) -> ApiMockState {
    load_api_mocks_from_checked(path).unwrap_or_default()
}

pub fn save_api_mocks(state: &ApiMockState) -> Result<(), String> {
    save_api_mocks_to(&api_mocks_path(), state)
}

fn save_api_mocks_to(path: &Path, state: &ApiMockState) -> Result<(), String> {
    let saved = ApiMockPersist::from(state);
    let content = serde_json::to_vec_pretty(&saved).map_err(|err| err.to_string())?;
    crate::platform::atomic_write(path, &content).map_err(|err| err.to_string())
}

pub fn api_mocks_path() -> PathBuf {
    api_mock_data_dir().join("api_mocks.json")
}

fn api_mock_data_dir() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir().join("rriter_api_mock_tests");
    }
    #[cfg(not(test))]
    {
        crate::platform::data_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::ApiMethod;
    use crate::app::api_mock::types::{
        ApiMockResponse, ApiPythonRuntimeMode, ApiUvStatus, default_api_mock_python_script,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_persist_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "rriter-api-mock-{name}-{}-{}",
                std::process::id(),
                TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
            .join("api_mocks.json")
    }

    fn cleanup_test_path(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn persist_roundtrip_keeps_routes_and_uv_settings() {
        let path = test_persist_path("roundtrip");
        let mut state = ApiMockState {
            bind_host: "0.0.0.0".to_string(),
            port: 4101,
            mode: ApiMockMode::MockSelectedProxyRest,
            proxy_base_url: "https://backend.test".to_string(),
            ..Default::default()
        };
        state.uv.status = ApiUvStatus::Ready;
        state.uv.configured_path = Some(PathBuf::from(r"C:\Program Files\uv\uv.exe"));
        state.route_overrides.push(ApiMockRouteOverride {
            source_key: "https://example.test/openapi.json".to_string(),
            method: ApiMethod::Get,
            path: "/users".to_string(),
            enabled: true,
            proxy_when_disabled: false,
            response: ApiMockResponse::Json("{\"ok\":true}".to_string()),
            python: None,
            extra_input_fields: Vec::new(),
            extra_output_fields: Vec::new(),
        });
        state.manual_routes.push(ApiManualRoute {
            stable_id: "manual-1".to_string(),
            method: ApiMethod::Post,
            path: "/login".to_string(),
            enabled: true,
            response: ApiMockResponse::Text("ok".to_string()),
            python: None,
            input_fields: Vec::new(),
            output_fields: Vec::new(),
        });

        let _ = save_api_mocks_to(&path, &state);
        let loaded = load_api_mocks_from(&path);

        assert!(!loaded.enabled);
        assert_eq!(loaded.bind_host, "0.0.0.0");
        assert_eq!(loaded.port, 4101);
        assert_eq!(loaded.mode, ApiMockMode::MockSelectedProxyRest);
        assert_eq!(loaded.proxy_base_url, "https://backend.test");
        assert_eq!(loaded.uv.status, ApiUvStatus::Ready);
        assert_eq!(loaded.uv.python_version, "3.13");
        assert_eq!(
            loaded.uv.configured_path,
            Some(PathBuf::from(r"C:\Program Files\uv\uv.exe"))
        );
        assert_eq!(loaded.route_overrides.len(), 1);
        assert_eq!(loaded.manual_routes.len(), 1);

        cleanup_test_path(&path);
    }

    #[test]
    fn legacy_selected_only_mode_loads_as_selected_proxy() {
        let loaded = ApiMockState::from(ApiMockPersist {
            version: API_MOCK_PERSIST_VERSION,
            bind_host: "127.0.0.1".to_string(),
            port: 4101,
            mode: ApiMockMode::MockSelectedOnly,
            proxy_base_url: "https://backend.test".to_string(),
            uv: ApiUvState::default(),
            route_overrides: Vec::new(),
            manual_routes: Vec::new(),
        });

        assert_eq!(loaded.mode, ApiMockMode::MockSelectedProxyRest);
    }

    #[test]
    fn legacy_implicit_mock_all_with_route_migrates_to_selected_proxy() {
        let loaded = ApiMockState::from(ApiMockPersist {
            version: 2,
            bind_host: "0.0.0.0".to_string(),
            port: 4010,
            mode: ApiMockMode::MockAll,
            proxy_base_url: String::new(),
            uv: ApiUvState::default(),
            route_overrides: vec![ApiMockRouteOverride {
                source_key: "https://example.test/openapi.json".to_string(),
                method: ApiMethod::Get,
                path: "/users".to_string(),
                enabled: true,
                proxy_when_disabled: false,
                response: ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            }],
            manual_routes: Vec::new(),
        });

        assert_eq!(loaded.mode, ApiMockMode::MockSelectedProxyRest);
    }

    #[test]
    fn current_explicit_mock_all_with_route_stays_mock_all() {
        let loaded = ApiMockState::from(ApiMockPersist {
            version: API_MOCK_PERSIST_VERSION,
            bind_host: "0.0.0.0".to_string(),
            port: 4010,
            mode: ApiMockMode::MockAll,
            proxy_base_url: String::new(),
            uv: ApiUvState::default(),
            route_overrides: vec![ApiMockRouteOverride {
                source_key: "https://example.test/openapi.json".to_string(),
                method: ApiMethod::Get,
                path: "/users".to_string(),
                enabled: true,
                proxy_when_disabled: false,
                response: ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            }],
            manual_routes: Vec::new(),
        });

        assert_eq!(loaded.mode, ApiMockMode::MockAll);
    }

    #[test]
    fn custom_python_runtime_config_persists() {
        let path = test_persist_path("custom-python");
        let mut state = ApiMockState::default();
        state.uv.mode = ApiPythonRuntimeMode::CustomPython;
        state.uv.custom_python_path = Some(PathBuf::from("/opt/python/bin/python"));
        state.uv.python_version = "3.12".to_string();

        let _ = save_api_mocks_to(&path, &state);
        let loaded = load_api_mocks_from(&path);

        assert_eq!(loaded.uv.mode, ApiPythonRuntimeMode::CustomPython);
        assert_eq!(
            loaded.uv.custom_python_path,
            Some(PathBuf::from("/opt/python/bin/python"))
        );
        assert_eq!(loaded.uv.python_version, "3.12");

        cleanup_test_path(&path);
    }

    #[test]
    fn persisted_local_bind_migrates_to_lan() {
        let path = test_persist_path("local-bind");
        let saved = ApiMockPersist {
            version: API_MOCK_PERSIST_VERSION,
            bind_host: "127.0.0.1".to_string(),
            port: 4010,
            mode: ApiMockMode::MockAll,
            proxy_base_url: String::new(),
            uv: ApiUvState::default(),
            route_overrides: Vec::new(),
            manual_routes: Vec::new(),
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&saved).expect("serialize"),
        )
        .expect("write mock persist");

        let loaded = load_api_mocks_from(&path);

        assert_eq!(loaded.bind_host, "0.0.0.0");
        cleanup_test_path(&path);
    }

    #[test]
    fn old_python_mock_without_contract_loads_with_empty_contract() {
        let path = test_persist_path("legacy-contract");
        let legacy = serde_json::json!({
            "version": 1,
            "bind_host": "0.0.0.0",
            "port": 4010,
            "mode": "MockAll",
            "proxy_base_url": "",
            "uv": ApiUvState::default(),
            "route_overrides": [{
                "source_key": "https://example.test/openapi.json",
                "method": "Get",
                "path": "/users",
                "enabled": true,
                "response": "Generated",
                "python": {
                    "enabled": true,
                    "prelude": "",
                    "body": "return json_response({})",
                    "timeout_ms": 1000
                },
                "extra_input_fields": [],
                "extra_output_fields": []
            }],
            "manual_routes": []
        });
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .expect("write legacy");

        let loaded = load_api_mocks_from(&path);
        let script = loaded.route_overrides[0].python.as_ref().expect("script");

        assert!(script.contract.is_empty());
        cleanup_test_path(&path);
    }

    #[test]
    fn new_python_mock_contract_roundtrips() {
        let path = test_persist_path("new-contract");
        let mut state = ApiMockState::default();
        let mut script = default_api_mock_python_script();
        script.contract.query.enabled = true;
        script
            .contract
            .query
            .fields
            .push(crate::app::api_mock::types::ApiMockContractField::new(
                "page",
                crate::app::api_mock::types::ApiMockContractFieldKind::Integer,
                false,
            ));
        state.route_overrides.push(ApiMockRouteOverride {
            source_key: "https://example.test/openapi.json".to_string(),
            method: ApiMethod::Get,
            path: "/users".to_string(),
            enabled: true,
            proxy_when_disabled: false,
            response: ApiMockResponse::Generated,
            python: Some(script),
            extra_input_fields: Vec::new(),
            extra_output_fields: Vec::new(),
        });

        let _ = save_api_mocks_to(&path, &state);
        let loaded = load_api_mocks_from(&path);
        let script = loaded.route_overrides[0].python.as_ref().expect("script");

        assert!(script.contract.query.enabled);
        assert_eq!(script.contract.query.fields[0].name, "page");
        cleanup_test_path(&path);
    }

    #[test]
    fn r3_105_corrupt_mock_state_is_reported_and_backed_up() {
        let path = test_persist_path("r3-corrupt");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&path, "{broken json").unwrap();
        let error = load_api_mocks_from_checked(&path).unwrap_err();
        assert!(error.contains("повреждена"));
        let parent = path.parent().unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy();
        assert!(std::fs::read_dir(parent).unwrap().flatten().any(|entry| {
            entry.file_name().to_string_lossy().starts_with(&format!("{stem}.corrupt-"))
        }));
        cleanup_test_path(&path);
    }

}
