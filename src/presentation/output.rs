use crate::application::apply::{ApplyOutcome, ApplyReport};
use crate::application::current::CurrentCursor;
use crate::domain::cursor::CursorTheme;

pub const EMPTY_LIST_HINT: &str =
    "No cursor themes registered. Use `cambia-cursor add <PATH>` to register one.";

/// One line per theme; `active` is matched against names case-insensitively.
pub fn format_list(themes: &[CursorTheme], active: Option<&str>) -> String {
    if themes.is_empty() {
        return format!("{EMPTY_LIST_HINT}\n");
    }
    themes
        .iter()
        .map(|t| {
            let marker = if active.is_some_and(|a| a.eq_ignore_ascii_case(&t.name)) {
                '*'
            } else {
                ' '
            };
            let alias = t.alias.as_ref().map_or("-", |a| a.as_str());
            format!("{marker} {}  {alias}  {}\n", t.name, t.path.display())
        })
        .collect()
}

pub fn format_current(current: &CurrentCursor) -> String {
    match current {
        CurrentCursor::Registered(theme) => match &theme.alias {
            Some(alias) => format!("{} (alias {})", theme.name, alias.as_str()),
            None => theme.name.clone(),
        },
        CurrentCursor::Unregistered(name) => format!("{name} (not registered)"),
        CurrentCursor::None => "none".to_string(),
    }
}

pub fn format_added(theme: &CursorTheme) -> String {
    match &theme.alias {
        Some(alias) => format!("added {} (alias {})", theme.name, alias.as_str()),
        None => format!("added {}", theme.name),
    }
}

pub fn format_removed(theme: &CursorTheme) -> String {
    format!("removed {}", theme.name)
}

pub fn format_apply(report: &ApplyReport) -> String {
    let mut out = String::new();
    for entry in &report.entries {
        match &entry.outcome {
            ApplyOutcome::Ok => out.push_str(&format!("ok {}\n", entry.label)),
            ApplyOutcome::Failed(msg) => {
                out.push_str(&format!("failed {}: {msg}\n", entry.label));
            }
        }
    }
    let ok = report
        .entries
        .iter()
        .filter(|e| e.outcome == ApplyOutcome::Ok)
        .count();
    out.push_str(&format!(
        "applied {}: {ok} of {} steps succeeded\n",
        report.theme.name,
        report.entries.len()
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::apply::ApplyEntry;
    use crate::application::fakes::theme;

    #[test]
    fn list_marks_the_active_theme_and_dashes_missing_aliases() {
        let themes = [theme("Nordzy", Some("nz")), theme("Bibata", None)];
        let out = format_list(&themes, Some("bibata"));
        assert_eq!(
            out,
            "  Nordzy  nz  /icons/Nordzy\n* Bibata  -  /icons/Bibata\n"
        );
    }

    #[test]
    fn list_without_active_has_no_marker() {
        let out = format_list(&[theme("Nordzy", None)], None);
        assert_eq!(out, "  Nordzy  -  /icons/Nordzy\n");
    }

    #[test]
    fn empty_list_prints_the_hint() {
        assert_eq!(format_list(&[], None), format!("{EMPTY_LIST_HINT}\n"));
    }

    #[test]
    fn current_variants() {
        let reg = CurrentCursor::Registered(theme("Nordzy", Some("nz")));
        assert_eq!(format_current(&reg), "Nordzy (alias nz)");
        let reg = CurrentCursor::Registered(theme("Bibata", None));
        assert_eq!(format_current(&reg), "Bibata");
        let raw = CurrentCursor::Unregistered("Adwaita".into());
        assert_eq!(format_current(&raw), "Adwaita (not registered)");
        assert_eq!(format_current(&CurrentCursor::None), "none");
    }

    #[test]
    fn added_with_and_without_alias() {
        assert_eq!(
            format_added(&theme("Nordzy", Some("nz"))),
            "added Nordzy (alias nz)"
        );
        assert_eq!(format_added(&theme("Bibata", None)), "added Bibata");
    }

    #[test]
    fn removed_names_the_theme() {
        assert_eq!(format_removed(&theme("Bibata", None)), "removed Bibata");
    }

    fn entry(label: &'static str, failure: Option<&str>) -> ApplyEntry {
        ApplyEntry {
            label,
            outcome: match failure {
                None => ApplyOutcome::Ok,
                Some(m) => ApplyOutcome::Failed(m.to_string()),
            },
        }
    }

    #[test]
    fn apply_report_lists_each_step_and_a_summary() {
        let report = ApplyReport {
            theme: theme("Nordzy", Some("nz")),
            entries: vec![
                entry("icon-path", None),
                entry("hyprctl", Some("no socket")),
            ],
        };
        assert_eq!(
            format_apply(&report),
            "ok icon-path\nfailed hyprctl: no socket\napplied Nordzy: 1 of 2 steps succeeded\n"
        );
    }

    #[test]
    fn apply_report_all_ok() {
        let report = ApplyReport {
            theme: theme("Nordzy", None),
            entries: vec![entry("a", None), entry("b", None)],
        };
        assert!(format_apply(&report).ends_with("applied Nordzy: 2 of 2 steps succeeded\n"));
    }
}
