use super::{
    Message,
    app::{Info, Request},
    listing::Listing,
};
use anyhow::Result;
use std::{
    collections::VecDeque,
    panic::catch_unwind,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc::Sender},
    thread,
};

pub(super) struct Thumbnail {
    pub path: PathBuf,
    pub folder: bool,
    pub size: (u32, u32),
}

/// The latest preview request, then thumbnails in the order the grid wants
/// them. Each new thumbnail list replaces the queue; paths a worker is
/// already decoding are not queued twice.
#[derive(Default)]
struct Mailbox {
    preview: Option<Request>,
    thumbnails: VecDeque<Thumbnail>,
    busy: Vec<PathBuf>,
    closed: bool,
}

#[derive(Default)]
pub(super) struct Loader {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
}

enum Job {
    Preview(Request),
    Thumbnail(Thumbnail),
}

impl Loader {
    pub fn start(tx: Sender<Message>) -> Result<Self> {
        let loader = Self::default();
        let workers = thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 4));
        for index in 0..workers {
            let mailbox = Arc::clone(&loader.mailbox);
            let tx = tx.clone();
            thread::Builder::new()
                .name(format!("candy-preview-{index}"))
                .spawn(move || work(&mailbox, &tx))?;
        }
        Ok(loader)
    }

    pub fn request(&self, request: Request) {
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().preview = Some(request);
        ready.notify_one();
    }

    pub fn thumbnails(&self, list: Vec<Thumbnail>) {
        let (lock, ready) = &*self.mailbox;
        let mut mailbox = lock.lock().unwrap();
        let Mailbox {
            thumbnails, busy, ..
        } = &mut *mailbox;
        *thumbnails = list
            .into_iter()
            .filter(|thumbnail| !busy.contains(&thumbnail.path))
            .collect();
        ready.notify_all();
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().closed = true;
        ready.notify_all();
    }
}

fn work(mailbox: &(Mutex<Mailbox>, Condvar), tx: &Sender<Message>) {
    let (lock, ready) = mailbox;
    loop {
        let job = {
            let mut state = ready
                .wait_while(lock.lock().unwrap(), |m| {
                    !m.closed && m.preview.is_none() && m.thumbnails.is_empty()
                })
                .unwrap();
            if state.closed {
                return;
            }
            if let Some(request) = state.preview.take() {
                Job::Preview(request)
            } else {
                let thumbnail = state.thumbnails.pop_front().unwrap();
                state.busy.push(thumbnail.path.clone());
                Job::Thumbnail(thumbnail)
            }
        };
        let sent = match job {
            Job::Preview(request) => tx.send(Message::Preview {
                generation: request.generation,
                result: guarded(
                    &request.path,
                    request.folder,
                    request.max_width,
                    request.max_height,
                ),
            }),
            Job::Thumbnail(thumbnail) => {
                let result = guarded(
                    &thumbnail.path,
                    thumbnail.folder,
                    thumbnail.size.0,
                    thumbnail.size.1,
                );
                let path = thumbnail.path.clone();
                // Clear the mark after the result is queued, so a new list
                // cannot ask for this path again before the result arrives.
                let sent = tx.send(Message::Thumbnail {
                    path: thumbnail.path,
                    size: thumbnail.size,
                    result,
                });
                lock.lock().unwrap().busy.retain(|busy| *busy != path);
                sent
            }
        };
        if sent.is_err() {
            return;
        }
    }
}

fn guarded(path: &Path, folder: bool, max_width: u32, max_height: u32) -> Result<Loaded, String> {
    catch_unwind(|| load(path, folder, max_width, max_height))
        .map_err(|_| String::from("Image decoder panicked"))
        .and_then(|r| r.map_err(|error| format!("{error:#}")))
}

pub(super) struct Rendered {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub info: Info,
}

pub(super) enum Loaded {
    Picture(Rendered),
    /// A folder with nothing to show; its counts still go in the status line.
    Empty(Info),
}

fn load(path: &Path, folder: bool, max_width: u32, max_height: u32) -> Result<Loaded> {
    if !folder {
        let preview = candy_macos::load_preview(path, max_width, max_height)?;
        let info = Info::File {
            metadata: preview.metadata,
            file_size: preview.file_size,
        };
        return render(preview.pixels, info).map(Loaded::Picture);
    }
    // A folder shows its first image, the one opening it would select.
    let listing = Listing::scan(path)?;
    let info = Info::Folder {
        images: listing.images().len(),
        folders: listing.folders(),
    };
    let Some(cover) = listing.images().first() else {
        return Ok(Loaded::Empty(info));
    };
    match candy_macos::load_preview(&cover.path, max_width, max_height) {
        Ok(preview) => render(preview.pixels, info).map(Loaded::Picture),
        Err(_) => Ok(Loaded::Empty(info)),
    }
}

fn render(pixels: image::RgbaImage, info: Info) -> Result<Rendered> {
    let (width, height) = pixels.dimensions();
    Ok(Rendered {
        png: crate::terminal::encode_png(pixels)?,
        width,
        height,
        info,
    })
}
