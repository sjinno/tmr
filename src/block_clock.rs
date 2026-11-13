//! Wide, solid pixels keep the clock legible in tall terminal cells.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
};

const DIGITS: [[&str; 5]; 10] = [
    ["111", "101", "101", "101", "111"],
    ["010", "110", "010", "010", "111"],
    ["111", "001", "111", "100", "111"],
    ["111", "001", "111", "001", "111"],
    ["101", "101", "111", "001", "001"],
    ["111", "100", "111", "001", "111"],
    ["111", "100", "111", "101", "111"],
    ["111", "001", "010", "010", "010"],
    ["111", "101", "111", "101", "111"],
    ["111", "101", "111", "001", "111"],
];

fn pixels(time: &str, scale: usize) -> String {
    // Two deliberate sizes: 5 rows, or a modest 7 rows. Never fill the screen.
    let on = "█".repeat(scale + 1);
    let off = " ".repeat(scale + 1);
    let gap = " ";
    (0..5)
        .flat_map(|row| {
            let line = time
                .chars()
                .map(|c| {
                    if let Some(digit) = c.to_digit(10) {
                        DIGITS[digit as usize][row]
                            .chars()
                            .map(|pixel| {
                                if pixel == '1' {
                                    on.as_str()
                                } else {
                                    off.as_str()
                                }
                            })
                            .collect::<String>()
                    } else if c == ':' && (row == 1 || row == 3) {
                        on.clone()
                    } else {
                        off.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(gap);
            let repeats = if scale == 2 && (row == 1 || row == 3) {
                2
            } else {
                1
            };
            std::iter::repeat_n(line, repeats)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scale_for(area: Rect, time: &str) -> usize {
    [2, 1]
        .into_iter()
        .find(|&scale| {
            let text = ratatui::text::Text::from(pixels(time, scale));
            text.width() <= usize::from(area.width) && text.height() <= usize::from(area.height)
        })
        .unwrap_or(0)
}

pub fn render(frame: &mut Frame, area: Rect, time: &str, color: Color) {
    let scale = scale_for(area, time);
    let text = if scale > 0 {
        ratatui::text::Text::from(pixels(time, scale))
    } else {
        ratatui::text::Text::from(time.to_string())
    };
    draw(frame, area, text, color);
}

pub fn render_compact(frame: &mut Frame, area: Rect, time: &str, color: Color) {
    let text = ratatui::text::Text::from(pixels(time, 0));
    let text =
        if text.width() <= usize::from(area.width) && text.height() <= usize::from(area.height) {
            text
        } else {
            ratatui::text::Text::from(time.to_string())
        };
    draw(frame, area, text, color);
}

fn draw(frame: &mut Frame, area: Rect, text: ratatui::text::Text<'_>, color: Color) {
    let height = (text.height() as u16).min(area.height);
    let centered = Rect::new(
        area.x,
        area.y + (area.height - height) / 2,
        area.width,
        height,
    );
    frame.render_widget(
        Paragraph::new(text)
            .centered()
            .style(Style::default().fg(color)),
        centered,
    );
}

pub fn small_digits(count: &str) -> Vec<String> {
    (0..3)
        .map(|pair| {
            count
                .chars()
                .filter_map(|character| match character.to_digit(10) {
                    Some(digit) => Some(DIGITS[digit as usize]),
                    None if character == ':' => Some(["0", "1", "0", "1", "0"]),
                    _ => None,
                })
                .map(|glyph| {
                    (0..glyph[0].len())
                        .map(|column| {
                            let top = glyph[pair * 2].as_bytes()[column] == b'1';
                            let bottom =
                                pair * 2 + 1 < 5 && glyph[pair * 2 + 1].as_bytes()[column] == b'1';
                            match (top, bottom) {
                                (true, true) => '█',
                                (true, false) => '▀',
                                (false, true) => '▄',
                                (false, false) => ' ',
                            }
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

pub fn render_small(frame: &mut Frame, area: Rect, count: &str, color: Color) {
    let lines = small_digits(count);
    let text = ratatui::text::Text::from(lines.join("\n"));
    let text = if area.height >= 3 && text.width() <= usize::from(area.width) {
        text
    } else {
        ratatui::text::Text::from(count.to_string())
    };
    draw(frame, area, text, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn scaling_respects_both_dimensions() {
        assert_eq!(scale_for(Rect::new(0, 0, 64, 14), "12:34"), 2);
        assert_eq!(scale_for(Rect::new(0, 0, 96, 20), "12:34"), 2);
        assert_eq!(scale_for(Rect::new(0, 0, 240, 80), "12:34"), 2);
        assert_eq!(ratatui::text::Text::from(pixels("12:34", 2)).height(), 7);
        assert_eq!(scale_for(Rect::new(0, 0, 96, 5), "12:34"), 1);
        assert_eq!(scale_for(Rect::new(0, 0, 32, 20), "12:34"), 1);
    }
    #[test]
    fn clock_scales_to_available_space() {
        for (width, height, time, large) in [
            (32, 6, "12:34", true),
            (56, 5, "999:59:59", true),
            (64, 14, "12:34", true),
            (96, 20, "12:34", true),
            (24, 5, "12:34", false),
            (10, 5, "12:34", false),
            (24, 3, "12:34", false),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| render(f, f.area(), time, Color::Green))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert_eq!(text.contains('█'), large);
            if !large {
                assert!(text.contains(time));
            } else {
                assert!(!text.contains(time));
            }
        }
    }
}
