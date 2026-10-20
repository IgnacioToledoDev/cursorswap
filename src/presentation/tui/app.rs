//! TUI state and input handling. Pure with respect to the terminal: every
//! side effect goes through the application use cases held by `Services`.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Size;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;

use super::preview::{PreviewCache, PreviewEntry};
use crate::application::add::AddInput;
use crate::application::apply::{ApplyOutcome, ApplyReport, ApplyStatus};
use crate::application::install::{InstallInput, InstallReport};
use crate::application::update::{UpdateStatus, skip_version};
use crate::application::{add, apply, install, list, remove, search};
use crate::domain::cursor::CursorTheme;
use crate::domain::error::ConfirmError;
use crate::domain::package::PackageInfo;
use crate::domain::ports::{Confirm, CursorState};
use crate::domain::update::update_hint;
use crate::presentation::Services;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    PathInput,
    AliasInput,
    ConfirmRemove,
    UpdatePrompt,
    SearchInput,
    SearchResults,
    ConfirmInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Quit,
    /// The user accepted the update to this version.
    Update(String),
    /// The user confirmed installing this AUR package. The caller suspends the
    /// terminal, then calls `App::install_package`.
    Install(String),
}

pub struct App {
    pub themes: Vec<CursorTheme>,
    pub selected: usize,
    pub active: Option<String>,
    pub mode: Mode,
    /// The cursor state previewed for whichever theme is selected.
    pub state: CursorState,
    pub input: String,
    pub status: String,
    /// The newer version offered by the background check, until answered.
    pub update_offer: Option<String>,
    pub installed: String,
    /// AUR search results and the highlighted one.
    pub results: Vec<PackageInfo>,
    pub result_selected: usize,
    pending_path: Option<PathBuf>,
    previews: PreviewCache,
    picker: Picker,
}

impl App {
    pub fn new(picker: Picker, services: &Services) -> App {
        let mut app = App {
            themes: Vec::new(),
            selected: 0,
            active: None,
            mode: Mode::Browse,
            state: CursorState::Default,
            input: String::new(),
            status: String::new(),
            update_offer: None,
            installed: services.update.installed.clone(),
            results: Vec::new(),
            result_selected: 0,
            pending_path: None,
            previews: PreviewCache::default(),
            picker,
        };
        app.refresh(services, None);
        app
    }

