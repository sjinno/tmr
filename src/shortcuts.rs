use crate::{ACCENT, MUTED};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

pub const TIMERS: &[(&str, &str)] = &[
    ("c", "clocks"),
    ("d", "delete"),
    ("e", "edit"),
    ("f", "favorite"),
    ("J/K", "move ↓/↑"),
    ("n", "new"),
    ("q", "quit"),
    ("r", "reset"),
    ("s", "sort by use"),
    ("Space", "start/pause"),
    ("/", "jump"),
    ("↑/↓", "select"),
];
pub const CLOCKS: &[(&str, &str)] = &[
    ("d", "remove"),
    ("n", "add"),
    ("q", "quit"),
    ("t", "timers"),
    ("←/→", "select"),
];

fn column_widths(width: u16, entries: &[(&str, &str)]) -> Vec<(usize, usize)> {
    // Prefer two rows, with additional rows on narrow terminals.
    for columns in (1..=entries.len().div_ceil(2).max(1)).rev() {
        let widths: Vec<_> = (0..columns)
            .map(|column| {
                let mut key_width = 0;
                let mut description_width = 0;
                for &(key, description) in entries.iter().skip(column).step_by(columns) {
                    key_width = key_width.max(Span::raw(key).width());
                    description_width = description_width.max(Span::raw(description).width());
                }
                (key_width, key_width + 1 + description_width + 2)
            })
            .collect();
        let total = widths
            .iter()
            .map(|(_, width)| width)
            .sum::<usize>()
            .saturating_sub(2);
        if total <= usize::from(width) || columns == 1 {
            return widths;
        }
    }
    unreachable!()
}

pub fn height(width: u16, entries: &[(&str, &str)]) -> u16 {
    entries.len().div_ceil(column_widths(width, entries).len()) as u16
}

pub fn render(frame: &mut Frame, area: Rect, entries: &[(&str, &str)]) {
    let widths = column_widths(area.width, entries);
    let columns = widths.len();
    for (index, &(key, description)) in entries.iter().enumerate() {
        let row = index / columns;
        if row >= usize::from(area.height) {
            break;
        }
        let column = index % columns;
        let offset = widths[..column]
            .iter()
            .map(|(_, width)| width)
            .sum::<usize>() as u16;
        let cell = Rect::new(
            area.x + offset,
            area.y + row as u16,
            (widths[column].1 as u16).min(area.width.saturating_sub(offset)),
            1,
        );
        let padding = widths[column].0 + 1 - Span::raw(key).width();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(key, Style::default().fg(ACCENT).bold()),
                Span::raw(" ".repeat(padding)),
                Span::styled(description, Style::default().fg(MUTED)),
            ])),
            cell,
        );
    }
}
