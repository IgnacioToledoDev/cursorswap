//! The `update_check` switch in `<config dir>/config.toml`.

use std::path::Path;

/// Returns false only when the file explicitly sets `update_check = false`.
/// A missing, unreadable or malformed file keeps the check enabled.
pub fn update_check_enabled(config_file: &Path) -> bool {
    let value = std::fs::read_to_string(config_file)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("update_check").and_then(toml::Value::as_bool));
    value != Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn enabled_for(content: Option<&str>) -> bool {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        if let Some(content) = content {
            std::fs::write(&path, content).unwrap();
        }
        update_check_enabled(&path)
    }

    #[test]
    fn explicit_false_disables() {
        assert!(!enabled_for(Some("update_check = false\n")));
        assert!(!enabled_for(Some("other = 1\nupdate_check = false\n")));
    }

    #[test]
    fn everything_else_keeps_it_enabled() {
        assert!(enabled_for(None));
        assert!(enabled_for(Some("")));
        assert!(enabled_for(Some("update_check = true\n")));
        assert!(enabled_for(Some("update_check = \"false\"\n")));
        assert!(enabled_for(Some("[[[[ broken")));
        assert!(enabled_for(Some("other = 1\n")));
    }
}
