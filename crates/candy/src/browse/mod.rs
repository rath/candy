//! Full-screen terminal image browser. Only the event loop writes to the tty.
mod app;
mod listing;
mod loader;
mod view;

use crate::{cli::Display, terminal};
use anyhow::{Context, Result, ensure};
use app::{Action, App, Effect, Request, Screen};
use listing::Listing;
use loader::{Loader, Rendered};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::ListState,
};
use std::{
    io::{self, IsTerminal, Write},
    panic,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

enum Message {
    Input(io::Result<Event>),
    Preview {
        generation: u64,
        result: Result<Rendered, String>,
    },
}

const HANDOFF_TIMEOUT: Duration = Duration::from_millis(500);
// The browser has no GUI fallback; `r` decodes and sends the preview again.
const HANDOFF_TIMEOUT_ERROR: &str = "Terminal did not read the preview file; check tmux allow-passthrough (use all), then press r to retry";

struct PendingFile {
    path: tempfile::TempPath,
    deadline: Instant,
    retried: bool,
    control: String,
    placement: view::Shown,
    image: Rendered,
}

impl PendingFile {
    fn poll(&mut self) -> Result<bool> {
        if terminal::file_consumed(&self.path)? {
            return Ok(true);
        }
        if Instant::now() >= self.deadline {
            ensure!(!self.retried, HANDOFF_TIMEOUT_ERROR);
            self.path = terminal::send_file(io::stdout().lock(), &self.image.png, &self.control)?;
            self.deadline = Instant::now() + HANDOFF_TIMEOUT;
            self.retried = true;
        }
        Ok(false)
    }
}

struct Session {
    terminal: DefaultTerminal,
    list: ListState,
    tmux: bool,
    ids: terminal::ImageIds,
    live: Vec<u32>,
    shown: Option<view::Shown>,
    pending: Option<PendingFile>,
}

impl Session {
    fn start(tmux: bool) -> Result<Self> {
        // Indexed foregrounds carry Kitty image IDs. Crossterm suppresses
        // those bytes under NO_COLOR, even though the UI uses default colors.
        ratatui::crossterm::style::force_color_output(true);
        let terminal = ratatui::try_init().inspect_err(|_| ratatui::restore())?;
        let session = Self {
            terminal,
            list: ListState::default(),
            tmux,
            ids: terminal::ImageIds::new(),
            live: Vec::with_capacity(2),
            shown: None,
            pending: None,
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
            cell: terminal::cell_size(&terminal::window_size()),
        })
    }

    fn draw(&mut self, app: &App) -> Result<()> {
        if app.entry().is_none() {
            self.shown = None;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.placement.generation != app.generation)
        {
            self.pending = None;
        }
        self.terminal
            .draw(|frame| view::draw(frame, app, &mut self.list, self.shown))?;
        let shown = self.shown.map(|image| image.id);
        let pending = self.pending.as_ref().map(|p| p.placement.id);
        let keep = |id: &u32| Some(*id) == shown || Some(*id) == pending;
        if self.live.iter().any(|id| !keep(id)) {
            let mut out = io::stdout().lock();
            for id in self.live.iter().filter(|id| !keep(id)) {
                terminal::delete(&mut out, *id, self.tmux)?;
            }
            out.flush()?;
            self.live.retain(keep);
        }
        Ok(())
    }

    fn upload(&mut self, app: &mut App, image: Rendered) -> Result<()> {
        let (columns, rows) = view::span(image.width, image.height, app.screen.cell);
        let mut id = self.ids.next();
        if self.shown.is_some_and(|shown| shown.id == id) {
            // A cancelled file handoff used the other slot without painting it.
            id = self.ids.next();
        }
        // Own the ID before the first byte goes out, including partial writes.
        self.live.push(id);
        let control = terminal::placement_control(id, columns.into(), rows.into());
        let mut out = io::stdout().lock();
        let path = terminal::transmit(
            &mut out,
            &image.png,
            &control,
            self.tmux,
            terminal::Transfer::Browser,
        )?;
        out.flush()?;
        let placement = view::Shown {
            generation: app.generation,
            id,
            columns,
            rows,
        };
        if let Some(path) = path {
            self.pending = Some(PendingFile {
                path,
                deadline: Instant::now() + HANDOFF_TIMEOUT,
                retried: false,
                control,
                placement,
                image,
            });
        } else {
            self.shown = Some(placement);
            app.apply_preview(app.generation, Ok(image.info));
        }
        Ok(())
    }

    /// Receive input and decoded images even while local tmux has a PNG in flight.
    /// A completed or failed handoff needs a frame before the old ID is deleted.
    fn receive(&mut self, rx: &Receiver<Message>, app: &mut App) -> Result<Option<Message>> {
        while let Some(pending) = self.pending.as_mut() {
            let result = pending.poll();
            if !matches!(result, Ok(false)) {
                let pending = self.pending.take().unwrap();
                self.shown = result.is_ok().then_some(pending.placement);
                app.apply_preview(app.generation, Ok(pending.image.info));
                app.preview_error = result.err().map(|error| format!("{error:#}"));
                return Ok(None);
            }
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(message) => return Ok(Some(message)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(error) => return Err(error).context("Browser workers stopped"),
            }
        }
        rx.recv().map(Some).context("Browser workers stopped")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut out = io::stdout().lock();
        // A congested tmux client can discard a delete sent after a large
        // frame. Retry cleanup for both slots this session actually issued.
        let ids = if self.tmux {
            self.ids.issued()
        } else {
            &self.live
        };
        for &id in ids {
            let _ = terminal::delete(&mut out, id, self.tmux);
        }
        let _ = out.flush();
        drop(out);
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
    let tmux = std::env::var_os("TMUX").is_some();
    if tmux {
        terminal::ensure_passthrough()?;
    }
    ensure!(
        matches!(display, Display::Kitty) || terminal::placeholders_supported(),
        "This terminal cannot show images in the browser (it needs Kitty graphics with Unicode placeholders, as in Ghostty or kitty); use --display kitty to try anyway"
    );

    let mut session = Session::start(tmux)?;
    let (tx, rx) = mpsc::channel();
    let _input = InputThread::start(tx.clone())?;
    let loader = Loader::start(tx)?;
    let mut app = App::new(listing, selected);
    let mut request = None;
    update(&mut app, Action::Resize(session.screen()?), &mut request);
    loop {
        if let Some(request) = request.take()
            && request.generation == app.generation
        {
            loader.request(request);
        }
        session.draw(&app)?;
        let Some(message) = session.receive(&rx, &mut app)? else {
            continue;
        };
        // A pasted or repeated key can queue many selections in one frame.
        // Apply them in order, then decode and paint the last visible state.
        // Bound the batch so a continuous input stream still gets frames.
        let mut preview = None;
        for message in std::iter::once(message).chain(rx.try_iter().take(255)) {
            match message {
                Message::Input(input) => {
                    let action = match input.context("Cannot read terminal input")? {
                        Event::Key(key) => key_action(key),
                        Event::Resize(..) => Some(Action::Resize(session.screen()?)),
                        _ => None,
                    };
                    if let Some(action) = action
                        && update(&mut app, action, &mut request)
                    {
                        return Ok(());
                    }
                }
                Message::Preview { generation, result } if generation == app.generation => {
                    preview = Some((generation, result));
                }
                Message::Preview { .. } => {}
            }
        }
        if let Some((generation, result)) = preview
            && generation == app.generation
        {
            let result = result.and_then(|image| {
                session
                    .upload(&mut app, image)
                    .map_err(|error| format!("{error:#}"))
            });
            if let Err(error) = result {
                session.shown = None;
                app.apply_preview(generation, Err(error));
            }
        }
    }
}

// Defer the decode request until all currently queued input has been handled.
fn update(app: &mut App, action: Action, request: &mut Option<Request>) -> bool {
    let mut effects = app.update(action);
    while let Some(effect) = effects.pop() {
        match effect {
            Effect::Request(next) => *request = Some(next),
            Effect::Rescan => match Listing::scan(&app.listing.directory) {
                Ok(listing) => effects.extend(app.update(Action::Rescanned(listing))),
                Err(error) => app.notice = Some(format!("{error:#}")),
            },
            Effect::Quit => return true,
        }
    }
    false
}
