use super::{
    app::{App, Info, Screen},
    listing::{Kind, display_text},
};
use crate::terminal::{self, DIACRITICS};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListState, Paragraph, Wrap},
};

#[derive(Clone, Copy)]
pub(super) struct Shown {
    pub generation: u64,
    pub id: u32,
    pub columns: u16,
    pub rows: u16,
}

pub(super) fn pixel_box(screen: Screen, list_width: u16) -> (u32, u32) {
    let limit = DIACRITICS.len() as u16;
    let cols = screen.columns.saturating_sub(list_width).min(limit);
    let rows = screen.rows.saturating_sub(1).min(limit);
    (
        u32::from(cols) * screen.cell.0,
        u32::from(rows) * screen.cell.1,
    )
}

pub(super) fn span(width: u32, height: u32, cell: (u32, u32)) -> (u16, u16) {
    // The loader returns at most the pixel box (297 cells on either axis).
    (
        width.div_ceil(cell.0) as u16,
        height.div_ceil(cell.1) as u16,
    )
}

/// Centers an image's placeholder cells in `area`.
pub(super) fn draw_image(frame: &mut Frame, area: Rect, id: u32, columns: u16, rows: u16) {
    if columns > area.width || rows > area.height {
        return;
    }
    let x = area.x + (area.width - columns) / 2;
    let y = area.y + (area.height - rows) / 2;
    let mut symbol = String::with_capacity(16);
    let buffer = frame.buffer_mut();
    for row in 0..rows {
        for col in 0..columns {
            buffer[(x + col, y + row)]
                .set_symbol(terminal::placeholder(
                    &mut symbol,
                    row.into(),
                    col.into(),
                    (id >> 24) as usize,
                ))
                .set_fg(Color::Indexed(id as u8));
        }
    }
}

pub(super) fn draw(frame: &mut Frame, app: &App, list_state: &mut ListState, shown: Option<Shown>) {
    let area = frame.area();
    let height = area.height.saturating_sub(1);
    // Placeholder sizes were computed for the app's screen; a frame of
    // another size is about to be replaced by a resize.
    let images = (area.width, area.height) == (app.screen.columns, app.screen.rows);
    let width = app.list_width().min(area.width);
    let list = Rect::new(area.x, area.y, width, height);
    if width > 0 {
        draw_list(frame, list, app, list_state);
    }
    let pane = Rect::new(list.right(), area.y, area.width - width, height);
    if let Some(error) = &app.preview_error {
        centered_text(frame, pane, error);
    } else if let Some(message) = empty_message(app) {
        centered_text(frame, pane, &message);
    } else if let Some(shown) = shown
        && shown.generation == app.generation
        && images
    {
        draw_image(frame, pane, shown.id, shown.columns, shown.rows);
    }
    if area.height > 0 {
        status(
            frame,
            Rect::new(area.x, area.y + height, area.width, 1),
            app,
        );
    }
}

fn empty_message(app: &App) -> Option<String> {
    let none_here = || format!("No images in {}", app.listing.directory.display());
    match app.entry().map(|entry| entry.kind) {
        None => Some(none_here()),
        Some(Kind::Parent) if app.listing.entries.len() == 1 => Some(none_here()),
        Some(Kind::Folder) if matches!(app.info, Some(Info::Folder { images: 0, .. })) => {
            Some("No images".into())
        }
        _ => None,
    }
}

fn title(app: &App, width: u16) -> Line<'static> {
    let directory = &app.listing.directory;
    let title = std::env::home_dir()
        .and_then(|home| directory.strip_prefix(home).ok())
        .map_or_else(
            || directory.clone(),
            |rest| std::path::Path::new("~").join(rest),
        );
    let title = Line::from(display_text(&title.to_string_lossy()))
        .style(Style::new().add_modifier(Modifier::DIM));
    if title.width() > usize::from(width) {
        title.right_aligned()
    } else {
        title
    }
}

