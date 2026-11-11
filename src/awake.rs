use std::{
    io,
    process::{self, Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Awake {
    retry_after: Option<Instant>,
    child: Option<Child>,
}

impl Awake {
    pub fn update(&mut self, running: bool) -> io::Result<()> {
        if !running {
            self.release();
            self.retry_after = None;
            return Ok(());
        }
        if let Some(child) = &mut self.child {
            if child.try_wait()?.is_none() {
                return Ok(());
            }
            self.child = None;
            return Err(io::Error::other("caffeinate exited; retrying shortly"));
        }
        let now = Instant::now();
        if self.retry_after.is_some_and(|retry| now < retry) {
            return Ok(());
        }
        // Throttle retries if the helper cannot start or repeatedly exits.
        self.retry_after = Some(now + Duration::from_secs(5));
        self.child = Some(
            Command::new("/usr/bin/caffeinate")
                .args(["-di", "-w", &process::id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        Ok(())
    }

    fn release(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        self.release();
    }
}
