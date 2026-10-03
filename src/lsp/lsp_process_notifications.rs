impl LspProcess {
    /// textDocument/didClose
    pub fn notify_close(&mut self, path: &PathBuf) {
        let uri = path_to_uri(path);
        if self.open_uris.remove(&uri) {
            if self.send_command(Cmd::Close { uri: uri.clone() }, "didClose") {
                if self.current_uri.as_deref() == Some(uri.as_str()) {
                    self.current_uri = self.open_uris.iter().next().cloned();
                }
                if self.open_uris.is_empty() {
                    self.open_file_data = None;
                }
            }
        }
    }

    pub fn notify_saved(&mut self, path: &PathBuf) {
        let uri = path_to_uri(path);
        if self.open_uris.contains(&uri) {
            let _ = self.send_command(Cmd::Save { uri }, "didSave");
        }
    }
}
