use crate::model::Timer;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn path() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("TMR_DATA_FILE") {
        return Ok(path.into());
    }
    directories::ProjectDirs::from("", "", "tmr")
        .map(|dirs| dirs.data_dir().join("timers.json"))
        .ok_or_else(|| io::Error::other("Cannot locate data directory; set TMR_DATA_FILE"))
}
pub fn load(path: &Path) -> io::Result<Vec<Timer>> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut timers: Vec<Timer> = serde_json::from_slice(&data).map_err(io::Error::other)?;
    for timer in &mut timers {
        if timer.seconds == 0 || timer.seconds > 999 * 3600 {
            return Err(io::Error::other("Invalid saved timer"));
        }
        timer.reset();
    }
    Ok(timers)
}
pub fn save(path: &Path, timers: &[Timer]) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(timers).map_err(io::Error::other)?,
    )?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_corruption() {
        let dir = std::env::temp_dir().join(format!(
            "tmr-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("timers.json");
        assert!(load(&path).unwrap().is_empty());
        let mut timer = Timer::new(String::new(), 90);
        timer.favorite = true;
        timer.memo = "Steep, then remove the tea bag".into();
        timer.toggle(std::time::Instant::now());
        save(&path, &[timer]).unwrap();
        let timers = load(&path).unwrap();
        assert!(timers[0].name.is_empty());
        assert_eq!(timers[0].memo, "Steep, then remove the tea bag");
        assert_eq!(timers[0].label(), "01:30 timer");
        assert_eq!(timers[0].uses, 1);
        assert!(timers[0].favorite);
        assert!(timers[0].started.is_none());
        assert_eq!(timers[0].remaining.as_secs(), 90);
        fs::write(&path, r#"[{"name":"Old timer","seconds":60}]"#).unwrap();
        assert!(load(&path).unwrap()[0].memo.is_empty());
        fs::write(&path, "broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
        fs::remove_dir_all(dir).unwrap();
    }
}
