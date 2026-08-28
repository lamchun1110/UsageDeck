//! Shared home-directory resolution for local credential and usage paths.

use std::path::{Path, PathBuf};

/// `$HOME` (or `USERPROFILE` on Windows), or an empty path when neither is set.
pub(crate) fn home_directory() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Expands a leading `~` (or `~\` on Windows) against `home`. A bare `~` maps
/// to `home` itself; anything without a tilda prefix is returned as-is.
pub(crate) fn expand_home(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        return home.to_path_buf();
    }
    if let Some(relative) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        return home.join(relative);
    }
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::{expand_home, home_directory};
    use std::path::Path;

    #[test]
    fn expands_tilde_forms_and_passes_others_through() {
        let home = Path::new("/home/dev");
        assert_eq!(expand_home("~", home), home);
        assert_eq!(
            expand_home("~/.config/usagedeck", home),
            Path::new("/home/dev/.config/usagedeck")
        );
        assert_eq!(
            expand_home("~\\AppData", home),
            Path::new("/home/dev/AppData")
        );
        assert_eq!(expand_home("/etc/config", home), Path::new("/etc/config"));
        assert_eq!(expand_home("", home), Path::new(""));
    }

    #[test]
    fn home_directory_returns_a_path_without_trailing_separators() {
        let home = home_directory();
        assert!(!home.to_string_lossy().ends_with('/'));
    }
}
