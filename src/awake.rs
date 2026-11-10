use std::{
    io,
    process::{self, Child, Command, Stdio},
};

#[derive(Default)]
pub struct Awake {
    running: bool,
    child: Option<Child>,
}

impl Awake {
    pub fn update(&mut self, running: bool) -> io::Result<()> {
        let was_running = std::mem::replace(&mut self.running, running);
        if !running {
            self.release();
        } else if !was_running {
            self.child = Some(
                Command::new("/usr/bin/caffeinate")
                    .args(["-i", "-w", &process::id().to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            );
        } else if let Some(child) = &mut self.child
            && child.try_wait()?.is_some()
        {
            self.child = None;
            return Err(io::Error::other("caffeinate exited"));
        }
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
