use crate::{ACCENT, BG, MUTED, block_clock, popup, shortcuts, storage, theme};
use chrono::Utc;
use chrono_tz::{TZ_VARIANTS, Tz};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
};
use std::{fs, io, path::Path};

#[derive(Default)]
struct Picker {
    query: String,
    selected: usize,
    list: ListState,
}

#[derive(Default)]
pub struct WorldClocks {
    zones: Vec<Tz>,
    dirty: bool,
    selected: usize,
    picker: Option<Picker>,
    deleting: bool,
    notice: String,
}

fn matches(query: &str) -> Vec<Tz> {
    if query.trim().is_empty() {
        return vec![
            chrono_tz::UTC,
            chrono_tz::America::Los_Angeles,
            chrono_tz::America::New_York,
            chrono_tz::Europe::London,
            chrono_tz::Europe::Paris,
            chrono_tz::Asia::Tokyo,
            chrono_tz::Asia::Kolkata,
            chrono_tz::Asia::Singapore,
            chrono_tz::Australia::Sydney,
        ];
    }
    let words: Vec<_> = query
        .to_lowercase()
        .replace(['_', '/'], " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    TZ_VARIANTS
        .iter()
        .copied()
        .filter(|zone| {
            let name = zone.name().to_lowercase().replace(['_', '/'], " ");
            words.iter().all(|word| name.contains(word))
        })
        .collect()
}

impl WorldClocks {
    pub fn load(path: &Path) -> io::Result<Self> {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let names: Vec<String> = serde_json::from_slice(&data).map_err(io::Error::other)?;
        let zones = names
            .iter()
            .map(|name| name.parse::<Tz>().map_err(io::Error::other))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            zones,
            ..Self::default()
        })
    }

    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let names: Vec<_> = self.zones.iter().map(|zone| zone.name()).collect();
        storage::write_json(path, &names)?;
        self.dirty = false;
        Ok(())
    }

    pub fn browsing(&self) -> bool {
        self.picker.is_none() && !self.deleting
    }

    /// Returns true when the saved locations change.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        if let Some(picker) = &mut self.picker {
            let results = matches(&picker.query);
            match key.code {
                KeyCode::Esc => self.picker = None,
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if picker.query.chars().count() < 80 {
                        picker.query.push(c);
                    }
                    picker.selected = 0;
                }
                KeyCode::Backspace => {
                    picker.query.pop();
                    picker.selected = 0;
                }
                KeyCode::Down if !results.is_empty() => {
                    picker.selected = (picker.selected + 1) % results.len()
                }
                KeyCode::Up if !results.is_empty() => {
                    picker.selected = (picker.selected + results.len() - 1) % results.len()
                }
                KeyCode::Enter => {
                    if let Some(&zone) = results.get(picker.selected) {
                        if let Some(index) = self.zones.iter().position(|&saved| saved == zone) {
                            self.selected = index;
                            self.notice = "that timezone is already added.".into();
                            self.picker = None;
                        } else {
                            self.zones.push(zone);
                            self.dirty = true;
                            self.selected = self.zones.len() - 1;
                            self.picker = None;
                            self.notice = format!("added {}", zone.name().to_lowercase());
                            return true;
                        }
                    }
                }
                _ => {}
            }
        } else if self.deleting {
            match key.code {
                KeyCode::Esc | KeyCode::Char('n') => self.deleting = false,
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.zones.remove(self.selected);
                    self.dirty = true;
                    self.selected = self.selected.min(self.zones.len().saturating_sub(1));
                    self.deleting = false;
                    self.notice = "clock removed.".into();
                    return true;
                }
                _ => {}
            }
        } else {
            match key.code {
                code @ (KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down)
                    if key.modifiers.contains(KeyModifiers::ALT) && !self.zones.is_empty() =>
                {
                    let next = if matches!(code, KeyCode::Left | KeyCode::Up) {
                        self.selected.saturating_sub(1)
                    } else {
                        (self.selected + 1).min(self.zones.len() - 1)
                    };
                    if next != self.selected {
                        self.zones.swap(self.selected, next);
                        self.selected = next;
                        self.dirty = true;
                        return true;
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('/') => {
                    self.picker = Some(Picker::default());
                    self.notice.clear();
                }
                KeyCode::Char('d') if !self.zones.is_empty() => self.deleting = true,
                KeyCode::Right | KeyCode::Down | KeyCode::Char('j') if !self.zones.is_empty() => {
                    self.selected = (self.selected + 1) % self.zones.len()
                }
                KeyCode::Left | KeyCode::Up | KeyCode::Char('k') if !self.zones.is_empty() => {
                    self.selected = (self.selected + self.zones.len() - 1) % self.zones.len()
                }
                _ => {}
            }
        }
        false
    }

    pub fn draw(&mut self, f: &mut Frame, status: &str) {
        let area = f.area();
        let page = theme::page(area);
        let [header, grid, notice, footer] = Layout::vertical([
            Constraint::Length(theme::header_height(area.height)),
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(shortcuts::height(page.width, shortcuts::CLOCKS, 1)),
        ])
        .areas(page);
        theme::header(f, header, "world clocks");
        if self.zones.is_empty() {
            f.render_widget(Paragraph::new("no clocks yet. press n to add a city or timezone.\ntry tokyo, new york, london, or utc.").wrap(Wrap { trim: true }), grid);
        } else {
            let columns = usize::from((grid.width / 33).clamp(1, 4)).min(self.zones.len());
            let rows = usize::from((grid.height.saturating_add(1) / 13).max(1));
            let capacity = rows * columns;
            let first = self.selected / capacity * capacity;
            let end = (first + capacity).min(self.zones.len());
            let row_count = (end - first).div_ceil(columns);
            let row_areas = Layout::vertical(vec![Constraint::Length(12); row_count])
                .spacing(1)
                .split(grid);
            let now = Utc::now();
            for (row, zones) in row_areas.iter().zip(self.zones[first..end].chunks(columns)) {
                let cells = Layout::horizontal(vec![Constraint::Length(32); columns])
                    .spacing(1)
                    .split(*row);
                for (&zone, &cell) in zones.iter().zip(cells.iter()) {
                    let index = self
                        .zones
                        .iter()
                        .position(|&saved| saved == zone)
                        .unwrap_or(0);
                    let color = if index == self.selected {
                        ACCENT
                    } else {
                        MUTED
                    };
                    let tile = theme::panel("")
                        .title(format!(
                            " {} ",
                            zone.name()
                                .rsplit('/')
                                .next()
                                .unwrap_or(zone.name())
                                .replace('_', " ")
                                .to_lowercase()
                        ))
                        .border_style(Style::default().fg(color))
                        .padding(Padding::new(2, 2, 1, 1));
                    let inner = tile.inner(cell);
                    f.render_widget(tile, cell);
                    let [digits, date] =
                        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
                    let local = now.with_timezone(&zone);
                    block_clock::render_compact(
                        f,
                        digits,
                        &local.format("%H:%M").to_string(),
                        Color::Reset,
                    );
                    f.render_widget(
                        Paragraph::new(
                            local
                                .format("%a, %b %-d\n%Z · utc %:z")
                                .to_string()
                                .to_lowercase(),
                        )
                        .centered()
                        .style(Style::default().fg(MUTED)),
                        date,
                    );
                }
            }
        }
        f.render_widget(
            Paragraph::new(format!("{}\n{status}", self.notice)).style(Style::default().fg(MUTED)),
            notice,
        );
        shortcuts::render(f, footer, shortcuts::CLOCKS, 1);
        if let Some(picker) = &mut self.picker {
            let bounds = popup(area, 70, 22);
            f.render_widget(Clear, bounds);
            let panel = theme::panel("")
                .title(" add a place ")
                .style(Style::default().bg(BG));
            let inner = panel.inner(bounds);
            f.render_widget(panel, bounds);
            let [input, results_area, help] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .areas(inner);
            f.render_widget(
                Paragraph::new(format!("search: {}▏", picker.query))
                    .style(Style::default().fg(ACCENT)),
                input,
            );
            let results = matches(&picker.query);
            if results.is_empty() {
                f.render_widget(
                    Paragraph::new("no matches. try a city like tokyo or new york."),
                    results_area,
                );
            } else {
                let items: Vec<_> = results
                    .iter()
                    .map(|zone| {
                        ListItem::new(
                            zone.name()
                                .rsplit('/')
                                .next()
                                .unwrap_or(zone.name())
                                .replace('_', " ")
                                .to_lowercase(),
                        )
                    })
                    .collect();
                picker.list.select(Some(picker.selected));
                f.render_stateful_widget(
                    List::new(items)
                        .highlight_style(Style::default().fg(ACCENT).bold())
                        .highlight_symbol("› "),
                    results_area,
                    &mut picker.list,
                );
            }
            f.render_widget(
                Paragraph::new("popular locations · type to search all timezones\n↑/↓ select · enter add · esc cancel")
                    .style(Style::default().fg(MUTED)),
                help,
            );
        } else if self.deleting {
            let bounds = popup(area, 54, 6);
            f.render_widget(Clear, bounds);
            f.render_widget(
                Paragraph::new(format!(
                    "remove {}?\nenter / y remove · esc / n cancel",
                    self.zones[self.selected].name().to_lowercase()
                ))
                .wrap(Wrap { trim: true })
                .block(theme::panel("").title(" remove clock "))
                .style(Style::default().bg(BG)),
                bounds,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ratatui::{Terminal, backend::TestBackend};

    fn press(clocks: &mut WorldClocks, code: KeyCode) -> bool {
        clocks.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn search_add_duplicate_and_remove() {
        assert!(matches("new york").contains(&chrono_tz::America::New_York));
        assert!(matches("ASIA/TOKYO").contains(&chrono_tz::Asia::Tokyo));
        assert!(matches("not a timezone").is_empty());
        let mut clocks = WorldClocks::default();
        for _ in 0..2 {
            press(&mut clocks, KeyCode::Char('n'));
            for c in "tokyo".chars() {
                press(&mut clocks, KeyCode::Char(c));
            }
            press(&mut clocks, KeyCode::Enter);
        }
        assert_eq!(clocks.zones, vec![chrono_tz::Asia::Tokyo]);
        assert!(clocks.browsing());
        press(&mut clocks, KeyCode::Char('d'));
        press(&mut clocks, KeyCode::Esc);
        assert_eq!(clocks.zones.len(), 1);
        press(&mut clocks, KeyCode::Char('d'));
        assert!(press(&mut clocks, KeyCode::Enter));
        assert!(clocks.zones.is_empty());
    }

    #[test]
    fn timezone_offsets_follow_dst_and_date_rollover() {
        let winter = Utc.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).unwrap();
        let summer = Utc.with_ymd_and_hms(2026, 7, 1, 20, 0, 0).unwrap();
        assert_eq!(
            winter
                .with_timezone(&chrono_tz::America::New_York)
                .format("%H:%M %:z")
                .to_string(),
            "15:00 -05:00"
        );
        assert_eq!(
            summer
                .with_timezone(&chrono_tz::America::New_York)
                .format("%H:%M %:z")
                .to_string(),
            "16:00 -04:00"
        );
        assert_eq!(
            winter
                .with_timezone(&chrono_tz::Asia::Tokyo)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-01-02 05:00"
        );
    }

    #[test]
    fn saved_locations_and_responsive_tiles() {
        let path = std::env::temp_dir().join(format!(
            "tmr-world-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut clocks = WorldClocks {
            zones: vec![chrono_tz::Asia::Tokyo, chrono_tz::Europe::London],
            dirty: true,
            ..WorldClocks::default()
        };
        clocks.save(&path).unwrap();
        let mut loaded = WorldClocks::load(&path).unwrap();
        assert_eq!(loaded.zones, clocks.zones);
        for (width, height) in [(100, 32), (44, 16), (160, 50)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| loaded.draw(f, "")).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains("tokyo"));
            if width == 100 {
                assert!(text.contains('█'));
            }
            press(&mut loaded, KeyCode::Char('n'));
            terminal.draw(|f| loaded.draw(f, "")).unwrap();
            press(&mut loaded, KeyCode::Esc);
        }
        assert!(loaded.key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)));
        assert_eq!(loaded.selected, 1);
        assert_eq!(loaded.zones[1], chrono_tz::Asia::Tokyo);
        assert!(!loaded.key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)));
        loaded.save(&path).unwrap();
        assert_eq!(WorldClocks::load(&path).unwrap().zones, loaded.zones);
        assert!(loaded.key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)));
        assert_eq!(loaded.selected, 0);
        assert_eq!(loaded.zones[0], chrono_tz::Asia::Tokyo);
        fs::write(&path, "[\"Bogus/Zone\"]").unwrap();
        assert!(WorldClocks::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
