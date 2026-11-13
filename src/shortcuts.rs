use crate::{ACCENT, MUTED};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

pub const TIMERS: &[(&str, &str)] = &[
    ("n", "new"),
    ("Space", "start/pause"),
    ("r", "reset"),
    ("e", "edit"),
    ("↑/↓", "select"),
    ("/", "jump"),
    ("f", "favorite"),
    ("d", "delete"),
    ("J/K", "move ↓/↑"),
    ("s", "sort by use"),
    ("c", "clocks"),
    ("q", "quit"),
];
pub const CLOCKS: &[(&str, &str)] = &[
    ("n", "add"),
    ("d", "remove"),
    ("←/→", "select"),
    ("t", "timers"),
    ("q", "quit"),
];

fn columns(width: u16) -> usize {
    usize::from((width / 20).clamp(1, 4))
}

pub fn height(width: u16, entries: &[(&str, &str)]) -> u16 {
    entries.len().div_ceil(columns(width)) as u16
}

pub fn render(frame: &mut Frame, area: Rect, entries: &[(&str, &str)]) {
    let columns = columns(area.width);
    let cell_width = area.width / columns as u16;
    for (index, &(key, description)) in entries.iter().enumerate() {
        let row = index / columns;
        if row >= usize::from(area.height) {
            break;
        }
        let cell = Rect::new(
            area.x + (index % columns) as u16 * cell_width,
            area.y + row as u16,
            cell_width,
            1,
        );
        let padding = 6usize.saturating_sub(Span::raw(key).width());
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
