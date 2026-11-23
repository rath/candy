//! The thumbnail grid: geometry, uploaded thumbnails and tile drawing.
use super::{
    app::{App, Info, Screen},
    listing::Kind,
    loader::Thumbnail,
    view,
};
use crate::terminal::DIACRITICS;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType},
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Target tile width in cells, frame included.
const TILE_COLUMNS: u16 = 20;

/// Tiles are a frame drawn around the selection, a square image box and a
/// name row. The folder title and the status line take one row each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Grid {
    pub columns: usize,
    pub rows: usize,
    pub tile_width: u16,
    pub tile_height: u16,
    pub left: u16,
}

impl Grid {
    pub fn new(screen: Screen) -> Self {
        let height = screen.rows.saturating_sub(2);
        let columns = (screen.columns / TILE_COLUMNS).max(1);
        let tile_width = screen.columns / columns;
        let limit = DIACRITICS.len() as u16;
        let inner = tile_width.saturating_sub(2).min(limit);
        let square = (u32::from(inner) * screen.cell.0 + screen.cell.1 / 2) / screen.cell.1;
        // Shorten the box rather than show a single row of tiles.
        let fit = if height / 2 >= 6 {
            height / 2 - 3
        } else {
            height.saturating_sub(3)
        };
        let image_rows = u16::try_from(square)
            .unwrap_or(u16::MAX)
            .min(fit)
            .min(limit)
            .max(1);
        let rows = height / (image_rows + 3);
        // Spread the rows left below the last tile over the boxes, which
        // lets portrait images grow; wide ones stay limited by the width.
        let tile_height = height
            .checked_div(rows)
            .map_or(image_rows + 3, |height| height.min(limit + 3));
        Self {
            columns: usize::from(columns),
            rows: usize::from(rows),
            tile_width,
            tile_height,
            left: (screen.columns - columns * tile_width) / 2,
        }
    }

    pub fn page(&self) -> usize {
        self.rows.max(1) * self.columns
    }

    fn inner_width(&self) -> u16 {
        self.tile_width
            .saturating_sub(2)
            .min(DIACRITICS.len() as u16)
    }

    fn image_rows(&self) -> u16 {
        self.tile_height - 3
    }

    /// The pixel box each thumbnail is fitted to.
    pub fn image_size(&self, cell: (u32, u32)) -> (u32, u32) {
        (
            u32::from(self.inner_width()) * cell.0,
            u32::from(self.image_rows()) * cell.1,
        )
    }
}

pub(super) enum Thumb {
    /// Sent as a local tmux file the terminal has not read yet.
    Uploading,
    Ready {
        id: u32,
        columns: u16,
        rows: u16,
        info: Info,
    },
    /// A folder without a cover image.
    Empty(Info),
    Failed(String),
}

/// Thumbnails uploaded for one folder listing at one tile size.
#[derive(Default)]
pub(super) struct Thumbs {
    directory: PathBuf,
    scans: u64,
    size: (u32, u32),
    pub map: HashMap<PathBuf, Thumb>,
    requested: Vec<PathBuf>,
}

impl Thumbs {
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Forgets thumbnails of another listing or tile size, and those more
    /// than a page off screen. Their IDs are deleted after the next frame.
    pub fn retain(&mut self, app: &App) {
        let size = app.grid().image_size(app.screen.cell);
        if self.directory != app.listing.directory || self.scans != app.scans || self.size != size {
            self.map.clear();
            self.requested.clear();
            self.directory.clone_from(&app.listing.directory);
            self.scans = app.scans;
            self.size = size;
        }
        let kept: HashSet<&Path> = app.listing.entries[app.kept_tiles()]
            .iter()
            .map(|entry| entry.path.as_path())
            .collect();
        self.map.retain(|path, _| kept.contains(path.as_path()));
    }

    /// Thumbnails to decode, most wanted first: the selection, the screen,
    /// the next page, then the previous one.
    pub fn missing(&self, app: &App) -> Vec<Thumbnail> {
        if !app.in_grid() || self.size.0 == 0 || self.size.1 == 0 {
            return Vec::new();
        }
        let visible = app.visible_tiles();
        let kept = app.kept_tiles();
        let order = std::iter::once(app.selected)
            .chain(visible.clone())
            .chain(visible.end..kept.end)
            .chain((kept.start..visible.start).rev());
        let mut seen = HashSet::new();
        order
            .filter_map(|index| app.listing.entries.get(index))
            .filter(|entry| entry.kind != Kind::Parent && !self.map.contains_key(&entry.path))
            .filter(|entry| seen.insert(&entry.path))
            .map(|entry| Thumbnail {
                path: entry.path.clone(),
                folder: entry.kind == Kind::Folder,
                size: self.size,
            })
            .collect()
    }

    /// Whether this request list differs from the one the loader holds.
    pub fn changed(&mut self, list: &[Thumbnail]) -> bool {
        if self.requested.len() == list.len()
            && self.requested.iter().zip(list).all(|(a, b)| *a == b.path)
        {
            return false;
        }
        self.requested = list.iter().map(|t| t.path.clone()).collect();
        true
    }

    /// Whether a decoded thumbnail still belongs on screen or near it.
    pub fn wants(&self, app: &App, path: &Path, size: (u32, u32)) -> bool {
        size == self.size
            && !self.map.contains_key(path)
            && app.listing.entries[app.kept_tiles()]
                .iter()
                .any(|entry| entry.path == path)
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.values().filter_map(|thumb| match thumb {
            Thumb::Ready { id, .. } => Some(*id),
            _ => None,
        })
    }
}

