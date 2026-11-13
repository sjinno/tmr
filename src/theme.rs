use ratatui::{
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Padding},
};

pub const ACCENT: Color = Color::Rgb(196, 181, 253);
pub const MINT: Color = Color::Rgb(159, 219, 191);
pub const MUTED: Color = Color::Rgb(151, 158, 178);
pub const BG: Color = Color::Reset;

pub fn page(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    area.inner(ratatui::layout::Margin::new(
        if area.width >= 70 { 2 } else { 1 },
        1,
    ))
}

pub fn header_height(height: u16) -> u16 {
    if height < 24 { 3 } else { 7 }
}

pub fn panel<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .padding(Padding::horizontal(1))
        .title(title)
}

pub fn header(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, subtitle: &str) {
    use ratatui::{layout::Rect, widgets::Paragraph};
    let logo = " █           \n▀█▀ █▀█▀█ █▀▀\n █▄ █ █ █ █  ";
    let logo_y = area.y + area.height.saturating_sub(4) / 2;
    let logo_area = Rect::new(area.x, logo_y, area.width.min(13), area.height.min(3));
    frame.render_widget(
        Paragraph::new(logo).style(Style::default().fg(ACCENT)),
        logo_area,
    );
    if area.width > 17 && area.height > 1 {
        let label = Rect::new(area.x + 17, logo_y + 1, area.width - 17, 1);
        frame.render_widget(
            Paragraph::new(subtitle).style(Style::default().fg(MUTED)),
            label,
        );
    }
}
