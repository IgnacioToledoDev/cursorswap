//! Rendering: a pure function of `App` state.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui_image::Image;

use super::app::{App, Mode};
use super::preview::PreviewEntry;

const EMPTY_STATE: &str = "No cursor themes registered.\nPress a to add one.";
const BROWSE_HELP: &str = "j/k move  Enter apply  a add  d remove  q quit";
const PROMPT_HELP: &str = "Enter confirm  Esc cancel";
const UPDATE_HELP: &str = "y update  n not now  s skip this version";

struct Areas {
    list: Rect,
    preview: Rect,
    status: Rect,
    help: Rect,
}

fn areas(area: Rect) -> Areas {
    let [main, status, help] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let [list, preview] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(main);
    Areas {
        list,
        preview,
        status,
        help,
    }
}

/// Cell size requested for the preview image: half of the bordered pane interior.
pub fn preview_size(area: Rect) -> Size {
    half(Block::bordered().inner(areas(area).preview).as_size())
}

fn half(size: Size) -> Size {
    Size::new(size.width.div_ceil(2), size.height.div_ceil(2))
}

pub fn render(frame: &mut Frame, app: &App) {
    let areas = areas(frame.area());
    render_list(frame, app, areas.list);
    render_preview(frame, app, areas.preview);
    frame.render_widget(Paragraph::new(status_text(app)), areas.status);
    let help = match app.mode {
        Mode::Browse => BROWSE_HELP,
        Mode::UpdatePrompt => UPDATE_HELP,
        _ => PROMPT_HELP,
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::new().add_modifier(Modifier::DIM)),
        areas.help,
    );
    if let (Mode::UpdatePrompt, Some(latest)) = (app.mode, &app.update_offer) {
        render_update_modal(frame, latest, &app.installed);
    }
}

fn render_update_modal(frame: &mut Frame, latest: &str, installed: &str) {
    let area = frame.area();
    let size = Size::new(60.min(area.width), 7.min(area.height));
    let popup = centered(area, size);
    let text = format!(
        "cursorswap {latest} is available (you have {installed}).\n\n\
         Install it now and restart?\n\n\
         [y] update   [n] not now   [s] skip this version"
    );
    let modal = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" Update available "));
    frame.render_widget(Clear, popup);
    frame.render_widget(modal, popup);
}

fn status_text(app: &App) -> String {
    match app.mode {
        Mode::Browse => app.status.clone(),
        Mode::PathInput => format!("Theme directory: {}_", app.input),
        Mode::AliasInput => format!("Alias (optional, empty for none): {}_", app.input),
        Mode::UpdatePrompt => app.status.clone(),
        Mode::ConfirmRemove => match app.selected_theme() {
            Some(theme) => format!("Remove {}? y confirms, any other key cancels", theme.name),
            None => String::new(),
        },
    }
}