/// Draws the visible tiles. `images` is false while the frame and the
/// thumbnails disagree about the screen size.
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, thumbs: &Thumbs, images: bool) {
    let grid = app.grid();
    let image_rows = grid.image_rows();
    for index in app.visible_tiles() {
        let entry = &app.listing.entries[index];
        let row = (index / grid.columns - app.grid_top) as u16;
        let column = (index % grid.columns) as u16;
        let tile = Rect::new(
            area.x + grid.left + column * grid.tile_width,
            area.y + row * grid.tile_height,
            grid.tile_width,
            grid.tile_height,
        )
        .intersection(area);
        if tile.width < 3 || tile.height < grid.tile_height {
            continue;
        }
        let selected = index == app.selected;
        if selected {
            frame.render_widget(Block::bordered().border_type(BorderType::Rounded), tile);
        }
        let inner = Rect::new(tile.x + 1, tile.y + 1, tile.width - 2, image_rows);
        match (entry.kind, thumbs.map.get(&entry.path)) {
            (Kind::Parent, _) => note(frame, inner, "↑"),
            (
                _,
                Some(Thumb::Ready {
                    id, columns, rows, ..
                }),
            ) if images => view::draw_image(frame, inner, *id, *columns, *rows),
            (_, Some(Thumb::Empty(Info::Folder { images: 0, .. }))) => {
                note(frame, inner, "No images");
            }
            (_, Some(Thumb::Failed(_))) => note(frame, inner, "Cannot preview"),
            _ => {}
        }
        let name = Rect::new(inner.x, inner.bottom(), inner.width, 1);
        let style = if selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        frame.render_widget(
            Line::from(Span::styled(
                fit_middle(&entry.name, usize::from(name.width)),
                style,
            ))
            .centered(),
            name,
        );
    }
}

/// A dim one-line note centered in a tile, omitted when it does not fit.
fn note(frame: &mut Frame, area: Rect, text: &str) {
    let line = Line::from(text).style(Style::new().add_modifier(Modifier::DIM));
    if line.width() > usize::from(area.width) || area.height == 0 {
        return;
    }
    let row = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
    frame.render_widget(line.centered(), row);
}

/// Shortens a name to `width` columns by replacing its middle with an
/// ellipsis, keeping numbers and extensions at both ends readable. Cuts fall
/// only before characters that occupy columns, so combining marks and
/// decomposed jamo stay with their base character.
pub(super) fn fit_middle(text: &str, width: usize) -> String {
    let total = Line::raw(text).width();
    if total <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut buffer = [0; 4];
    let widths: Vec<(char, usize)> = text
        .chars()
        .map(|c| (c, Line::raw(&*c.encode_utf8(&mut buffer)).width()))
        .collect();
    let budget = width - 1;
    let mut head = String::new();
    let mut head_width = 0;
    let mut split = 0;
    for (index, &(c, w)) in widths.iter().enumerate() {
        if w > 0 && head_width + w > budget.div_ceil(2) {
            break;
        }
        head.push(c);
        head_width += w;
        split = index + 1;
    }
    let mut tail = Vec::new();
    let mut tail_width = 0;
    let mut pending = Vec::new();
    for &(c, w) in widths[split..].iter().rev() {
        pending.push(c);
        if w == 0 {
            continue;
        }
        if head_width + tail_width + w > budget {
            break;
        }
        tail_width += w;
        tail.append(&mut pending);
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_fill_the_width_and_keep_two_rows_when_they_can() {
        let screen = |columns, rows| Screen {
            columns,
            rows,
            cell: (18, 40),
        };
        // Square 18 × 8 boxes fit two rows; the spare rows make them 18 × 11.
        let grid = Grid::new(screen(100, 30));
        assert_eq!(
            (
                grid.columns,
                grid.rows,
                grid.tile_width,
                grid.tile_height,
                grid.left
            ),
            (5, 2, 20, 14, 0)
        );
        assert_eq!(grid.image_size((18, 40)), (18 * 18, 11 * 40));
        // A square box would allow one row; shorter boxes keep two.
        let short = Grid::new(screen(76, 26));
        assert_eq!((short.columns, short.rows, short.tile_width), (3, 2, 25));
        assert_eq!(short.tile_height, 12);
        assert_eq!(short.left, 0);
        let tiny = Grid::new(screen(10, 4));
        assert_eq!((tiny.columns, tiny.rows, tiny.image_rows()), (1, 0, 1));
        assert_eq!(Grid::new(Screen::default()).image_size((8, 16)), (0, 16));
        let centered = Grid::new(screen(103, 30));
        assert_eq!((centered.tile_width, centered.left), (20, 1));
    }

    #[test]
    fn long_names_keep_both_ends_and_whole_characters() {
        assert_eq!(fit_middle("cups-01-ace.webp", 16), "cups-01-ace.webp");
        assert_eq!(
            fit_middle("02-high-priestess.webp", 17),
            "02-high-…ess.webp"
        );
        assert_eq!(Line::raw(fit_middle("東京東京東京.png", 9)).width(), 9);
        // Decomposed syllables keep their vowel and final jamo on either side.
        let han = "\u{1112}\u{1161}\u{11ab}";
        assert_eq!(
            fit_middle(&format!("{han}{han}{han}.png"), 7),
            format!("{han}….png")
        );
        assert_eq!(fit_middle(&format!("abcdef{han}"), 6), format!("abc…{han}"));
        assert_eq!(fit_middle("abc", 0), "");
        assert_eq!(fit_middle("abcdef", 1), "…");
    }
}
