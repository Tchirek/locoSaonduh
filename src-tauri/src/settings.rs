use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub id: String,
    pub alias: String,
    pub source_id: Option<String>,
    pub output_id: Option<String>,
}

impl Settings {
    pub fn load_or_create(dir: &Path) -> Self {
        let path = dir.join("settings.json");
        let existing = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok());
        if let Some(mut settings) = existing {
            if settings.alias.trim().is_empty() {
                settings.alias = default_alias();
            }
            if settings.id.trim().is_empty() {
                settings.id = new_id();
            }
            return settings;
        }

        let settings = Settings {
            id: new_id(),
            alias: default_alias(),
            source_id: None,
            output_id: None,
        };
        if let Err(error) = settings.save(dir) {
            log::warn!("could not persist initial settings: {error}");
        }
        settings
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).context("could not create settings directory")?;
        let path: PathBuf = dir.join("settings.json");
        let temporary = dir.join("settings.json.tmp");
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(&temporary, json).context("could not write settings")?;
        std::fs::rename(&temporary, &path).context("could not replace settings")?;
        Ok(())
    }
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn default_alias() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "locoSaonduh".to_owned());
    let mut alias = host
        .split('.')
        .next()
        .unwrap_or("locoSaonduh")
        .replace(['-', '_'], " ");
    alias.truncate(32);
    alias.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_is_stable() {
        let dir = std::env::temp_dir().join(format!("locosaonduh-test-{}", new_id()));
        let first = Settings::load_or_create(&dir);
        let second = Settings::load_or_create(&dir);
        assert_eq!(first.id, second.id);
        assert_eq!(first.alias, second.alias);
        assert!(!first.alias.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_alias_is_repaired() {
        let dir = std::env::temp_dir().join(format!("locosaonduh-test-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            br#"{"id":"abc","alias":"  ","sourceId":null,"outputId":null}"#,
        )
        .unwrap();
        let settings = Settings::load_or_create(&dir);
        assert!(!settings.alias.trim().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