fn render_list(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" Themes ");
    if app.themes.is_empty() {
        let empty = Paragraph::new(EMPTY_STATE)
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(empty, area);
        return;
    }
    let items: Vec<ListItem> = app
        .themes
        .iter()
        .map(|theme| {
            let marker = if app.is_active(theme) { '*' } else { ' ' };
            let alias = theme
                .alias
                .as_ref()
                .map_or(String::new(), |a| format!("  [{}]", a.as_str()));
            ListItem::new(format!("{marker} {}{alias}", theme.name))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_symbol("> ")
        .highlight_style(Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.selected_theme() {
        Some(theme) => format!(" {} - {} ", theme.name, theme.path.display()),
        None => " Preview ".to_string(),
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match app.selected_preview() {
        Some(PreviewEntry::Failed(message)) => {
            let text = format!("no preview\n{message}");
            let notice = Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(notice, inner);
        }
        Some(PreviewEntry::Loaded(_)) => {
            if let Some(protocol) = app.current_protocol() {
                let target = centered(inner, protocol.size());
                frame.render_widget(Image::new(protocol), target);
            }
        }
        None => {}
    }
}

fn centered(area: Rect, size: Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui_image::picker::Picker;

    use crate::application::fakes::{
        FakeCurrent, FakeInspector, FakePreviewLoader, FakeRepository, theme,
    };
    use crate::domain::cursor::CursorTheme;
    use crate::domain::ports::{Preview, PreviewFrame};
    use crate::presentation::Services;

    fn two_tone() -> Preview {
        Preview {
            frames: vec![PreviewFrame {
                width: 8,
                height: 8,
                // Two tones so halfblocks need a glyph rather than a plain background.
                rgba: [[255, 0, 0, 255].repeat(32), [0, 0, 255, 255].repeat(32)].concat(),
                delay_ms: 0,
            }],
        }
    }

    fn services(
        themes: Vec<CursorTheme>,
        current: Option<&str>,
        loader: FakePreviewLoader,
    ) -> Services {
        Services {
            repo: Box::new(FakeRepository::with(themes)),
            inspector: Box::new(FakeInspector {
                valid: true,
                name: Some("Dir".to_string()),
            }),
            current: Box::new(FakeCurrent(Ok(current.map(String::from)))),
            preview: Box::new(loader),
            appliers: Vec::new(),
            update: crate::presentation::update::fixture::disabled(),
        }
    }

    fn themes() -> Vec<CursorTheme> {
        vec![theme("Nordzy", Some("nz")), theme("Bibata", None)]
    }

    fn app_with(
        themes: Vec<CursorTheme>,
        current: Option<&str>,
        loader: FakePreviewLoader,
    ) -> (App, Services) {
        let services = services(themes, current, loader);
        let app = App::new(Picker::halfblocks(), &services);
        (app, services)
    }

    fn draw(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let area = Rect::new(0, 0, width, height);
        app.prepare_preview(preview_size(area));
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    fn screen(lines: &[String]) -> String {
        lines.join("\n")
    }

    fn offered_app() -> (App, Services) {
        let (mut app, services) =
            app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        app.installed = "0.1.0".into();
        app.on_update_status(crate::application::update::UpdateStatus::Available {
            latest: "0.2.0".into(),
        });
        (app, services)
    }

    #[test]
    fn the_update_modal_names_both_versions_and_the_choices() {
        let (mut app, _s) = offered_app();
        let text = screen(&draw(&mut app, 100, 24));
        for needle in [
            "Update available",
            "0.2.0",
            "0.1.0",
            "[y] update",
            "[n] not now",
            "[s] skip this version",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in\n{text}");
        }
    }

    #[test]
    fn the_help_line_describes_the_modal_keys() {
        let (mut app, _s) = offered_app();
        let lines = draw(&mut app, 100, 24);
        let help = lines.last().unwrap();
        assert!(help.contains("y update"), "{help}");
        assert!(help.contains("s skip"), "{help}");
    }

    #[test]
    fn no_modal_is_drawn_without_an_offer() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        let text = screen(&draw(&mut app, 100, 24));
        assert!(!text.contains("Update available"), "{text}");
    }

    #[test]
    fn the_list_shows_names_aliases_and_the_active_marker() {
        let (mut app, _s) = app_with(
            themes(),
            Some("bibata"),
            FakePreviewLoader::returning(two_tone()),
        );
        let text = screen(&draw(&mut app, 80, 20));
        assert!(text.contains("Nordzy  [nz]"), "{text}");
        assert!(text.contains("* Bibata"), "{text}");
        assert!(!text.contains("* Nordzy"), "{text}");
    }

    #[test]
    fn the_preview_title_names_the_selected_theme_and_its_path() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        let text = screen(&draw(&mut app, 100, 20));
        assert!(text.contains("Nordzy - /icons/Nordzy"), "{text}");
    }

    #[test]
    fn the_empty_registry_explains_how_to_add_a_theme() {
        let (mut app, _s) = app_with(vec![], None, FakePreviewLoader::returning(two_tone()));
        let text = screen(&draw(&mut app, 80, 20));
        assert!(text.contains("No cursor themes registered."), "{text}");
        assert!(text.contains("Press a to add one."), "{text}");
    }

    #[test]
    fn a_failed_preview_shows_no_preview_and_the_error() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::failing("missing cursor"));
        let text = screen(&draw(&mut app, 100, 20));
        assert!(text.contains("no preview"), "{text}");
        assert!(text.contains("missing cursor"), "{text}");
    }

    #[test]
    fn a_loaded_preview_draws_image_cells_inside_the_pane() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        let lines = draw(&mut app, 100, 20);
        let preview_columns: String = lines[1..19]
            .iter()
            .map(|line| line.chars().skip(42).collect::<String>())
            .collect();
        assert!(
            preview_columns
                .chars()
                .any(|c| matches!(c, '▀' | '▄' | '█')),
            "{preview_columns}"
        );
    }

    #[test]
    fn the_last_line_is_the_key_help() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        let lines = draw(&mut app, 80, 20);
        let help = lines.last().unwrap();
        for key in ["j/k", "Enter apply", "a add", "d remove", "q quit"] {
            assert!(help.contains(key), "{help}");
        }
    }

    #[test]
    fn the_status_line_sits_above_the_help() {
        let (mut app, _s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        app.status = "applied Nordzy: 5 of 5 steps succeeded".to_string();
        let lines = draw(&mut app, 80, 20);
        assert!(lines[18].contains("applied Nordzy"), "{:?}", lines[18]);
    }

    #[test]
    fn prompts_replace_the_status_line() {
        let (mut app, mut s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        app.handle_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &mut s,
        );
        app.handle_key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &mut s,
        );
        let lines = draw(&mut app, 80, 20);
        assert!(lines[18].contains("Theme directory: /_"), "{:?}", lines[18]);
        assert!(lines[19].contains("Esc cancel"), "{:?}", lines[19]);
    }

    #[test]
    fn the_removal_prompt_names_the_theme() {
        let (mut app, mut s) = app_with(themes(), None, FakePreviewLoader::returning(two_tone()));
        app.handle_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            &mut s,
        );
        let lines = draw(&mut app, 80, 20);
        assert!(lines[18].contains("Remove Nordzy?"), "{:?}", lines[18]);
    }

    #[test]
    fn tiny_and_huge_terminals_do_not_panic() {
        for (w, h) in [(0, 0), (1, 1), (10, 3), (3, 10), (40, 2), (300, 80)] {
            let (mut app, _s) = app_with(
                themes(),
                Some("Nordzy"),
                FakePreviewLoader::returning(two_tone()),
            );
            draw(&mut app, w, h);
            let (mut empty, _s) = app_with(vec![], None, FakePreviewLoader::failing("x"));
            draw(&mut empty, w, h);
        }
    }

    #[test]
    fn preview_size_is_half_of_the_bordered_pane_interior() {
        let size = preview_size(Rect::new(0, 0, 100, 20));
        assert_eq!(size, Size::new(29, 8));
        assert_eq!(preview_size(Rect::new(0, 0, 1, 1)), Size::new(0, 0));
    }

    #[test]
    fn half_rounds_up_so_a_non_empty_pane_never_collapses() {
        assert_eq!(half(Size::new(58, 16)), Size::new(29, 8));
        assert_eq!(half(Size::new(57, 15)), Size::new(29, 8));
        assert_eq!(half(Size::new(1, 1)), Size::new(1, 1));
        assert_eq!(half(Size::new(0, 0)), Size::new(0, 0));
        assert_eq!(half(Size::new(1, 0)), Size::new(1, 0));
    }
}
