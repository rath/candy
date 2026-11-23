use super::{
    Message,
    app::{FileInfo, Request},
};
use anyhow::Result;
use std::{
    panic::catch_unwind,
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
                    let result = catch_unwind(|| render(&request))
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

fn render(request: &Request) -> Result<FileInfo> {
    Ok(FileInfo {
        metadata: candy_macos::inspect(&request.path)?,
        file_size: std::fs::metadata(&request.path)?.len(),
    })
}
