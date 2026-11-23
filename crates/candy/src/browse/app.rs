use super::{
    grid::Grid,
    listing::{Entry, Kind, Listing},
};
use candy_macos::Metadata;
use std::{ops::Range, path::PathBuf};

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

pub(super) enum Info {
    File { metadata: Metadata, file_size: u64 },
    Folder { images: usize, folders: usize },
}

pub(super) struct Request {
    pub generation: u64,
    pub path: PathBuf,
    pub folder: bool,
    pub max_width: u32,
    pub max_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    List,
    Grid,
}

pub(super) enum Action {
    Previous,
    Next,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    Resize(Screen),
    Rescan,
    Rescanned(Listing),
    Entered(Listing),
    Activate,
    Parent,
    ToggleGrid,
    Escape,
    Edit,
    Quit,
}

pub(super) enum Effect {
    Request(Request),
    OpenEditor(PathBuf),
    Enter(PathBuf),
    Rescan,
    Quit,
}

pub(super) struct App {
    pub listing: Listing,
    pub selected: usize,
    pub screen: Screen,
    pub generation: u64,
    /// Counts listings, so thumbnails can tell a rescan from a redraw.
    pub scans: u64,
    pub info: Option<Info>,
    pub preview_error: Option<String>,
    pub notice: Option<String>,
    pub layout: Layout,
    /// The first tile row on screen.
    pub grid_top: usize,
    viewer: bool,
}

impl App {
    pub fn new(listing: Listing, selected: usize) -> Self {
        Self {
            listing,
            selected,
            screen: Screen::default(),
            generation: 0,
            scans: 0,
            info: None,
            preview_error: None,
            notice: None,
            layout: Layout::List,
            grid_top: 0,
            viewer: false,
        }
    }

    pub fn entry(&self) -> Option<&Entry> {
        self.listing.entries.get(self.selected)
    }

