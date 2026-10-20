use std::path::PathBuf;

use crate::domain::cursor::{Alias, CursorTheme, RESERVED_WORDS};
use crate::domain::package::validate_package_name;
use crate::domain::ports::{Confirm, CursorRepository, PackageManager, ThemeInspector};

use super::add::{AddInput, add};
use super::error::ApplicationError;

pub struct InstallInput {
    pub package: String,
    pub alias: Option<String>,
}

/// A theme directory that was already registered and therefore left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTheme {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub package: String,
    pub registered: Vec<CursorTheme>,
    pub skipped: Vec<SkippedTheme>,
}

/// Installs `input.package` after the user confirms, then registers every cursor
/// theme directory the package owns.
///
/// Input is validated before anything runs. `--alias` needs exactly one theme
/// directory, which is only known once the package is installed; with several
/// the package stays installed and nothing is registered.
pub fn install(
    packages: &dyn PackageManager,
    confirm: &dyn Confirm,
    repo: &mut dyn CursorRepository,
    inspector: &dyn ThemeInspector,
    input: InstallInput,
) -> Result<InstallReport, ApplicationError> {
    validate_package_name(&input.package)?;
    if let Some(alias) = &input.alias {
        Alias::new_checked(alias, &RESERVED_WORDS)?;
    }
    packages.ensure_available()?;

    let question = format!(
        "Install package {} with yay? yay will show the PKGBUILD for review and may ask for your sudo password.",
        input.package
    );
    if !confirm.confirm(&question)? {
        return Err(ApplicationError::Declined);
    }
    packages.install(&input.package)?;

    let files = packages.installed_files(&input.package)?;
    let dirs = theme_dirs(&files, inspector);
    if dirs.is_empty() {
        return Err(ApplicationError::NoThemeInPackage(input.package));
    }
    if input.alias.is_some() && dirs.len() > 1 {
        let paths = dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ApplicationError::AliasNeedsSingleTheme {
            package: input.package,
            count: dirs.len(),
            paths,
        });
    }

    let mut report = InstallReport {
        package: input.package,
        registered: Vec::new(),
        skipped: Vec::new(),
    };
    for dir in dirs {
        let name = inspector.dir_name(&dir).unwrap_or_default();
        let known = repo
            .list()?
            .iter()
            .any(|t| t.path == dir || t.name.eq_ignore_ascii_case(&name));
        if known {
            report.skipped.push(SkippedTheme { name, path: dir });
            continue;
        }
        let theme = add(
            repo,
            inspector,
            AddInput {
                path: dir,
                name: None,
                alias: input.alias.clone(),
            },
        )?;
        report.registered.push(theme);
    }
    Ok(report)
}

