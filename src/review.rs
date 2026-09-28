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

    /// The commit SHA of the most recently reviewed file, if any.
    pub fn latest_commit(&self) -> Option<String> {
        self.entries
            .iter()
            .max_by_key(|(_, timestamp)| **timestamp)
            .and_then(|(key, _)| key.split('\t').next())
            .map(str::to_string)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_commit_picks_most_recent() {
        let mut store = ReviewStore {
            entries: HashMap::new(),
            path: None,
            dirty: false,
        };
        store.entries.insert("aaa\tfile1".to_string(), 100);
        store.entries.insert("bbb\tfile2".to_string(), 200);
        assert_eq!(store.latest_commit().as_deref(), Some("bbb"));
    }

    #[test]
    fn latest_commit_is_none_when_empty() {
        let store = ReviewStore::default();
        assert_eq!(store.latest_commit(), None);
    }
}