fn draw_list(frame: &mut Frame, list: Rect, app: &App, list_state: &mut ListState) {
    let block = Block::new().borders(Borders::RIGHT);
    let inner = block.inner(list);
    frame.render_widget(block, list);
    frame.render_widget(
        title(app, inner.width),
        Rect::new(inner.x, inner.y, inner.width, inner.height.min(1)),
    );
    let items = List::new(app.listing.entries.iter().map(|entry| entry.name.as_str()))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    list_state.select(app.entry().map(|_| app.selected));
    frame.render_stateful_widget(
        items,
        Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        ),
        list_state,
    );
}

fn centered_text(frame: &mut Frame, area: Rect, message: &str) {
    if area.is_empty() {
        return;
    }
    let line = Line::from(display_text(message));
    let lines = u16::try_from(line.width().div_ceil(usize::from(area.width)))
        .unwrap_or(u16::MAX)
        .clamp(1, area.height);
    let offset = (area.height - lines) / 2;
    let text = Paragraph::new(line).centered().wrap(Wrap { trim: true });
    frame.render_widget(
        text,
        Rect::new(area.x, area.y + offset, area.width, area.height - offset),
    );
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn summary(images: usize, folders: usize) -> String {
    match (images, folders) {
        (0, 0) => "No images".into(),
        (_, 0) => count(images, "image"),
        (0, _) => count(folders, "folder"),
        _ => format!("{}, {}", count(images, "image"), count(folders, "folder")),
    }
}

fn status(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(notice) = &app.notice {
        frame.render_widget(Line::from(display_text(notice)), area);
        return;
    }
    let entry = app.entry();
    let image = entry.is_some_and(|entry| entry.kind == Kind::Image);
    let index = if image {
        format!(
            "{}/{}  ",
            app.selected - app.listing.first_image + 1,
            app.listing.images().len()
        )
    } else {
        String::new()
    };
    let details = match &app.info {
        Some(Info::File {
            metadata,
            file_size,
        }) => {
            let (w, h) = metadata.display_dimensions();
            let format = metadata.format.unwrap_or(&metadata.uti);
            format!(
                "  {format}  {w} × {h}  {}",
                crate::inspect::human_size(*file_size)
            )
        }
        Some(Info::Folder { images, folders }) => format!("  {}", summary(*images, *folders)),
        None => String::new(),
    };
    let narrow = app.screen.columns < 60;
    let keys = match (image, narrow) {
        (false, true) => "  ⏎ open  q quit",
        (false, false) => "  ⏎ open  ⌫ up  q quit",
        (true, true) => "  e edit  q quit",
        _ if app.in_viewer() => "  ⏎ list  e edit  q quit",
        _ => "  ⏎ view  e edit  q quit",
    };
    let hint = Line::from(keys).style(Style::new().add_modifier(Modifier::DIM));
    let name = entry.map_or("", |entry| &entry.name);
    let index_w = index.len() as u16;
    let details_w = Line::raw(&details).width() as u16;
    // Keep the dimensions readable when a filename has to be truncated.
    let hint_w = if area.width
        >= index_w
            .saturating_add(details_w)
            .saturating_add(hint.width() as u16 + 8)
    {
        hint.width() as u16
    } else {
        0
    };
    let name_w = area
        .width
        .saturating_sub(index_w)
        .saturating_sub(details_w)
        .saturating_sub(hint_w)
        .min(Line::raw(name).width() as u16);
    let mut x = area.x;
    for (line, width) in [
        (Line::from(index), index_w),
        (Line::from(name), name_w),
        (Line::from(details), details_w),
    ] {
        let width = width.min(area.right().saturating_sub(x));
        frame.render_widget(line, Rect::new(x, area.y, width, 1));
        x += width;
    }
    frame.render_widget(hint, Rect::new(area.right() - hint_w, area.y, hint_w, 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::app::{Action, Screen, sample_app};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn list_clips_wide_names_and_draws_error_in_its_own_pane() {
        let mut app = sample_app(&["東京東京東京東京東京東京東京東京東京東京.png"]);
        app.screen = Screen {
            columns: 80,
            rows: 9,
            ..Screen::default()
        };
        app.preview_error = Some("Bad image".into());
        let mut terminal = Terminal::new(TestBackend::new(80, 9)).unwrap();
        terminal
            .draw(|frame| draw(frame, &app, &mut ListState::default(), None))
            .unwrap();
        let buf = terminal.backend().buffer();
        assert!(buf[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert_eq!(buf[(2, 1)].symbol(), "東");
        assert_eq!(buf[(31, 1)].symbol(), "│");
        assert_eq!(buf[(32, 1)].symbol(), " ");
        assert_eq!(buf[(0, 8)].symbol(), "1");
        let middle: String = (32..80).map(|x| buf[(x, 3)].symbol()).collect();
        assert!(middle.contains("Bad image"));
    }

    #[test]
    fn fitted_cells_round_up_and_stay_within_the_diacritic_table() {
        let screen = Screen {
            columns: 800,
            rows: 500,
            cell: (10, 20),
        };
        assert_eq!(pixel_box(screen, 32), (2970, 5940));
        assert_eq!(span(13, 41, screen.cell), (2, 3));
        assert_eq!(
            pixel_box(
                Screen {
                    columns: 90,
                    rows: 10,
                    ..screen
                },
                30
            ),
            (600, 180)
        );
        assert_eq!(pixel_box(Screen::default(), 0), (0, 0));
    }

    #[test]
    fn centered_cells_name_their_image_row_and_column() {
        let mut app = sample_app(&["preview.png"]);
        app.screen = Screen {
            columns: 80,
            rows: 10,
            ..Screen::default()
        };
        let shown = Shown {
            id: (4 << 24) | 197,
            generation: 0,
            columns: 4,
            rows: 3,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
        terminal
            .draw(|frame| draw(frame, &app, &mut ListState::default(), Some(shown)))
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut symbol = String::new();
        for row in 0..3u16 {
            for col in 0..4u16 {
                let cell = &buf[(54 + col, 3 + row)];
                assert_eq!(
                    cell.symbol(),
                    terminal::placeholder(&mut symbol, row.into(), col.into(), 4)
                );
                assert_eq!(cell.fg, Color::Indexed(197));
            }
        }
        for point in [(53, 3), (58, 5), (54, 2), (57, 6)] {
            assert_eq!(buf[point].symbol(), " ");
        }
        app.generation += 1;
        terminal
            .draw(|frame| draw(frame, &app, &mut ListState::default(), Some(shown)))
            .unwrap();
        assert_eq!(terminal.backend().buffer()[(54, 3)].symbol(), " ");
    }

    #[test]
    fn folders_show_counts_and_empty_folders_say_so() {
        let mut app = sample_app(&["../", "cups/", "empty/", "a.png"]);
        app.update(Action::Resize(Screen {
            columns: 80,
            rows: 10,
            ..Screen::default()
        }));
        app.selected = 1;
        app.info = Some(Info::Folder {
            images: 14,
            folders: 1,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
        terminal
            .draw(|frame| draw(frame, &app, &mut ListState::default(), None))
            .unwrap();
        let status = row(terminal.backend().buffer(), 9);
        assert!(status.starts_with("cups/  14 images, 1 folder"), "{status}");
        assert!(status.trim_end().ends_with("⏎ open  ⌫ up  q quit"));
        app.selected = 2;
        app.info = Some(Info::Folder {
            images: 0,
            folders: 0,
        });
        terminal
            .draw(|frame| draw(frame, &app, &mut ListState::default(), None))
            .unwrap();
        let buf = terminal.backend().buffer();
        assert!((0..9).any(|y| row(buf, y).contains("No images")));
        assert!(row(buf, 9).starts_with("empty/  No images"));
        // An emptied folder keeps only its parent link.
        let mut empty = sample_app(&["../"]);
        empty.screen = app.screen;
        terminal
            .draw(|frame| draw(frame, &empty, &mut ListState::default(), None))
            .unwrap();
        let buf = terminal.backend().buffer();
        assert!((0..9).any(|y| row(buf, y).contains("No images in /images")));
    }
}
