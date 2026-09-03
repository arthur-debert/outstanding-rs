use clapfig::{Clapfig, SearchPath, TypedBuilder};
use serde::{Deserialize, Serialize};
use standout::TermSettings;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
pub(crate) struct TdooConfig {
    pub(crate) store: Option<String>,
    #[clapfig(default = false)]
    pub(crate) reverse: bool,
    pub(crate) term: TermSettings,
}

impl TdooConfig {
    pub(crate) fn store_path(&self) -> PathBuf {
        match &self.store {
            Some(path) => PathBuf::from(path),
            None => default_store_path(std::env::var_os("HOME"), std::env::var_os("USERPROFILE")),
        }
    }
}

fn default_store_path(home: Option<OsString>, user_profile: Option<OsString>) -> PathBuf {
    home.or(user_profile)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todos.json")
}

pub(crate) fn builder(user_scope: SearchPath) -> TypedBuilder<TdooConfig> {
    Clapfig::typed::<TdooConfig>()
        .app_name("tdoo")
        .search_paths(vec![user_scope.clone(), SearchPath::Cwd])
        .persist_scope("local", SearchPath::Cwd)
        .persist_scope("global", user_scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_store_is_used_as_given() {
        let config = TdooConfig {
            store: Some("custom.json".into()),
            reverse: false,
            term: TermSettings::default(),
        };

        assert_eq!(config.store_path(), PathBuf::from("custom.json"));
    }

    #[test]
    fn home_precedes_windows_user_profile() {
        let path = default_store_path(Some("/home/alice".into()), Some("C:\\Users\\Alice".into()));

        assert_eq!(path, PathBuf::from("/home/alice").join(".todos.json"));
    }

    #[test]
    fn windows_user_profile_is_a_portable_fallback() {
        let path = default_store_path(None, Some("C:\\Users\\Alice".into()));

        assert_eq!(path, PathBuf::from("C:\\Users\\Alice").join(".todos.json"));
    }

    #[test]
    fn missing_home_variables_fall_back_to_current_directory() {
        let path = default_store_path(None, None);

        assert_eq!(path, PathBuf::from(".").join(".todos.json"));
    }
}
