use super::types::{
    ApiManualRoute, ApiMockMode, ApiMockRouteOverride, ApiMockServerStatus, ApiMockState,
    ApiUvState,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const API_MOCK_PERSIST_VERSION: u32 = 2;
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
            mode: state.mode,
            proxy_base_url: state.proxy_base_url.clone(),
            uv: state.uv.clone(),
            route_overrides: state.route_overrides.clone(),
            manual_routes: state.manual_routes.clone(),
        }
    }
}

impl From<ApiMockPersist> for ApiMockState {
    fn from(saved: ApiMockPersist) -> Self {
        Self {
            enabled: false,
            bind_host: lan_bind_host(&saved.bind_host).to_string(),
            port: saved.port.max(1),
            mode: saved.mode,
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

pub fn load_api_mocks() -> ApiMockState {
    std::fs::read_to_string(api_mocks_path())
        .ok()
        .and_then(|content| serde_json::from_str::<ApiMockPersist>(&content).ok())
        .map(ApiMockState::from)
        .unwrap_or_default()
}

pub fn save_api_mocks(state: &ApiMockState) {
    let saved = ApiMockPersist::from(state);
    let Ok(content) = serde_json::to_string_pretty(&saved) else {
        return;
    };
    if let Some(dir) = api_mocks_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(api_mocks_path(), content);
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
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_default();
        base.join("rriter")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api_client::ApiMethod;
    use crate::app::api_mock::types::{
        ApiMockResponse, ApiPythonRuntimeMode, ApiUvStatus, default_api_mock_python_script,
    };

    #[test]
    fn persist_roundtrip_keeps_routes_and_uv_settings() {
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
        let mut state = ApiMockState {
            bind_host: "0.0.0.0".to_string(),
            port: 4101,
            mode: ApiMockMode::MockSelectedProxyRest,
            proxy_base_url: "https://backend.test".to_string(),
            ..Default::default()
        };
        state.uv.status = ApiUvStatus::Ready;
        state.uv.configured_path = Some(PathBuf::from("/usr/bin/uv"));
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

        save_api_mocks(&state);
        let loaded = load_api_mocks();

        assert!(!loaded.enabled);
        assert_eq!(loaded.bind_host, "0.0.0.0");
        assert_eq!(loaded.port, 4101);
        assert_eq!(loaded.mode, ApiMockMode::MockSelectedProxyRest);
        assert_eq!(loaded.proxy_base_url, "https://backend.test");
        assert_eq!(loaded.uv.status, ApiUvStatus::Ready);
        assert_eq!(loaded.uv.python_version, "3.13");
        assert_eq!(loaded.route_overrides.len(), 1);
        assert_eq!(loaded.manual_routes.len(), 1);

        let _ = std::fs::remove_dir_all(api_mock_data_dir());
    }

    #[test]
    fn custom_python_runtime_config_persists() {
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
        let mut state = ApiMockState::default();
        state.uv.mode = ApiPythonRuntimeMode::CustomPython;
        state.uv.custom_python_path = Some(PathBuf::from("/opt/python/bin/python"));
        state.uv.python_version = "3.12".to_string();

        save_api_mocks(&state);
        let loaded = load_api_mocks();

        assert_eq!(loaded.uv.mode, ApiPythonRuntimeMode::CustomPython);
        assert_eq!(
            loaded.uv.custom_python_path,
            Some(PathBuf::from("/opt/python/bin/python"))
        );
        assert_eq!(loaded.uv.python_version, "3.12");

        let _ = std::fs::remove_dir_all(api_mock_data_dir());
    }

    #[test]
    fn persisted_local_bind_migrates_to_lan() {
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
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
        if let Some(dir) = api_mocks_path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(
            api_mocks_path(),
            serde_json::to_string_pretty(&saved).expect("serialize"),
        )
        .expect("write mock persist");

        let loaded = load_api_mocks();

        assert_eq!(loaded.bind_host, "0.0.0.0");
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
    }

    #[test]
    fn old_python_mock_without_contract_loads_with_empty_contract() {
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
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
        if let Some(dir) = api_mocks_path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(
            api_mocks_path(),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .expect("write legacy");

        let loaded = load_api_mocks();
        let script = loaded.route_overrides[0].python.as_ref().expect("script");

        assert!(script.contract.is_empty());
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
    }

    #[test]
    fn new_python_mock_contract_roundtrips() {
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
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

        save_api_mocks(&state);
        let loaded = load_api_mocks();
        let script = loaded.route_overrides[0].python.as_ref().expect("script");

        assert!(script.contract.query.enabled);
        assert_eq!(script.contract.query.fields[0].name, "page");
        let _ = std::fs::remove_dir_all(api_mock_data_dir());
    }
}
