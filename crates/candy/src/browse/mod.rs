//! Full-screen terminal image browser. Only the event loop writes to the tty.
mod app;
mod listing;
mod loader;
mod view;

use crate::{cli::Display, terminal};
use anyhow::{Context, Result, ensure};
use app::{Action, App, Effect, FileInfo, Screen};
use listing::Listing;
use loader::Loader;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::ListState,
};
use std::{
    io::{self, IsTerminal},
    panic,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

enum Message {
    Input(io::Result<Event>),
    Preview {
        generation: u64,
        result: Result<FileInfo, String>,
    },
}

struct Session {
    terminal: DefaultTerminal,
    list: ListState,
}

impl Session {
    fn start() -> Result<Self> {
        let terminal = ratatui::try_init().inspect_err(|_| ratatui::restore())?;
        let session = Self {
            terminal,
            list: ListState::default(),
        };
        // try_init installs a process-wide hook that writes to stdout. A
        // caught decoder panic must become a preview error inside the TUI.
        let hook = panic::take_hook();
        let owner = thread::current().id();
        panic::set_hook(Box::new(move |info| {
            if thread::current().id() == owner {
                hook(info);
            }
        }));
        Ok(session)
    }

    fn screen(&self) -> Result<Screen> {
        let size = self.terminal.size()?;
        Ok(Screen {
            columns: size.width,
            rows: size.height,
        })
    }

    fn draw(&mut self, app: &App) -> Result<()> {
        self.terminal
            .draw(|frame| view::draw(frame, app, &mut self.list))?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        ratatui::restore();
    }
}

struct InputThread {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl InputThread {
    fn start(tx: Sender<Message>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("candy-input".into())
            .spawn(move || {
                let read = panic::catch_unwind(|| -> io::Result<()> {
                    while !stopping.load(Ordering::Acquire) {
                        if event::poll(Duration::from_millis(40))?
                            && tx.send(Message::Input(Ok(event::read()?))).is_err()
                        {
                            break;
                        }
                    }
                    Ok(())
                })
                .unwrap_or_else(|_| Err(io::Error::other("Terminal input reader panicked")));
                if let Err(error) = read {
                    let _ = tx.send(Message::Input(Err(error)));
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for InputThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn key_action(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        return Some(Action::Quit);
    }
    if !(key.modifiers - KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    Some(match key.code {
        KeyCode::Down | KeyCode::Right | KeyCode::Char('j' | 'l' | ' ') => Action::Next,
        KeyCode::Up | KeyCode::Left | KeyCode::Char('k' | 'h') => Action::Previous,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::First,
        KeyCode::End | KeyCode::Char('G') => Action::Last,
        KeyCode::Char('r') => Action::Rescan,
        KeyCode::Esc | KeyCode::Char('q') => Action::Quit,
        _ => return None,
    })
}

pub(crate) fn run(path: &Path, display: Display) -> Result<()> {
    ensure!(
        !matches!(display, Display::Gui),
        "The browser runs in the terminal; use `candy edit <file>` for the editor"
    );
    let (listing, selected) = Listing::open(path)?;
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "The browser needs an interactive terminal"
    );
    if std::env::var_os("TMUX").is_some() {
        terminal::ensure_passthrough()?;
    }
    ensure!(
        matches!(display, Display::Kitty) || terminal::supported(),
        "This terminal cannot show images in the browser (it needs Kitty graphics with Unicode placeholders, as in Ghostty or kitty); use --display kitty to try anyway"
    );

    let mut session = Session::start()?;
    let (tx, rx) = mpsc::channel();
    let _input = InputThread::start(tx.clone())?;
    let loader = Loader::start(tx)?;
    let mut app = App::new(listing, selected);
    let mut effects = app.update(Action::Resize(session.screen()?));
    loop {
        while let Some(effect) = effects.pop() {
            match effect {
                Effect::Request(request) => loader.request(request),
                Effect::Rescan => match Listing::scan(&app.listing.directory) {
                    Ok(listing) => effects.extend(app.update(Action::Rescanned(listing))),
                    Err(error) => app.notice = Some(format!("{error:#}")),
                },
                Effect::Quit => return Ok(()),
            }
        }
        session.draw(&app)?;
        loop {
            match rx.recv().context("Browser workers stopped")? {
                Message::Input(input) => {
                    let action = match input.context("Cannot read terminal input")? {
                        Event::Key(key) => key_action(key),
                        Event::Resize(..) => Some(Action::Resize(session.screen()?)),
                        _ => None,
                    };
                    if let Some(action) = action {
                        effects = app.update(action);
                        break;
                    }
                }
                Message::Preview { generation, result } => {
                    if app.apply_preview(generation, result) {
                        break;
                    }
                }
            }
        }
    }
}
