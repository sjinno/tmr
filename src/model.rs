use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
pub struct Timer {
    pub name: String,
    #[serde(default)]
    pub memo: String,
    // Duration for timers, repeating alert interval for stopwatches (0 disables alerts).
    pub seconds: u64,
    #[serde(default)]
    pub stopwatch: bool,
    #[serde(default)]
    pub rest_seconds: Option<u64>,
    #[serde(skip)]
    alerted_intervals: u64,
    #[serde(default)]
    pub uses: u64,
    #[serde(default)]
    pub favorite: bool,
    #[serde(skip)]
    pub remaining: Duration,
    #[serde(skip)]
    pub started: Option<Instant>,
    #[serde(skip)]
    pub started_at: Option<time::OffsetDateTime>,
    #[serde(skip)]
    pub finished: bool,
}

impl Timer {
    pub fn label(&self) -> String {
        if self.name.trim().is_empty() && self.stopwatch {
            self.kind_name().into()
        } else if self.name.trim().is_empty() {
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
            stopwatch: false,
            rest_seconds: None,
            alerted_intervals: 0,
            uses: 0,
            favorite: false,
            remaining: Duration::from_secs(seconds),
            started: None,
            started_at: None,
            finished: false,
        }
    }
    pub fn left(&self, now: Instant) -> Duration {
        self.started.map_or(self.remaining, |start| {
            let elapsed = now.duration_since(start);
            if self.stopwatch {
                self.remaining.saturating_add(elapsed)
            } else {
                self.remaining.saturating_sub(elapsed)
            }
        })
    }
    pub fn display(&self, now: Instant) -> String {
        if self.finished {
            return "fin".into();
        }
        let value = self.left(now);
        clock(if self.stopwatch {
            Duration::from_secs(value.as_secs())
        } else {
            value
        })
    }
    pub fn kind_name(&self) -> &'static str {
        if self.rest_seconds.is_some() {
            "pomodoro"
        } else if self.stopwatch {
            "stopwatch"
        } else {
            "timer"
        }
    }
    /// Current phase, active elapsed time within it, and its duration.
    pub fn phase(&self, now: Instant) -> Option<(&'static str, Duration, u64)> {
        let rest = self.rest_seconds?;
        let position = self.left(now).as_secs_f64() % (self.seconds + rest) as f64;
        if position < self.seconds as f64 {
            Some(("work", Duration::from_secs_f64(position), self.seconds))
        } else {
            Some((
                "rest",
                Duration::from_secs_f64(position - self.seconds as f64),
                rest,
            ))
        }
    }
    pub fn completed_work_intervals(&self, now: Instant) -> Option<u64> {
        let rest = self.rest_seconds?;
        let elapsed = self.left(now).as_secs();
        let cycle = self.seconds + rest;
        Some(elapsed / cycle + u64::from(elapsed % cycle >= self.seconds))
    }
    pub fn progress(&self, now: Instant) -> f64 {
        if let Some((_, elapsed, seconds)) = self.phase(now) {
            return elapsed.as_secs_f64() / seconds as f64;
        }
        if self.seconds == 0 {
            return 0.0;
        }
        let elapsed = self.left(now).as_secs_f64();
        if self.stopwatch {
            (elapsed % self.seconds as f64) / self.seconds as f64
        } else {
            (1.0 - elapsed / self.seconds as f64).clamp(0.0, 1.0)
        }
    }
    pub fn setting(&self, now: Instant) -> String {
        if let Some((phase, elapsed, seconds)) = self.phase(now) {
            return format!(
                "{phase} · next in {} · work {} / rest {}",
                clock(Duration::from_secs(seconds).saturating_sub(elapsed)),
                clock(Duration::from_secs(self.seconds)),
                clock(Duration::from_secs(self.rest_seconds.unwrap_or(0)))
            );
        }
        if !self.stopwatch {
            format!("duration: {}", clock(Duration::from_secs(self.seconds)))
        } else if self.seconds == 0 {
            "stopwatch · alerts off".into()
        } else {
            format!(
                "stopwatch · alert every {}",
                clock(Duration::from_secs(self.seconds))
            )
        }
    }
    pub fn reset(&mut self) {
        self.remaining = if self.stopwatch {
            Duration::ZERO
        } else {
            Duration::from_secs(self.seconds)
        };
        self.alerted_intervals = 0;
        self.started = None;
        self.started_at = None;
        self.finished = false;
    }
    pub fn toggle(&mut self, now: Instant) {
        if self.started.is_some() {
            self.remaining = self.left(now);
            self.started = None;
        } else {
            if self.finished || (!self.stopwatch && self.remaining.is_zero()) {
                self.reset();
            }
            if self.started_at.is_none() {
                self.started_at = Some(time::OffsetDateTime::now_utc());
                self.uses = self.uses.saturating_add(1);
            }
            self.started = Some(now);
        }
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(rest) = self.rest_seconds {
            if self.started.is_some() {
                let elapsed = self.left(now).as_secs();
                let cycle = self.seconds + rest;
                let transitions =
                    (elapsed / cycle) * 2 + u64::from(elapsed % cycle >= self.seconds);
                if transitions > self.alerted_intervals {
                    self.alerted_intervals = transitions;
                    return true;
                }
            }
            return false;
        }
        if self.stopwatch {
            if self.started.is_some() && self.seconds > 0 {
                let intervals = self.left(now).as_secs() / self.seconds;
                if intervals > self.alerted_intervals {
                    self.alerted_intervals = intervals;
                    return true;
                }
            }
            return false;
        }
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
        || "use 1h, 6m 10s, or 100s. max: 999 hours; duration must be positive.".to_string();
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
    fn pomodoro_alternates_without_resetting_elapsed_time() {
        let now = Instant::now();
        let mut timer = Timer::new("focus".into(), 30);
        timer.stopwatch = true;
        timer.rest_seconds = Some(10);
        timer.reset();
        timer.toggle(now);
        assert_eq!(timer.phase(now).unwrap().0, "work");
        assert!(!timer.tick(now + Duration::from_secs(29)));
        assert_eq!(
            timer.completed_work_intervals(now + Duration::from_secs(29)),
            Some(0)
        );
        assert!(timer.tick(now + Duration::from_secs(30)));
        assert_eq!(
            timer.completed_work_intervals(now + Duration::from_secs(30)),
            Some(1)
        );
        assert_eq!(
            timer.phase(now + Duration::from_secs(30)).unwrap().0,
            "rest"
        );
        assert!(!timer.tick(now + Duration::from_secs(30)));
        assert!(timer.tick(now + Duration::from_secs(40)));
        assert_eq!(
            timer.completed_work_intervals(now + Duration::from_secs(40)),
            Some(1)
        );
        assert_eq!(
            timer.phase(now + Duration::from_secs(40)).unwrap().0,
            "work"
        );
        timer.toggle(now + Duration::from_secs(45));
        assert!(!timer.tick(now + Duration::from_secs(99)));
        assert_eq!(timer.display(now + Duration::from_secs(99)), "00:45");
        timer.toggle(now + Duration::from_secs(100));
        assert!(!timer.tick(now + Duration::from_secs(124)));
        assert!(timer.tick(now + Duration::from_secs(125)));
        assert_eq!(
            timer.phase(now + Duration::from_secs(125)).unwrap().0,
            "rest"
        );
        assert_eq!(timer.display(now + Duration::from_secs(125)), "01:10");
        assert_eq!(
            timer.completed_work_intervals(now + Duration::from_secs(125)),
            Some(2)
        );
        assert_eq!(timer.uses, 1);
        assert!(timer.tick(now + Duration::from_secs(1000)));
        assert!(!timer.tick(now + Duration::from_secs(1000)));
        assert!(timer.started.is_some());
        timer.reset();
        assert_eq!(timer.display(now), "00:00");
        assert_eq!(timer.completed_work_intervals(now), Some(0));
        assert_eq!(timer.phase(now).unwrap().0, "work");
    }
    #[test]
    fn stopwatch_intervals_follow_active_time() {
        let now = Instant::now();
        let mut watch = Timer::new("Stretch".into(), 30);
        watch.stopwatch = true;
        watch.reset();
        watch.toggle(now);
        assert_eq!(watch.display(now), "00:00");
        assert!(!watch.tick(now + Duration::from_secs(29)));
        assert!(watch.tick(now + Duration::from_secs(30)));
        assert!(!watch.tick(now + Duration::from_secs(30)));
        watch.toggle(now + Duration::from_secs(35));
        assert!(!watch.tick(now + Duration::from_secs(100)));
        watch.toggle(now + Duration::from_secs(100));
        assert_eq!(watch.display(now + Duration::from_secs(100)), "00:35");
        assert_eq!(watch.uses, 1);
        assert!(watch.tick(now + Duration::from_secs(125)));
        // A delayed frame coalesces missed intervals into one alert.
        assert!(watch.tick(now + Duration::from_secs(300)));
        assert!(!watch.tick(now + Duration::from_secs(300)));
        assert!(watch.started.is_some());
        assert!(!watch.finished);
        watch.reset();
        assert_eq!(watch.display(now), "00:00");
        watch.toggle(now);
        assert!(watch.tick(now + Duration::from_secs(30)));
        watch.seconds = 0;
        watch.reset();
        watch.toggle(now);
        assert!(!watch.tick(now + Duration::from_secs(3600)));
        assert_eq!(watch.display(now + Duration::from_secs(3600)), "01:00:00");
    }
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
        assert_eq!(t.display(now + Duration::from_secs(15)), "fin");
        assert!(!t.tick(now + Duration::from_secs(16)));
        t.toggle(now + Duration::from_secs(17));
        assert_ne!(t.display(now + Duration::from_secs(17)), "fin");
        assert_eq!(t.uses, 2);
        assert_eq!(t.left(now + Duration::from_secs(17)).as_secs(), 10);
    }
}