    /// Same definition as the CLI `list`: case-insensitive name match.
    pub fn is_active(&self, theme: &CursorTheme) -> bool {
        self.active
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(&theme.name))
    }

    pub fn selected_theme(&self) -> Option<&CursorTheme> {
        self.themes.get(self.selected)
    }

    pub fn selected_preview(&self) -> Option<&PreviewEntry> {
        self.previews.get(&self.selected_theme()?.path, self.state)
    }

    pub fn current_protocol(&self) -> Option<&Protocol> {
        match self.selected_preview()? {
            PreviewEntry::Loaded(loaded) => loaded.current_protocol(),
            PreviewEntry::Failed(_) => None,
        }
    }

    /// Encodes the selected preview's current frame for a pane of `size` cells.
    pub fn prepare_preview(&mut self, size: Size) {
        let Some(path) = self.themes.get(self.selected).map(|t| t.path.clone()) else {
            return;
        };
        if let Some(PreviewEntry::Loaded(loaded)) = self.previews.get_mut(&path, self.state) {
            loaded.prepare(&self.picker, size);
        }
    }

    /// Advances the selected preview's animation; returns whether to redraw.
    pub fn tick(&mut self, elapsed: Duration) -> bool {
        let Some(path) = self.themes.get(self.selected).map(|t| t.path.clone()) else {
            return false;
        };
        match self.previews.get_mut(&path, self.state) {
            Some(PreviewEntry::Loaded(loaded)) => loaded.tick(elapsed),
            _ => false,
        }
    }

    /// Receives the background check result. A modal opens only when nothing
    /// else is in progress; otherwise the offer waits for the next idle moment.
    pub fn on_update_status(&mut self, status: UpdateStatus) {
        match status {
            UpdateStatus::Available { latest } => {
                self.update_offer = Some(latest);
                self.open_pending_offer();
            }
            UpdateStatus::Managed { latest, method } => {
                self.status = format!("cursorswap {latest} is available. {}", update_hint(method));
            }
            _ => {}
        }
    }

    /// Drains the background check channel; returns whether to redraw.
    pub fn poll_update(&mut self, rx: &Receiver<UpdateStatus>) -> bool {
        match rx.try_recv() {
            Ok(status) => {
                self.on_update_status(status);
                true
            }
            Err(_) => false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, services: &mut Services) -> Outcome {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        let outcome = match self.mode {
            Mode::Browse => self.browse_key(key.code, services),
            Mode::PathInput | Mode::AliasInput => self.input_key(key.code, services),
            Mode::ConfirmRemove => self.confirm_key(key.code, services),
            Mode::UpdatePrompt => self.update_key(key.code, services),
            Mode::SearchInput => self.search_input_key(key.code, services),
            Mode::SearchResults => self.results_key(key.code, services),
            Mode::ConfirmInstall => self.confirm_install_key(key.code),
        };
        // An offer that arrived mid-task opens as soon as the task is over.
        self.open_pending_offer();
        outcome
    }

    fn open_pending_offer(&mut self) {
        if self.mode == Mode::Browse && self.update_offer.is_some() {
            self.mode = Mode::UpdatePrompt;
        }
    }

    fn update_key(&mut self, code: KeyCode, services: &mut Services) -> Outcome {
        match code {
            KeyCode::Char('y') => {
                self.mode = Mode::Browse;
                if let Some(version) = self.update_offer.take() {
                    return Outcome::Update(version);
                }
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.update_offer = None;
            }
            KeyCode::Char('s') => {
                self.mode = Mode::Browse;
                if let Some(version) = self.update_offer.take()
                    && let Err(err) = skip_version(services.update.store.as_ref(), &version)
                {
                    self.status = format!("could not remember the choice: {err}");
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn browse_key(&mut self, code: KeyCode, services: &mut Services) -> Outcome {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Outcome::Quit,
            KeyCode::Char('j') | KeyCode::Down => {
                self.select(self.selected.saturating_add(1), services)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.select(self.selected.saturating_sub(1), services)
            }
            KeyCode::Char(']') => self.select_state(self.state.next(), services),
            KeyCode::Char('[') => self.select_state(self.state.prev(), services),
            KeyCode::Enter => self.apply_selected(services),
            KeyCode::Char('a') => {
                self.mode = Mode::PathInput;
                self.input.clear();
                self.status.clear();
            }
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Char('d') if self.selected_theme().is_some() => {
                self.mode = Mode::ConfirmRemove;
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn input_key(&mut self, code: KeyCode, services: &mut Services) -> Outcome {
        match code {
            KeyCode::Esc => self.cancel_prompt(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Enter if self.mode == Mode::PathInput => self.submit_path(),
            KeyCode::Enter => self.submit_alias(services),
            _ => {}
        }
        Outcome::Continue
    }

    fn confirm_key(&mut self, code: KeyCode, services: &mut Services) -> Outcome {
        self.mode = Mode::Browse;
        if code == KeyCode::Char('y') {
            self.remove_selected(services);
        }
        Outcome::Continue
    }

    fn open_search(&mut self) {
        self.mode = Mode::SearchInput;
        self.input.clear();
        self.status.clear();
    }

    fn search_input_key(&mut self, code: KeyCode, services: &Services) -> Outcome {
        match code {
            KeyCode::Esc => self.cancel_prompt(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Enter => self.submit_search(services),
            _ => {}
        }
        Outcome::Continue
    }

    fn submit_search(&mut self, services: &Services) {
        let term = self.input.trim().to_string();
        self.cancel_prompt();
        if term.is_empty() {
            return;
        }
        match search::search(services.searcher.as_ref(), &term) {
            Ok(found) if found.is_empty() => {
                self.status = format!("no cursor themes found for {term}");
            }
            Ok(found) => {
                self.status = format!("{} result(s) for {term}", found.len());
                self.results = found;
                self.result_selected = 0;
                self.mode = Mode::SearchResults;
            }
            Err(err) => self.status = format!("error: {err}"),
        }
    }

    fn results_key(&mut self, code: KeyCode, services: &Services) -> Outcome {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_results(),
            KeyCode::Char('j') | KeyCode::Down => self.select_result(self.result_selected + 1),
            KeyCode::Char('k') | KeyCode::Up => {
                self.select_result(self.result_selected.saturating_sub(1))
            }
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Enter => self.request_install(services),
            _ => {}
        }
        Outcome::Continue
    }

    fn select_result(&mut self, index: usize) {
        if !self.results.is_empty() {
            self.result_selected = index.min(self.results.len() - 1);
        }
    }

    fn close_results(&mut self) {
        self.mode = Mode::Browse;
        self.results.clear();
        self.result_selected = 0;
    }

    /// Checks the installer first, so the user is never asked to confirm
    /// something that cannot run.
    fn request_install(&mut self, services: &Services) {
        if self.results.get(self.result_selected).is_none() {
            return;
        }
        match services.packages.ensure_available() {
            Ok(()) => self.mode = Mode::ConfirmInstall,
            Err(err) => self.status = format!("error: {err}"),
        }
    }

    fn confirm_install_key(&mut self, code: KeyCode) -> Outcome {
        if code != KeyCode::Char('y') {
            self.mode = Mode::SearchResults;
            return Outcome::Continue;
        }
        let package = self
            .results
            .get(self.result_selected)
            .map(|p| p.name.clone());
        self.close_results();
        match package {
            Some(name) => Outcome::Install(name),
            None => Outcome::Continue,
        }
    }

    /// Runs the confirmed install through the same use case as the CLI, then
    /// reloads the registry. The caller has already handed the terminal to the
    /// installer, which prompts the user itself.
    pub fn install_package(&mut self, package: &str, services: &mut Services) {
        let input = InstallInput {
            package: package.to_string(),
            alias: None,
        };
        let result = install::install(
            services.packages.as_ref(),
            &Confirmed,
            services.repo.as_mut(),
            services.inspector.as_ref(),
            input,
        );
        match result {
            Ok(report) => {
                self.status = summarize_install(&report);
                let added = report.registered.last().map(|t| t.name.clone());
                self.refresh(services, added.as_deref());
            }
            Err(err) => {
                self.status = format!("error: {err}");
                self.refresh_keeping_status(services);
            }
        }
    }

    fn refresh_keeping_status(&mut self, services: &Services) {
        let status = std::mem::take(&mut self.status);
        self.refresh(services, None);
        if self.status.is_empty() {
            self.status = status;
        }
    }

    fn select(&mut self, index: usize, services: &Services) {
        if self.themes.is_empty() {
            return;
        }
        self.selected = index.min(self.themes.len() - 1);
        self.load_selected_preview(services);
    }

    fn select_state(&mut self, state: CursorState, services: &Services) {
        self.state = state;
        self.load_selected_preview(services);
    }

    fn load_selected_preview(&mut self, services: &Services) {
        if let Some(theme) = self.themes.get(self.selected) {
            self.previews
                .ensure(theme, self.state, services.preview.as_ref());
        }
    }

    /// Reloads the registry and the active name; keeps the selection on
    /// `select` when given, otherwise clamps the previous index.
    fn refresh(&mut self, services: &Services, select: Option<&str>) {
        match list::list(services.repo.as_ref()) {
            Ok(themes) => self.themes = themes,
            Err(err) => {
                self.themes.clear();
                self.status = format!("error: {err}");
            }
        }
        self.refresh_active(services);
        let wanted = select.and_then(|name| self.themes.iter().position(|t| t.name == name));
        self.selected = wanted.unwrap_or(self.selected);
        self.selected = self.selected.min(self.themes.len().saturating_sub(1));
        self.load_selected_preview(services);
    }

    // The marker is cosmetic: an unreadable config must not hide the list.
    fn refresh_active(&mut self, services: &Services) {
        self.active = services.current.current_name().ok().flatten();
    }

    fn apply_selected(&mut self, services: &Services) {
        let Some(theme) = self.selected_theme() else {
            self.status = "no theme to apply: press a to add one".to_string();
            return;
        };
        let query = theme.name.clone();
        match apply::apply(services.repo.as_ref(), &services.appliers, &query) {
            Ok(report) => {
                self.status = summarize_apply(&report);
                if report.status() == ApplyStatus::AllApplied {
                    self.active = Some(report.theme.name);
                } else {
                    self.refresh_active(services);
                }
            }
            Err(err) => self.status = format!("error: {err}"),
        }
    }

    fn cancel_prompt(&mut self) {
        self.mode = Mode::Browse;
        self.input.clear();
        self.pending_path = None;
    }

    fn submit_path(&mut self) {
        let path = self.input.trim().to_string();
        self.input.clear();
        if path.is_empty() {
            self.cancel_prompt();
            return;
        }
        self.pending_path = Some(PathBuf::from(path));
        self.mode = Mode::AliasInput;
    }

    fn submit_alias(&mut self, services: &mut Services) {
        let alias = self.input.trim().to_string();
        let path = self.pending_path.take();
        self.cancel_prompt();
        let Some(path) = path else { return };
        let input = AddInput {
            path,
            name: None,
            alias: (!alias.is_empty()).then_some(alias),
        };
        match add::add(services.repo.as_mut(), services.inspector.as_ref(), input) {
            Ok(theme) => {
                self.status = format!("added {}", theme.name);
                self.refresh(services, Some(&theme.name));
            }
            Err(err) => self.status = format!("error: {err}"),
        }
    }

    fn remove_selected(&mut self, services: &mut Services) {
        let Some(name) = self.selected_theme().map(|t| t.name.clone()) else {
            return;
        };
        match remove::remove(services.repo.as_mut(), &name) {
            Ok(theme) => {
                self.status = format!("removed {}", theme.name);
                self.refresh(services, None);
            }
            Err(err) => self.status = format!("error: {err}"),
        }
    }
}

/// The TUI asks for confirmation in its own modal, before the terminal is
/// handed over, so the install use case must not ask again.
struct Confirmed;

impl Confirm for Confirmed {
    fn confirm(&self, _prompt: &str) -> Result<bool, ConfirmError> {
        Ok(true)
    }
}

fn summarize_install(report: &InstallReport) -> String {
    let names = |themes: Vec<&str>| themes.join(", ");
    let mut text = format!("installed {}", report.package);
    if !report.registered.is_empty() {
        let added = report.registered.iter().map(|t| t.name.as_str()).collect();
        text.push_str(&format!("; added {}", names(added)));
    }
    if !report.skipped.is_empty() {
        let skipped = report.skipped.iter().map(|t| t.name.as_str()).collect();
        text.push_str(&format!("; already registered: {}", names(skipped)));
    }
    text
}

fn summarize_apply(report: &ApplyReport) -> String {
    let total = report.entries.len();
    let failed: Vec<String> = report
        .entries
        .iter()
        .filter_map(|entry| match &entry.outcome {
            ApplyOutcome::Ok => None,
            ApplyOutcome::Failed(msg) => Some(format!("{}: {msg}", entry.label)),
        })
        .collect();
    let ok = total - failed.len();
    let mut text = format!(
        "applied {}: {ok} of {total} steps succeeded",
        report.theme.name
    );
    if !failed.is_empty() {
        text.push_str(&format!("; failed {}", failed.join(", ")));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::application::fakes::{
        CallLog, FakeCurrent, FakeInspector, FakePackages, FakePreviewLoader, FakeRepository,
        applier, theme,
    };
    use crate::domain::package::PackageInfo;
    use crate::domain::ports::{CursorState, Preview, PreviewFrame};

    struct Harness {
        app: App,
        services: Services,
        calls: CallLog,
        loads: Rc<RefCell<Vec<PathBuf>>>,
        loads_states: Rc<RefCell<Vec<CursorState>>>,
    }

    fn one_frame() -> Preview {
        Preview {
            frames: vec![PreviewFrame {
                width: 2,
                height: 2,
                rgba: vec![255; 16],
                delay_ms: 0,
            }],
        }
    }

    fn harness_with(
        repo: FakeRepository,
        failing_appliers: &[bool],
        current: Option<&str>,
        loader: FakePreviewLoader,
    ) -> Harness {
        let calls = CallLog::default();
        let loads = Rc::clone(&loader.loads);
        let loads_states = Rc::clone(&loader.states);
        let appliers = failing_appliers
            .iter()
            .enumerate()
            .map(|(i, fail)| applier(["alpha", "beta", "gamma"][i], *fail, &calls))
            .collect();
        let (searcher, packages, confirm) = crate::application::fakes::idle_package_services();
        let services = Services {
            repo: Box::new(repo),
            inspector: Box::new(FakeInspector {
                valid: true,
                name: Some("Dir".to_string()),
            }),
            current: Box::new(FakeCurrent(Ok(current.map(String::from)))),
            preview: Box::new(loader),
            appliers,
            searcher,
            packages,
            confirm,
            update: crate::presentation::update::fixture::disabled(),
        };
        let app = App::new(Picker::halfblocks(), &services);
        Harness {
            app,
            services,
            calls,
            loads,
            loads_states,
        }
    }

    fn harness(themes: Vec<CursorTheme>) -> Harness {
        harness_with(
            FakeRepository::with(themes),
            &[false],
            None,
            FakePreviewLoader::returning(one_frame()),
        )
    }

    fn two_themes() -> Vec<CursorTheme> {
        vec![theme("Nordzy", Some("nz")), theme("Bibata", None)]
    }

    fn press(h: &mut Harness, code: KeyCode) -> Outcome {
        h.app
            .handle_key(KeyEvent::new(code, KeyModifiers::NONE), &mut h.services)
    }

    fn ch(h: &mut Harness, c: char) -> Outcome {
        press(h, KeyCode::Char(c))
    }

    fn type_text(h: &mut Harness, text: &str) {
        for c in text.chars() {
            ch(h, c);
        }
    }

    fn registry_names(h: &Harness) -> Vec<String> {
        h.services
            .repo
            .list()
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    // --- update offer ---

    fn update_harness() -> (Harness, crate::presentation::update::fixture::Probe) {
        use crate::presentation::update::fixture::{Options, available, build};
        let mut h = harness(two_themes());
        let (update, probe) = build(Options::with(available())).into_parts();
        h.services.update = update;
        h.app.installed = h.services.update.installed.clone();
        (h, probe)
    }

    fn available_v(version: &str) -> UpdateStatus {
        UpdateStatus::Available {
            latest: version.into(),
        }
    }

    #[test]
    fn the_modal_appears_when_the_check_reports_a_newer_version() {
        let (mut h, _) = update_harness();
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
        assert_eq!(h.app.update_offer.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn keys_work_normally_before_the_modal_appears() {
        let (mut h, _) = update_harness();
        ch(&mut h, 'j');
        assert_eq!(h.app.selected, 1);
        ch(&mut h, 'y');
        ch(&mut h, 's');
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(press(&mut h, KeyCode::Char('d')), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::ConfirmRemove);
    }

    #[test]
    fn yes_requests_the_install_of_the_offered_version() {
        let (mut h, probe) = update_harness();
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(ch(&mut h, 'y'), Outcome::Update("0.2.0".into()));
        assert!(
            probe.installed.borrow().is_empty(),
            "installing is the caller's job"
        );
    }

    #[test]
    fn no_and_escape_dismiss_without_side_effects() {
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            let (mut h, probe) = update_harness();
            h.app.on_update_status(available_v("0.2.0"));
            assert_eq!(press(&mut h, code), Outcome::Continue);
            assert_eq!(h.app.mode, Mode::Browse);
            assert_eq!(h.app.update_offer, None);
            assert_eq!(probe.store.saves.get(), 0);
        }
    }

    #[test]
    fn skip_stores_the_version_and_dismisses() {
        let (mut h, probe) = update_harness();
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(ch(&mut h, 's'), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(probe.store.state.borrow().skipped.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn the_modal_captures_other_keys_but_ctrl_c_still_quits() {
        let (mut h, _) = update_harness();
        h.app.on_update_status(available_v("0.2.0"));
        for c in ['j', 'q', 'a', 'd'] {
            assert_eq!(ch(&mut h, c), Outcome::Continue);
        }
        assert_eq!(h.app.selected, 0);
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(h.app.handle_key(ctrl_c, &mut h.services), Outcome::Quit);
    }

    #[test]
    fn an_offer_waits_while_the_user_is_typing() {
        let (mut h, _) = update_harness();
        ch(&mut h, 'a');
        type_text(&mut h, "/x");
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(h.app.mode, Mode::PathInput);
        assert_eq!(h.app.input, "/x");
        press(&mut h, KeyCode::Esc);
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
    }

    #[test]
    fn a_pending_confirmation_keeps_its_own_y_key() {
        let (mut h, _) = update_harness();
        ch(&mut h, 'd');
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(h.app.mode, Mode::ConfirmRemove);
        assert_eq!(ch(&mut h, 'y'), Outcome::Continue);
        assert_eq!(registry_names(&h), ["Bibata"]);
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
    }

    #[test]
    fn the_modal_does_not_wipe_the_result_of_an_apply() {
        let (mut h, _) = update_harness();
        press(&mut h, KeyCode::Enter);
        let status = h.app.status.clone();
        assert!(status.contains("applied"), "{status}");
        h.app.on_update_status(available_v("0.2.0"));
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
        assert_eq!(h.app.status, status);
        assert_eq!(h.calls.borrow().len(), 1);
    }

    #[test]
    fn managed_installs_get_a_status_notice_instead_of_a_modal() {
        let (mut h, _) = update_harness();
        h.app.on_update_status(UpdateStatus::Managed {
            latest: "0.2.0".into(),
            method: crate::domain::update::InstallMethod::PackageManager,
        });
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.status.contains("0.2.0"), "{}", h.app.status);
        assert!(h.app.status.contains("package manager"), "{}", h.app.status);
    }

    #[test]
    fn quiet_results_change_nothing() {
        let statuses = [
            UpdateStatus::UpToDate {
                latest: "0.1.0".into(),
            },
            UpdateStatus::Skipped {
                latest: "0.2.0".into(),
            },
            UpdateStatus::Unavailable("offline".into()),
        ];
        for status in statuses {
            let (mut h, _) = update_harness();
            h.app.on_update_status(status);
            assert_eq!(h.app.mode, Mode::Browse);
            assert!(h.app.status.is_empty());
            assert_eq!(h.app.update_offer, None);
        }
    }

    #[test]
    fn polling_picks_up_the_background_result_without_blocking() {
        use std::sync::mpsc;
        let (mut h, _) = update_harness();
        let (tx, rx) = mpsc::channel();
        assert!(!h.app.poll_update(&rx));
        assert_eq!(h.app.mode, Mode::Browse);
        tx.send(available_v("0.2.0")).unwrap();
        assert!(h.app.poll_update(&rx));
        assert_eq!(h.app.mode, Mode::UpdatePrompt);
        drop(tx);
        assert!(!h.app.poll_update(&rx));
    }

    // --- startup ---

    #[test]
    fn starts_with_the_registered_themes_and_the_first_selected() {
        let h = harness(two_themes());
        assert_eq!(h.app.themes.len(), 2);
        assert_eq!(h.app.selected, 0);
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(h.app.selected_theme().unwrap().name, "Nordzy");
    }

    #[test]
    fn the_active_theme_is_matched_case_insensitively() {
        let h = harness_with(
            FakeRepository::with(two_themes()),
            &[false],
            Some("bibata"),
            FakePreviewLoader::returning(one_frame()),
        );
        assert!(!h.app.is_active(&h.app.themes[0]));
        assert!(h.app.is_active(&h.app.themes[1]));
    }

    #[test]
    fn an_unreadable_registry_is_reported_in_the_status() {
        let h = harness_with(
            FakeRepository::failing(),
            &[false],
            None,
            FakePreviewLoader::returning(one_frame()),
        );
        assert!(h.app.themes.is_empty());
        assert!(h.app.status.contains("boom"), "{}", h.app.status);
    }

    // --- navigation ---

    #[test]
    fn down_and_up_move_the_selection() {
        let mut h = harness(two_themes());
        press(&mut h, KeyCode::Down);
        assert_eq!(h.app.selected, 1);
        press(&mut h, KeyCode::Up);
        assert_eq!(h.app.selected, 0);
    }

    #[test]
    fn j_and_k_move_the_selection() {
        let mut h = harness(two_themes());
        ch(&mut h, 'j');
        assert_eq!(h.app.selected, 1);
        ch(&mut h, 'k');
        assert_eq!(h.app.selected, 0);
    }

    #[test]
    fn the_selection_clamps_at_both_ends() {
        let mut h = harness(two_themes());
        press(&mut h, KeyCode::Up);
        assert_eq!(h.app.selected, 0);
        ch(&mut h, 'j');
        ch(&mut h, 'j');
        ch(&mut h, 'j');
        assert_eq!(h.app.selected, 1);
    }

    #[test]
    fn navigation_keys_are_safe_on_an_empty_list() {
        let mut h = harness(vec![]);
        for code in [
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Enter,
            KeyCode::Char('d'),
        ] {
            assert_eq!(press(&mut h, code), Outcome::Continue);
        }
        assert_eq!(h.app.selected, 0);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.selected_theme().is_none());
    }

    // --- preview loading ---

    #[test]
    fn only_the_selected_theme_is_loaded_at_startup() {
        let h = harness(two_themes());
        assert_eq!(*h.loads.borrow(), [PathBuf::from("/icons/Nordzy")]);
    }

    #[test]
    fn moving_loads_the_new_theme_once() {
        let mut h = harness(two_themes());
        ch(&mut h, 'j');
        ch(&mut h, 'k');
        ch(&mut h, 'j');
        assert_eq!(
            *h.loads.borrow(),
            [
                PathBuf::from("/icons/Nordzy"),
                PathBuf::from("/icons/Bibata")
            ]
        );
    }

    #[test]
    fn a_failed_preview_keeps_navigation_and_apply_working() {
        let mut h = harness_with(
            FakeRepository::with(two_themes()),
            &[false],
            None,
            FakePreviewLoader::failing("no cursor file"),
        );
        assert!(matches!(
            h.app.selected_preview(),
            Some(PreviewEntry::Failed(m)) if m.contains("no cursor file")
        ));
        ch(&mut h, 'j');
        press(&mut h, KeyCode::Enter);
        assert_eq!(*h.calls.borrow(), ["alpha:Bibata"]);
    }

    #[test]
    fn preparing_encodes_the_selected_preview() {
        let mut h = harness(two_themes());
        assert!(h.app.current_protocol().is_none());
        h.app.prepare_preview(Size::new(10, 5));
        assert!(h.app.current_protocol().is_some());
    }

    #[test]
    fn ticking_without_a_selection_does_not_redraw() {
        let mut h = harness(vec![]);
        h.app.prepare_preview(Size::new(10, 5));
        assert!(!h.app.tick(Duration::from_secs(1)));
    }

    #[test]
    fn ticking_a_static_preview_does_not_redraw() {
        let mut h = harness(two_themes());
        assert!(!h.app.tick(Duration::from_secs(1)));
    }

    #[test]
    fn ticking_an_animated_preview_requests_a_redraw() {
        let frames = (0..2)
            .map(|_| PreviewFrame {
                width: 2,
                height: 2,
                rgba: vec![255; 16],
                delay_ms: 100,
            })
            .collect();
        let mut h = harness_with(
            FakeRepository::with(two_themes()),
            &[false],
            None,
            FakePreviewLoader::returning(Preview { frames }),
        );
        assert!(!h.app.tick(Duration::from_millis(50)));
        assert!(h.app.tick(Duration::from_millis(60)));
    }

    // --- cursor states ---

    fn harness_lacking(state: CursorState) -> Harness {
        harness_with(
            FakeRepository::with(two_themes()),
            &[false],
            None,
            FakePreviewLoader::lacking(one_frame(), state),
        )
    }

    fn requested_states(h: &Harness) -> Vec<CursorState> {
        h.loads_states.borrow().clone()
    }

    #[test]
    fn the_default_state_is_selected_at_startup() {
        let h = harness(two_themes());
        assert_eq!(h.app.state, CursorState::Default);
        assert_eq!(requested_states(&h), [CursorState::Default]);
    }

    #[test]
    fn right_bracket_selects_the_next_state_and_loads_it_lazily() {
        let mut h = harness(two_themes());
        ch(&mut h, ']');
        assert_eq!(h.app.state, CursorState::Pointer);
        assert_eq!(
            requested_states(&h),
            [CursorState::Default, CursorState::Pointer]
        );
        assert!(matches!(
            h.app.selected_preview(),
            Some(PreviewEntry::Loaded(_))
        ));
    }

    #[test]
    fn left_bracket_wraps_to_the_last_state() {
        let mut h = harness(two_themes());
        ch(&mut h, '[');
        assert_eq!(h.app.state, CursorState::EwResize);
        ch(&mut h, ']');
        assert_eq!(h.app.state, CursorState::Default);
    }

    #[test]
    fn revisiting_a_state_does_not_reload_it() {
        let mut h = harness(two_themes());
        ch(&mut h, ']');
        ch(&mut h, '[');
        ch(&mut h, ']');
        assert_eq!(h.loads.borrow().len(), 2);
    }

    #[test]
    fn the_selected_state_persists_when_moving_to_another_theme() {
        let mut h = harness(two_themes());
        ch(&mut h, ']');
        ch(&mut h, 'j');
        assert_eq!(h.app.state, CursorState::Pointer);
        assert_eq!(
            requested_states(&h).last().copied(),
            Some(CursorState::Pointer)
        );
        assert_eq!(
            h.loads.borrow().last(),
            Some(&PathBuf::from("/icons/Bibata"))
        );
    }

    #[test]
    fn a_missing_state_is_a_failed_entry_and_keeps_everything_working() {
        let mut h = harness_lacking(CursorState::Pointer);
        ch(&mut h, ']');
        assert!(matches!(
            h.app.selected_preview(),
            Some(PreviewEntry::Failed(_))
        ));
        assert!(h.app.current_protocol().is_none());
        h.app.prepare_preview(Size::new(10, 5));
        assert!(!h.app.tick(Duration::from_secs(1)));
        ch(&mut h, 'j');
        press(&mut h, KeyCode::Enter);
        assert_eq!(*h.calls.borrow(), ["alpha:Bibata"]);
        ch(&mut h, ']');
        assert_eq!(h.app.state, CursorState::Text);
        assert!(matches!(
            h.app.selected_preview(),
            Some(PreviewEntry::Loaded(_))
        ));
    }

    #[test]
    fn the_state_keys_are_text_while_typing_a_prompt() {
        let mut h = harness(two_themes());
        ch(&mut h, 'a');
        ch(&mut h, ']');
        assert_eq!(h.app.input, "]");
        assert_eq!(h.app.state, CursorState::Default);
    }

    #[test]
    fn the_state_keys_do_nothing_without_themes() {
        let mut h = harness(vec![]);
        ch(&mut h, ']');
        assert!(h.loads.borrow().is_empty());
    }

    // --- apply ---

    #[test]
    fn enter_applies_the_selected_theme_and_stays_open() {
        let mut h = harness(two_themes());
        ch(&mut h, 'j');
        assert_eq!(press(&mut h, KeyCode::Enter), Outcome::Continue);
        assert_eq!(*h.calls.borrow(), ["alpha:Bibata"]);
    }

    #[test]
    fn a_full_apply_refreshes_the_active_marker_and_status() {
        let mut h = harness_with(
            FakeRepository::with(two_themes()),
            &[false, false],
            None,
            FakePreviewLoader::returning(one_frame()),
        );
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.active.as_deref(), Some("Nordzy"));
        assert!(h.app.status.contains("Nordzy"), "{}", h.app.status);
        assert!(h.app.status.contains("2 of 2"), "{}", h.app.status);
    }

    #[test]
    fn a_partial_apply_lists_the_failed_steps_and_keeps_the_marker() {
        let mut h = harness_with(
            FakeRepository::with(two_themes()),
            &[true, false, true],
            Some("Bibata"),
            FakePreviewLoader::returning(one_frame()),
        );
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.active.as_deref(), Some("Bibata"));
        assert!(h.app.status.contains("1 of 3"), "{}", h.app.status);
        assert!(h.app.status.contains("alpha"), "{}", h.app.status);
        assert!(h.app.status.contains("gamma"), "{}", h.app.status);
        assert!(!h.app.status.contains("beta"), "{}", h.app.status);
    }

    #[test]
    fn a_repository_error_during_apply_surfaces_in_the_status() {
        let mut h = harness(two_themes());
        h.services.repo = Box::new(FakeRepository::failing());
        press(&mut h, KeyCode::Enter);
        assert!(h.app.status.contains("boom"), "{}", h.app.status);
        assert!(h.calls.borrow().is_empty());
    }

    // --- add flow ---

    #[test]
    fn a_opens_the_path_prompt() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        assert_eq!(h.app.mode, Mode::PathInput);
    }

    #[test]
    fn typing_in_a_prompt_never_triggers_commands() {
        let mut h = harness(two_themes());
        ch(&mut h, 'a');
        assert_eq!(type_and_outcome(&mut h, "qjdk"), Outcome::Continue);
        assert_eq!(h.app.input, "qjdk");
        assert_eq!(h.app.selected, 0);
    }

    fn type_and_outcome(h: &mut Harness, text: &str) -> Outcome {
        let mut last = Outcome::Continue;
        for c in text.chars() {
            last = ch(h, c);
        }
        last
    }

    #[test]
    fn backspace_edits_the_input() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        type_text(&mut h, "/ab");
        press(&mut h, KeyCode::Backspace);
        assert_eq!(h.app.input, "/a");
    }

    #[test]
    fn the_add_flow_registers_a_theme_with_alias_and_selects_it() {
        let mut h = harness(two_themes());
        ch(&mut h, 'a');
        type_text(&mut h, "/themes/Dir");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::AliasInput);
        assert!(h.app.input.is_empty());
        type_text(&mut h, "dr");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(registry_names(&h), ["Nordzy", "Bibata", "Dir"]);
        assert_eq!(h.app.themes.len(), 3);
        assert_eq!(h.app.selected_theme().unwrap().name, "Dir");
        assert_eq!(
            h.app
                .selected_theme()
                .unwrap()
                .alias
                .as_ref()
                .unwrap()
                .as_str(),
            "dr"
        );
        assert!(h.app.status.contains("Dir"), "{}", h.app.status);
    }

    #[test]
    fn an_empty_alias_registers_without_one() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        type_text(&mut h, "/themes/Dir");
        press(&mut h, KeyCode::Enter);
        press(&mut h, KeyCode::Enter);
        assert!(h.app.themes[0].alias.is_none());
    }

    #[test]
    fn an_empty_path_cancels_the_prompt() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(registry_names(&h).is_empty());
    }

    #[test]
    fn an_invalid_theme_directory_is_reported_in_the_status() {
        let mut h = harness(vec![]);
        h.services.inspector = Box::new(FakeInspector {
            valid: false,
            name: None,
        });
        ch(&mut h, 'a');
        type_text(&mut h, "/nope");
        press(&mut h, KeyCode::Enter);
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(
            h.app.status.contains("not a valid cursor theme directory"),
            "{}",
            h.app.status
        );
        assert!(registry_names(&h).is_empty());
    }

    #[test]
    fn a_rejected_alias_is_reported_in_the_status() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        type_text(&mut h, "/themes/Dir");
        press(&mut h, KeyCode::Enter);
        type_text(&mut h, "list");
        press(&mut h, KeyCode::Enter);
        assert!(h.app.status.contains("reserved"), "{}", h.app.status);
        assert!(registry_names(&h).is_empty());
    }

    #[test]
    fn escape_cancels_the_path_prompt_without_quitting() {
        let mut h = harness(two_themes());
        ch(&mut h, 'a');
        type_text(&mut h, "/x");
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.input.is_empty());
    }

    #[test]
    fn escape_cancels_the_alias_prompt_and_adds_nothing() {
        let mut h = harness(vec![]);
        ch(&mut h, 'a');
        type_text(&mut h, "/themes/Dir");
        press(&mut h, KeyCode::Enter);
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(registry_names(&h).is_empty());
    }

    // --- remove flow ---

    #[test]
    fn d_asks_for_confirmation() {
        let mut h = harness(two_themes());
        ch(&mut h, 'd');
        assert_eq!(h.app.mode, Mode::ConfirmRemove);
        assert_eq!(registry_names(&h).len(), 2);
    }

    #[test]
    fn y_confirms_the_removal_and_clamps_the_selection() {
        let mut h = harness(two_themes());
        ch(&mut h, 'j');
        ch(&mut h, 'd');
        ch(&mut h, 'y');
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(registry_names(&h), ["Nordzy"]);
        assert_eq!(h.app.selected, 0);
        assert!(h.app.status.contains("Bibata"), "{}", h.app.status);
    }

    #[test]
    fn removing_the_last_theme_leaves_an_empty_safe_state() {
        let mut h = harness(vec![theme("Only", None)]);
        ch(&mut h, 'd');
        ch(&mut h, 'y');
        assert!(h.app.themes.is_empty());
        assert!(h.app.selected_theme().is_none());
    }

    #[test]
    fn any_other_key_cancels_the_removal() {
        let mut h = harness(two_themes());
        ch(&mut h, 'd');
        ch(&mut h, 'n');
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(registry_names(&h).len(), 2);
    }

    #[test]
    fn escape_cancels_the_removal_without_quitting() {
        let mut h = harness(two_themes());
        ch(&mut h, 'd');
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(registry_names(&h).len(), 2);
    }

    #[test]
    fn a_failing_removal_is_reported_in_the_status() {
        let mut h = harness(two_themes());
        ch(&mut h, 'd');
        h.services.repo = Box::new(FakeRepository::failing());
        ch(&mut h, 'y');
        assert!(h.app.status.contains("boom"), "{}", h.app.status);
        assert_eq!(h.app.mode, Mode::Browse);
    }

    // --- AUR search and install ---

    fn cursor_pkg(name: &str, description: &str) -> PackageInfo {
        PackageInfo {
            repository: "aur".into(),
            name: name.into(),
            version: "1.0-1".into(),
            description: description.into(),
            installed: false,
        }
    }

    const DIR_FILES: [&str; 1] = ["/usr/share/icons/Dir/index.theme"];

    /// Swaps in package fakes built by `fake`, sharing the harness call log.
    fn packages_with(h: &mut Harness, fake: impl Fn(&CallLog) -> FakePackages) {
        h.services.searcher = Box::new(fake(&h.calls));
        h.services.packages = Box::new(fake(&h.calls));
    }

    fn aur_fake(log: &CallLog) -> FakePackages {
        let mut fake = FakePackages::with_files(log, &DIR_FILES);
        fake.results = vec![
            cursor_pkg("nordzy-cursors", "Nord cursors"),
            cursor_pkg("htop", "process viewer"),
            cursor_pkg("bibata-cursor-theme", "Material cursors"),
        ];
        fake
    }

    /// Opens the prompt, searches "nord" and leaves the app on the results.
    fn searched() -> Harness {
        let mut h = harness(two_themes());
        packages_with(&mut h, aur_fake);
        ch(&mut h, '/');
        type_text(&mut h, "nord");
        press(&mut h, KeyCode::Enter);
        h
    }

    /// Confirms the highlighted result and runs the install the way the loop does.
    fn confirm_and_install(h: &mut Harness) {
        press(h, KeyCode::Enter);
        let Outcome::Install(package) = ch(h, 'y') else {
            panic!("expected an install request");
        };
        h.app.install_package(&package, &mut h.services);
    }

    fn log_entries(h: &Harness) -> Vec<String> {
        h.calls.borrow().clone()
    }

    fn installs(h: &Harness) -> usize {
        log_entries(h)
            .iter()
            .filter(|e| e.starts_with("install:"))
            .count()
    }

    #[test]
    fn slash_opens_the_search_prompt() {
        let mut h = harness(two_themes());
        ch(&mut h, '/');
        assert_eq!(h.app.mode, Mode::SearchInput);
        assert!(h.app.input.is_empty());
    }

    #[test]
    fn typing_in_the_search_prompt_never_triggers_commands() {
        let mut h = harness(two_themes());
        ch(&mut h, '/');
        assert_eq!(type_and_outcome(&mut h, "qjd]"), Outcome::Continue);
        assert_eq!(h.app.input, "qjd]");
        assert_eq!(h.app.selected, 0);
    }

    #[test]
    fn escape_and_an_empty_term_leave_the_search_prompt_without_searching() {
        let mut h = harness(two_themes());
        packages_with(&mut h, aur_fake);
        ch(&mut h, '/');
        type_text(&mut h, "x");
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        ch(&mut h, '/');
        type_text(&mut h, "  ");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(log_entries(&h).is_empty());
    }

    #[test]
    fn a_search_lists_only_cursor_like_packages_and_selects_the_first() {
        let h = searched();
        assert_eq!(h.app.mode, Mode::SearchResults);
        let names: Vec<_> = h.app.results.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["nordzy-cursors", "bibata-cursor-theme"]);
        assert_eq!(h.app.result_selected, 0);
        assert!(h.app.status.contains("nord"), "{}", h.app.status);
        assert_eq!(log_entries(&h), ["search:nord"]);
    }

    #[test]
    fn a_search_without_matches_reports_it_and_stays_in_browse() {
        let mut h = harness(two_themes());
        packages_with(&mut h, FakePackages::new);
        ch(&mut h, '/');
        type_text(&mut h, "zzz");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(
            h.app.status.contains("no cursor themes"),
            "{}",
            h.app.status
        );
        assert!(h.app.status.contains("zzz"), "{}", h.app.status);
    }

    #[test]
    fn a_failing_search_is_reported_and_navigation_keeps_working() {
        let mut h = harness(two_themes());
        packages_with(&mut h, |log| {
            let mut fake = FakePackages::new(log);
            fake.search_fails = true;
            fake
        });
        ch(&mut h, '/');
        type_text(&mut h, "nord");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.status.contains("search failed"), "{}", h.app.status);
        ch(&mut h, 'j');
        assert_eq!(h.app.selected, 1);
    }

    #[test]
    fn an_invalid_search_term_is_reported_without_searching() {
        let mut h = harness(two_themes());
        packages_with(&mut h, aur_fake);
        ch(&mut h, '/');
        type_text(&mut h, "-S");
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.status.starts_with("error:"), "{}", h.app.status);
        assert!(log_entries(&h).is_empty());
    }

    #[test]
    fn the_results_move_and_clamp_without_touching_the_theme_list() {
        let mut h = searched();
        press(&mut h, KeyCode::Up);
        assert_eq!(h.app.result_selected, 0);
        ch(&mut h, 'j');
        ch(&mut h, 'j');
        assert_eq!(h.app.result_selected, 1);
        press(&mut h, KeyCode::Up);
        assert_eq!(h.app.result_selected, 0);
        ch(&mut h, 'j');
        ch(&mut h, 'k');
        assert_eq!(h.app.result_selected, 0);
        assert_eq!(h.app.selected, 0);
    }

    #[test]
    fn escape_closes_the_results_without_quitting() {
        let mut h = searched();
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.results.is_empty());
        assert_eq!(h.app.selected_theme().unwrap().name, "Nordzy");
    }

    #[test]
    fn slash_in_the_results_starts_a_new_search() {
        let mut h = searched();
        ch(&mut h, '/');
        assert_eq!(h.app.mode, Mode::SearchInput);
        assert!(h.app.input.is_empty());
    }

    #[test]
    fn enter_on_a_result_asks_for_confirmation_before_installing() {
        let mut h = searched();
        assert_eq!(press(&mut h, KeyCode::Enter), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::ConfirmInstall);
        assert_eq!(installs(&h), 0);
    }

    #[test]
    fn a_missing_installer_is_reported_before_asking_anything() {
        let mut h = searched();
        packages_with(&mut h, |log| {
            let mut fake = aur_fake(log);
            fake.missing_tool = true;
            fake
        });
        press(&mut h, KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::SearchResults);
        assert!(h.app.status.contains("yay"), "{}", h.app.status);
        assert_eq!(installs(&h), 0);
    }

    #[test]
    fn declining_the_installation_returns_to_the_results() {
        let mut h = searched();
        press(&mut h, KeyCode::Enter);
        assert_eq!(ch(&mut h, 'n'), Outcome::Continue);
        assert_eq!(h.app.mode, Mode::SearchResults);
        assert_eq!(h.app.results.len(), 2);
        assert_eq!(installs(&h), 0);
    }

    #[test]
    fn confirming_emits_the_install_of_the_selected_package_without_running_it() {
        let mut h = searched();
        ch(&mut h, 'j');
        press(&mut h, KeyCode::Enter);
        assert_eq!(
            ch(&mut h, 'y'),
            Outcome::Install("bibata-cursor-theme".into())
        );
        assert_eq!(h.app.mode, Mode::Browse);
        assert_eq!(installs(&h), 0, "running yay is the event loop's job");
    }

    #[test]
    fn installing_registers_the_theme_selects_it_and_reports_it() {
        let mut h = searched();
        confirm_and_install(&mut h);
        assert_eq!(registry_names(&h), ["Nordzy", "Bibata", "Dir"]);
        assert_eq!(h.app.themes.len(), 3);
        assert_eq!(h.app.selected_theme().unwrap().name, "Dir");
        assert!(h.app.status.contains("nordzy-cursors"), "{}", h.app.status);
        assert!(h.app.status.contains("Dir"), "{}", h.app.status);
        assert!(
            !log_entries(&h).iter().any(|e| e.starts_with("confirm:")),
            "the TUI already asked"
        );
        assert_eq!(h.app.mode, Mode::Browse);
    }

    #[test]
    fn installing_an_already_registered_theme_says_so() {
        let mut h = harness(vec![theme("Dir", None)]);
        packages_with(&mut h, aur_fake);
        h.app.install_package("nordzy-cursors", &mut h.services);
        assert_eq!(registry_names(&h), ["Dir"]);
        assert!(h.app.status.contains("already"), "{}", h.app.status);
    }

    #[test]
    fn a_failed_install_is_reported_and_keeps_everything_working() {
        let mut h = searched();
        packages_with(&mut h, |log| {
            let mut fake = aur_fake(log);
            fake.install_fails = true;
            fake
        });
        confirm_and_install(&mut h);
        assert!(h.app.status.contains("install failed"), "{}", h.app.status);
        assert_eq!(registry_names(&h), ["Nordzy", "Bibata"]);
        ch(&mut h, 'j');
        assert_eq!(h.app.selected, 1);
    }

    #[test]
    fn a_package_without_a_theme_is_reported() {
        let mut h = searched();
        packages_with(&mut h, |log| {
            let mut fake = aur_fake(log);
            fake.files = Vec::new();
            fake
        });
        h.app.install_package("nordzy-cursors", &mut h.services);
        assert!(h.app.status.contains("no cursor theme"), "{}", h.app.status);
    }

    #[test]
    fn ctrl_c_quits_from_the_search_modes() {
        for setup in ["/", "/nord\n", "/nord\n\n"] {
            let mut h = harness(two_themes());
            packages_with(&mut h, aur_fake);
            for c in setup.chars() {
                match c {
                    '\n' => press(&mut h, KeyCode::Enter),
                    c => ch(&mut h, c),
                };
            }
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(
                h.app.handle_key(key, &mut h.services),
                Outcome::Quit,
                "after {setup:?}"
            );
        }
    }

    // --- quitting ---

    #[test]
    fn q_quits_in_browse_mode() {
        let mut h = harness(two_themes());
        assert_eq!(ch(&mut h, 'q'), Outcome::Quit);
    }

    #[test]
    fn escape_quits_in_browse_mode() {
        let mut h = harness(two_themes());
        assert_eq!(press(&mut h, KeyCode::Esc), Outcome::Quit);
    }

    #[test]
    fn ctrl_c_quits_from_every_mode() {
        for setup in ["", "a", "ad", "d"] {
            let mut h = harness(two_themes());
            for c in setup.chars() {
                ch(&mut h, c);
            }
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(
                h.app.handle_key(key, &mut h.services),
                Outcome::Quit,
                "after {setup:?}"
            );
        }
    }
}
