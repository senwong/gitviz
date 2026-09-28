//! Minimal git access by shelling out to `git`.
//!
//! Keeping this dependency-free (no libgit2) means the app stays small and
//! builds fast relative to the full Zed stack.

use std::path::Path;
use std::process::Command;

const RECORD_SEP: char = '\u{1f}';

#[derive(Clone, Debug)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author: String,
    pub timestamp: i64,
    pub subject: String,
    /// Assigned by [`crate::layout::assign_lanes`].
    pub lane: usize,
    /// Lanes with a line passing straight through this row.
    pub through: Vec<usize>,
    /// Lanes whose line curves into this commit's lane at the dot.
    pub incoming: Vec<usize>,
    /// Lanes this commit curves out to, for its non-first parents.
    pub outgoing: Vec<usize>,
    /// Whether a line comes into the commit's lane from the row above.
    pub top_line: bool,
    /// Whether the commit's lane continues into the row below.
    pub bottom_line: bool,
}

impl Commit {
    pub fn short_sha(&self) -> &str {
        let end = self.sha.len().min(8);
        &self.sha[..end]
    }
}

#[derive(Clone, Debug, Default)]
pub struct LogFilter {
    pub branches: bool,
    pub remotes: bool,
    pub tags: bool,
    pub first_parent: bool,
}

#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub added: u32,
    pub removed: u32,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct CommitDetail {
    pub message: String,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub files: Vec<ChangedFile>,
}

fn run(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git").arg("-C").arg(repo).args(args).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Returns the current branch name, or `None` when HEAD is unborn/detached.
pub fn head_branch(repo: &Path) -> Option<String> {
    let name = run(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    match name.trim() {
        "" => None,
        "HEAD" => Some("HEAD (detached)".to_string()),
        name => Some(name.to_string()),
    }
}

/// Loads up to `limit` commits, newest first, in date order.
pub fn log(repo: &Path, limit: usize, filter: &LogFilter) -> anyhow::Result<Vec<Commit>> {
    let format = format!(
        "--format=%H{RECORD_SEP}%P{RECORD_SEP}%D{RECORD_SEP}%an{RECORD_SEP}%at{RECORD_SEP}%s"
    );
    let limit_arg = format!("-n{limit}");

    let mut args: Vec<String> = vec!["log".into(), "--date-order".into(), limit_arg, format];
    if filter.first_parent {
        args.push("--first-parent".into());
    }
    if filter.branches {
        args.push("--branches".into());
    }
    if filter.remotes {
        args.push("--remotes".into());
    }
    if filter.tags {
        args.push("--tags".into());
    }
    args.push("HEAD".into());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let text = run(repo, &arg_refs)?;

    let mut commits = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(RECORD_SEP);
        let sha = fields.next().unwrap_or_default().to_string();
        let parents = fields
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .map(String::from)
            .collect();
        let refs = fields
            .next()
            .unwrap_or_default()
            .split(',')
            .map(|ref_name| ref_name.trim().to_string())
            .filter(|ref_name| !ref_name.is_empty() && ref_name != "HEAD")
            .collect();
        let author = fields.next().unwrap_or_default().to_string();
        let timestamp = fields.next().unwrap_or("0").parse().unwrap_or(0);
        let subject = fields.next().unwrap_or_default().to_string();
        commits.push(Commit {
            sha,
            parents,
            refs,
            author,
            timestamp,
            subject,
            lane: 0,
            through: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
            top_line: false,
            bottom_line: false,
        });
    }
    Ok(commits)
}

/// Loads the full commit message plus the numstat of changed files.
pub fn commit_detail(repo: &Path, sha: &str) -> anyhow::Result<CommitDetail> {
    let header = run(
        repo,
        &["show", "-s", "--format=%B\x1f%an\x1f%ae\x1f%at", sha],
    )?;
    let mut parts = header.splitn(4, '\u{1f}');
    let message = parts.next().unwrap_or_default().trim_end().to_string();
    let author = parts.next().unwrap_or_default().to_string();
    let email = parts.next().unwrap_or_default().to_string();
    let timestamp = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);

    let numstat = run(repo, &["show", "--numstat", "--format=", sha]).unwrap_or_default();
    let mut files = Vec::new();
    for line in numstat.lines() {
        let mut columns = line.split('\t');
        let added = columns.next().unwrap_or_default();
        let removed = columns.next().unwrap_or_default();
        let Some(path) = columns.next() else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        files.push(ChangedFile {
            added: added.parse().unwrap_or(0),
            removed: removed.parse().unwrap_or(0),
            path: path.to_string(),
        });
    }

    Ok(CommitDetail {
        message,
        author,
        email,
        timestamp,
        files,
    })
}

pub fn local_branches(repo: &Path) -> Vec<String> {
    run(repo, &["branch", "--format=%(refname:short)"])
        .map(|output| lines(&output))
        .unwrap_or_default()
}

pub fn tags(repo: &Path) -> Vec<String> {
    run(repo, &["tag"])
        .map(|output| lines(&output))
        .unwrap_or_default()
}

// -- Mutating operations ---------------------------------------------------

pub fn checkout_branch(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["switch", name]).map(|_| ())
}

pub fn create_branch(repo: &Path, name: &str, base: Option<&str>) -> anyhow::Result<()> {
    match base {
        Some(base) => run(repo, &["switch", "-c", name, base]).map(|_| ()),
        None => run(repo, &["switch", "-c", name]).map(|_| ()),
    }
}

