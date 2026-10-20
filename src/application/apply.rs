use crate::domain::cursor::{CursorTheme, resolve};
use crate::domain::ports::{CursorApplier, CursorRepository};

use super::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Ok,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyEntry {
    pub label: &'static str,
    pub outcome: ApplyOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStatus {
    AllApplied,
    Partial,
    AllFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub theme: CursorTheme,
    pub entries: Vec<ApplyEntry>,
}

impl ApplyReport {
    pub fn status(&self) -> ApplyStatus {
        let ok = self
            .entries
            .iter()
            .filter(|e| e.outcome == ApplyOutcome::Ok)
            .count();
        if ok == self.entries.len() {
            ApplyStatus::AllApplied
        } else if ok == 0 {
            ApplyStatus::AllFailed
        } else {
            ApplyStatus::Partial
        }
    }
}

pub fn apply(
    repo: &dyn CursorRepository,
    appliers: &[Box<dyn CursorApplier>],
    query: &str,
) -> Result<ApplyReport, ApplicationError> {
    let themes = repo.list()?;
    let theme = resolve(&themes, query)?.clone();
    let entries = appliers
        .iter()
        .map(|applier| ApplyEntry {
            label: applier.label(),
            outcome: match applier.apply(&theme) {
                Ok(()) => ApplyOutcome::Ok,
                Err(err) => ApplyOutcome::Failed(err.to_string()),
            },
        })
        .collect();
    Ok(ApplyReport { theme, entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{CallLog, FakeRepository, applier, theme};
    use crate::domain::error::DomainError;

    fn repo() -> FakeRepository {
        FakeRepository::with(vec![theme("Nordzy", Some("nz")), theme("Bibata", None)])
    }

    fn log() -> CallLog {
        CallLog::default()
    }

    #[test]
    fn resolves_by_name() {
        let log = log();
        let appliers = vec![applier("a", false, &log)];
        let report = apply(&repo(), &appliers, "bibata").unwrap();
        assert_eq!(report.theme.name, "Bibata");
        assert_eq!(*log.borrow(), ["a:Bibata"]);
    }

    #[test]
    fn resolves_by_alias() {
        let log = log();
        let appliers = vec![applier("a", false, &log)];
        let report = apply(&repo(), &appliers, "NZ").unwrap();
        assert_eq!(report.theme.name, "Nordzy");
    }

    #[test]
    fn not_found_runs_no_applier() {
        let log = log();
        let appliers = vec![applier("a", false, &log)];
        let err = apply(&repo(), &appliers, "missing").unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::NotFound(_))
        ));
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn runs_every_applier_when_the_first_fails() {
        let log = log();
        let appliers = vec![
            applier("first", true, &log),
            applier("second", false, &log),
            applier("third", false, &log),
        ];
        let report = apply(&repo(), &appliers, "nz").unwrap();
        assert_eq!(log.borrow().len(), 3);
        let labels: Vec<_> = report.entries.iter().map(|e| e.label).collect();
        assert_eq!(labels, ["first", "second", "third"]);
        assert!(matches!(report.entries[0].outcome, ApplyOutcome::Failed(_)));
        assert_eq!(report.entries[1].outcome, ApplyOutcome::Ok);
    }

    #[test]
    fn failure_message_is_kept() {
        let log = log();
        let appliers = vec![applier("x", true, &log)];
        let report = apply(&repo(), &appliers, "nz").unwrap();
        assert_eq!(
            report.entries[0].outcome,
            ApplyOutcome::Failed("command failed: x failed".to_string())
        );
    }

    fn status_for(flags: &[bool]) -> ApplyStatus {
        let log = log();
        let appliers: Vec<_> = flags.iter().map(|f| applier("x", *f, &log)).collect();
        apply(&repo(), &appliers, "nz").unwrap().status()
    }

    #[test]
    fn classifies_all_applied() {
        assert_eq!(status_for(&[false, false]), ApplyStatus::AllApplied);
    }

    #[test]
    fn classifies_partial() {
        assert_eq!(status_for(&[true, false]), ApplyStatus::Partial);
    }

    #[test]
    fn classifies_all_failed() {
        assert_eq!(status_for(&[true, true]), ApplyStatus::AllFailed);
    }

    #[test]
    fn propagates_repository_failure() {
        let err = apply(&FakeRepository::failing(), &[], "nz").unwrap_err();
        assert!(matches!(err, ApplicationError::Repository(_)));
    }
}
