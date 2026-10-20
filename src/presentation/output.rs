use crate::application::apply::{ApplyOutcome, ApplyReport};
use crate::application::current::CurrentCursor;
use crate::application::install::InstallReport;
use crate::domain::cursor::CursorTheme;
use crate::domain::package::PackageInfo;

pub const EMPTY_LIST_HINT: &str =
    "No cursor themes registered. Use `cursorswap add <PATH>` to register one.";

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

/// yay-style listing: `repo/name version [installed]` plus an indented description.
pub fn format_search(packages: &[PackageInfo], term: &str) -> String {
    if packages.is_empty() {
        return format!("No cursor themes found for {term:?}.\n");
    }
    let mut out = String::new();
    for p in packages {
        let marker = if p.installed { " [installed]" } else { "" };
        out.push_str(&format!(
            "{}/{} {}{marker}\n",
            p.repository, p.name, p.version
        ));
        if !p.description.is_empty() {
            out.push_str(&format!("    {}\n", p.description));
        }
    }
    out
}

pub fn format_install(report: &InstallReport) -> String {
    let mut out = format!("installed package {}\n", report.package);
    for theme in &report.registered {
        out.push_str(&format!("{}\n", format_added(theme)));
    }
    for skipped in &report.skipped {
        out.push_str(&format!("skipped {} (already registered)\n", skipped.name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::apply::ApplyEntry;
    use crate::application::fakes::theme;
    use crate::application::install::SkippedTheme;

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

    fn package(name: &str, description: &str, installed: bool) -> PackageInfo {
        PackageInfo {
            repository: "aur".into(),
            name: name.into(),
            version: "2.5.0-1".into(),
            description: description.into(),
            installed,
        }
    }

    #[test]
    fn search_lists_name_version_and_description() {
        let packages = [
            package("nordzy-cursors", "Nord cursors", false),
            package("bibata", "Material cursors", true),
        ];
        assert_eq!(
            format_search(&packages, "x"),
            "aur/nordzy-cursors 2.5.0-1\n    Nord cursors\n\
             aur/bibata 2.5.0-1 [installed]\n    Material cursors\n"
        );
    }

    #[test]
    fn search_omits_an_empty_description_line() {
        let out = format_search(&[package("p", "", false)], "x");
        assert_eq!(out, "aur/p 2.5.0-1\n");
    }

    #[test]
    fn search_without_results_names_the_term() {
        let out = format_search(&[], "nordzy");
        assert_eq!(out, "No cursor themes found for \"nordzy\".\n");
    }

    #[test]
    fn install_report_lists_added_and_skipped_themes() {
        let report = InstallReport {
            package: "pk".into(),
            registered: vec![theme("Nordzy", Some("nz"))],
            skipped: vec![SkippedTheme {
                name: "Bibata".into(),
                path: "/icons/Bibata".into(),
            }],
        };
        assert_eq!(
            format_install(&report),
            "installed package pk\nadded Nordzy (alias nz)\nskipped Bibata (already registered)\n"
        );
    }
}
