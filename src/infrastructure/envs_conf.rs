use std::fs;
use std::path::Path;

use super::appliers::DEFAULT_CURSOR_SIZE;

/// Parses one `env = KEY,value` line of a Hyprland config (without its line
/// ending). Returns the key and the byte offset where the value starts.
/// Commented and unrelated lines yield `None`.
pub fn parse_env_line(line: &str) -> Option<(&str, usize)> {
    let rest = line.trim_start().strip_prefix("env")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let (key, after) = rest.split_once(',')?;
    let value = after.trim_start();
    Some((key.trim_end(), line.len() - value.len()))
}

/// Reads `XCURSOR_SIZE` from a Hyprland env file; any failure yields the default.
pub fn read_cursor_size(path: &Path) -> u32 {
    let Ok(content) = fs::read_to_string(path) else {
        return DEFAULT_CURSOR_SIZE;
    };
    content
        .lines()
        .find_map(|line| {
            let (key, at) = parse_env_line(line)?;
            (key == "XCURSOR_SIZE").then(|| line[at..].trim().parse::<u32>().ok())
        })
        .flatten()
        .filter(|size| *size > 0)
        .unwrap_or(DEFAULT_CURSOR_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_line() {
        let line = "env = XCURSOR_THEME,Nordzy";
        let (key, at) = parse_env_line(line).unwrap();
        assert_eq!(key, "XCURSOR_THEME");
        assert_eq!(&line[at..], "Nordzy");
    }

    #[test]
    fn tolerates_spacing_everywhere() {
        let line = "  env=XCURSOR_THEME ,  Nordzy";
        let (key, at) = parse_env_line(line).unwrap();
        assert_eq!(key, "XCURSOR_THEME");
        assert_eq!(&line[at..], "Nordzy");
    }

    #[test]
    fn rejects_comments_and_other_lines() {
        for line in [
            "# env = XCURSOR_THEME,x",
            "",
            "bind = SUPER,Q,exec,x",
            "environment = A,b",
            "env XCURSOR_THEME,x",
        ] {
            assert!(parse_env_line(line).is_none(), "{line:?}");
        }
    }

    #[test]
    fn value_may_be_empty() {
        let line = "env = XCURSOR_THEME,";
        let (_, at) = parse_env_line(line).unwrap();
        assert_eq!(&line[at..], "");
    }

    fn size_in(content: Option<&str>) -> u32 {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        if let Some(c) = content {
            fs::write(&path, c).unwrap();
        }
        read_cursor_size(&path)
    }

    #[test]
    fn reads_the_configured_size() {
        let conf = "env = XCURSOR_THEME,Nordzy\nenv = XCURSOR_SIZE,32\n";
        assert_eq!(size_in(Some(conf)), 32);
    }

    #[test]
    fn tolerates_spacing_and_trailing_blanks() {
        assert_eq!(size_in(Some("env=XCURSOR_SIZE , 48  \n")), 48);
    }

    #[test]
    fn defaults_when_absent() {
        assert_eq!(
            size_in(Some("env = XCURSOR_THEME,x\n")),
            DEFAULT_CURSOR_SIZE
        );
    }

    #[test]
    fn defaults_on_garbage_or_zero() {
        for value in ["big", "", "-3", "0", "2.5", "99999999999"] {
            let conf = format!("env = XCURSOR_SIZE,{value}\n");
            assert_eq!(size_in(Some(&conf)), DEFAULT_CURSOR_SIZE, "{value:?}");
        }
    }

    #[test]
    fn defaults_when_the_file_is_missing() {
        assert_eq!(size_in(None), DEFAULT_CURSOR_SIZE);
    }

    #[test]
    fn ignores_commented_lines() {
        assert_eq!(
            size_in(Some("# env = XCURSOR_SIZE,64\n")),
            DEFAULT_CURSOR_SIZE
        );
    }
}
