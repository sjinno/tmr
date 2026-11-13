//! Play the bundled alarm without blocking the event loop.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const AUDIO: &[u8] = include_bytes!("../assets/alarm.mp3");

#[derive(Default)]
pub struct Alarm {
    path: Option<PathBuf>,
    child: Option<Child>,
    until: Option<Instant>,
}

impl Alarm {
    pub fn play(&mut self, seconds: Option<u64>) -> io::Result<()> {
        // Alarms finishing together share the current sound rather than overlap.
        if self.child.is_some() {
            return Ok(());
        }
        if self.path.is_none() {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("tmr-alarm-{}-{stamp}.mp3", std::process::id()));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            if let Err(error) = file.write_all(AUDIO) {
                let _ = fs::remove_file(&path);
                return Err(error);
            }
            self.path = Some(path);
        }
        self.until = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
        if let Err(error) = self.spawn_player() {
            self.until = None;
            return Err(error);
        }
        Ok(())
    }

    fn spawn_player(&mut self) -> io::Result<()> {
        let path = self.path.as_ref().expect("alarm file initialized");
        let mut command = if cfg!(target_os = "macos") {
            Command::new("/usr/bin/afplay")
        } else {
            let mut command = Command::new("ffplay");
            command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
            command
        };
        self.child = Some(
            command
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        Ok(())
    }

    /// Reap the player and report playback errors so the caller can ring a bell.
    pub fn poll(&mut self) -> io::Result<()> {
        if self.until.is_some_and(|until| Instant::now() >= until) {
            self.stop();
            return Ok(());
        }
        if let Some(child) = &mut self.child
            && let Some(status) = child.try_wait()?
        {
            self.child = None;
            if !status.success() {
                self.until = None;
                return Err(io::Error::other("alarm player failed"));
            }
            if self.until.is_some()
                && let Err(error) = self.spawn_player()
            {
                self.until = None;
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn stop(&mut self) {
        self.until = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        self.stop();
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn duration_expiry_stops_and_reaps_player() {
        let child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let mut alarm = Alarm {
            path: None,
            child: Some(child),
            until: Some(Instant::now()),
        };
        alarm.poll().unwrap();
        assert!(alarm.child.is_none());
        assert!(alarm.until.is_none());
        alarm.stop();
    }
}
