use super::{
    Message,
    app::{Info, Request},
    listing::Listing,
};
use anyhow::Result;
use std::{
    panic::catch_unwind,
    path::Path,
    sync::{Arc, Condvar, Mutex, mpsc::Sender},
    thread,
};

#[derive(Default)]
struct Mailbox {
    pending: Option<Request>,
    closed: bool,
}

#[derive(Default)]
pub(super) struct Loader {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
}

impl Loader {
    pub fn start(tx: Sender<Message>) -> Result<Self> {
        let loader = Self::default();
        let mailbox = Arc::clone(&loader.mailbox);
        thread::Builder::new()
            .name("candy-preview".into())
            .spawn(move || {
                let (lock, ready) = &*mailbox;
                loop {
                    let mut state = ready
                        .wait_while(lock.lock().unwrap(), |m| m.pending.is_none() && !m.closed)
                        .unwrap();
                    if state.closed {
                        break;
                    }
                    let request = state.pending.take().unwrap();
                    drop(state);
                    let generation = request.generation;
                    let result = catch_unwind(|| {
                        load(
                            &request.path,
                            request.folder,
                            request.max_width,
                            request.max_height,
                        )
                    })
                    .map_err(|_| String::from("Image decoder panicked"))
                    .and_then(|r| r.map_err(|error| format!("{error:#}")));
                    if tx.send(Message::Preview { generation, result }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(loader)
    }

    pub fn request(&self, request: Request) {
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().pending = Some(request);
        ready.notify_one();
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().closed = true;
        ready.notify_one();
    }
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
