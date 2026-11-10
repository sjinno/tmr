//! Play the bundled alarm without blocking the event loop.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const AUDIO: &[u8] = include_bytes!("../assets/alarm.mp3");

#[derive(Default)]
pub struct Alarm {
    path: Option<PathBuf>,
    child: Option<Child>,
}

impl Alarm {
    pub fn play(&mut self) -> io::Result<()> {
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
            self.path = Some(path);
            file.write_all(AUDIO)?;
        }
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
        if let Some(child) = &mut self.child
            && let Some(status) = child.try_wait()?
        {
            self.child = None;
            if !status.success() {
                return Err(io::Error::other("Alarm player failed"));
            }
        }
        Ok(())
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}
