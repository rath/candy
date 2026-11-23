use super::listing::{Entry, Listing};
use candy_macos::Metadata;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Screen {
    pub columns: u16,
    pub rows: u16,
    pub cell: (u32, u32),
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            columns: 0,
            rows: 0,
            cell: (8, 16),
        }
    }
}

pub(super) struct FileInfo {
    pub metadata: Metadata,
    pub file_size: u64,
}

pub(super) struct Request {
    pub generation: u64,
    pub path: PathBuf,
    pub max_width: u32,
    pub max_height: u32,
}

pub(super) enum Action {
    Previous,
    Next,
    PageUp,
    PageDown,
    First,
    Last,
    Resize(Screen),
    Rescan,
    Rescanned(Listing),
    ToggleViewer,
    Escape,
    Edit,
    Quit,
}

pub(super) enum Effect {
    Request(Request),
    OpenEditor(PathBuf),
    Rescan,
    Quit,
}

pub(super) struct App {
    pub listing: Listing,
    pub selected: usize,
    pub screen: Screen,
    pub generation: u64,
    pub info: Option<FileInfo>,
    pub preview_error: Option<String>,
    pub notice: Option<String>,
    viewer: bool,
}

impl App {
    pub fn new(listing: Listing, selected: usize) -> Self {
        Self {
            listing,
            selected,
            screen: Screen::default(),
            generation: 0,
            info: None,
            preview_error: None,
            notice: None,
            viewer: false,
        }
    }

    pub fn entry(&self) -> Option<&Entry> {
        self.listing.files.get(self.selected)
    }

    pub fn list_width(&self) -> u16 {
        if self.in_viewer() {
            return 0;
        }
        self.listing
            .longest_name
            .saturating_add(4)
            .max(24)
            .min((u32::from(self.screen.columns) * 2 / 5) as u16)
    }

    pub fn in_viewer(&self) -> bool {
        self.viewer || self.screen.columns < 60
    }

    fn request(&mut self) -> Vec<Effect> {
        self.generation += 1;
        self.info = None;
        self.preview_error = None;
        self.notice = None;
        let (max_width, max_height) = super::view::pixel_box(self.screen, self.list_width());
        if max_width == 0 || max_height == 0 {
            return Vec::new();
        }
        self.entry().map_or_else(Vec::new, |entry| {
            vec![Effect::Request(Request {
                generation: self.generation,
                path: entry.path.clone(),
                max_width,
                max_height,
            })]
        })
    }

    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        let last = self.listing.files.len().saturating_sub(1);
        let page = usize::from(self.screen.rows.saturating_sub(2).max(1));
        let selected = match action {
            Action::Previous => self.selected.saturating_sub(1),
            Action::Next => self.selected.saturating_add(1).min(last),
            Action::PageUp => self.selected.saturating_sub(page),
            Action::PageDown => self.selected.saturating_add(page).min(last),
            Action::First => 0,
            Action::Last => last,
            Action::Resize(screen) => {
                if self.screen == screen {
                    return Vec::new();
                }
                self.screen = screen;
                return self.request();
            }
            Action::Rescan => return vec![Effect::Rescan],
            Action::Edit => {
                return self.entry().map_or_else(Vec::new, |entry| {
                    vec![Effect::OpenEditor(entry.path.clone())]
                });
            }
            Action::Escape if !self.in_viewer() => return vec![Effect::Quit],
            Action::ToggleViewer | Action::Escape => {
                if self.screen.columns < 60 {
                    return Vec::new();
                }
                self.viewer = !self.viewer;
                return self.request();
            }
            Action::Rescanned(listing) => {
                self.selected = self
                    .entry()
                    .and_then(|entry| listing.files.iter().position(|e| e.path == entry.path))
                    .unwrap_or_else(|| self.selected.min(listing.files.len().saturating_sub(1)));
                self.listing = listing;
                return self.request();
            }
            Action::Quit => return vec![Effect::Quit],
        };
        if selected == self.selected {
            return Vec::new();
        }
        self.selected = selected;
        self.request()
    }

    pub fn apply_preview(&mut self, generation: u64, result: Result<FileInfo, String>) -> bool {
        if generation != self.generation {
            return false;
        }
        match result {
            Ok(info) => self.info = Some(info),
            Err(error) => self.preview_error = Some(error),
        }
        true
    }
}

