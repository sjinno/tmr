use crate::storage;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub alarm_seconds: Option<u64>,
    #[serde(skip)]
    pub dirty: bool,
}

impl Settings {
    pub fn load(path: &Path) -> io::Result<Self> {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let settings: Self = serde_json::from_slice(&data).map_err(io::Error::other)?;
        if settings
            .alarm_seconds
            .is_some_and(|seconds| seconds == 0 || seconds > 999 * 3600)
        {
            return Err(io::Error::other("invalid alarm duration"));
        }
        Ok(settings)
    }
    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        if self.dirty {
            storage::write_json(path, self)?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_validation() {
        let path = std::env::temp_dir().join(format!(
            "tmr-settings-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(Settings::load(&path).unwrap().alarm_seconds, None);
        let mut settings = Settings {
            alarm_seconds: Some(30),
            dirty: true,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().alarm_seconds, Some(30));
        fs::write(&path, r#"{"alarm_seconds":0}"#).unwrap();
        assert!(Settings::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
