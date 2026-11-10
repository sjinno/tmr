use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
pub struct Timer {
    pub name: String,
    #[serde(default)]
    pub memo: String,
    pub seconds: u64,
    #[serde(default)]
    pub uses: u64,
    #[serde(default)]
    pub favorite: bool,
    #[serde(skip)]
    pub remaining: Duration,
    #[serde(skip)]
    pub started: Option<Instant>,
    #[serde(skip)]
    pub finished: bool,
}

impl Timer {
    pub fn label(&self) -> String {
        if self.name.trim().is_empty() {
            format!("{} timer", clock(Duration::from_secs(self.seconds)))
        } else {
            self.name.clone()
        }
    }
    pub fn new(name: String, seconds: u64) -> Self {
        Self {
            name,
            memo: String::new(),
            seconds,
            uses: 0,
            favorite: false,
            remaining: Duration::from_secs(seconds),
            started: None,
            finished: false,
        }
    }
    pub fn left(&self, now: Instant) -> Duration {
        self.started.map_or(self.remaining, |start| {
            self.remaining.saturating_sub(now.duration_since(start))
        })
    }
    pub fn reset(&mut self) {
        self.remaining = Duration::from_secs(self.seconds);
        self.started = None;
        self.finished = false;
    }
    pub fn toggle(&mut self, now: Instant) {
        if self.started.is_some() {
            self.remaining = self.left(now);
            self.started = None;
        } else {
            if self.finished || self.remaining.is_zero() {
                self.reset();
            }
            if self.remaining == Duration::from_secs(self.seconds) {
                self.uses = self.uses.saturating_add(1);
            }
            self.started = Some(now);
        }
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.started.is_some() && self.left(now).is_zero() {
            self.started = None;
            self.remaining = Duration::ZERO;
            self.finished = true;
            return true;
        }
        false
    }
}

pub fn parse_duration(input: &str) -> Result<u64, String> {
    let input = input.trim();
    let error =
        || "Use 1h, 6m 10s, or 100s. Max: 999 hours; duration must be positive.".to_string();
    if input.bytes().any(|b| b.is_ascii_alphabetic()) {
        let mut rest = input;
        let mut total = 0u64;
        while !rest.is_empty() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return Err(error());
            }
            let value: u64 = rest[..digits].parse().map_err(|_| error())?;
            rest = rest[digits..].trim_start();
            let multiplier = match rest.as_bytes().first() {
                Some(b'h' | b'H') => 3600,
                Some(b'm' | b'M') => 60,
                Some(b's' | b'S') => 1,
                _ => return Err(error()),
            };
            total = value
                .checked_mul(multiplier)
                .and_then(|n| total.checked_add(n))
                .ok_or_else(error)?;
            rest = rest[1..].trim_start();
        }
        return if total > 0 && total <= 999 * 3600 {
            Ok(total)
        } else {
            Err(error())
        };
    }
    let parts: Vec<_> = if input.contains(':') {
        input.split(':').collect()
    } else {
        input.split_whitespace().collect()
    };
    if parts.is_empty() || parts.len() > 3 {
        return Err(error());
    }
    let mut total = 0u64;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error());
        }
        let n: u64 = part.parse().map_err(|_| error())?;
        if i > 0 && n >= 60 {
            return Err(error());
        }
        total = total
            .checked_mul(60)
            .and_then(|v| v.checked_add(n))
            .ok_or_else(error)?;
    }
    if total == 0 || total > 999 * 3600 {
        return Err(error());
    }
    Ok(total)
}

pub fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations() {
        for (s, n) in [
            ("1h", 3600),
            ("6m 10s", 370),
            ("100s", 100),
            ("1h30m", 5400),
            (" 1H  30M 0S ", 5400),
            ("1 h 30 m", 5400),
            ("100m", 6000),
            ("999h", 999 * 3600),
            ("10", 10),
            ("10 10", 610),
            ("1 30 0 ", 5400),
            ("  1   30  0  ", 5400),
            ("90", 90),
            ("1:30", 90),
            ("1:02:03", 3723),
        ] {
            assert_eq!(parse_duration(s), Ok(n));
        }
        for s in [
            "",
            "   ",
            "0",
            "0 0 0",
            "-1",
            "1 60",
            "1 60 0",
            "1 2 3 4",
            "1 2:3",
            "1:60",
            "1::2",
            "1:2:3:4",
            "999999999999999999999999",
            "0s",
            "1000h",
            "999h 1s",
            "999999999999999999999999h",
            "18446744073709551615h",
            "1.5h",
            "-1h",
            "1d",
            "h",
            "1h 30",
            "1:30m",
            "1hour",
            "1m 🐓",
        ] {
            assert!(parse_duration(s).is_err());
        }
    }
    #[test]
    fn optional_name_uses_duration_label() {
        let timer = Timer::new(String::new(), 90);
        assert!(timer.name.is_empty());
        assert_eq!(timer.label(), "01:30 timer");
        assert_eq!(Timer::new("Tea".into(), 90).label(), "Tea");
    }
    #[test]
    fn pause_resume_finish_reuse() {
        let now = Instant::now();
        let mut t = Timer::new("Tea".into(), 10);
        t.toggle(now);
        t.toggle(now + Duration::from_secs(3));
        assert_eq!(t.remaining.as_secs(), 7);
        t.toggle(now + Duration::from_secs(8));
        assert_eq!(t.uses, 1);
        assert!(!t.tick(now + Duration::from_secs(14)));
        assert!(t.tick(now + Duration::from_secs(15)));
        assert!(!t.tick(now + Duration::from_secs(16)));
        t.toggle(now + Duration::from_secs(17));
        assert_eq!(t.uses, 2);
        assert_eq!(t.left(now + Duration::from_secs(17)).as_secs(), 10);
    }
}
