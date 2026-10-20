use std::io;
use std::path::PathBuf;

use crate::domain::error::PackageError;
use crate::domain::package::PackageInfo;
use crate::domain::ports::{PackageManager, PackageSearch};

use super::appliers::command::CommandRunner;

const YAY_HINT: &str = "install it from https://github.com/Jguer/yay#installation";

/// Package search and installation through `yay`, plus `pacman -Ql` for file lists.
pub struct YayPackageManager<R> {
    runner: R,
}

impl<R: CommandRunner> YayPackageManager<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: CommandRunner> PackageSearch for YayPackageManager<R> {
    fn search(&self, term: &str) -> Result<Vec<PackageInfo>, PackageError> {
        let out = self
            .runner
            .run("yay", &["-Ss", term])
            .map_err(|e| spawn_error("yay", e))?;
        if out.success {
            return Ok(parse_yay_search(&out.stdout));
        }
        // pacman-style tools exit non-zero without a message when nothing matches.
        if out.stdout.trim().is_empty() && out.stderr.is_empty() {
            return Ok(Vec::new());
        }
        Err(PackageError::Command(format!(
            "`yay -Ss {term}` failed: {}",
            out.stderr
        )))
    }
}

impl<R: CommandRunner> PackageManager for YayPackageManager<R> {
    fn ensure_available(&self) -> Result<(), PackageError> {
        let out = self
            .runner
            .run("yay", &["--version"])
            .map_err(|e| spawn_error("yay", e))?;
        if out.success {
            Ok(())
        } else {
            Err(PackageError::Command(format!(
                "`yay --version` failed: {}",
                out.stderr
            )))
        }
    }

    fn install(&self, package: &str) -> Result<(), PackageError> {
        // Never add `--noconfirm`: yay must show the PKGBUILD review and the sudo prompt.
        let ok = self
            .runner
            .run_interactive("yay", &["-S", package])
            .map_err(|e| spawn_error("yay", e))?;
        if ok {
            Ok(())
        } else {
            Err(PackageError::Command(format!(
                "`yay -S {package}` did not complete successfully"
            )))
        }
    }

    fn installed_files(&self, package: &str) -> Result<Vec<PathBuf>, PackageError> {
        let out = self
            .runner
            .run("pacman", &["-Ql", package])
            .map_err(|e| spawn_error("pacman", e))?;
        if out.success {
            Ok(parse_pacman_files(&out.stdout))
        } else {
            Err(PackageError::Command(format!(
                "`pacman -Ql {package}` failed: {}",
                out.stderr
            )))
        }
    }
}

fn spawn_error(tool: &str, err: io::Error) -> PackageError {
    if err.kind() == io::ErrorKind::NotFound {
        let hint = if tool == "yay" {
            YAY_HINT
        } else {
            "make sure it is installed"
        };
        PackageError::ToolMissing {
            tool: tool.to_string(),
            hint: hint.to_string(),
        }
    } else {
        PackageError::Command(format!("cannot run `{tool}`: {err}"))
    }
}

/// Parses `yay -Ss` output: a header line per package followed by indented
/// description lines. Records with an unreadable header are dropped.
pub(super) fn parse_yay_search(output: &str) -> Vec<PackageInfo> {
    let mut packages: Vec<PackageInfo> = Vec::new();
    // False while inside a record whose header could not be read.
    let mut in_record = false;
    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        if line.starts_with(char::is_whitespace) {
            if let (true, Some(last)) = (in_record, packages.last_mut()) {
                if !last.description.is_empty() {
                    last.description.push(' ');
                }
                last.description.push_str(line.trim());
            }
        } else {
            let header = parse_header(line);
            in_record = header.is_some();
            packages.extend(header);
        }
    }
    packages
}

