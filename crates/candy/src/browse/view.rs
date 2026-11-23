use super::{
    app::{App, Screen},
    listing::display_text,
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

fn draw_image(frame: &mut Frame, area: Rect, shown: Shown) {
    if shown.columns > area.width || shown.rows > area.height {
        return;
    }
    let x = area.x + (area.width - shown.columns) / 2;
    let y = area.y + (area.height - shown.rows) / 2;
    let mut symbol = String::with_capacity(16);
    let buffer = frame.buffer_mut();
    for row in 0..shown.rows {
        for col in 0..shown.columns {
            buffer[(x + col, y + row)]
                .set_symbol(terminal::placeholder(
                    &mut symbol,
                    row.into(),
                    col.into(),
                    (shown.id >> 24) as usize,
                ))
                .set_fg(Color::Indexed(shown.id as u8));
        }
    }
}

pub(super) fn draw(frame: &mut Frame, app: &App, list_state: &mut ListState, shown: Option<Shown>) {
    let area = frame.area();
    let height = area.height.saturating_sub(1);
    let width = app.list_width().min(area.width);
    let list = Rect::new(area.x, area.y, width, height);
    let block = Block::new().borders(Borders::RIGHT);
    let inner = block.inner(list);
    frame.render_widget(block, list);

    let directory = &app.listing.directory;
    let title = std::env::home_dir()
        .and_then(|home| directory.strip_prefix(home).ok())
        .map_or_else(
            || directory.clone(),
            |rest| std::path::Path::new("~").join(rest),
        );
    let mut title = Line::from(display_text(&title.to_string_lossy()))
        .style(Style::new().add_modifier(Modifier::DIM));
    if title.width() > usize::from(inner.width) {
        title = title.right_aligned();
    }
    frame.render_widget(
        title,
        Rect::new(inner.x, inner.y, inner.width, inner.height.min(1)),
    );
    let items = List::new(app.listing.files.iter().map(|entry| entry.name.as_str()))
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

    let pane = Rect::new(list.right(), area.y, area.width - width, height);
    if let Some(error) = &app.preview_error {
        centered_text(frame, pane, error);
    } else if app.entry().is_none() {
        centered_text(
            frame,
            pane,
            &format!("No images in {}", directory.display()),
        );
    } else if let Some(shown) = shown
        && shown.generation == app.generation
        && (area.width, area.height) == (app.screen.columns, app.screen.rows)
    {
        draw_image(frame, pane, shown);
    }
    if area.height > 0 {
        status(
            frame,
            Rect::new(area.x, area.y + height, area.width, 1),
            app,
        );
    }
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

fn status(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(notice) = &app.notice {
        frame.render_widget(Line::from(display_text(notice)), area);
        return;
    }
    let index = if app.entry().is_some() {
        app.selected + 1
    } else {
        0
    };
    let index = format!("{index}/{}  ", app.listing.files.len());
    let details = app
        .info
        .as_ref()
        .map(|info| {
            let (w, h) = info.metadata.display_dimensions();
            let format = info.metadata.format.unwrap_or(&info.metadata.uti);
            format!(
                "  {format}  {w} × {h}  {}",
                crate::inspect::human_size(info.file_size)
            )
        })
        .unwrap_or_default();
    let hint = Line::from("  r rescan  q quit").style(Style::new().add_modifier(Modifier::DIM));
    let name = app.entry().map_or("", |entry| &entry.name);
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
    use crate::browse::app::{Screen, sample_app};
    use ratatui::{Terminal, backend::TestBackend};

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
}