#[cfg(test)]
pub(super) fn sample_app(names: &[&str]) -> App {
    App::new(
        Listing {
            directory: "/images".into(),
            files: names
                .iter()
                .map(|&name| Entry {
                    path: PathBuf::from("/images").join(name),
                    name: name.into(),
                })
                .collect(),
            longest_name: 30,
        },
        0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_clamps_pages_and_invalidates_old_results() {
        let mut app = sample_app(&["a", "b", "c", "d", "e", "f"]);
        app.update(Action::Resize(Screen {
            columns: 80,
            rows: 5,
            ..Screen::default()
        }));
        let generation = app.generation;
        assert!(app.update(Action::Previous).is_empty());
        app.update(Action::PageDown);
        assert_eq!(app.selected, 3);
        assert_eq!(app.generation, generation + 1);
        assert!(!app.apply_preview(generation, Err("stale".into())));
        assert!(app.preview_error.is_none());
        app.update(Action::PageDown);
        assert_eq!(app.selected, 5);
        assert!(app.update(Action::Next).is_empty());
        app.update(Action::PageUp);
        assert_eq!(app.selected, 2);
        app.update(Action::First);
        let generation = app.generation;
        app.update(Action::Resize(Screen {
            columns: 90,
            rows: 5,
            ..Screen::default()
        }));
        assert_eq!(app.generation, generation + 1);
        assert!(app.apply_preview(app.generation, Err("corrupt".into())));
        assert_eq!(app.preview_error.as_deref(), Some("corrupt"));
    }

    #[test]
    fn rescan_preserves_path_then_clamps_and_can_recover_from_empty() {
        let mut app = sample_app(&["b", "c"]);
        app.update(Action::Resize(Screen {
            columns: 80,
            rows: 24,
            ..Screen::default()
        }));
        app.update(Action::Last);
        app.update(Action::Rescanned(sample_app(&["a", "b", "c"]).listing));
        assert_eq!(app.selected, 2);
        app.update(Action::Rescanned(sample_app(&["b"]).listing));
        assert_eq!(app.selected, 0);
        assert!(
            app.update(Action::Rescanned(sample_app(&[]).listing))
                .is_empty()
        );
        assert!(app.entry().is_none());
        assert!(matches!(
            app.update(Action::Rescanned(sample_app(&["d"]).listing))
                .as_slice(),
            [Effect::Request(_)]
        ));
    }

    #[test]
    fn viewer_mode_and_escape_follow_the_available_columns() {
        let mut app = sample_app(&["a", "b"]);
        let wide = Screen {
            columns: 80,
            rows: 24,
            ..Screen::default()
        };
        app.update(Action::Resize(wide));
        let list_width = app.list_width();
        assert!(list_width >= 24);
        let effects = app.update(Action::ToggleViewer);
        assert!(matches!(&effects[..], [Effect::Request(r)] if r.max_width == 80 * 8));
        assert_eq!(app.list_width(), 0);
        app.update(Action::Next);
        assert!(
            matches!(&app.update(Action::Edit)[..], [Effect::OpenEditor(path)] if path.ends_with("b"))
        );
        app.update(Action::Escape);
        assert_eq!(app.list_width(), list_width);
        assert!(matches!(&app.update(Action::Escape)[..], [Effect::Quit]));
        app.update(Action::Resize(Screen {
            columns: 59,
            ..wide
        }));
        assert!(app.in_viewer() && app.update(Action::ToggleViewer).is_empty());
        assert!(app.update(Action::Escape).is_empty() && app.list_width() == 0);
        app.update(Action::Resize(wide));
        assert_eq!(app.list_width(), list_width);
    }
}
