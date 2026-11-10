mod alarm;
mod block_clock;
mod model;
mod storage;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use model::{Timer, clock, parse_duration};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const ACCENT: Color = Color::Rgb(159, 211, 212);
const MUTED: Color = Color::Rgb(192, 195, 204);
const HIGHLIGHT: Color = Color::Rgb(52, 59, 68);
const BG: Color = Color::Reset;

enum Mode {
    Browse,
    Form {
        edit: bool,
        name: String,
        duration: String,
        field: bool,
        error: String,
    },
    Delete,
}
struct App {
    timers: Vec<Timer>,
    selected: usize,
    mode: Mode,
    status: String,
    path: PathBuf,
    list: ListState,
}
impl App {
    fn persist(&mut self) -> bool {
        match storage::save(&self.path, &self.timers) {
            Ok(()) => true,
            Err(e) => {
                self.status =
                    format!("Save failed: {e}. Changes remain in memory; retry before quitting.");
                false
            }
        }
    }
    fn key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return self.persist();
        }
        match &mut self.mode {
            Mode::Form {
                edit,
                name,
                duration,
                field,
                error,
            } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => *field = !*field,
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let text = if *field { duration } else { name };
                    if text.chars().count() < 80 {
                        text.push(c);
                    }
                }
                KeyCode::Backspace => {
                    if *field {
                        duration.pop();
                    } else {
                        name.pop();
                    }
                }
                KeyCode::Enter => match parse_duration(duration) {
                    Err(e) => *error = e,
                    Ok(seconds) => {
                        if *edit {
                            let t = &mut self.timers[self.selected];
                            t.name = name.trim().to_string();
                            t.seconds = seconds;
                            t.reset();
                            self.status = "Timer saved. Space to start.".into();
                        } else {
                            let mut timer = Timer::new(name.trim().to_string(), seconds);
                            timer.toggle(Instant::now());
                            self.timers.push(timer);
                            self.selected = self.timers.len() - 1;
                            self.status = "Timer created and running. Space to pause.".into();
                        }
                        self.mode = Mode::Browse;
                        self.persist();
                    }
                },
                _ => {}
            },
            Mode::Delete => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.timers.remove(self.selected);
                    self.selected = self.selected.min(self.timers.len().saturating_sub(1));
                    self.mode = Mode::Browse;
                    self.status = "Timer deleted.".into();
                    self.persist();
                }
                KeyCode::Esc | KeyCode::Char('n') => self.mode = Mode::Browse,
                _ => {}
            },
            Mode::Browse => match key.code {
                KeyCode::Char('q') => return self.persist(),
                KeyCode::Char('n') => {
                    self.mode = Mode::Form {
                        edit: false,
                        name: String::new(),
                        duration: String::new(),
                        field: false,
                        error: String::new(),
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.timers.is_empty() {
                        self.selected = (self.selected + 1) % self.timers.len();
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if !self.timers.is_empty() {
                        self.selected = (self.selected + self.timers.len() - 1) % self.timers.len();
                    }
                }
                KeyCode::Char('s') => {
                    self.timers.sort_by_key(|t| {
                        (std::cmp::Reverse(t.favorite), std::cmp::Reverse(t.uses))
                    });
                    self.selected = 0;
                    self.status = "Sorted by favorites, then most used.".into();
                    self.persist();
                }
                code if !self.timers.is_empty() => {
                    let t = &mut self.timers[self.selected];
                    match code {
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            t.toggle(Instant::now());
                            self.persist();
                        }
                        KeyCode::Char('r') => t.reset(),
                        KeyCode::Char('f') => {
                            t.favorite = !t.favorite;
                            self.persist();
                        }
                        KeyCode::Char('e') => {
                            self.mode = Mode::Form {
                                edit: true,
                                name: t.name.clone(),
                                duration: format!("{}s", t.seconds),
                                field: false,
                                error: String::new(),
                            }
                        }
                        KeyCode::Char('d') => self.mode = Mode::Delete,
                        KeyCode::Char('K') if self.selected > 0 => {
                            self.timers.swap(self.selected, self.selected - 1);
                            self.selected -= 1;
                            self.persist();
                        }
                        KeyCode::Char('J') if self.selected + 1 < self.timers.len() => {
                            self.timers.swap(self.selected, self.selected + 1);
                            self.selected += 1;
                            self.persist();
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
        }
        false
    }
    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(
            Block::default().style(Style::default().bg(BG).fg(Color::Reset)),
            area,
        );
        if area.width < 44 || area.height < 16 {
            f.render_widget(
                Paragraph::new("tmr · Please resize to at least 44 × 16.\nq to quit")
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(area);
        let running = self.timers.iter().filter(|t| t.started.is_some()).count();
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(" tmr ", Style::default().fg(Color::Black).bg(ACCENT).bold()),
                    Span::styled("  a little room for your time", Style::default().fg(MUTED)),
                ]),
                Line::from(format!(
                    " {} saved  ·  {} running",
                    self.timers.len(),
                    running
                )),
            ]),
            rows[0],
        );
        let columns = if area.width >= 85 {
            Layout::horizontal([
                Constraint::Length((area.width / 3).clamp(32, 48)),
                Constraint::Min(1),
            ])
            .split(rows[1])
        } else {
            Layout::horizontal([Constraint::Percentage(100), Constraint::Length(0)]).split(rows[1])
        };
        let now = Instant::now();
        let items: Vec<ListItem> = self
            .timers
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let state = if t.finished {
                    "DONE"
                } else if t.started.is_some() {
                    "RUN "
                } else if t.remaining < Duration::from_secs(t.seconds) {
                    "PAUSE"
                } else {
                    "READY"
                };
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            format!(" {:02} {} ", i + 1, if t.favorite { "★" } else { "·" }),
                            Style::default().fg(ACCENT),
                        ),
                        Span::styled(t.label(), Style::default().bold()),
                    ]),
                    Line::from(Span::styled(
                        format!(
                            "      {}  {:5}  ·  {} uses",
                            clock(t.left(now)),
                            state,
                            t.uses
                        ),
                        Style::default().fg(if t.finished { ACCENT } else { MUTED }),
                    )),
                    Line::from(""),
                ])
            })
            .collect();
        self.list.select(if self.timers.is_empty() {
            None
        } else {
            Some(self.selected)
        });
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" TIMERS / your order ")
            .border_style(Style::default().fg(MUTED));
        if self.timers.is_empty() {
            f.render_widget(Paragraph::new("\n  Nothing on the clock.\n\n  Press n to create your first timer.\n  Tea, a deep work session, a quick break…").block(block).wrap(Wrap { trim: false }), columns[0]);
        } else {
            f.render_stateful_widget(
                List::new(items)
                    .block(block)
                    .highlight_style(Style::default().bg(HIGHLIGHT))
                    .highlight_symbol("▎"),
                columns[0],
                &mut self.list,
            );
        }
        if columns[1].width > 0 {
            let panel = Block::default()
                .borders(Borders::ALL)
                .title(" ON THE CLOCK ")
                .border_style(Style::default().fg(MUTED));
            let inside = panel.inner(columns[1]);
            f.render_widget(panel, columns[1]);
            if let Some(t) = self.timers.get(self.selected) {
                let spacious = inside.height >= 20;
                let gap = if spacious { 2 } else { 1 };
                let parts = Layout::vertical([
                    Constraint::Length(if spacious { 2 } else { 1 }),
                    Constraint::Min(1),
                    Constraint::Length(gap),
                    Constraint::Length(1),
                    Constraint::Length(gap),
                    Constraint::Length(if spacious { 2 } else { 0 }),
                ])
                .margin(1)
                .split(inside);
                f.render_widget(
                    Paragraph::new(t.label()).style(Style::default().fg(ACCENT).bold()),
                    parts[0],
                );
                block_clock::render(f, parts[1], &clock(t.left(now)), Color::Reset);
                let ratio = (1.0 - t.left(now).as_secs_f64() / t.seconds as f64).clamp(0.0, 1.0);
                f.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(ACCENT).bg(HIGHLIGHT))
                        .ratio(ratio)
                        .label(if t.finished {
                            "Complete"
                        } else if t.started.is_some() {
                            "Running"
                        } else {
                            "Space to start / resume"
                        }),
                    parts[3],
                );
                f.render_widget(
                    Paragraph::new(format!(
                        "Duration  {}\nStarted   {} times",
                        clock(Duration::from_secs(t.seconds)),
                        t.uses
                    ))
                    .style(Style::default().fg(MUTED)),
                    parts[5],
                );
            }
        }
        f.render_widget(
            Paragraph::new(self.status.as_str())
                .style(Style::default().fg(ACCENT))
                .wrap(Wrap { trim: true }),
            rows[2],
        );
        f.render_widget(Paragraph::new("n new   Space start/pause   r reset   e edit   d delete\n↑/↓ select   f favorite   J/K rank   s sort by use   q quit").style(Style::default().fg(MUTED)), rows[3]);
        match &self.mode {
            Mode::Browse => {}
            Mode::Form {
                edit,
                name,
                duration,
                field,
                error,
            } => {
                let popup = popup(area, 66, 12);
                f.render_widget(Clear, popup);
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} Name (optional)  {}",
                            if !field { "›" } else { " " },
                            name
                        ),
                        Style::default().fg(if !field { ACCENT } else { Color::White }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(" {} Duration  {}", if *field { "›" } else { " " }, duration),
                        Style::default().fg(if *field { ACCENT } else { Color::White }),
                    )),
                    Line::from("   e.g. 1h / 6m 10s / 100s / 1h30m"),
                    Line::from(""),
                    Line::from(Span::styled(
                        error.as_str(),
                        Style::default().fg(Color::LightRed),
                    )),
                    Line::from("Tab switch · Backspace erase · Enter save · Esc cancel"),
                ];
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .block(
                            Block::bordered()
                                .title(if *edit {
                                    " EDIT TIMER · saves reset the countdown "
                                } else {
                                    " NEW TIMER "
                                })
                                .border_style(Style::default().fg(ACCENT)),
                        )
                        .style(Style::default().bg(BG)),
                    popup,
                );
            }
            Mode::Delete => {
                let popup = popup(area, 58, 7);
                f.render_widget(Clear, popup);
                f.render_widget(
                    Paragraph::new(format!(
                        "Delete “{}”?\n\nEnter / y delete · Esc / n cancel",
                        self.timers[self.selected].label()
                    ))
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::bordered()
                            .title(" DELETE TIMER ")
                            .border_style(Style::default().fg(Color::LightRed)),
                    )
                    .style(Style::default().bg(BG)),
                    popup,
                );
            }
        }
    }
}
fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn main() -> io::Result<()> {
    let path = storage::path()?;
    let timers = storage::load(&path)
        .map_err(|e| io::Error::other(format!("Could not load {}: {e}", path.display())))?;
    let mut app = App {
        timers,
        selected: 0,
        mode: Mode::Browse,
        status: "Make time for something. Press n to add a timer.".into(),
        path,
        list: ListState::default(),
    };
    let mut alarm = alarm::Alarm::default();
    ratatui::run(|terminal| -> io::Result<()> {
        loop {
            if alarm.poll().is_err() {
                print!("\x07");
                io::stdout().flush()?;
                app.status = "Audio playback failed; used terminal bell instead.".into();
            }
            let now = Instant::now();
            let mut completed = Vec::new();
            for t in &mut app.timers {
                if t.tick(now) {
                    completed.push(t.label());
                }
            }
            if !completed.is_empty() {
                app.status = format!(
                    "Finished: {} · r to reset, Space to run again",
                    completed.join(", ")
                );
                if alarm.play().is_err() {
                    print!("\x07");
                    io::stdout().flush()?;
                    app.status
                        .push_str(" · Audio unavailable; used terminal bell.");
                }
            }
            terminal.draw(|f| app.draw(f))?;
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && app.key(key)
            {
                return Ok(());
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_empty_full_and_small_layouts() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = App {
            timers: vec![],
            selected: 0,
            mode: Mode::Browse,
            status: String::new(),
            path: PathBuf::new(),
            list: ListState::default(),
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Nothing on the clock"));
        app.timers.push(Timer::new("Focus".into(), 1500));
        app.mode = Mode::Form {
            edit: true,
            name: "Focus".into(),
            duration: "25:00".into(),
            field: true,
            error: String::new(),
        };
        terminal.draw(|f| app.draw(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("EDIT TIMER"));
        for (width, height) in [(44, 16), (20, 5), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| app.draw(f)).unwrap();
        }
    }
}
