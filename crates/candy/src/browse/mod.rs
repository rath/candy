//! Full-screen terminal image browser. Only the event loop writes to the tty.
mod app;
mod grid;
mod listing;
mod loader;
mod view;

use crate::{cli::Display, terminal};
use anyhow::{Context, Result, ensure};
use app::{Action, App, Effect, Request, Screen};
use grid::{Thumb, Thumbs};
use listing::Listing;
use loader::{Loaded, Loader, Rendered};
use ratatui::{
    DefaultTerminal,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
        execute,
        terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
    },
    widgets::ListState,
};
use std::{
    io::{self, IsTerminal, Write},
    panic,
    path::{Path, PathBuf},
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
        result: Result<Loaded, String>,
    },
    Thumbnail {
        path: PathBuf,
        size: (u32, u32),
        result: Result<Loaded, String>,
    },
}

const HANDOFF_TIMEOUT: Duration = Duration::from_millis(500);
// The browser has no GUI fallback; `r` decodes and sends the preview again.
const HANDOFF_TIMEOUT_ERROR: &str = "Terminal did not read the preview file; check tmux allow-passthrough (use all), then press r to retry";

/// Where an image goes once the terminal has read its file.
enum Target {
    Preview(view::Shown),
    Thumbnail {
        path: PathBuf,
        size: (u32, u32),
        columns: u16,
        rows: u16,
    },
}

struct PendingFile {
    path: tempfile::TempPath,
    deadline: Instant,
    retried: bool,
    control: String,
    id: u32,
    image: Rendered,
    target: Target,
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

/// An image the terminal has been sent, and the file local tmux must still read.
struct Sent {
    id: u32,
    columns: u16,
    rows: u16,
    control: String,
    file: Option<tempfile::TempPath>,
}

struct Session {
    terminal: DefaultTerminal,
    list: ListState,
    list_directory: PathBuf,
    tmux: bool,
    ids: terminal::ImageIds,
    live: Vec<u32>,
    shown: Option<view::Shown>,
    pending: Vec<PendingFile>,
    thumbs: Thumbs,
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
            list_directory: PathBuf::new(),
            tmux,
            ids: terminal::ImageIds::new(),
            live: Vec::new(),
            shown: None,
            pending: Vec::new(),
            thumbs: Thumbs::default(),
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

    /// Drops thumbnails the grid no longer keeps and queues the missing ones.
    fn sync(&mut self, app: &App, loader: &Loader) {
        self.thumbs.retain(app);
        let missing = self.thumbs.missing(app);
        if self.thumbs.changed(&missing) {
            loader.thumbnails(missing);
        }
    }

    fn draw(&mut self, app: &App) -> Result<()> {
        // No preview replaces the large image while the grid is up.
        if app.entry().is_none() || app.in_grid() {
            self.shown = None;
        }
        if self.list_directory != app.listing.directory {
            self.list = ListState::default();
            self.list_directory.clone_from(&app.listing.directory);
        }
        let thumbs = &self.thumbs;
        self.pending.retain(|pending| match &pending.target {
            Target::Preview(placement) => placement.generation == app.generation,
            Target::Thumbnail { path, size, .. } => {
                *size == thumbs.size() && matches!(thumbs.map.get(path), Some(Thumb::Uploading))
            }
        });
        // A frame of placeholder cells is tens of kilobytes. Without
        // synchronized output, the terminal (or tmux) can paint it halfway,
        // showing the top rows of an image or a cell without its diacritics.
        execute!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        self.terminal.draw(|frame| {
            view::draw(frame, app, &mut self.list, self.shown, &self.thumbs);
        })?;
        execute!(self.terminal.backend_mut(), EndSynchronizedUpdate)?;
        // Delete images only after the frame that stopped showing them.
        let shown = self.shown.map(|image| image.id);
        let mut keep: Vec<u32> = self.thumbs.ids().collect();
        keep.extend(shown);
        keep.extend(self.pending.iter().map(|pending| pending.id));
        if self.live.iter().any(|id| !keep.contains(id)) {
            let mut out = io::stdout().lock();
            for &id in self.live.iter().filter(|id| !keep.contains(id)) {
                terminal::delete(&mut out, id, self.tmux)?;
                self.ids.release(id);
            }
            out.flush()?;
            self.live.retain(|id| keep.contains(id));
        }
        Ok(())
    }

    fn send(&mut self, image: &Rendered, cell: (u32, u32)) -> Result<Sent> {
        let (columns, rows) = view::span(image.width, image.height, cell);
        let id = self
            .ids
            .next()
            .context("Too many images on screen; make the terminal smaller")?;
        // Own the ID before the first byte goes out, including partial writes.
        self.live.push(id);
        let control = terminal::placement_control(id, columns.into(), rows.into());
        let mut out = io::stdout().lock();
        let file = terminal::transmit(
            &mut out,
            &image.png,
            &control,
            self.tmux,
            terminal::Transfer::Browser,
        )?;
        out.flush()?;
        Ok(Sent {
            id,
            columns,
            rows,
            control,
            file,
        })
    }

    fn upload(&mut self, app: &mut App, image: Rendered) -> Result<()> {
        let Sent {
            id,
            columns,
            rows,
            control,
            file,
        } = self.send(&image, app.screen.cell)?;
        let placement = view::Shown {
            generation: app.generation,
            id,
            columns,
            rows,
        };
        if let Some(path) = file {
            self.pending.push(PendingFile {
                path,
                deadline: Instant::now() + HANDOFF_TIMEOUT,
                retried: false,
                control,
                id,
                image,
                target: Target::Preview(placement),
            });
        } else {
            self.shown = Some(placement);
            app.apply_preview(app.generation, Ok(image.info));
        }
        Ok(())
    }

