//! Repository-level configuration that can be committed alongside the code,
//! mirroring vscode-git-graph's "Export your Git Graph Repository
//! Configuration". Stored as a simple `key=value` file at `.gitviz.conf`.

use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = ".gitviz.conf";

#[derive(Clone, Debug)]
pub struct RepoConfig {
    pub branches: bool,
    pub remotes: bool,
    pub tags: bool,
    pub first_parent: bool,
    pub show_stashes: bool,
    pub show_uncommitted: bool,
    pub include_untracked: bool,
    pub combine_refs: bool,
    pub emoji: bool,
    pub markdown: bool,
    pub date_commit: bool,
    pub columns_date: bool,
    pub columns_author: bool,
    pub columns_commit: bool,
    pub repo_order: String,
    /// Branch glob patterns (e.g. `heads/feature/*`); empty means show all.
    pub branch_globs: Vec<String>,
    /// Custom lane colours as `#rrggbb`; empty means use the built-in preset.
    pub lane_colors: Vec<String>,
    /// Context menu action keys to hide.
    pub hidden_actions: Vec<String>,
    /// Custom emoji shortcodes as `code:emoji`.
    pub emoji_mappings: Vec<String>,
    /// Graph connector style: `rounded` (default) or `angular`.
    pub graph_style: String,
    /// Custom Pull Request URL template (empty = use the built-in providers).
    pub pr_provider: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            branches: true,
            remotes: true,
            tags: true,
            first_parent: false,
            show_stashes: true,
            show_uncommitted: true,
            include_untracked: true,
            combine_refs: true,
            emoji: true,
            markdown: true,
            date_commit: false,
            columns_date: true,
            columns_author: true,
            columns_commit: true,
            repo_order: "name".to_string(),
            branch_globs: Vec::new(),
            lane_colors: Vec::new(),
            hidden_actions: Vec::new(),
            emoji_mappings: Vec::new(),
            graph_style: "rounded".to_string(),
            pr_provider: String::new(),
        }
    }
}

impl RepoConfig {
    pub fn path(repo: &Path) -> PathBuf {
        repo.join(FILE_NAME)
    }

    pub fn load(repo: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(repo)).ok()?;
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let flag = value == "true";
            match key {
                "branches" => config.branches = flag,
                "remotes" => config.remotes = flag,
                "tags" => config.tags = flag,
                "first_parent" => config.first_parent = flag,
                "show_stashes" => config.show_stashes = flag,
                "show_uncommitted" => config.show_uncommitted = flag,
                "include_untracked" => config.include_untracked = flag,
                "combine_refs" => config.combine_refs = flag,
                "emoji" => config.emoji = flag,
                "markdown" => config.markdown = flag,
                "date_commit" => config.date_commit = flag,
                "columns_date" => config.columns_date = flag,
                "columns_author" => config.columns_author = flag,
                "columns_commit" => config.columns_commit = flag,
                "repo_order" => config.repo_order = value.to_string(),
                "branch_globs" => {
                    config.branch_globs = split_list(value);
                }
                "lane_colors" => {
                    config.lane_colors = split_list(value);
                }
                "hidden_actions" => {
                    config.hidden_actions = split_list(value);
                }
                "emoji_mappings" => {
                    config.emoji_mappings = split_list(value);
                }
                "graph_style" => config.graph_style = value.to_string(),
                "pr_provider" => config.pr_provider = value.to_string(),
                _ => {}
            }
        }
        Some(config)
    }

    pub fn save(&self, repo: &Path) -> anyhow::Result<()> {
        let mut output = String::from("# gitviz repository configuration\n");
        for (key, value) in [
            ("branches", self.branches),
            ("remotes", self.remotes),
            ("tags", self.tags),
            ("first_parent", self.first_parent),
            ("show_stashes", self.show_stashes),
            ("show_uncommitted", self.show_uncommitted),
            ("include_untracked", self.include_untracked),
            ("combine_refs", self.combine_refs),
            ("emoji", self.emoji),
            ("markdown", self.markdown),
            ("date_commit", self.date_commit),
            ("columns_date", self.columns_date),
            ("columns_author", self.columns_author),
            ("columns_commit", self.columns_commit),
        ] {
            output.push_str(&format!("{key}={value}\n"));
        }
        output.push_str(&format!("repo_order={}\n", self.repo_order));
        output.push_str(&format!("branch_globs={}\n", self.branch_globs.join(";")));
        output.push_str(&format!("lane_colors={}\n", self.lane_colors.join(";")));
        output.push_str(&format!(
            "hidden_actions={}\n",
            self.hidden_actions.join(";")
        ));
        output.push_str(&format!(
            "emoji_mappings={}\n",
            self.emoji_mappings.join(";")
        ));
        output.push_str(&format!("graph_style={}\n", self.graph_style));
        output.push_str(&format!("pr_provider={}\n", self.pr_provider));
        std::fs::write(Self::path(repo), output)?;
        Ok(())
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split([';', ','])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitviz-cfg-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_configuration() {
        let dir = temp_dir("roundtrip");
        let mut config = RepoConfig::default();
        config.first_parent = true;
        config.emoji = false;
        config.repo_order = "path".to_string();
        config.branch_globs = vec!["heads/feature/*".to_string(), "heads/fix/*".to_string()];
        config.lane_colors = vec!["#e06c75".to_string(), "#61afef".to_string()];
        config.hidden_actions = vec!["revert".to_string()];
        config.emoji_mappings = vec!["shipit:🚢".to_string()];
        config.graph_style = "angular".to_string();
        config.pr_provider = "https://git.example.com/{owner}/{repo}/compare/{base}...{head}".to_string();
        config.save(&dir).unwrap();

        let loaded = RepoConfig::load(&dir).unwrap();
        assert!(loaded.first_parent);
        assert!(!loaded.emoji);
        assert_eq!(loaded.repo_order, "path");
        assert_eq!(loaded.branch_globs.len(), 2);
        assert_eq!(loaded.lane_colors, vec!["#e06c75", "#61afef"]);
        assert_eq!(loaded.hidden_actions, vec!["revert"]);
        assert_eq!(loaded.emoji_mappings, vec!["shipit:🚢"]);
        assert_eq!(loaded.graph_style, "angular");
        assert_eq!(
            loaded.pr_provider,
            "https://git.example.com/{owner}/{repo}/compare/{base}...{head}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_none() {
        let dir = temp_dir("missing");
        assert!(RepoConfig::load(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
