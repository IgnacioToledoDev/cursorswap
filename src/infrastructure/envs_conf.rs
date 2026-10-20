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
}
