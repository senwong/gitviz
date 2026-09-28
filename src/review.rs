//! Persistent code-review tracking, mirroring vscode-git-graph's Code Review:
//! reviewed files are remembered across sessions and expire after 90 days.
//!
//! Stored as a tab-separated `key\ttimestamp` file under
//! `~/.config/gitviz/reviews.tsv`. `key` is `"<sha>\t<path>"`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_AGE_SECS: u64 = 90 * 24 * 60 * 60;

#[derive(Default)]
pub struct ReviewStore {
    entries: HashMap<String, u64>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl ReviewStore {
    pub fn load() -> Self {
        let path = config_path();
        let mut entries = HashMap::new();
        if let Some(path) = &path
            && let Ok(text) = std::fs::read_to_string(path)
        {
            for line in text.lines() {
                if let Some((key, timestamp)) = line.rsplit_once('\t')
                    && let Ok(timestamp) = timestamp.parse::<u64>()
                {
                    entries.insert(key.to_string(), timestamp);
                }
            }
        }

        let mut store = Self {
            entries,
            path,
            dirty: false,
        };
        store.expire();
        store
    }

    pub fn is_reviewed(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn toggle(&mut self, key: &str) {
        if self.entries.remove(key).is_none() {
            self.entries.insert(key.to_string(), now());
        }
        self.dirty = true;
    }

    pub fn end_all(&mut self) {
        if !self.entries.is_empty() {
            self.entries.clear();
            self.dirty = true;
        }
    }

    fn expire(&mut self) {
        let now = now();
        let before = self.entries.len();
        self.entries
            .retain(|_, timestamp| now.saturating_sub(*timestamp) <= MAX_AGE_SECS);
        if self.entries.len() != before {
            self.dirty = true;
        }
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut output = String::new();
        for (key, timestamp) in &self.entries {
            output.push_str(key);
            output.push('\t');
            output.push_str(&timestamp.to_string());
            output.push('\n');
        }
        let _ = std::fs::write(path, output);
        self.dirty = false;
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/gitviz/reviews.tsv"))
}