fn parse_header(line: &str) -> Option<PackageInfo> {
    let mut tokens = line.split_whitespace();
    let (repository, name) = tokens.next()?.split_once('/')?;
    let version = tokens.next()?;
    if repository.is_empty() || name.is_empty() {
        return None;
    }
    let rest = tokens.collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    Some(PackageInfo {
        repository: repository.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        description: String::new(),
        installed: rest.contains("installed"),
    })
}

/// Parses `pacman -Ql` output (`<package> <path>` per line).
pub(super) fn parse_pacman_files(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(_, path)| path)
        .filter(|path| path.starts_with('/'))
        .map(|path| PathBuf::from(path.trim_end_matches('/')))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::appliers::command::fake::{Behavior, FakeRunner};

    const YAY_FIXTURE: &str = include_str!("fixtures/yay_ss_cursor.txt");
    const PACMAN_FIXTURE: &str = include_str!("fixtures/pacman_ql_vimix.txt");

    fn manager(runner: &FakeRunner) -> YayPackageManager<FakeRunner> {
        YayPackageManager::new(runner.clone())
    }

    #[test]
    fn parses_the_real_yay_sample() {
        let packages = parse_yay_search(YAY_FIXTURE);
        assert_eq!(packages.len(), 7);
        assert_eq!(
            packages[2],
            PackageInfo {
                repository: "aur".into(),
                name: "pandwaita".into(),
                version: "1.0-1".into(),
                description: "Icon theme based on Adwaita but with 3 cute panda cursors.".into(),
                installed: false,
            }
        );
        let vimix = packages.iter().find(|p| p.installed).unwrap();
        assert_eq!(vimix.name, "vimix-kanagawa-cursors");
        assert_eq!(vimix.version, "2.2.0-1");
        assert_eq!(
            packages[6].name,
            "gnome-shell-extension-peek-top-bar-on-fullscreen-git"
        );
        assert_eq!(packages[6].version, "r44.ff1bead-1");
    }

    #[test]
    fn empty_output_yields_no_packages() {
        assert!(parse_yay_search("").is_empty());
        assert!(parse_yay_search("\n\n  \n").is_empty());
    }

    #[test]
    fn garbled_output_is_skipped_without_panicking() {
        let garbled =
            "\u{0}\u{1}??\n    orphan description\nno-slash 1.0\naur/no-version\n    desc\n";
        assert!(parse_yay_search(garbled).is_empty());
    }

    #[test]
    fn valid_records_survive_garbled_neighbours() {
        let text =
            "junk\n    stray\naur/good 2-1 (+0 0.00) [1d]\n    Good one\naur/bad\n    skipped\n";
        let packages = parse_yay_search(text);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "good");
        assert_eq!(packages[0].description, "Good one");
    }

    #[test]
    fn multi_line_descriptions_are_joined() {
        let text = "aur/foo 1.0-1 (+0 0.00) [1d]\n    first line\n    second line\n";
        assert_eq!(
            parse_yay_search(text)[0].description,
            "first line second line"
        );
    }

    #[test]
    fn package_without_description_has_an_empty_one() {
        let text = "aur/foo 1.0-1\naur/bar 2.0-1\n";
        let packages = parse_yay_search(text);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].description, "");
    }

    #[test]
    fn repository_packages_and_installed_markers_are_recognised() {
        let text = "extra/adwaita-cursors 50.0-1 [Installed]\n    GNOME standard cursors\ncore/x 1-1 [Installed: 0.9-1]\n    d\r\n";
        let packages = parse_yay_search(text);
        assert_eq!(packages[0].repository, "extra");
        assert!(packages[0].installed);
        assert!(packages[1].installed);
        assert_eq!(packages[1].description, "d");
    }

    #[test]
    fn parses_pacman_file_list() {
        let files = parse_pacman_files(PACMAN_FIXTURE);
        assert_eq!(files.len(), 11);
        assert_eq!(files[0], PathBuf::from("/usr"));
        assert_eq!(
            files[4],
            PathBuf::from("/usr/share/icons/vimix-kanagawa-cursors-lotus/cursors")
        );
        assert_eq!(
            files[7],
            PathBuf::from("/usr/share/icons/vimix-kanagawa-cursors-lotus/index.theme")
        );
    }

    #[test]
    fn pacman_parser_keeps_spaces_and_skips_garbage() {
        let text = "pkg /usr/share/icons/My Theme/index.theme\n\nnonsense\npkg relative/path\n";
        assert_eq!(
            parse_pacman_files(text),
            [PathBuf::from("/usr/share/icons/My Theme/index.theme")]
        );
        assert!(parse_pacman_files("").is_empty());
    }

    #[test]
    fn search_runs_yay_ss_and_parses_stdout() {
        let runner = FakeRunner::ok().with_stdout(YAY_FIXTURE);
        let found = manager(&runner).search("cursor").unwrap();
        assert_eq!(found.len(), 7);
        assert_eq!(*runner.calls.borrow(), vec![vec!["yay", "-Ss", "cursor"]]);
    }

    #[test]
    fn search_without_matches_is_empty_not_an_error() {
        let runner = FakeRunner::new(Behavior::ExitNonZero(String::new()));
        assert!(manager(&runner).search("zzz").unwrap().is_empty());
    }

    #[test]
    fn search_failure_carries_stderr() {
        let runner = FakeRunner::new(Behavior::ExitNonZero("network down".into()));
        let err = manager(&runner).search("x").unwrap_err();
        assert!(matches!(err, PackageError::Command(ref m) if m.contains("network down")));
    }

    #[test]
    fn missing_yay_is_reported_with_install_instructions() {
        let runner = FakeRunner::new(Behavior::SpawnError);
        let err = manager(&runner).search("x").unwrap_err();
        assert!(matches!(err, PackageError::ToolMissing { ref tool, .. } if tool == "yay"));
        assert!(err.to_string().contains("github.com/Jguer/yay"));
        let err = manager(&runner).ensure_available().unwrap_err();
        assert!(matches!(err, PackageError::ToolMissing { .. }));
    }

    #[test]
    fn ensure_available_probes_yay_version() {
        let runner = FakeRunner::ok();
        manager(&runner).ensure_available().unwrap();
        assert_eq!(*runner.calls.borrow(), vec![vec!["yay", "--version"]]);
    }

    #[test]
    fn install_runs_exactly_yay_s_package_interactively() {
        let runner = FakeRunner::ok();
        manager(&runner).install("nordzy-cursors").unwrap();
        assert_eq!(
            *runner.interactive_calls.borrow(),
            vec![vec!["yay", "-S", "nordzy-cursors"]]
        );
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn install_failure_is_an_error() {
        let runner = FakeRunner::new(Behavior::ExitNonZero(String::new()));
        let err = manager(&runner).install("p").unwrap_err();
        assert!(matches!(err, PackageError::Command(ref m) if m.contains("yay -S p")));
    }

    #[test]
    fn install_with_missing_yay_is_tool_missing() {
        let runner = FakeRunner::new(Behavior::SpawnError);
        let err = manager(&runner).install("p").unwrap_err();
        assert!(matches!(err, PackageError::ToolMissing { .. }));
    }

    #[test]
    fn installed_files_runs_pacman_ql_captured() {
        let runner = FakeRunner::ok().with_stdout(PACMAN_FIXTURE);
        let files = manager(&runner)
            .installed_files("vimix-kanagawa-cursors")
            .unwrap();
        assert_eq!(files.len(), 11);
        assert_eq!(
            *runner.calls.borrow(),
            vec![vec!["pacman", "-Ql", "vimix-kanagawa-cursors"]]
        );
    }

    #[test]
    fn installed_files_failure_carries_stderr() {
        let runner = FakeRunner::new(Behavior::ExitNonZero("package not found".into()));
        let err = manager(&runner).installed_files("p").unwrap_err();
        assert!(matches!(err, PackageError::Command(ref m) if m.contains("package not found")));
    }
}