    fn kind(&self) -> Option<Kind> {
        self.entry().map(|entry| entry.kind)
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

    /// Whether one image fills the screen: by choice, or because the list
    /// leaves the preview too little room.
    pub fn in_viewer(&self) -> bool {
        self.viewer || (self.layout == Layout::List && self.screen.columns < 60)
    }

    pub fn viewer_from_grid(&self) -> bool {
        self.viewer && self.layout == Layout::Grid
    }

    pub fn in_grid(&self) -> bool {
        self.layout == Layout::Grid && !self.viewer
    }

    pub fn grid(&self) -> Grid {
        Grid::new(self.screen)
    }

    /// Tiles on screen.
    pub fn visible_tiles(&self) -> Range<usize> {
        let grid = self.grid();
        let start = self.grid_top * grid.columns;
        let end = (self.grid_top + grid.rows) * grid.columns;
        start.min(self.listing.entries.len())..end.min(self.listing.entries.len())
    }

    /// Tiles whose thumbnails stay loaded: the screen and a page either side.
    pub fn kept_tiles(&self) -> Range<usize> {
        let visible = self.visible_tiles();
        let page = self.grid().page();
        visible.start.saturating_sub(page)..(visible.end + page).min(self.listing.entries.len())
    }

    fn scroll(&mut self) {
        let grid = self.grid();
        let row = self.selected / grid.columns;
        let rows = grid.rows.max(1);
        if row < self.grid_top {
            self.grid_top = row;
        } else if row >= self.grid_top + rows {
            self.grid_top = row + 1 - rows;
        }
        let total = self.listing.entries.len().div_ceil(grid.columns);
        self.grid_top = self.grid_top.min(total.saturating_sub(rows));
    }

    fn request(&mut self) -> Vec<Effect> {
        self.generation += 1;
        self.info = None;
        self.preview_error = None;
        self.notice = None;
        self.scroll();
        // Grid tiles carry their own details; only the viewer needs a preview.
        if self.in_grid() {
            return Vec::new();
        }
        let (max_width, max_height) = super::view::pixel_box(self.screen, self.list_width());
        if max_width == 0 || max_height == 0 {
            return Vec::new();
        }
        match self.entry() {
            Some(entry) if entry.kind != Kind::Parent => vec![Effect::Request(Request {
                generation: self.generation,
                path: entry.path.clone(),
                folder: entry.kind == Kind::Folder,
                max_width,
                max_height,
            })],
            _ => Vec::new(),
        }
    }

    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        let last = self.listing.entries.len().saturating_sub(1);
        // The viewer flips through images only; folders sort before them.
        let first = if self.viewer {
            self.listing.first_image.min(last)
        } else {
            0
        };
        let (step, page) = if self.in_grid() {
            let grid = self.grid();
            (grid.columns, grid.page())
        } else {
            (1, usize::from(self.screen.rows.saturating_sub(2).max(1)))
        };
        let selected = match action {
            Action::Previous => self.selected.saturating_sub(1).max(first),
            Action::Next => self.selected.saturating_add(1).min(last),
            Action::Up if self.selected >= first + step => self.selected - step,
            Action::Up => self.selected,
            // Down stays put on the last row instead of sliding sideways.
            Action::Down if self.selected / step < last / step => {
                self.selected.saturating_add(step).min(last)
            }
            Action::Down => self.selected,
            Action::PageUp => self.selected.saturating_sub(page).max(first),
            Action::PageDown => self.selected.saturating_add(page).min(last),
            Action::First => first,
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
                return match self.entry() {
                    Some(entry) if entry.kind == Kind::Image => {
                        vec![Effect::OpenEditor(entry.path.clone())]
                    }
                    _ => Vec::new(),
                };
            }
            Action::Activate => {
                return match self.entry() {
                    None => Vec::new(),
                    Some(entry) if entry.kind != Kind::Image => {
                        vec![Effect::Enter(entry.path.clone())]
                    }
                    // A narrow list already shows only the image.
                    Some(_) if self.in_viewer() && !self.viewer => Vec::new(),
                    Some(_) => {
                        self.viewer = !self.viewer;
                        self.request()
                    }
                };
            }
            Action::Parent => {
                return self
                    .listing
                    .directory
                    .parent()
                    .map(|parent| vec![Effect::Enter(parent.to_owned())])
                    .unwrap_or_default();
            }
            Action::ToggleGrid => {
                self.layout = match self.layout {
                    Layout::List => Layout::Grid,
                    Layout::Grid => Layout::List,
                };
                self.viewer = false;
                return self.request();
            }
            Action::Escape => {
                if self.viewer {
                    self.viewer = false;
                } else if self.layout == Layout::Grid {
                    self.layout = Layout::List;
                } else if self.in_viewer() {
                    return Vec::new();
                } else {
                    return vec![Effect::Quit];
                }
                return self.request();
            }
            Action::Rescanned(listing) => {
                self.selected = self
                    .entry()
                    .and_then(|entry| listing.entries.iter().position(|e| e.path == entry.path))
                    .unwrap_or_else(|| self.selected.min(listing.entries.len().saturating_sub(1)));
                self.listing = listing;
                self.scans += 1;
                self.viewer &= self.kind() == Some(Kind::Image);
                return self.request();
            }
            Action::Entered(listing) => {
                // Going up selects the folder just left.
                let left = &self.listing.directory;
                self.selected = listing
                    .entries
                    .iter()
                    .position(|e| e.kind == Kind::Folder && &e.path == left)
                    .unwrap_or_else(|| listing.initial());
                self.listing = listing;
                self.scans += 1;
                self.viewer = false;
                self.grid_top = 0;
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

    pub fn apply_preview(&mut self, generation: u64, result: Result<Info, String>) -> bool {
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
            entries: names
                .iter()
                .map(|&name| Entry {
                    path: if name == "../" {
                        PathBuf::from("/")
                    } else {
                        PathBuf::from("/images").join(name.trim_end_matches('/'))
                    },
                    name: name.into(),
                    kind: if name == "../" {
                        Kind::Parent
                    } else if name.ends_with('/') {
                        Kind::Folder
                    } else {
                        Kind::Image
                    },
                })
                .collect(),
            longest_name: 30,
            first_image: names
                .iter()
                .position(|name| !name.ends_with('/'))
                .unwrap_or(names.len()),
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
        assert!(app.update(Action::Down).is_empty());
        app.update(Action::PageUp);
        assert_eq!(app.selected, 2);
        app.update(Action::Up);
        assert_eq!(app.selected, 1);
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
        assert_eq!(app.scans, 4);
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
        let effects = app.update(Action::Activate);
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
        assert!(app.in_viewer() && app.update(Action::Activate).is_empty());
        assert!(app.update(Action::Escape).is_empty() && app.list_width() == 0);
        app.update(Action::Resize(wide));
        assert_eq!(app.list_width(), list_width);
    }

    #[test]
    fn folders_open_on_enter_and_the_viewer_skips_them() {
        let mut app = sample_app(&["../", "cups/", "major/", "a", "b"]);
        app.update(Action::Resize(Screen {
            columns: 80,
            rows: 24,
            ..Screen::default()
        }));
        app.update(Action::First);
        assert!(app.update(Action::Edit).is_empty());
        let root = std::path::Path::new("/");
        assert!(matches!(&app.update(Action::Activate)[..], [Effect::Enter(p)] if p == root));
        // The parent link has nothing to preview.
        assert!(matches!(&app.update(Action::Next)[..], [Effect::Request(r)] if r.folder));
        assert!(app.update(Action::Previous).is_empty());
        app.selected = 1;
        let effects = app.update(Action::Next);
        assert!(
            matches!(&effects[..], [Effect::Request(r)] if r.folder && r.path.ends_with("major"))
        );
        assert!(
            matches!(&app.update(Action::Activate)[..], [Effect::Enter(p)] if p.ends_with("major"))
        );
        assert!(matches!(&app.update(Action::Parent)[..], [Effect::Enter(p)] if p == root));
        app.update(Action::Last);
        app.update(Action::Activate);
        assert!(app.in_viewer());
        for _ in 0..4 {
            app.update(Action::Previous);
        }
        assert_eq!(app.entry().unwrap().name, "a");
        app.update(Action::First);
        assert_eq!(app.entry().unwrap().name, "a");
        // Entering another folder leaves the viewer; going up selects the folder left.
        let mut cups = sample_app(&["../", "x"]).listing;
        cups.directory = "/images/cups".into();
        app.update(Action::Entered(cups));
        assert!(!app.in_viewer());
        assert_eq!(app.entry().unwrap().name, "x");
        let mut up = sample_app(&["../", "cups/", "major/", "a"]).listing;
        up.directory = "/images".into();
        app.update(Action::Entered(up));
        assert_eq!(app.entry().unwrap().name, "cups/");
    }

    #[test]
    fn the_grid_moves_by_rows_scrolls_and_backs_out_to_the_list() {
        let names: Vec<String> = (0..40).map(|i| format!("{i:02}.png")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut app = sample_app(&names);
        app.update(Action::Resize(Screen {
            columns: 100,
            rows: 30,
            cell: (18, 40),
        }));
        assert!(app.update(Action::ToggleGrid).is_empty());
        let grid = app.grid();
        assert_eq!((grid.columns, grid.rows), (5, 2));
        app.update(Action::Down);
        assert_eq!(app.selected, 5);
        app.update(Action::Up);
        app.update(Action::Up);
        assert_eq!(app.selected, 0);
        app.update(Action::PageDown);
        assert_eq!((app.selected, app.grid_top), (10, 1));
        assert_eq!(app.visible_tiles(), 5..15);
        assert_eq!(app.kept_tiles(), 0..25);
        app.update(Action::Last);
        assert_eq!((app.selected, app.grid_top), (39, 6));
        app.update(Action::Down);
        assert_eq!(app.selected, 39, "the last row stays put");
        app.selected = 34;
        app.update(Action::Down);
        assert_eq!(app.selected, 39);
        // Enter opens the large view; Escape returns to the grid, then the list.
        assert!(
            matches!(&app.update(Action::Activate)[..], [Effect::Request(r)] if r.max_width == 100 * 18)
        );
        assert!(app.viewer_from_grid() && !app.in_grid());
        assert!(app.update(Action::Escape).is_empty());
        assert!(app.in_grid());
        assert!(matches!(
            &app.update(Action::Escape)[..],
            [Effect::Request(_)]
        ));
        assert_eq!(app.layout, Layout::List);
    }
}