    fn accept_thumbnail(
        &mut self,
        app: &App,
        path: PathBuf,
        size: (u32, u32),
        result: Result<Loaded, String>,
    ) {
        if !app.in_grid() || !self.thumbs.wants(app, &path, size) {
            return;
        }
        let thumb = match result {
            Ok(Loaded::Picture(image)) => match self.send(&image, app.screen.cell) {
                Ok(Sent {
                    id,
                    columns,
                    rows,
                    control,
                    file: Some(file),
                }) => {
                    self.pending.push(PendingFile {
                        path: file,
                        deadline: Instant::now() + HANDOFF_TIMEOUT,
                        retried: false,
                        control,
                        id,
                        image,
                        target: Target::Thumbnail {
                            path: path.clone(),
                            size,
                            columns,
                            rows,
                        },
                    });
                    Thumb::Uploading
                }
                Ok(Sent {
                    id,
                    columns,
                    rows,
                    file: None,
                    ..
                }) => Thumb::Ready {
                    id,
                    columns,
                    rows,
                    info: image.info,
                },
                Err(error) => Thumb::Failed(format!("{error:#}")),
            },
            Ok(Loaded::Empty(info)) => Thumb::Empty(info),
            Err(error) => Thumb::Failed(error),
        };
        self.thumbs.map.insert(path, thumb);
    }

    fn finish(&mut self, app: &mut App, pending: PendingFile, result: Result<()>) {
        match pending.target {
            Target::Preview(placement) => {
                self.shown = result.is_ok().then_some(placement);
                app.apply_preview(app.generation, Ok(pending.image.info));
                app.preview_error = result.err().map(|error| format!("{error:#}"));
            }
            Target::Thumbnail {
                path,
                columns,
                rows,
                ..
            } => {
                let thumb = match result {
                    Ok(()) => Thumb::Ready {
                        id: pending.id,
                        columns,
                        rows,
                        info: pending.image.info,
                    },
                    Err(error) => Thumb::Failed(format!("{error:#}")),
                };
                self.thumbs.map.insert(path, thumb);
            }
        }
    }

    /// Receive input and decoded images even while local tmux has PNGs in flight.
    /// A completed or failed handoff needs a frame before an old ID is deleted.
    fn receive(&mut self, rx: &Receiver<Message>, app: &mut App) -> Result<Option<Message>> {
        while !self.pending.is_empty() {
            let mut finished = false;
            let mut index = 0;
            while index < self.pending.len() {
                let result = self.pending[index].poll();
                if matches!(result, Ok(false)) {
                    index += 1;
                    continue;
                }
                let pending = self.pending.remove(index);
                self.finish(app, pending, result.map(drop));
                finished = true;
            }
            if finished {
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
        // frame. Retry cleanup for every ID this session actually issued.
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
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Right | KeyCode::Char('l' | ' ') => Action::Next,
        KeyCode::Left | KeyCode::Char('h') => Action::Previous,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::First,
        KeyCode::End | KeyCode::Char('G') => Action::Last,
        KeyCode::Tab | KeyCode::BackTab => Action::ToggleGrid,
        KeyCode::Backspace => Action::Parent,
        KeyCode::Char('r') => Action::Rescan,
        KeyCode::Char('e') => Action::Edit,
        KeyCode::Enter => Action::Activate,
        KeyCode::Esc => Action::Escape,
        KeyCode::Char('q') => Action::Quit,
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
        session.sync(&app, &loader);
        session.draw(&app)?;
        let Some(message) = session.receive(&rx, &mut app)? else {
            continue;
        };
        // A pasted or repeated key can queue many selections in one frame.
        // Apply them in order, then decode and paint the last visible state.
        // Bound the batch so a continuous input stream still gets frames.
        let mut preview = None;
        let mut thumbnails = Vec::new();
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
                Message::Thumbnail { path, size, result } => thumbnails.push((path, size, result)),
            }
        }
        if let Some((generation, result)) = preview
            && generation == app.generation
        {
            let result = match result {
                Ok(Loaded::Picture(image)) => session
                    .upload(&mut app, image)
                    .map_err(|error| format!("{error:#}")),
                Ok(Loaded::Empty(info)) => {
                    session.shown = None;
                    app.apply_preview(generation, Ok(info));
                    Ok(())
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                session.shown = None;
                app.apply_preview(generation, Err(error));
            }
        }
        // Thumbnails arrive after input, so they land in the grid it shows.
        session.sync(&app, &loader);
        for (path, size, result) in thumbnails {
            session.accept_thumbnail(&app, path, size, result);
        }
    }
}

// Defer the decode request until all currently queued input has been handled.
fn update(app: &mut App, action: Action, request: &mut Option<Request>) -> bool {
    let mut effects = app.update(action);
    while let Some(effect) = effects.pop() {
        match effect {
            Effect::Request(next) => *request = Some(next),
            Effect::OpenEditor(path) => {
                app.notice = crate::launcher::launch_captured(path)
                    .err()
                    .map(|error| format!("{error:#}"));
            }
            Effect::Enter(path) => match Listing::enter(&path) {
                Ok(listing) => effects.extend(app.update(Action::Entered(listing))),
                Err(error) => app.notice = Some(format!("{error:#}")),
            },
            Effect::Rescan => match Listing::scan(&app.listing.directory) {
                Ok(listing) => effects.extend(app.update(Action::Rescanned(listing))),
                Err(error) => app.notice = Some(format!("{error:#}")),
            },
            Effect::Quit => return true,
        }
    }
    false
}
