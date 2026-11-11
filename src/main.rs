mod alarm;
#[cfg(target_os = "macos")]
mod awake;
mod block_clock;
mod model;
mod storage;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use model::{Timer, clock, parse_duration};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Wrap},
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
    Jump {
        number: String,
        error: String,
    },
    Form {
        edit: bool,
        name: String,
        duration: String,
        memo: String,
        field: usize,
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
            Mode::Jump { number, error } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    if number.len() < 20 {
                        number.push(c);
                    }
                    error.clear();
                }
                KeyCode::Backspace => {
                    number.pop();
                    error.clear();
                }
                KeyCode::Enter => {
                    match number
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n > 0 && n <= self.timers.len())
                    {
                        Some(n) => {
                            self.selected = n - 1;
                            self.status = format!("Selected timer #{n}. Space to start / pause.");
                            self.mode = Mode::Browse;
                        }
                        None => {
                            *error =
                                format!("Enter a timer number from 1 to {}.", self.timers.len())
                        }
                    }
                }
                _ => {}
            },
            Mode::Form {
                edit,
                name,
                duration,
                memo,
                field,
                error,
            } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Tab | KeyCode::Down => *field = (*field + 1) % 3,
                KeyCode::BackTab | KeyCode::Up => *field = (*field + 2) % 3,
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let text = match *field {
                        0 => name,
                        1 => duration,
                        _ => memo,
                    };
                    if text.chars().count() < 80 {
                        text.push(c);
                    }
                }
                KeyCode::Backspace => {
                    match *field {
                        0 => name,
                        1 => duration,
                        _ => memo,
                    }
                    .pop();
                }
                KeyCode::Enter => match parse_duration(duration) {
                    Err(e) => *error = e,
                    Ok(seconds) => {
                        if *edit {
                            let t = &mut self.timers[self.selected];
                            t.name = name.trim().to_string();
                            t.memo = memo.trim().to_string();
                            t.seconds = seconds;
                            t.reset();
                            self.status = "Timer saved. Space to start.".into();
                        } else {
                            let mut timer = Timer::new(name.trim().to_string(), seconds);
                            timer.memo = memo.trim().to_string();
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
                KeyCode::Char('/') => {
                    if self.timers.is_empty() {
                        self.status = "No timers yet. Press n to create one.".into();
                    } else {
                        self.mode = Mode::Jump {
                            number: String::new(),
                            error: String::new(),
                        };
                    }
                }
                KeyCode::Char('q') => return self.persist(),
                KeyCode::Char('n') => {
                    self.mode = Mode::Form {
                        edit: false,
                        name: String::new(),
                        duration: String::new(),
                        memo: String::new(),
                        field: 0,
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
                                memo: t.memo.clone(),
                                field: 0,
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
                    "RUN"
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
                    Line::from(vec![
                        Span::raw(format!("      {}  ", clock(t.left(now)))),
                        Span::styled(
                            format!("{state:5}"),
                            if t.started.is_some() {
                                Style::default().fg(ACCENT).bold()
                            } else {
                                Style::default()
                            },
                        ),
                        Span::raw(format!("  ·  {} uses", t.uses)),
                    ])
                    .style(Style::default().fg(if t.finished {
                        ACCENT
                    } else {
                        MUTED
                    })),
                    Line::from(Span::styled(
                        if t.memo.is_empty() {
                            String::new()
                        } else {
                            format!("      {}", t.memo)
                        },
                        Style::default().fg(MUTED),
                    )),
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
            if running > 1 {
                let [summary, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
                    .horizontal_margin(1)
                    .areas(inside);
                let active: Vec<_> = self
                    .timers
                    .iter()
                    .enumerate()
                    .filter(|(_, timer)| timer.started.is_some())
                    .collect();
                const BOX_HEIGHT: u16 = 5;
                const BOX_STRIDE: u16 = BOX_HEIGHT + 1;
                let capacity = usize::from((list.height.saturating_add(1) / BOX_STRIDE).max(1));
                let selected = active.iter().position(|(index, _)| *index == self.selected);
                let first = selected.unwrap_or(0) / capacity * capacity;
                let end = (first + capacity).min(running);
                let heading = if end - first < running {
                    format!("{running} running · showing {}–{end}", first + 1)
                } else {
                    format!("{running} running")
                };
                f.render_widget(
                    Paragraph::new(heading).style(Style::default().fg(MUTED)),
                    summary,
                );
                for (row, &(index, timer)) in active[first..end].iter().enumerate() {
                    let bounds = Rect::new(
                        list.x,
                        list.y + row as u16 * BOX_STRIDE,
                        list.width,
                        BOX_HEIGHT.min(list.height),
                    );
                    let border = if index == self.selected {
                        ACCENT
                    } else {
                        MUTED
                    };
                    let card = Block::bordered()
                        .title(Line::styled(
                            format!(" #{:02} {} ", index + 1, timer.label()),
                            Style::default().bold(),
                        ))
                        .border_style(Style::default().fg(border))
                        .padding(Padding::horizontal(1));
                    let gauge_width = usize::from(card.inner(bounds).width);
                    let remaining = timer.left(now);
                    let mut metadata =
                        format!("duration: {}", clock(Duration::from_secs(timer.seconds)));
                    if let Some(utc) = timer.started_at {
                        metadata.push_str(&format!(", started_at: {}", format_start_time(utc)));
                    }
                    let progress =
                        (1.0 - remaining.as_secs_f64() / timer.seconds as f64).clamp(0.0, 1.0);
                    let filled = (progress * gauge_width as f64).round() as usize;
                    let gauge = Line::from(vec![
                        Span::styled("━".repeat(filled), Style::default().fg(ACCENT)),
                        Span::styled(
                            "─".repeat(gauge_width - filled),
                            Style::default().fg(MUTED).dim(),
                        ),
                    ]);
                    f.render_widget(
                        Paragraph::new(vec![
                            Line::styled(metadata, Style::default().fg(MUTED)),
                            Line::styled(clock(remaining), Style::default().fg(ACCENT).bold()),
                            gauge,
                        ])
                        .block(card),
                        bounds,
                    );
                }
            } else if let Some(t) = self.timers.get(self.selected) {
                let spacious = inside.height >= 20;
                let gap = if spacious { 2 } else { 1 };
                let memo_height = if t.memo.is_empty() {
                    0
                } else {
                    let width = usize::from(inside.width.saturating_sub(4)).max(1);
                    (Text::from(t.memo.as_str()).width().div_ceil(width) as u16 + 2).min(5)
                };
                let parts = Layout::vertical([
                    Constraint::Length(if spacious { 2 } else { 1 }),
                    Constraint::Min(1),
                    Constraint::Length(gap),
                    Constraint::Length(1),
                    Constraint::Length(gap),
                    Constraint::Length(memo_height),
                    Constraint::Length(
                        u16::from(t.started_at.is_some()) + if spacious { 2 } else { 0 },
                    ),
                ])
                .margin(1)
                .split(inside);
                f.render_widget(
                    Paragraph::new(t.label()).style(Style::default().fg(ACCENT).bold()),
                    parts[0],
                );
                if memo_height > 0 {
                    f.render_widget(
                        Paragraph::new(t.memo.as_str())
                            .wrap(Wrap { trim: true })
                            .block(
                                Block::bordered()
                                    .title(" MEMO ")
                                    .border_style(Style::default().fg(MUTED)),
                            ),
                        parts[5],
                    );
                }
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
                let mut details = Vec::new();
                if let Some(utc) = t.started_at {
                    details.push(Line::from(format!(
                        "Started at  {}",
                        format_start_time(utc)
                    )));
                }
                if spacious {
                    details.push(Line::from(format!(
                        "Duration    {}",
                        clock(Duration::from_secs(t.seconds))
                    )));
                    details.push(Line::from(format!("Started     {} times", t.uses)));
                }
                f.render_widget(
                    Paragraph::new(details).style(Style::default().fg(MUTED)),
                    parts[6],
                );
            }
        }
        f.render_widget(
            Paragraph::new(self.status.as_str())
                .style(Style::default().fg(ACCENT))
                .wrap(Wrap { trim: true }),
            rows[2],
        );
        f.render_widget(Paragraph::new("n new   Space start/pause   r reset   e edit   d delete\n↑/↓ select   / jump   f favorite   J/K rank   s sort   q quit").style(Style::default().fg(MUTED)), rows[3]);
        match &self.mode {
            Mode::Browse => {}
            Mode::Jump { number, error } => {
                let popup = popup(area, 58, 7);
                f.render_widget(Clear, popup);
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(format!("/{number}▏")),
                        Line::from("Use the number shown in the timer list."),
                        Line::from(Span::styled(
                            error.as_str(),
                            Style::default().fg(Color::LightRed),
                        )),
                        Line::from("Enter jump · Backspace erase · Esc cancel"),
                    ])
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::bordered()
                            .title(" JUMP TO TIMER ")
                            .border_style(Style::default().fg(ACCENT)),
                    )
                    .style(Style::default().bg(BG)),
                    popup,
                );
            }
            Mode::Form {
                edit,
                name,
                duration,
                memo,
                field,
                error,
            } => {
                let popup = popup(area, 66, 16);
                f.render_widget(Clear, popup);
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} Name (optional)  {}",
                            if *field == 0 { "›" } else { " " },
                            name
                        ),
                        Style::default().fg(if *field == 0 { ACCENT } else { Color::White }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} Duration  {}",
                            if *field == 1 { "›" } else { " " },
                            duration
                        ),
                        Style::default().fg(if *field == 1 { ACCENT } else { Color::White }),
                    )),
                    Line::from("   e.g. 1h / 6m 10s / 100s / 1h30m"),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} Memo (optional)  {}",
                            if *field == 2 { "›" } else { " " },
                            memo
                        ),
                        Style::default().fg(if *field == 2 { ACCENT } else { Color::Reset }),
                    )),
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
fn format_start_time(utc: time::OffsetDateTime) -> String {
    let (started_at, zone) = match time::UtcOffset::local_offset_at(utc) {
        Ok(offset) => (utc.to_offset(offset), ""),
        Err(_) => (utc, " UTC"),
    };
    format!("{:02}:{:02}{zone}", started_at.hour(), started_at.minute(),)
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
    #[cfg(target_os = "macos")]
    let mut awake = awake::Awake::default();
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
            #[cfg(target_os = "macos")]
            if let Err(e) = awake.update(app.timers.iter().any(|timer| timer.started.is_some())) {
                app.status = format!("Could not keep Mac awake: {e}");
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
    fn jump_selects_multi_digit_numbers_without_starting_timers() {
        let mut app = App {
            timers: (1..=12)
                .map(|n| Timer::new(format!("Timer {n}"), 60))
                .collect(),
            selected: 0,
            mode: Mode::Browse,
            status: String::new(),
            path: PathBuf::new(),
            list: ListState::default(),
        };
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('0'));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.mode, Mode::Jump { error, .. } if !error.is_empty()));
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Char('1'));
        press(&mut app, KeyCode::Char('2'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.selected, 11);
        assert!(matches!(app.mode, Mode::Browse));
        assert!(app.timers.iter().all(|t| t.started.is_none()));
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('1'));
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.selected, 11);
        app.timers.clear();
        press(&mut app, KeyCode::Char('/'));
        assert!(matches!(app.mode, Mode::Browse));
    }
    #[test]
    fn memo_can_be_created_edited_and_cleared() {
        let path = std::env::temp_dir().join(format!(
            "tmr-memo-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App {
            timers: vec![],
            selected: 0,
            mode: Mode::Browse,
            status: String::new(),
            path: path.clone(),
            list: ListState::default(),
        };
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Tab);
        for c in "10s".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Tab);
        for c in "Tea 🫖".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(storage::load(&path).unwrap()[0].memo, "Tea 🫖");
        assert!(app.timers[0].started.is_some());
        press(&mut app, KeyCode::Char('e'));
        // Moving backward from Name reaches Memo.
        press(&mut app, KeyCode::BackTab);
        press(&mut app, KeyCode::Char('!'));
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.timers[0].memo, "Tea 🫖");
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::BackTab);
        for _ in 0..5 {
            press(&mut app, KeyCode::Backspace);
        }
        press(&mut app, KeyCode::Enter);
        assert!(storage::load(&path).unwrap()[0].memo.is_empty());
        std::fs::remove_file(path).unwrap();
    }
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
        app.timers[0].memo = "Take a break afterward".into();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer();
        let right_panel: String = (4..26)
            .flat_map(|y| (34..99).map(move |x| buffer[(x, y)].symbol()))
            .collect();
        assert!(right_panel.contains("MEMO"));
        assert!(right_panel.contains("Take a break afterward"));
        app.mode = Mode::Form {
            edit: true,
            name: "Focus".into(),
            duration: "25:00".into(),
            memo: "Take a short break afterward".into(),
            field: 1,
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
