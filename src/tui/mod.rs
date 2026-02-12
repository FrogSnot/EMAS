mod app;
mod ui;

use std::io::Stdout;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::arena::{Arena, ArenaEvent};
use crate::config::Cli;

use app::{App, FieldKind, Screen, FIELD_COUNT};

enum AppEvent {
    Terminal(Event),
    Arena(ArenaEvent),
}

pub async fn run_tui(cli: &Cli) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new(cli);

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    let term_tx = tx.clone();
    let _input_thread = std::thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(ev) = event::read() {
                    if term_tx.send(AppEvent::Terminal(ev)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let result = event_loop(&mut terminal, &mut app, tx, &mut rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    tx: UnboundedSender<AppEvent>,
    rx: &mut UnboundedReceiver<AppEvent>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.start_requested {
            app.start_requested = false;
            app.error_message = None;
            match app.build_config() {
                Ok((config, problem)) => {
                    app.screen = Screen::Running;
                    app.generation = 0;
                    app.team_scores.clear();
                    app.logs.clear();
                    app.best_name.clear();
                    app.best_score = 0.0;
                    app.status = "Starting...".into();

                    let event_tx = tx.clone();
                    tokio::spawn(async move {
                        let arena = Arena::new(config);
                        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();

                        let bridge_tx = event_tx.clone();
                        tokio::spawn(async move {
                            while let Some(ev) = progress_rx.recv().await {
                                if bridge_tx.send(AppEvent::Arena(ev)).is_err() {
                                    break;
                                }
                            }
                        });

                        match arena.run_with_progress(&problem, progress_tx).await {
                            Ok(result) => {
                                let _ =
                                    event_tx.send(AppEvent::Arena(ArenaEvent::Completed(result)));
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(AppEvent::Arena(ArenaEvent::Error(e.to_string())));
                            }
                        }
                    });
                }
                Err(e) => {
                    app.error_message = Some(e.to_string());
                }
            }
        }

        while let Ok(ev) = rx.try_recv() {
            process_event(app, ev);
        }

        if app.should_quit {
            break;
        }

        tokio::select! {
            Some(ev) = rx.recv() => {
                process_event(app, ev);
                while let Ok(ev) = rx.try_recv() {
                    process_event(app, ev);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn process_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Terminal(Event::Key(key)) => {
            if key.kind == KeyEventKind::Press {
                handle_key(app, key);
            }
        }
        AppEvent::Terminal(Event::Resize(_, _)) => {}
        AppEvent::Arena(arena_event) => {
            app.handle_arena_event(arena_event);
        }
        _ => {}
    }
}

fn handle_key(app: &mut App, key: event::KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    match app.screen {
        Screen::Setup => handle_setup_key(app, key),
        Screen::Running => handle_running_key(app, key),
        Screen::Results => handle_results_key(app, key),
    }
}

fn handle_setup_key(app: &mut App, key: event::KeyEvent) {
    if app.editing {
        handle_editing_key(app, key);
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Tab | KeyCode::Down => {
            app.error_message = None;
            app.selected_field = (app.selected_field + 1) % FIELD_COUNT;
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.error_message = None;
            app.selected_field = if app.selected_field == 0 {
                FIELD_COUNT - 1
            } else {
                app.selected_field - 1
            };
        }
        KeyCode::Enter => {
            let field = &app.fields[app.selected_field];
            match &field.kind {
                FieldKind::Select { .. } => {
                    app.fields[app.selected_field].select_next();
                }
                FieldKind::Text => {
                    app.editing = true;
                    app.cursor = app.fields[app.selected_field].value.len();
                }
            }
        }
        KeyCode::Left => {
            app.fields[app.selected_field].select_prev();
        }
        KeyCode::Right => {
            app.fields[app.selected_field].select_next();
        }
        KeyCode::F(5) => {
            app.start_requested = true;
        }
        _ => {}
    }
}

fn handle_editing_key(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.editing = false;
        }
        KeyCode::Enter => {
            app.editing = false;
            app.selected_field = (app.selected_field + 1) % FIELD_COUNT;
        }
        KeyCode::Tab => {
            app.editing = false;
            app.selected_field = (app.selected_field + 1) % FIELD_COUNT;
        }
        KeyCode::BackTab => {
            app.editing = false;
            app.selected_field = if app.selected_field == 0 {
                FIELD_COUNT - 1
            } else {
                app.selected_field - 1
            };
        }
        KeyCode::Backspace => {
            app.delete_char_back();
        }
        KeyCode::Delete => {
            app.delete_char_forward();
        }
        KeyCode::Left => {
            app.move_cursor_left();
        }
        KeyCode::Right => {
            app.move_cursor_right();
        }
        KeyCode::Home => {
            app.move_cursor_home();
        }
        KeyCode::End => {
            app.move_cursor_end();
        }
        KeyCode::F(5) => {
            app.editing = false;
            app.start_requested = true;
        }
        KeyCode::Char(c) => {
            app.insert_char(c);
        }
        _ => {}
    }
}

fn handle_running_key(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        _ => {}
    }
}

fn handle_results_key(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Char('n') => {
            app.reset_for_new_run();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_offset = app.scroll_offset.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_offset = app.scroll_offset.saturating_add(1);
        }
        KeyCode::PageUp => {
            app.scroll_offset = app.scroll_offset.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.scroll_offset = app.scroll_offset.saturating_add(10);
        }
        KeyCode::Home => {
            app.scroll_offset = 0;
        }
        _ => {}
    }
}
