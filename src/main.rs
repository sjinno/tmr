mod alarm;
#[cfg(target_os = "macos")]
mod awake;
mod block_clock;
mod model;
mod settings;
mod shortcuts;
mod storage;
mod theme;
mod world_clock;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use model::{Timer, parse_duration};
use ratatui::{
    prelude::*,
    widgets::{Block, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Wrap},
};
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use theme::{ACCENT, BG, MINT, MUTED};

enum Mode {
    Browse,
    Settings {
        duration: String,
        error: String,
        clocks: bool,
    },
    Clocks,
    Choose {
        stopwatch: bool,
    },
    Jump {
        number: String,
        error: String,
    },
    Form {
        edit: bool,
        stopwatch: bool,
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
    clocks: world_clock::WorldClocks,
    settings: settings::Settings,
    selected: usize,
    mode: Mode,
    status: String,
    path: PathBuf,
    list: ListState,
}
impl App {
    fn persist(&mut self) -> bool {
        match storage::save(&self.path, &self.timers)
            .and_then(|()| self.clocks.save(&self.path.with_extension("clocks.json")))
            .and_then(|()| {
                self.settings
                    .save(&self.path.with_extension("settings.json"))
            }) {
            Ok(()) => true,
            Err(e) => {
                self.status =
                    format!("save failed: {e}. changes remain in memory; retry before quitting.");
                false
            }
        }
    }
    fn key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return self.persist();
        }
        if key.code == KeyCode::Char(',')
            && (matches!(self.mode, Mode::Browse)
                || (matches!(self.mode, Mode::Clocks) && self.clocks.browsing()))
        {
            self.mode = Mode::Settings {
                duration: self
                    .settings
                    .alarm_seconds
                    .map(|seconds| format!("{seconds}s"))
                    .unwrap_or_default(),
                error: String::new(),
                clocks: matches!(self.mode, Mode::Clocks),
            };
            return false;
        }
        if matches!(self.mode, Mode::Clocks) {
            if self.clocks.browsing() {
                match key.code {
                    KeyCode::Char('t' | 'c') | KeyCode::Esc => {
                        self.mode = Mode::Browse;
                        return false;
                    }
                    KeyCode::Char('q') => return self.persist(),
                    _ => {}
                }
            }
            if self.clocks.key(key) {
                self.persist();
            }
            return false;
        }
        match &mut self.mode {
            Mode::Settings {
                duration,
                error,
                clocks,
            } => match key.code {
                KeyCode::Esc => self.mode = if *clocks { Mode::Clocks } else { Mode::Browse },
                KeyCode::Backspace => {
                    duration.pop();
                    error.clear();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if duration.len() < 80 {
                        duration.push(c);
                    }
                }
                KeyCode::Enter => {
                    let parsed = if duration.trim().is_empty() {
                        Ok(None)
                    } else {
                        parse_duration(duration).map(Some)
                    };
                    match parsed {
                        Err(e) => *error = e,
                        Ok(seconds) => {
                            self.settings.alarm_seconds = seconds;
                            self.settings.dirty = true;
                            self.mode = if *clocks { Mode::Clocks } else { Mode::Browse };
                            self.status = "alarm duration saved.".into();
                            self.persist();
                        }
                    }
                }
                _ => {}
            },
            Mode::Clocks => unreachable!(),
            Mode::Choose { stopwatch } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab => {
                    *stopwatch = !*stopwatch
                }
                KeyCode::Char('t') => *stopwatch = false,
                KeyCode::Char('s') => *stopwatch = true,
                KeyCode::Enter => {
                    self.mode = Mode::Form {
                        edit: false,
                        stopwatch: *stopwatch,
                        name: String::new(),
                        duration: String::new(),
                        memo: String::new(),
                        field: 0,
                        error: String::new(),
                    }
                }
                _ => {}
            },
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
                            self.status = format!("selected timer #{n}. space to start / pause.");
                            self.mode = Mode::Browse;
                        }
                        None => {
                            *error =
                                format!("enter a timer number from 1 to {}.", self.timers.len())
                        }
                    }
                }
                _ => {}
            },
            Mode::Form {
                edit,
                stopwatch,
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
                KeyCode::Enter => match if *stopwatch && duration.trim().is_empty() {
                    Ok(0)
                } else {
                    parse_duration(duration)
                } {
                    Err(e) => *error = e,
                    Ok(seconds) => {
                        if *edit {
                            let t = &mut self.timers[self.selected];
                            t.name = name.trim().to_string();
                            t.memo = memo.trim().to_string();
                            t.seconds = seconds;
                            t.stopwatch = *stopwatch;
                            t.reset();
                            self.status = "saved. space to start.".into();
                        } else {
                            let mut timer = Timer::new(name.trim().to_string(), seconds);
                            timer.stopwatch = *stopwatch;
                            timer.reset();
                            timer.memo = memo.trim().to_string();
                            timer.toggle(Instant::now());
                            self.timers.push(timer);
                            self.selected = self.timers.len() - 1;
                            self.status = "created and running. space to pause.".into();
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
                    self.status = "timer deleted.".into();
                    self.persist();
                }
                KeyCode::Esc | KeyCode::Char('n') => self.mode = Mode::Browse,
                _ => {}
            },
            Mode::Browse => match key.code {
                code @ (KeyCode::Up | KeyCode::Down)
                    if key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    if !self.timers.is_empty() {
                        let next = if code == KeyCode::Up {
                            self.selected.saturating_sub(1)
                        } else {
                            (self.selected + 1).min(self.timers.len() - 1)
                        };
                        if next != self.selected {
                            self.timers.swap(self.selected, next);
                            self.selected = next;
                            self.persist();
                        }
                    }
                }
                KeyCode::Char('c') => self.mode = Mode::Clocks,
                KeyCode::Char('/') => {
                    if self.timers.is_empty() {
                        self.status = "no timers yet. press n to create one.".into();
                    } else {
                        self.mode = Mode::Jump {
                            number: String::new(),
                            error: String::new(),
                        };
                    }
                }
                KeyCode::Char('q') => return self.persist(),
                KeyCode::Char('n') => self.mode = Mode::Choose { stopwatch: false },
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
                    self.status = "sorted by favorites, then most used.".into();
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
                                stopwatch: t.stopwatch,
                                name: t.name.clone(),
                                duration: if t.stopwatch && t.seconds == 0 {
                                    String::new()
                                } else {
                                    format!("{}s", t.seconds)
                                },
                                memo: t.memo.clone(),
                                field: 0,
                                error: String::new(),
                            }
                        }
                        KeyCode::Char('d') => self.mode = Mode::Delete,
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
                Paragraph::new("tmr · please resize to at least 44 × 16.\nq to quit")
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        if matches!(self.mode, Mode::Clocks) {
            self.clocks.draw(f, &self.status);
            return;
        }
        let page = theme::page(area);
        let rows = Layout::vertical([
            Constraint::Length(theme::header_height(area.height)),
            Constraint::Min(5),
            Constraint::Length(if area.height < 22 { 1 } else { 2 }),
            Constraint::Length(shortcuts::height(page.width, shortcuts::TIMERS, 2)),
        ])
        .split(page);
        let running = self.timers.iter().filter(|t| t.started.is_some()).count();
        theme::header(f, rows[0], "");
        let counts_area = Rect::new(
            rows[0].x + 17,
            rows[0].y,
            rows[0].width.saturating_sub(17),
            rows[0].height.min(6),
        );
        let saved_label = self.timers.len().to_string();
        let running_label = running.to_string();
        let count_width = (saved_label.len().max(running_label.len()) as u16 * 5 + 7)
            .max(15)
            .min(counts_area.width.saturating_sub(2) / 2);
        let counts = Layout::horizontal([
            Constraint::Length(count_width),
            Constraint::Length(count_width),
            Constraint::Min(0),
        ])
        .spacing(1)
        .split(counts_area);
        for (bounds, title, label, color) in [
            (counts[0], " saved ", saved_label.as_str(), ACCENT),
            (
                counts[1],
                " running ",
                running_label.as_str(),
                if running > 0 { MINT } else { MUTED },
            ),
        ] {
            let panel = theme::panel(title)
                .padding(Padding::new(1, 1, u16::from(bounds.height >= 4), 0))
                .border_style(Style::default().fg(color));
            let inner = panel.inner(bounds);
            f.render_widget(panel, bounds);
            block_clock::render_small(f, inner, label, color);
        }
        let columns = if area.width >= 85 {
            Layout::horizontal([
                Constraint::Length((area.width / 3).clamp(32, 48)),
                Constraint::Min(1),
            ])
            .spacing(1)
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
                    "done"
                } else if t.started.is_some() {
                    "run"
                } else if t.started_at.is_some() {
                    "pause"
                } else {
                    "ready"
                };
                let id = block_clock::small_digits(&format!("{:02}", i + 1));
                let id_width = id[0].chars().count();
                let title = Line::from(vec![
                    Span::raw(if t.favorite { "★ " } else { "" }),
                    Span::styled(t.label(), Style::default().bold()),
                ]);
                let timing = Line::from(vec![
                    Span::styled(
                        t.display(now),
                        Style::default().fg(if t.started.is_some() { MINT } else { MUTED }),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        state,
                        Style::default()
                            .fg(if t.started.is_some() { MINT } else { MUTED })
                            .bold(),
                    ),
                ]);
                let details = Line::from(vec![
                    Span::styled(
                        if t.stopwatch { "stopwatch" } else { "timer" },
                        Style::default().fg(if t.stopwatch { MINT } else { ACCENT }),
                    ),
                    Span::styled(format!(" · {} uses", t.uses), Style::default().fg(MUTED)),
                ]);
                let mut lines: Vec<Line> = [title, timing, details]
                    .into_iter()
                    .enumerate()
                    .map(|(row, text)| {
                        let mut spans = vec![
                            Span::styled(id[row].clone(), Style::default().fg(ACCENT)),
                            Span::raw("  "),
                        ];
                        spans.extend(text.spans);
                        Line::from(spans)
                    })
                    .collect();
                if !t.memo.is_empty() {
                    lines.push(Line::styled(
                        format!("{}{}", " ".repeat(id_width + 2), t.memo),
                        Style::default().fg(MUTED),
                    ));
                }
                lines.push(Line::default());
                ListItem::new(lines)
            })
            .collect();
        self.list.select(if self.timers.is_empty() {
            None
        } else {
            Some(self.selected)
        });
        let block = theme::panel(" your timers ")
            .padding(Padding::new(1, 1, 1, 1))
            .border_style(Style::default().fg(MUTED));
        if self.timers.is_empty() {
            f.render_widget(Paragraph::new("\n  nothing on the clock.\n\n  press n to create your first timer.\n  tea, a deep work session, a quick break…").block(block).wrap(Wrap { trim: false }), columns[0]);
        } else {
            f.render_stateful_widget(
                List::new(items)
                    .block(block)
                    .highlight_style(Style::default().bold())
                    .highlight_symbol("▎"),
                columns[0],
                &mut self.list,
            );
        }
        if columns[1].width > 0 {
            let panel = theme::panel(" on the clock ").border_style(Style::default().fg(MUTED));
            let inside = panel.inner(columns[1]);
            f.render_widget(panel, columns[1]);
            if running > 1 {
                let [summary, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)])
                    .horizontal_margin(1)
                    .areas(inside);
                let active: Vec<_> = self
                    .timers
                    .iter()
                    .enumerate()
                    .filter(|(_, timer)| timer.started.is_some())
                    .collect();
                const BOX_HEIGHT: u16 = 7;
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
                    let card = theme::panel("")
                        .title(Line::styled(
                            format!(" #{:02} {} ", index + 1, timer.label()),
                            Style::default().bold(),
                        ))
                        .border_style(Style::default().fg(border))
                        .padding(Padding::new(2, 2, 1, 1));
                    let inner = card.inner(bounds);
                    let gauge_width = usize::from(inner.width);
                    let mut metadata = timer.setting();
                    if let Some(utc) = timer.started_at {
                        metadata.push_str(&format!(", started_at: {}", format_start_time(utc)));
                    }
                    let progress = timer.progress(now);
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
                            Line::styled(timer.display(now), Style::default().fg(ACCENT).bold()),
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
                    Constraint::Length(5),
                ])
                .margin(1)
                .split(inside);
                f.render_widget(
                    Paragraph::new(t.label())
                        .centered()
                        .style(Style::default().fg(ACCENT).bold()),
                    parts[0],
                );
                if memo_height > 0 {
                    f.render_widget(
                        Paragraph::new(t.memo.as_str())
                            .wrap(Wrap { trim: true })
                            .block(
                                theme::panel("")
                                    .title(" memo ")
                                    .border_style(Style::default().fg(MUTED)),
                            ),
                        parts[5],
                    );
                }
                block_clock::render(f, parts[1], &t.display(now), Color::Reset);
                let ratio = t.progress(now);
                f.render_widget(
                    Gauge::default()
                        .gauge_style(Style::default().fg(ACCENT).bg(BG))
                        .ratio(ratio)
                        .label(if t.finished {
                            "complete"
                        } else if t.started.is_some() {
                            "running"
                        } else {
                            "space to start / resume"
                        }),
                    parts[3],
                );
                let started = t
                    .started_at
                    .map(format_start_time)
                    .unwrap_or_else(|| "--:--".into());
                let duration = if t.stopwatch && t.seconds == 0 {
                    "off".into()
                } else {
                    model::clock(Duration::from_secs(t.seconds))
                };
                let used = t.uses.to_string();
                let headers = [
                    "started at",
                    if t.stopwatch {
                        "alert every"
                    } else {
                        "duration"
                    },
                    "used",
                ];
                let values = [started.as_str(), duration.as_str(), used.as_str()];
                let cell_width = headers
                    .iter()
                    .chain(values.iter())
                    .map(|cell| cell.len())
                    .max()
                    .unwrap_or(0);
                let widths = [cell_width; 3];
                let border = |left: char, joint: char, right: char| {
                    let segments = widths.map(|width| "─".repeat(width + 2));
                    Line::styled(
                        format!("{left}{}{right}", segments.join(&joint.to_string())),
                        Style::default().fg(MUTED),
                    )
                };
                let row = |cells: [&str; 3], style: Style| {
                    let mut spans = vec![Span::styled("│", Style::default().fg(MUTED))];
                    for (cell, width) in cells.into_iter().zip(widths) {
                        spans.push(Span::styled(format!(" {cell:^width$} "), style));
                        spans.push(Span::styled("│", Style::default().fg(MUTED)));
                    }
                    Line::from(spans)
                };
                f.render_widget(
                    Paragraph::new(vec![
                        border('╭', '┬', '╮'),
                        row(headers, Style::default().fg(MUTED)),
                        border('├', '┼', '┤'),
                        row(values, Style::default()),
                        border('╰', '┴', '╯'),
                    ])
                    .centered(),
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
        shortcuts::render(f, rows[3], shortcuts::TIMERS, 2);
        match &self.mode {
            Mode::Settings {
                duration, error, ..
            } => {
                let bounds = popup(area, 64, 11);
                f.render_widget(Clear, bounds);
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(""),
                        Line::styled(
                            format!("alarm duration  {duration}▏"),
                            Style::default().fg(ACCENT),
                        ),
                        Line::from("e.g. 5s / 30s / 1m · blank plays the sound once"),
                        Line::from("repeats or trims the sound to the selected duration"),
                        Line::from("applies to timer and stopwatch alerts"),
                        Line::from(""),
                        Line::styled(error.as_str(), Style::default().fg(Color::LightRed)),
                        Line::from("enter save · esc cancel / silence alarm"),
                    ])
                    .wrap(Wrap { trim: true })
                    .block(theme::panel(" settings "))
                    .style(Style::default().bg(BG)),
                    bounds,
                );
            }
            Mode::Clocks => unreachable!(),
            Mode::Choose { stopwatch } => {
                let popup = popup(area, 50, 6);
                f.render_widget(Clear, popup);
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled(
                            format!("{} timer · count down", if !stopwatch { "›" } else { " " }),
                            Style::default().fg(if !stopwatch { ACCENT } else { MUTED }),
                        )),
                        Line::from(Span::styled(
                            format!(
                                "{} stopwatch · count up",
                                if *stopwatch { "›" } else { " " }
                            ),
                            Style::default().fg(if *stopwatch { ACCENT } else { MUTED }),
                        )),
                        Line::from(""),
                        Line::from("↑/↓ choose · enter continue · esc cancel"),
                    ])
                    .block(theme::panel("").title(" a little moment "))
                    .style(Style::default().bg(BG)),
                    popup,
                );
            }
            Mode::Browse => {}
            Mode::Jump { number, error } => {
                let popup = popup(area, 58, 7);
                f.render_widget(Clear, popup);
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(format!("/{number}▏")),
                        Line::from("use the number shown in the timer list."),
                        Line::from(Span::styled(
                            error.as_str(),
                            Style::default().fg(Color::LightRed),
                        )),
                        Line::from("enter jump · backspace erase · esc cancel"),
                    ])
                    .wrap(Wrap { trim: true })
                    .block(
                        theme::panel("")
                            .title(" jump to timer ")
                            .border_style(Style::default().fg(ACCENT)),
                    )
                    .style(Style::default().bg(BG)),
                    popup,
                );
            }
            Mode::Form {
                edit,
                stopwatch,
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
                            " {} {:15}  {}",
                            if *field == 0 { "›" } else { " " },
                            "name (optional)",
                            name
                        ),
                        Style::default().fg(if *field == 0 { ACCENT } else { Color::Reset }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} {:15}  {}",
                            if *field == 1 { "›" } else { " " },
                            if *stopwatch {
                                "alert every"
                            } else {
                                "duration"
                            },
                            duration
                        ),
                        Style::default().fg(if *field == 1 { ACCENT } else { Color::Reset }),
                    )),
                    Line::from(if *stopwatch {
                        "   e.g. 5m / 30s · blank disables alerts"
                    } else {
                        "   e.g. 1h / 6m 10s / 100s / 1h30m"
                    }),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!(
                            " {} {:15}  {}",
                            if *field == 2 { "›" } else { " " },
                            "memo (optional)",
                            memo
                        ),
                        Style::default().fg(if *field == 2 { ACCENT } else { Color::Reset }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        error.as_str(),
                        Style::default().fg(Color::LightRed),
                    )),
                    Line::from("tab switch · backspace erase · enter save · esc cancel"),
                ];
                f.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .block(
                            theme::panel("")
                                .title(if *stopwatch {
                                    if *edit {
                                        " edit stopwatch · saving resets it "
                                    } else {
                                        " new stopwatch "
                                    }
                                } else if *edit {
                                    " edit timer · saving resets it "
                                } else {
                                    " new timer "
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
                        "delete “{}”?\n\nenter / y delete · esc / n cancel",
                        self.timers[self.selected].label()
                    ))
                    .wrap(Wrap { trim: true })
                    .block(
                        theme::panel("")
                            .title(" delete timer ")
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
        Err(_) => (utc, " utc"),
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
        .map_err(|e| io::Error::other(format!("could not load {}: {e}", path.display())))?;
    let mut app = App {
        timers,
        clocks: world_clock::WorldClocks::load(&path.with_extension("clocks.json"))?,
        settings: settings::Settings::load(&path.with_extension("settings.json"))?,
        selected: 0,
        mode: Mode::Browse,
        status: String::new(),
        path,
        list: ListState::default(),
    };
    let mut alarm = alarm::Alarm::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&shutdown))?;
    }
    #[cfg(target_os = "macos")]
    let mut awake = awake::Awake::default();
    ratatui::run(|terminal| -> io::Result<()> {
        loop {
            if shutdown.load(Ordering::Relaxed) {
                return Ok(());
            }
            if alarm.poll().is_err() {
                print!("\x07");
                io::stdout().flush()?;
                app.status = "audio playback failed; used terminal bell instead.".into();
            }
            let now = Instant::now();
            let mut completed = Vec::new();
            for t in &mut app.timers {
                if t.tick(now) {
                    completed.push(if t.stopwatch {
                        format!("{} · interval alert at {}", t.label(), t.display(now))
                    } else {
                        format!("{} · finished", t.label())
                    });
                }
            }
            if !completed.is_empty() {
                app.status = completed.join(", ");
                if alarm.play(app.settings.alarm_seconds).is_err() {
                    print!("\x07");
                    io::stdout().flush()?;
                    app.status
                        .push_str(" · audio unavailable; used terminal bell.");
                }
            }
            #[cfg(target_os = "macos")]
            if let Err(e) = awake.update(app.timers.iter().any(|timer| timer.started.is_some())) {
                app.status = format!("could not keep mac awake: {e}");
            }
            terminal.draw(|f| app.draw(f))?;
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                if key.code == KeyCode::Esc {
                    alarm.stop();
                }
                if app.key(key) {
                    return Ok(());
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alarm_settings_save_and_return_to_clocks() {
        let path = std::env::temp_dir().join(format!(
            "tmr-settings-ui-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App {
            timers: vec![],
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
            selected: 0,
            mode: Mode::Clocks,
            status: String::new(),
            path: path.clone(),
            list: ListState::default(),
        };
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        press(&mut app, KeyCode::Char(','));
        press(&mut app, KeyCode::Char('0'));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.mode, Mode::Settings { error, .. } if !error.is_empty()));
        press(&mut app, KeyCode::Backspace);
        for c in "5s".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::Clocks));
        assert_eq!(
            settings::Settings::load(&path.with_extension("settings.json"))
                .unwrap()
                .alarm_seconds,
            Some(5)
        );
        press(&mut app, KeyCode::Char(','));
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.alarm_seconds, None);
        std::fs::remove_file(path.with_extension("settings.json")).unwrap();
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn alt_arrows_reorder_and_keep_selection_with_timer() {
        let path = std::env::temp_dir().join(format!(
            "tmr-reorder-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App {
            timers: vec![Timer::new("a".into(), 10), Timer::new("b".into(), 20)],
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
            selected: 0,
            mode: Mode::Browse,
            status: String::new(),
            path: path.clone(),
            list: ListState::default(),
        };
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!(app.selected, 1);
        assert_eq!(app.timers[1].name, "a");
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!(app.selected, 1);
        app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected, 0);
        assert_eq!(app.timers[0].name, "b");
        app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert_eq!(app.selected, 0);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!(app.timers[1].name, "b");
        assert_eq!(storage::load(&path).unwrap()[1].name, "b");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn stopwatch_creation_and_editing() {
        let path = std::env::temp_dir().join(format!(
            "tmr-stopwatch-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App {
            timers: vec![],
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
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
        assert!(matches!(app.mode, Mode::Choose { .. }));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(
            app.mode,
            Mode::Form {
                stopwatch: true,
                ..
            }
        ));
        press(&mut app, KeyCode::Tab);
        for c in "30s".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.timers[0].stopwatch);
        assert_eq!(app.timers[0].seconds, 30);
        assert!(app.timers[0].started.is_some());
        assert_eq!(app.timers[0].display(Instant::now()), "00:00");
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Tab);
        for _ in 0..3 {
            press(&mut app, KeyCode::Backspace);
        }
        press(&mut app, KeyCode::Enter);
        let saved = storage::load(&path).unwrap();
        assert!(saved[0].stopwatch);
        assert_eq!(saved[0].seconds, 0);
        assert!(saved[0].started.is_none());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn jump_selects_multi_digit_numbers_without_starting_timers() {
        let mut app = App {
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
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
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
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
        press(&mut app, KeyCode::Enter);
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
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
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
        assert!(text.contains("nothing on the clock"));
        app.timers.push(Timer::new("Focus".into(), 1500));
        app.timers[0].toggle(Instant::now());
        app.timers[0].memo = "Take a break afterward".into();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer();
        let run_position = (8..22)
            .find_map(|y| {
                (2..30)
                    .find(|&x| {
                        buffer[(x, y)].symbol() == "r"
                            && buffer[(x + 1, y)].symbol() == "u"
                            && buffer[(x + 2, y)].symbol() == "n"
                    })
                    .map(|x| (x, y))
            })
            .expect("running label in the selected timer row");
        assert_eq!(buffer[run_position].fg, MINT);
        let (x, y) = run_position;
        // The countdown immediately precedes the two spaces before `run`.
        assert_eq!(buffer[(x - 3, y)].fg, MINT);
        let right_panel: String = (4..26)
            .flat_map(|y| (34..99).map(move |x| buffer[(x, y)].symbol()))
            .collect();
        assert!(right_panel.contains("memo"));
        assert!(right_panel.contains("Take a break afterward"));
        app.mode = Mode::Form {
            edit: true,
            stopwatch: false,
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
        assert!(text.contains("edit timer"));
        for (width, height) in [(44, 16), (20, 5), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| app.draw(f)).unwrap();
        }
    }
}
