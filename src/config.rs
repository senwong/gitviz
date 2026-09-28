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
        std::fs::write(Self::path(repo), output)?;
        Ok(())
    }
}
