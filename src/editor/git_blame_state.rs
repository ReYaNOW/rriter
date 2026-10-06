use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHeadSnapshot {
    pub repo_root: PathBuf,
    pub repo_key: crate::platform::PathKey,
    pub rel_path: PathBuf,
    pub head_oid: Option<git2::Oid>,
    pub path_in_head: bool,
}
