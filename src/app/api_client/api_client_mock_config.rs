impl ApiClientState {
    /// The single commit point for every API Mock config change: persists the config
    /// and hot-updates the running server, so no edit is saved without reaching it.
    pub(crate) fn commit_mock_config(&mut self) {
        self.persist_mock_config();
        self.refresh_mock_server();
    }

    /// Pushes a fresh snapshot to the running server after its non-config inputs
    /// (selected spec, loaded models) changed. Config edits go through `commit_mock_config`.
    pub(crate) fn refresh_mock_server(&mut self) {
        if !self.mock.server.is_live() {
            return;
        }
        let snapshot = self.mock_server_snapshot();
        self.mock.server.update_snapshot(snapshot);
    }
}

#[cfg(test)]
mod mock_config_tests {
    use crate::app::api_client::ApiFocus;

    /// Before the commit point, a MockProxyBase edit was saved but never reached the
    /// running server. Like other App-level commits it writes the cfg(test) API files.
    #[test]
    fn mock_proxy_base_commit_reaches_running_server_snapshot() {
        let mut app = crate::app::app_behavior_tests::test_app().expect("test app");
        app.ide_panel.api.mock.bind_host = "127.0.0.1".to_string();
        app.ide_panel.api.mock.port = 0;
        let snapshot = app.ide_panel.api.mock_server_snapshot();
        app.ide_panel.api.mock.server.start(snapshot).expect("start mock server");
        app.ide_panel.api.focused = Some(ApiFocus::MockProxyBase);
        app.ide_panel.api.input_editor.set_text_clean(" http://upstream.test ");

        app.commit_api_focus();

        let running = app.ide_panel.api.mock.server.snapshot().expect("live server");
        assert_eq!(running.proxy_base_url, "http://upstream.test");
        app.ide_panel.api.mock.server.stop();
        let _ = std::fs::remove_file(crate::app::api_mock::persist::api_mocks_path());
    }
}
