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

use theme::{ACCENT, BG, BLUE, MINT, MUTED, PEACH};

enum Mode {
    Browse,
    Settings {
        duration: String,
        error: String,
        clocks: bool,
    },
    Clocks,
    Choose {
        selected: usize,
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
        rest: Option<String>,
        rest_cache: String,
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
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('n' | 'p'))
            && let Mode::Form {
                stopwatch,
                rest,
                rest_cache,
                field,
                error,
                ..
            } = &mut self.mode
        {
            let pomodoro = rest.is_some();
            let memo_focused = *field == if pomodoro { 3 } else { 2 };
            let rest_focused = pomodoro && *field == 2;
            let current = if pomodoro { 2 } else { usize::from(*stopwatch) };
            let next = (current + if key.code == KeyCode::Char('n') { 1 } else { 2 }) % 3;
            if let Some(value) = rest.take() {
                *rest_cache = value;
            }
            *stopwatch = next != 0;
            *rest = (next == 2).then(|| rest_cache.clone());
            if memo_focused {
                *field = if next == 2 { 3 } else { 2 };
            } else if rest_focused && next != 2 {
                *field = 1;
            }
            error.clear();
            return false;
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
            Mode::Choose { selected } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Down | KeyCode::Tab => *selected = (*selected + 1) % 3,
                KeyCode::Up | KeyCode::BackTab => *selected = (*selected + 2) % 3,
                KeyCode::Char('t') => *selected = 0,
                KeyCode::Char('s') => *selected = 1,
                KeyCode::Char('p') => *selected = 2,
                KeyCode::Enter => {
                    self.mode = Mode::Form {
                        edit: false,
                        stopwatch: *selected != 0,
                        name: String::new(),
                        duration: String::new(),
                        rest: (*selected == 2).then(String::new),
                        rest_cache: String::new(),
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
                rest,
                memo,
                field,
                error,
                ..
            } => match key.code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Tab | KeyCode::Down => {
                    *field = (*field + 1) % if rest.is_some() { 4 } else { 3 }
                }
                KeyCode::BackTab | KeyCode::Up => {
                    *field = (*field + if rest.is_some() { 3 } else { 2 })
                        % if rest.is_some() { 4 } else { 3 }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let text = match *field {
                        0 => name,
                        1 => duration,
                        2 => rest.as_mut().unwrap_or(memo),
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
                        2 => rest.as_mut().unwrap_or(memo),
                        _ => memo,
                    }
                    .pop();
                }
                KeyCode::Enter => match (|| {
                    let seconds = if *stopwatch && rest.is_none() && duration.trim().is_empty() {
                        0
                    } else {
                        parse_duration(duration).map_err(|e| format!("interval: {e}"))?
                    };
                    let rest_seconds = rest
                        .as_deref()
                        .map(parse_duration)
                        .transpose()
                        .map_err(|e| format!("rest interval: {e}"))?;
                    Ok::<_, String>((seconds, rest_seconds))
                })() {
                    Err(e) => *error = e,
                    Ok((seconds, rest_seconds)) => {
                        if *edit {
                            let t = &mut self.timers[self.selected];
                            t.name = name.trim().to_string();
                            t.memo = memo.trim().to_string();
                            t.seconds = seconds;
                            t.stopwatch = *stopwatch;
                            t.rest_seconds = rest_seconds;
                            t.reset();
                            self.status = "saved. space to start.".into();
                        } else {
                            let mut timer = Timer::new(name.trim().to_string(), seconds);
                            timer.stopwatch = *stopwatch;
                            timer.rest_seconds = rest_seconds;
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
                KeyCode::Char('n') => self.mode = Mode::Choose { selected: 0 },
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
                                rest: t.rest_seconds.map(|seconds| format!("{seconds}s")),
                                rest_cache: String::new(),
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
                let state = if t.started.is_some() && t.rest_seconds.is_some() {
                    t.phase(now).map(|(phase, _, _)| phase).unwrap_or("run")
                } else if t.finished {
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
                let resting = matches!(t.phase(now), Some(("rest", _, _)));
                let timing_color = if t.finished {
                    Color::LightRed
                } else if t.started.is_none() {
                    MUTED
                } else if resting {
                    PEACH
                } else {
                    MINT
                };
                let timing = Line::from(vec![
                    Span::styled(t.display(now), Style::default().fg(timing_color)),
                    Span::raw("  "),
                    Span::styled(state, Style::default().fg(timing_color).bold()),
                ]);
                let details = Line::from(vec![
                    Span::styled(
                        t.kind_name(),
                        Style::default().fg(if t.rest_seconds.is_some() {
                            PEACH
                        } else if t.stopwatch {
                            MINT
                        } else {
                            BLUE
                        }),
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
                struct Card<'a> {
                    index: usize,
                    timer: &'a Timer,
                    details: Text<'static>,
                    memo: Text<'static>,
                    height: u16,
                }
                let card_width = list.width.min(86);
                let content_width = card_width.saturating_sub(6);
                let active: Vec<_> = self
                    .timers
                    .iter()
                    .enumerate()
                    .filter(|(_, timer)| timer.started.is_some())
                    .map(|(index, timer)| {
                        let mut details = vec![format!(
                            "started at: {}",
                            timer
                                .started_at
                                .map(format_start_time)
                                .unwrap_or_else(|| "--:--".into())
                        )];
                        if let Some(rest) = timer.rest_seconds {
                            if let Some((phase, _, _)) = timer.phase(now) {
                                details.push(format!("phase: {phase}"));
                            }
                            details.push(format!(
                                "work: {}",
                                model::clock(Duration::from_secs(timer.seconds))
                            ));
                            details
                                .push(format!("rest: {}", model::clock(Duration::from_secs(rest))));
                            details.push(format!(
                                "intervals: {}",
                                timer.completed_work_intervals(now).unwrap_or(0)
                            ));
                        } else if timer.stopwatch {
                            details.push(if timer.seconds == 0 {
                                "alert: off".into()
                            } else {
                                format!(
                                    "alert: {}",
                                    model::clock(Duration::from_secs(timer.seconds))
                                )
                            });
                            details.push(format!(
                                "laps: {}",
                                timer
                                    .completed_laps(now)
                                    .map(|count| count.to_string())
                                    .unwrap_or_else(|| "--".into())
                            ));
                        } else {
                            details.push(format!(
                                "duration: {}",
                                model::clock(Duration::from_secs(timer.seconds))
                            ));
                        }
                        details.push(format!("used: {}", timer.uses));
                        let details = wrap_card_text(&details.join(" | "), content_width);
                        let memo =
                            wrap_card_text(&timer.memo, content_width.min(80).saturating_sub(4));
                        let memo_height = if timer.memo.is_empty() {
                            0
                        } else {
                            memo.height() as u16 + 3
                        };
                        let height = 6 + details.height() as u16 + memo_height;
                        Card {
                            index,
                            timer,
                            details,
                            memo,
                            height,
                        }
                    })
                    .collect();
                let selected = active
                    .iter()
                    .position(|card| card.index == self.selected)
                    .unwrap_or(0);
                let mut pages = Vec::new();
                let mut first = 0;
                let mut used_height = 0;
                for (index, card) in active.iter().enumerate() {
                    if index > first && used_height + 1 + card.height > list.height {
                        pages.push(first..index);
                        first = index;
                        used_height = 0;
                    }
                    used_height += u16::from(index > first) + card.height;
                }
                pages.push(first..active.len());
                let page = pages
                    .iter()
                    .find(|page| page.contains(&selected))
                    .expect("selected running card");
                let heading = if page.len() < running {
                    format!(
                        "{running} running · showing {}–{}",
                        page.start + 1,
                        page.end
                    )
                } else {
                    format!("{running} running")
                };
                f.render_widget(
                    Paragraph::new(heading).style(Style::default().fg(MUTED)),
                    summary,
                );
                let mut y = list.y;
                for card in &active[page.clone()] {
                    let bounds = Rect::new(
                        list.x,
                        y,
                        card_width,
                        card.height.min(list.bottom().saturating_sub(y)),
                    );
                    let timer = card.timer;
                    let panel = theme::panel(format!(" #{:02} {} ", card.index + 1, timer.label()))
                        .border_style(Style::default().fg(if card.index == self.selected {
                            ACCENT
                        } else {
                            MUTED
                        }))
                        .padding(Padding::new(2, 2, 1, 1));
                    let inner = panel.inner(bounds);
                    f.render_widget(panel, bounds);
                    let memo_height = if timer.memo.is_empty() {
                        0
                    } else {
                        card.memo.height() as u16 + 2
                    };
                    let [details, countdown, gauge, _, memo] = Layout::vertical([
                        Constraint::Length(card.details.height() as u16),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(u16::from(memo_height > 0)),
                        Constraint::Length(memo_height),
                    ])
                    .areas(inner);
                    f.render_widget(
                        Paragraph::new(card.details.clone()).style(Style::default().fg(MUTED)),
                        details,
                    );
                    let color = if matches!(timer.phase(now), Some(("rest", _, _))) {
                        PEACH
                    } else {
                        ACCENT
                    };
                    f.render_widget(
                        Paragraph::new(timer.display(now)).style(Style::default().fg(color).bold()),
                        countdown,
                    );
                    let gauge = Rect::new(gauge.x, gauge.y, gauge.width.min(80), gauge.height);
                    let filled = (timer.progress(now) * f64::from(gauge.width)).round() as usize;
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled("━".repeat(filled), Style::default().fg(color)),
                            Span::styled(
                                "─".repeat(usize::from(gauge.width) - filled),
                                Style::default().fg(MUTED).dim(),
                            ),
                        ])),
                        gauge,
                    );
                    if memo_height > 0 {
                        let memo = Rect::new(memo.x, memo.y, memo.width.min(80), memo.height);
                        f.render_widget(
                            Paragraph::new(card.memo.clone()).block(theme::panel(" memo ")),
                            memo,
                        );
                    }
                    y = bounds.bottom().saturating_add(1);
                }
            } else if let Some(t) = self.timers.get(self.selected) {
                let spacious = inside.height >= 20;
                let gap = if spacious { 2 } else { 1 };
                let memo_text = wrap_card_text(
                    &t.memo,
                    inside.width.saturating_sub(2).min(80).saturating_sub(4),
                );
                let memo_height = if t.memo.is_empty() {
                    0
                } else {
                    (memo_text.height() as u16 + 2).min(5)
                };
                let countdown_height = if inside.height >= memo_height + 22 {
                    7
                } else {
                    5
                };
                let parts = Layout::vertical([
                    Constraint::Length(if spacious { 2 } else { 1 }),
                    Constraint::Min(0),
                    Constraint::Length(countdown_height),
                    Constraint::Length(4),
                    Constraint::Length(gap),
                    Constraint::Length(5),
                    Constraint::Length(memo_height),
                    Constraint::Min(0),
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
                        Paragraph::new(memo_text).block(
                            theme::panel("")
                                .title(" memo ")
                                .border_style(Style::default().fg(MUTED)),
                        ),
                        Rect::new(
                            parts[6].x + (parts[6].width - parts[6].width.min(80)) / 2,
                            parts[6].y,
                            parts[6].width.min(80),
                            parts[6].height,
                        ),
                    );
                }
                let resting = matches!(t.phase(now), Some(("rest", _, _)));
                block_clock::render(
                    f,
                    parts[2],
                    &t.display(now),
                    if t.finished {
                        Color::LightRed
                    } else if resting {
                        PEACH
                    } else {
                        Color::Reset
                    },
                );
                let ratio = t.progress(now);
                let gauge_width = parts[3].width.min(80);
                let gauge_area = Rect::new(
                    parts[3].x + (parts[3].width - gauge_width) / 2,
                    parts[3].y.saturating_add(3),
                    gauge_width,
                    parts[3].height.saturating_sub(3),
                );
                f.render_widget(
                    Gauge::default()
                        .gauge_style(
                            Style::default()
                                .fg(if resting { PEACH } else { ACCENT })
                                .bg(MUTED),
                        )
                        .ratio(ratio)
                        .label(Span::styled(
                            if let Some((phase, _, _)) = t.phase(now) {
                                phase
                            } else if t.finished {
                                "complete"
                            } else if t.started.is_some() {
                                "running"
                            } else {
                                "space to start / resume"
                            },
                            Style::default().fg(Color::Rgb(25, 27, 35)).bold(),
                        )),
                    gauge_area,
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
                let mut headers = vec![
                    "started at",
                    if t.rest_seconds.is_some() {
                        "work"
                    } else if t.stopwatch {
                        "alert every"
                    } else {
                        "duration"
                    },
                    "used",
                ];
                let completed = t
                    .completed_work_intervals(now)
                    .map(|count| count.to_string());
                let mut values = vec![started.as_str(), duration.as_str(), used.as_str()];
                let rest = t
                    .rest_seconds
                    .map(|seconds| model::clock(Duration::from_secs(seconds)));
                if let Some(rest) = &rest {
                    headers.insert(2, "rest");
                    values.insert(2, rest);
                }
                let laps = t
                    .completed_laps(now)
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "--".into());
                if t.stopwatch && t.rest_seconds.is_none() {
                    headers.insert(headers.len() - 1, "laps");
                    values.insert(values.len() - 1, &laps);
                }
                if let Some(count) = &completed {
                    headers.insert(headers.len() - 1, "intervals");
                    values.insert(values.len() - 1, count);
                }
                let mut cell_width = headers
                    .iter()
                    .chain(values.iter())
                    .map(|cell| cell.len())
                    .max()
                    .unwrap_or(0);
                if (cell_width + 3) * headers.len() + 1 > usize::from(parts[5].width) && t.stopwatch
                {
                    headers[0] = "start";
                    if completed.is_some() {
                        let intervals_column = headers.len() - 2;
                        headers[intervals_column] = "done";
                    } else {
                        headers[1] = "alert";
                    }
                    cell_width = headers
                        .iter()
                        .chain(values.iter())
                        .map(|cell| cell.len())
                        .max()
                        .unwrap_or(0);
                }
                let padding =
                    usize::from((cell_width + 3) * headers.len() < usize::from(parts[5].width));
                let widths = vec![cell_width; headers.len()];
                let border = |left: char, joint: char, right: char| {
                    let segments: Vec<_> = widths
                        .iter()
                        .map(|width| "─".repeat(width + padding * 2))
                        .collect();
                    Line::styled(
                        format!("{left}{}{right}", segments.join(&joint.to_string())),
                        Style::default().fg(MUTED),
                    )
                };
                let row = |cells: &[&str], style: Style| {
                    let mut spans = vec![Span::styled("│", Style::default().fg(MUTED))];
                    for (cell, width) in cells.iter().zip(&widths) {
                        spans.push(Span::styled(
                            format!(
                                "{space}{cell:^width$}{space}",
                                space = " ".repeat(padding),
                                width = *width
                            ),
                            style,
                        ));
                        spans.push(Span::styled("│", Style::default().fg(MUTED)));
                    }
                    Line::from(spans)
                };
                f.render_widget(
                    Paragraph::new(vec![
                        border('╭', '┬', '╮'),
                        row(&headers, Style::default().fg(MUTED)),
                        border('├', '┼', '┤'),
                        row(&values, Style::default()),
                        border('╰', '┴', '╯'),
                    ])
                    .centered(),
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
                        Line::from("applies to timer, stopwatch, and pomodoro alerts"),
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
            Mode::Choose { selected } => {
                let bounds = popup(area, 54, 7);
                f.render_widget(Clear, bounds);
                let mut lines: Vec<_> = [
                    "timer · count down",
                    "stopwatch · count up",
                    "pomodoro · work and rest",
                ]
                .into_iter()
                .enumerate()
                .map(|(index, label)| {
                    Line::styled(
                        format!("{} {label}", if index == *selected { "›" } else { " " }),
                        Style::default().fg(if index == *selected { ACCENT } else { MUTED }),
                    )
                })
                .collect();
                lines.push(Line::default());
                lines.push(Line::from("↑/↓ choose · enter continue · esc cancel"));
                f.render_widget(
                    Paragraph::new(lines)
                        .block(theme::panel(" a little moment "))
                        .style(Style::default().bg(BG)),
                    bounds,
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
                rest,
                memo,
                field,
                error,
                ..
            } => {
                let desired_height = if rest.is_some() { 24 } else { 20 };
                let popup = popup(
                    area,
                    62,
                    desired_height.min(area.height.saturating_sub(2).max(16)),
                );
                f.render_widget(Clear, popup);
                let title = if rest.is_some() {
                    if *edit {
                        " edit pomodoro "
                    } else {
                        " new pomodoro "
                    }
                } else if *stopwatch {
                    if *edit {
                        " edit stopwatch "
                    } else {
                        " new stopwatch "
                    }
                } else if *edit {
                    " edit timer "
                } else {
                    " new timer "
                };
                let panel = theme::panel(title)
                    .padding(Padding::horizontal(2))
                    .border_style(Style::default().fg(ACCENT))
                    .style(Style::default().bg(BG));
                let inner = panel.inner(popup);
                f.render_widget(panel, popup);
                let mut fields = vec![
                    ("name (optional)", name.as_str()),
                    (
                        if rest.is_some() {
                            "interval"
                        } else if *stopwatch {
                            "alert every"
                        } else {
                            "duration"
                        },
                        duration.as_str(),
                    ),
                ];
                if let Some(rest) = rest {
                    fields.push(("rest interval", rest.as_str()));
                }
                fields.push(("memo (optional)", memo.as_str()));
                let roomy = usize::from(inner.height) >= fields.len() * 4 + 4;
                let field_gap = u16::from(roomy);
                let help_height = if roomy { 4 } else { 2 };
                let [_, fields_area, _, help] = Layout::vertical([
                    Constraint::Length(u16::from(roomy)),
                    Constraint::Length(
                        fields.len() as u16 * 3 + fields.len().saturating_sub(1) as u16 * field_gap,
                    ),
                    Constraint::Min(0),
                    Constraint::Length(help_height),
                ])
                .areas(inner);
                let slots = Layout::vertical(vec![Constraint::Length(3); fields.len()])
                    .spacing(field_gap)
                    .split(fields_area);
                for (index, ((label, value), &slot)) in fields.iter().zip(slots.iter()).enumerate()
                {
                    let focused = index == *field;
                    let label = label.trim_end_matches(" (optional)");
                    let color = if focused { ACCENT } else { MUTED };
                    let mut border =
                        theme::panel(format!(" {}{label} ", if focused { "› " } else { "" }))
                            .border_style(if focused {
                                Style::default().fg(color)
                            } else {
                                Style::default().fg(color).dim()
                            });
                    if index == 0 || index == fields.len() - 1 {
                        border = border.title_top(
                            Line::from(" optional ")
                                .right_aligned()
                                .style(Style::default().fg(MUTED)),
                        );
                    }
                    let input_area = border.inner(slot);
                    f.render_widget(border, slot);
                    let mut spans = vec![Span::raw(value.to_string())];
                    if focused {
                        spans.push(Span::styled("▏", Style::default().fg(ACCENT)));
                    } else if value.is_empty() {
                        spans.push(Span::styled("—", Style::default().fg(MUTED).dim()));
                    }
                    let text = Line::from(spans);
                    let scroll = if focused {
                        text.width().saturating_sub(usize::from(input_area.width)) as u16
                    } else {
                        0
                    };
                    f.render_widget(Paragraph::new(text).scroll((0, scroll)), input_area);
                }
                let hint = if rest.is_some() {
                    "intervals required · e.g. 25m / 5m"
                } else if *stopwatch {
                    "e.g. 30s / 5m · blank disables alerts"
                } else {
                    "e.g. 1h / 6m 10s / 100s"
                };
                let mut lines = vec![];
                if help.height >= 4 {
                    lines.push(Line::styled(hint, Style::default().fg(MUTED)).centered());
                    lines.push(
                        Line::from(vec![
                            Span::styled("ctrl+n / ctrl+p", Style::default().fg(ACCENT).bold()),
                            Span::styled(" switch type", Style::default().fg(MUTED)),
                        ])
                        .centered(),
                    );
                }
                lines.push(if error.is_empty() {
                    Line::styled(
                        if *edit {
                            "saving resets this run"
                        } else {
                            "starts as soon as you save"
                        },
                        Style::default().fg(MUTED),
                    )
                    .centered()
                } else {
                    Line::styled(error.as_str(), Style::default().fg(Color::LightRed))
                });
                lines.push(
                    Line::from(vec![
                        Span::styled("tab", Style::default().fg(ACCENT).bold()),
                        Span::styled(" next   ", Style::default().fg(MUTED)),
                        Span::styled("enter", Style::default().fg(ACCENT).bold()),
                        Span::styled(" save   ", Style::default().fg(MUTED)),
                        Span::styled("esc", Style::default().fg(ACCENT).bold()),
                        Span::styled(" cancel", Style::default().fg(MUTED)),
                    ])
                    .centered(),
                );
                f.render_widget(Paragraph::new(lines), help);
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
fn wrap_card_text(text: &str, width: u16) -> Text<'static> {
    let width = usize::from(width).max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() {
            if Span::raw(line.as_str()).width() + 1 + Span::raw(word).width() > width {
                lines.push(Line::from(std::mem::take(&mut line)));
            } else {
                line.push(' ');
            }
        }
        for character in word.chars() {
            if !line.is_empty()
                && Span::raw(line.as_str()).width() + Span::raw(character.to_string()).width()
                    > width
            {
                lines.push(Line::from(std::mem::take(&mut line)));
            }
            line.push(character);
        }
    }
    if !line.is_empty() {
        lines.push(Line::from(line));
    }
    Text::from(lines)
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
                    completed.push(if let Some((phase, _, _)) = t.phase(now) {
                        format!("{} · {phase} interval started", t.label())
                    } else if t.stopwatch {
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
    fn running_cards_show_full_details_and_memo_below_gauge() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut pomodoro = Timer::new("focus".into(), 1500);
        pomodoro.stopwatch = true;
        pomodoro.rest_seconds = Some(300);
        pomodoro.memo = "stretch gently".into();
        pomodoro.reset();
        pomodoro.toggle(Instant::now());
        let mut watch = Timer::new("laps".into(), 30);
        watch.stopwatch = true;
        watch.reset();
        watch.toggle(Instant::now());
        let mut app = App {
            timers: vec![pomodoro, watch],
            clocks: world_clock::WorldClocks::default(),
            settings: settings::Settings::default(),
            selected: 0,
            mode: Mode::Browse,
            status: String::new(),
            path: PathBuf::new(),
            list: ListState::default(),
        };
        let mut terminal = Terminal::new(TestBackend::new(140, 60)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..60)
            .map(|y| {
                (50..139)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for label in [
            "work:",
            "rest:",
            "intervals:",
            "laps:",
            "used:",
            "memo",
            "stretch gently",
        ] {
            assert!(text.contains(label), "missing {label}");
        }
        let gauge_row = text
            .lines()
            .position(|line| line.contains('─') && line.contains("│") && !line.contains('╭'))
            .unwrap();
        let memo_row = text.lines().position(|line| line.contains("memo")).unwrap();
        assert!(memo_row > gauge_row);
        app.selected = 1;
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
    }
    #[test]
    fn pomodoro_requires_both_intervals_and_can_be_edited() {
        let path = std::env::temp_dir().join(format!(
            "tmr-pomodoro-{}-{}.json",
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
        press(&mut app, KeyCode::Char('p'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.mode, Mode::Form { error, .. } if !error.is_empty()));
        press(&mut app, KeyCode::Tab);
        for c in "25m".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(
            matches!(&app.mode, Mode::Form { error, .. } if error.starts_with("rest interval"))
        );
        press(&mut app, KeyCode::Tab);
        for c in "5m".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Tab);
        for c in "stretch".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.timers[0].kind_name(), "pomodoro");
        assert!(app.timers[0].started.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.timers[0].started.is_some());
        let saved = storage::load(&path).unwrap();
        assert_eq!(saved[0].seconds, 1500);
        assert_eq!(saved[0].rest_seconds, Some(300));
        assert_eq!(saved[0].memo, "stretch");
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(&app.mode, Mode::Form { rest: Some(rest), .. } if rest == "300s"));
        for code in [KeyCode::Char('n'), KeyCode::Char('p')] {
            for _ in 0..3 {
                app.key(KeyEvent::new(code, KeyModifiers::CONTROL));
            }
            assert!(
                matches!(&app.mode, Mode::Form { stopwatch: true, duration, rest: Some(rest), memo, .. }
                if duration == "1500s" && rest == "300s" && memo == "stretch")
            );
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.timers[0].started.is_none());
        std::fs::remove_file(path).unwrap();
    }
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
            rest: None,
            rest_cache: String::new(),
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