/// Directories that own an `index.theme` and that the inspector accepts, in file-list order.
fn theme_dirs(files: &[PathBuf], inspector: &dyn ThemeInspector) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for file in files {
        if file.file_name().is_some_and(|n| n == "index.theme")
            && let Some(dir) = file.parent()
            && !dirs.iter().any(|d| d == dir)
            && inspector.is_valid_theme_dir(dir)
        {
            dirs.push(dir.to_path_buf());
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{
        CallLog, FakeConfirm, FakePackages, FakeRepository, PathInspector, theme,
    };
    use crate::domain::error::{ConfirmError, DomainError, PackageError};

    const NORDZY: &str = "/usr/share/icons/Nordzy-cursors";

    fn one_theme_files() -> Vec<&'static str> {
        vec![
            "/usr",
            "/usr/share/icons/Nordzy-cursors",
            "/usr/share/icons/Nordzy-cursors/cursors",
            "/usr/share/icons/Nordzy-cursors/cursors/left_ptr",
            "/usr/share/icons/Nordzy-cursors/index.theme",
        ]
    }

    fn input(alias: Option<&str>) -> InstallInput {
        InstallInput {
            package: "nordzy-cursors".to_string(),
            alias: alias.map(str::to_string),
        }
    }

    fn valid(dirs: &[&str]) -> PathInspector {
        PathInspector(dirs.iter().map(PathBuf::from).collect())
    }

    struct Setup {
        log: CallLog,
        packages: FakePackages,
        confirm: FakeConfirm,
        repo: FakeRepository,
    }

    fn setup(files: &[&str]) -> Setup {
        let log = CallLog::default();
        Setup {
            packages: FakePackages::with_files(&log, files),
            confirm: FakeConfirm::answering(true, &log),
            repo: FakeRepository::default(),
            log,
        }
    }

    fn run(
        s: &mut Setup,
        inspector: &PathInspector,
        input: InstallInput,
    ) -> Result<InstallReport, ApplicationError> {
        install(&s.packages, &s.confirm, &mut s.repo, inspector, input)
    }

    #[test]
    fn installs_and_registers_a_single_theme() {
        let mut s = setup(&one_theme_files());
        let report = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap();
        assert_eq!(report.package, "nordzy-cursors");
        assert_eq!(report.registered.len(), 1);
        assert_eq!(report.registered[0].name, "Nordzy-cursors");
        assert_eq!(report.registered[0].path, PathBuf::from(NORDZY));
        assert!(report.skipped.is_empty());
        assert_eq!(s.repo.themes, report.registered);
    }

    #[test]
    fn checks_the_tool_then_confirms_then_installs_then_lists_files() {
        let mut s = setup(&one_theme_files());
        run(&mut s, &valid(&[NORDZY]), input(None)).unwrap();
        let log = s.log.borrow();
        assert_eq!(log.len(), 4);
        assert_eq!(log[0], "ensure");
        assert!(log[1].starts_with("confirm:") && log[1].contains("nordzy-cursors"));
        assert_eq!(log[2], "install:nordzy-cursors");
        assert_eq!(log[3], "files:nordzy-cursors");
    }

    #[test]
    fn missing_tool_stops_before_confirming_or_installing() {
        let mut s = setup(&one_theme_files());
        s.packages.missing_tool = true;
        let err = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Package(PackageError::ToolMissing { .. })
        ));
        assert_eq!(*s.log.borrow(), ["ensure"]);
        assert!(s.repo.themes.is_empty());
    }

    #[test]
    fn declined_confirmation_installs_nothing() {
        let mut s = setup(&one_theme_files());
        s.confirm = FakeConfirm::answering(false, &s.log);
        let err = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap_err();
        assert!(matches!(err, ApplicationError::Declined));
        assert_eq!(s.log.borrow().len(), 2);
        assert!(s.repo.themes.is_empty());
    }

    #[test]
    fn non_interactive_confirmation_installs_nothing() {
        let mut s = setup(&one_theme_files());
        s.confirm.answer = Err(ConfirmError::NotInteractive);
        let err = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Confirm(ConfirmError::NotInteractive)
        ));
        assert!(!s.log.borrow().iter().any(|e| e.starts_with("install:")));
    }

    #[test]
    fn invalid_package_name_runs_nothing() {
        let mut s = setup(&one_theme_files());
        let bad = InstallInput {
            package: "--noconfirm".to_string(),
            alias: None,
        };
        let err = run(&mut s, &valid(&[NORDZY]), bad).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::InvalidPackageName(_))
        ));
        assert!(s.log.borrow().is_empty());
    }

    #[test]
    fn invalid_or_reserved_alias_is_rejected_before_anything_runs() {
        for alias in ["install", "search", "too-long-alias", ""] {
            let mut s = setup(&one_theme_files());
            let err = run(&mut s, &valid(&[NORDZY]), input(Some(alias))).unwrap_err();
            assert!(matches!(err, ApplicationError::Domain(_)), "{alias:?}");
            assert!(s.log.borrow().is_empty(), "{alias:?}");
        }
    }

    #[test]
    fn install_failure_registers_nothing_and_skips_the_file_list() {
        let mut s = setup(&one_theme_files());
        s.packages.install_fails = true;
        let err = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Package(PackageError::Command(_))
        ));
        assert!(!s.log.borrow().iter().any(|e| e.starts_with("files:")));
        assert!(s.repo.themes.is_empty());
    }

    #[test]
    fn registers_every_theme_directory_of_a_package() {
        let mut s = setup(&[
            "/usr/share/icons/A/index.theme",
            "/usr/share/icons/A/cursors",
            "/usr/share/icons/B/index.theme",
            "/usr/share/icons/B/cursors",
        ]);
        let inspector = valid(&["/usr/share/icons/A", "/usr/share/icons/B"]);
        let report = run(&mut s, &inspector, input(None)).unwrap();
        let names: Vec<_> = report.registered.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn directories_the_inspector_rejects_are_ignored() {
        let mut s = setup(&[
            "/usr/share/icons/A/index.theme",
            "/usr/share/icons/Plain/index.theme",
        ]);
        let report = run(&mut s, &valid(&["/usr/share/icons/A"]), input(None)).unwrap();
        assert_eq!(report.registered.len(), 1);
        assert_eq!(report.registered[0].name, "A");
    }

    #[test]
    fn a_package_without_a_theme_registers_nothing_and_explains() {
        let mut s = setup(&["/usr/bin/tool", "/usr/share/doc/tool/readme"]);
        let err = run(&mut s, &valid(&[]), input(None)).unwrap_err();
        assert!(matches!(&err, ApplicationError::NoThemeInPackage(p) if p == "nordzy-cursors"));
        assert!(err.to_string().contains("index.theme"));
        assert!(s.repo.themes.is_empty());
    }

    #[test]
    fn a_theme_dir_listed_twice_is_registered_once() {
        let mut s = setup(&[
            "/usr/share/icons/A/index.theme",
            "/usr/share/icons/A/index.theme",
        ]);
        let report = run(&mut s, &valid(&["/usr/share/icons/A"]), input(None)).unwrap();
        assert_eq!(report.registered.len(), 1);
    }

    #[test]
    fn already_registered_path_is_skipped_without_error() {
        let mut s = setup(&one_theme_files());
        let existing = CursorTheme::new("Mine", None, PathBuf::from(NORDZY)).unwrap();
        s.repo = FakeRepository::with(vec![existing.clone()]);
        let report = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap();
        assert!(report.registered.is_empty());
        assert_eq!(
            report.skipped,
            [SkippedTheme {
                name: "Nordzy-cursors".to_string(),
                path: PathBuf::from(NORDZY)
            }]
        );
        assert_eq!(s.repo.themes, [existing]);
    }

    #[test]
    fn already_registered_name_is_skipped_without_error() {
        let mut s = setup(&one_theme_files());
        s.repo = FakeRepository::with(vec![theme("nordzy-CURSORS", None)]);
        let report = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap();
        assert!(report.registered.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(s.repo.themes.len(), 1);
    }

    #[test]
    fn new_themes_are_registered_next_to_skipped_ones() {
        let mut s = setup(&[
            "/usr/share/icons/A/index.theme",
            "/usr/share/icons/B/index.theme",
        ]);
        s.repo = FakeRepository::with(vec![theme("A", None)]);
        let inspector = valid(&["/usr/share/icons/A", "/usr/share/icons/B"]);
        let report = run(&mut s, &inspector, input(None)).unwrap();
        assert_eq!(report.registered.len(), 1);
        assert_eq!(report.registered[0].name, "B");
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn alias_is_applied_to_a_single_theme() {
        let mut s = setup(&one_theme_files());
        let report = run(&mut s, &valid(&[NORDZY]), input(Some("nz"))).unwrap();
        assert_eq!(report.registered[0].alias.as_ref().unwrap().as_str(), "nz");
    }

    #[test]
    fn alias_with_several_themes_is_an_error_and_registers_nothing() {
        let mut s = setup(&[
            "/usr/share/icons/A/index.theme",
            "/usr/share/icons/B/index.theme",
        ]);
        let inspector = valid(&["/usr/share/icons/A", "/usr/share/icons/B"]);
        let err = run(&mut s, &inspector, input(Some("ab"))).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::AliasNeedsSingleTheme { count: 2, .. }
        ));
        assert!(err.to_string().contains("/usr/share/icons/B"));
        assert!(s.repo.themes.is_empty());
    }

    #[test]
    fn registry_failure_is_reported() {
        let mut s = setup(&one_theme_files());
        s.repo = FakeRepository::failing();
        let err = run(&mut s, &valid(&[NORDZY]), input(None)).unwrap_err();
        assert!(matches!(err, ApplicationError::Repository(_)));
    }
}