pub fn create_tag(repo: &Path, name: &str, sha: &str) -> anyhow::Result<()> {
    run(repo, &["tag", name, sha]).map(|_| ())
}

pub fn cherry_pick(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["cherry-pick", sha]).map(|_| ())
}

pub fn revert(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["revert", "--no-edit", sha]).map(|_| ())
}

pub fn merge(repo: &Path, rev: &str) -> anyhow::Result<()> {
    run(repo, &["merge", rev]).map(|_| ())
}

pub fn rebase(repo: &Path, rev: &str) -> anyhow::Result<()> {
    run(repo, &["rebase", rev]).map(|_| ())
}

pub fn reset(repo: &Path, sha: &str, mode: ResetMode) -> anyhow::Result<()> {
    run(repo, &["reset", mode.flag(), sha]).map(|_| ())
}

#[derive(Clone, Copy, Debug)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusEntry {
    pub index_status: char,
    pub worktree_status: char,
    pub path: String,
}

impl StatusEntry {
    pub fn is_untracked(&self) -> bool {
        self.index_status == '?' && self.worktree_status == '?'
    }
}

/// Reads the working tree status (`git status --porcelain`).
pub fn status(repo: &Path, include_untracked: bool) -> Vec<StatusEntry> {
    let untracked = if include_untracked {
        "--untracked-files=all"
    } else {
        "--untracked-files=no"
    };
    let output = run(repo, &["status", "--porcelain", untracked]).unwrap_or_default();
    output
        .lines()
        .filter_map(|line| {
            let mut chars = line.chars();
            let index_status = chars.next()?;
            let worktree_status = chars.next()?;
            let path = line.get(3..)?.trim().to_string();
            Some(StatusEntry {
                index_status,
                worktree_status,
                path,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

pub fn stashes(repo: &Path) -> Vec<StashEntry> {
    run(repo, &["stash", "list", "--format=%s"])
        .map(|output| {
            lines(&output)
                .into_iter()
                .enumerate()
                .map(|(index, message)| StashEntry { index, message })
                .collect()
        })
        .unwrap_or_default()
}

/// SHA set of commits reachable from HEAD (bounded by `limit`).
pub fn head_ancestors(repo: &Path, limit: usize) -> std::collections::HashSet<String> {
    run(repo, &["rev-list", &format!("--max-count={limit}"), "HEAD"])
        .map(|output| lines(&output).into_iter().collect())
        .unwrap_or_default()
}

/// Unified diff text of a single file at a commit.
pub fn file_diff(repo: &Path, sha: &str, path: &str) -> String {
    run(repo, &["show", "--format=", "--no-color", "--", sha, path])
        .or_else(|_| run(repo, &["show", "--format=", "--no-color", sha, "--", path]))
        .unwrap_or_default()
}

pub fn compare_files(repo: &Path, from: &str, to: &str) -> Vec<ChangedFile> {
    let output = run(repo, &["diff", "--numstat", from, to]).unwrap_or_default();
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let added = columns.next()?.parse().ok()?;
            let removed = columns.next()?.parse().ok()?;
            let path = columns.next()?.to_string();
            Some(ChangedFile {
                added,
                removed,
                path,
            })
        })
        .collect()
}

pub fn delete_branch(repo: &Path, name: &str, force: bool) -> anyhow::Result<()> {
    run(repo, &["branch", if force { "-D" } else { "-d" }, name]).map(|_| ())
}

pub fn rename_branch(repo: &Path, old: &str, new: &str) -> anyhow::Result<()> {
    run(repo, &["branch", "-m", old, new]).map(|_| ())
}

pub fn pull(repo: &Path) -> anyhow::Result<()> {
    run(repo, &["pull"]).map(|_| ())
}

pub fn delete_tag(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["tag", "-d", name]).map(|_| ())
}

pub fn push_tag(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["push", "origin", name]).map(|_| ())
}

pub fn stash_push(repo: &Path, include_untracked: bool) -> anyhow::Result<()> {
    let args: Vec<&str> = if include_untracked {
        vec!["stash", "push", "--include-untracked"]
    } else {
        vec!["stash", "push"]
    };
    run(repo, &args).map(|_| ())
}

pub fn stash_apply(repo: &Path, index: usize) -> anyhow::Result<()> {
    run(repo, &["stash", "apply", &format!("stash@{{{index}}}")]).map(|_| ())
}

pub fn stash_pop(repo: &Path, index: usize) -> anyhow::Result<()> {
    run(repo, &["stash", "pop", &format!("stash@{{{index}}}")]).map(|_| ())
}

pub fn stash_drop(repo: &Path, index: usize) -> anyhow::Result<()> {
    run(repo, &["stash", "drop", &format!("stash@{{{index}}}")]).map(|_| ())
}

pub fn checkout_commit(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["checkout", sha]).map(|_| ())
}

/// Force-fetches branches and tags from all remotes, overwriting local tags.
pub fn fetch_all_tags(repo: &Path) -> anyhow::Result<()> {
    run(repo, &["fetch", "--all", "--tags", "--force"]).map(|_| ())
}

/// Pushes the current branch, setting its upstream when it has none.
pub fn push_current_branch(repo: &Path) -> anyhow::Result<()> {
    let branch = head_branch(repo).unwrap_or_else(|| "HEAD".to_string());
    let remotes = lines(&run(repo, &["remote"]).unwrap_or_default());
    let remote = if remotes.iter().any(|name| name == "origin") {
        "origin"
    } else {
        remotes.first().map(String::as_str).unwrap_or("origin")
    };
    run(repo, &["push", "--set-upstream", remote, &branch]).map(|_| ())
}
