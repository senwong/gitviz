//! Minimal git access by shelling out to `git`.
//!
//! Keeping this dependency-free (no libgit2) means the app stays small and
//! builds fast relative to the full Zed stack.

use std::path::Path;
use std::process::Command;

const RECORD_SEP: char = '\u{1f}';
const RECORD_END: char = '\u{1e}';

#[derive(Clone, Debug)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author: String,
    pub timestamp: i64,
    pub commit_timestamp: i64,
    pub author_date: String,
    pub commit_date: String,
    pub subject: String,
    /// Full commit message body (everything after the subject).
    pub body: String,
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
    pub use_mailmap: bool,
    pub include_reflogs: bool,
    pub remote_heads: bool,
    /// Show only commits that are reachable from tags (and not from any
    /// branch or remote).
    pub only_tags: bool,
    /// Show full ref names (`refs/heads/main`) instead of short names.
    pub full_refs: bool,
}

/// The `git log` decoration arguments for a filter.
pub fn decorate_args(full_refs: bool) -> Vec<String> {
    if full_refs {
        vec!["--decorate=full".to_string()]
    } else {
        Vec::new()
    }
}

/// The ref-related `git log` arguments implied by a filter. Pure so it can be
/// unit tested without a repository.
pub fn log_ref_args(filter: &LogFilter) -> Vec<String> {
    if filter.only_tags {
        return vec![
            "--tags".into(),
            "--not".into(),
            "--branches".into(),
            "--remotes".into(),
        ];
    }

    let mut args = Vec::new();
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
    if filter.remote_heads {
        args.push("--glob=refs/remotes/*/HEAD".into());
    }
    args
}

#[derive(Clone, Debug, Default)]
pub struct ChangedFile {
    /// `A`dded, `M`odified, `D`eleted, `R`enamed or `U`nmerged.
    pub status: char,
    pub added: u32,
    pub removed: u32,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct TagDetail {
    pub name: String,
    pub commit: String,
    pub tagger: String,
    pub date: String,
    pub message: String,
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
        "--format=%H{RECORD_SEP}%P{RECORD_SEP}%D{RECORD_SEP}%an{RECORD_SEP}%at{RECORD_SEP}%ct{RECORD_SEP}%ad{RECORD_SEP}%cd{RECORD_SEP}%s{RECORD_SEP}%b{RECORD_END}"
    );
    let limit_arg = format!("-n{limit}");

    let mut args: Vec<String> = vec![
        "log".into(),
        "--date-order".into(),
        "--date=iso".into(),
        limit_arg,
        format,
    ];
    if filter.use_mailmap {
        args.push("--use-mailmap".into());
    }
    if filter.include_reflogs {
        args.push("--reflog".into());
    }
    args.extend(decorate_args(filter.full_refs));
    args.extend(log_ref_args(filter));
    args.push("HEAD".into());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let text = run(repo, &arg_refs)?;

    let mut commits = Vec::new();
    for record in text.split(RECORD_END) {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split(RECORD_SEP);
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
        let commit_timestamp = fields.next().unwrap_or("0").parse().unwrap_or(0);
        let author_date = fields.next().unwrap_or_default().to_string();
        let commit_date = fields.next().unwrap_or_default().to_string();
        let subject = fields.next().unwrap_or_default().to_string();
        let body = fields.next().unwrap_or_default().trim().to_string();
        commits.push(Commit {
            sha,
            parents,
            refs,
            author,
            timestamp,
            commit_timestamp,
            author_date,
            commit_date,
            subject,
            body,
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

/// Stash references whose history contains `sha`.
pub fn stashes_containing(repo: &Path, sha: &str) -> Vec<String> {
    let mut result = Vec::new();
    for index in 0..stashes(repo).len() {
        let stash = format!("stash@{{{index}}}");
        if run(repo, &["merge-base", "--is-ancestor", sha, &stash]).is_ok() {
            result.push(stash);
        }
    }
    result
}

/// Local branches whose history contains `sha`.
pub fn branches_containing(repo: &Path, sha: &str) -> Vec<String> {
    run(
        repo,
        &["branch", "--contains", sha, "--format=%(refname:short)"],
    )
    .map(|output| lines(&output))
    .unwrap_or_default()
}

/// Tags whose history contains `sha`.
pub fn tags_containing(repo: &Path, sha: &str) -> Vec<String> {
    run(repo, &["tag", "--contains", sha])
        .map(|output| lines(&output))
        .unwrap_or_default()
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

    let mut statuses: std::collections::HashMap<String, char> = std::collections::HashMap::new();
    let name_status = run(repo, &["show", "--name-status", "--format=", sha]).unwrap_or_default();
    for line in name_status.lines() {
        let mut columns = line.split('\t');
        let Some(status) = columns.next() else {
            continue;
        };
        // Renames/copies carry two paths; the last field is the new path.
        let path = columns.last().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let code = status.chars().next().unwrap_or('M');
        statuses.insert(path.to_string(), code);
    }

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
            status: statuses.get(path).copied().unwrap_or('M'),
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

/// Remote-tracking branches (`origin/main`, ...), excluding symbolic
/// `*/HEAD` entries.
pub fn remote_branches(repo: &Path) -> Vec<String> {
    run(repo, &["branch", "-r", "--format=%(refname:short)"])
        .map(|output| {
            lines(&output)
                .into_iter()
                .filter(|name| !name.ends_with("/HEAD"))
                .collect()
        })
        .unwrap_or_default()
}

/// Parses `git rev-list --left-right --count <branch>...HEAD` output of the
/// form `"<ahead>\t<behind>"` into `(ahead, behind)`.
///
/// `--left-right` prints the commits unique to the left side (the branch) and
/// then the right side (HEAD), so the first column is how far the branch is
/// ahead and the second how far it is behind.
pub fn parse_ahead_behind(output: &str) -> Option<(usize, usize)> {
    let mut columns = output.split_whitespace();
    let ahead: usize = columns.next()?.parse().ok()?;
    let behind: usize = columns.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// How many commits `branch` is ahead of / behind the current HEAD.
pub fn ahead_behind(repo: &Path, branch: &str) -> Option<(usize, usize)> {
    let range = format!("{branch}...HEAD");
    let output = run(repo, &["rev-list", "--left-right", "--count", &range]).ok()?;
    parse_ahead_behind(&output)
}

pub fn tags(repo: &Path) -> Vec<String> {
    run(repo, &["tag"])
        .map(|output| lines(&output))
        .unwrap_or_default()
}

/// Annotated tags with their tagger and message, keyed by the tagged commit.
pub fn tags_with_details(repo: &Path) -> Vec<TagDetail> {
    let format = format!(
        "--format=%(refname:short){RECORD_SEP}%(*objectname){RECORD_SEP}%(taggername){RECORD_SEP}%(taggerdate:iso){RECORD_SEP}%(subject){RECORD_SEP}%(objecttype)"
    );
    let output = run(repo, &["for-each-ref", "refs/tags", &format]).unwrap_or_default();
    let mut details = Vec::new();
    for line in output.lines() {
        let mut fields = line.split(RECORD_SEP);
        let name = fields.next().unwrap_or_default().to_string();
        let commit = fields.next().unwrap_or_default().to_string();
        let tagger = fields.next().unwrap_or_default().to_string();
        let date = fields.next().unwrap_or_default().to_string();
        let message = fields.next().unwrap_or_default().to_string();
        let object_type = fields.next().unwrap_or_default();
        if object_type != "tag" || commit.is_empty() {
            continue;
        }
        details.push(TagDetail {
            name,
            commit,
            tagger,
            date,
            message,
        });
    }
    details
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

/// Arguments for creating an annotated tag with a message.
pub fn annotated_tag_args(name: &str, sha: &str, message: &str) -> Vec<String> {
    vec![
        "tag".to_string(),
        "-a".to_string(),
        name.to_string(),
        sha.to_string(),
        "-m".to_string(),
        message.to_string(),
    ]
}

pub fn create_annotated_tag(
    repo: &Path,
    name: &str,
    sha: &str,
    message: &str,
) -> anyhow::Result<()> {
    let args = annotated_tag_args(name, sha, message);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(repo, &refs).map(|_| ())
}

pub fn cherry_pick(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["cherry-pick", sha]).map(|_| ())
}

pub fn cherry_pick_allow_empty(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["cherry-pick", "--allow-empty", sha]).map(|_| ())
}

pub fn merge_no_ff(repo: &Path, rev: &str) -> anyhow::Result<()> {
    run(repo, &["merge", "--no-ff", rev]).map(|_| ())
}

pub fn merge_squash(repo: &Path, rev: &str) -> anyhow::Result<()> {
    run(repo, &["merge", "--squash", rev]).map(|_| ())
}

/// The `fetch` arguments used by the Refresh button. Pure so it is testable.
pub fn fetch_args(prune: bool, prune_tags: bool) -> Vec<String> {
    let mut args = vec![
        "fetch".to_string(),
        "--all".to_string(),
        "--tags".to_string(),
        "--force".to_string(),
    ];
    if prune {
        args.push("--prune".to_string());
    }
    if prune_tags {
        args.push("--prune-tags".to_string());
    }
    args
}

pub fn fetch_with(repo: &Path, prune: bool, prune_tags: bool) -> anyhow::Result<()> {
    let args = fetch_args(prune, prune_tags);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(repo, &refs).map(|_| ())
}

/// The extra flag for a force push, if any.
pub fn push_force_flag(force: bool) -> Option<&'static str> {
    force.then_some("--force-with-lease")
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

/// Contents of a file at a specific commit.
pub fn show_file(repo: &Path, sha: &str, path: &str) -> String {
    run(repo, &["show", &format!("{sha}:{path}")]).unwrap_or_default()
}

/// Unified diff of a single file between two commits.
pub fn compare_file_diff(repo: &Path, from: &str, to: &str, path: &str) -> String {
    run(repo, &["diff", "--no-color", from, to, "--", path]).unwrap_or_default()
}

pub fn compare_files(repo: &Path, from: &str, to: &str) -> Vec<ChangedFile> {
    let output = run(repo, &["diff", "--numstat", from, to]).unwrap_or_default();
    parse_numstat(&output)
}

/// Files that differ between the working tree and `sha`.
pub fn working_tree_files(repo: &Path, sha: &str) -> Vec<ChangedFile> {
    let output = run(repo, &["diff", "--numstat", sha]).unwrap_or_default();
    parse_numstat(&output)
}

/// Unified diff of a single file between the working tree and `sha`.
pub fn working_tree_file_diff(repo: &Path, sha: &str, path: &str) -> String {
    run(repo, &["diff", "--no-color", sha, "--", path]).unwrap_or_default()
}

fn parse_numstat(output: &str) -> Vec<ChangedFile> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let added = columns.next()?.parse().ok()?;
            let removed = columns.next()?.parse().ok()?;
            let path = columns.next()?.to_string();
            Some(ChangedFile {
                status: 'M',
                added,
                removed,
                path,
            })
        })
        .collect()
}

/// Files changed by a stash, from `git stash show --numstat`.
pub fn stash_files(repo: &Path, index: usize) -> Vec<ChangedFile> {
    let stash = format!("stash@{{{index}}}");
    let output = run(repo, &["stash", "show", "--numstat", &stash]).unwrap_or_default();
    parse_numstat(&output)
}

/// Unified diff of a single file in a stash, comparing against the stash's
/// first parent (its base commit).
pub fn stash_file_diff(repo: &Path, index: usize, path: &str) -> String {
    let stash = format!("stash@{{{index}}}");
    run(
        repo,
        &["diff", "--no-color", &format!("{stash}^1"), &stash, "--", path],
    )
    .unwrap_or_default()
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

/// Discards a single working-tree file, removing it if untracked.
pub fn discard_file(repo: &Path, path: &str, untracked: bool) -> anyhow::Result<()> {
    if untracked {
        run(repo, &["clean", "-f", "--", path]).map(|_| ())
    } else {
        run(repo, &["checkout", "--", path]).map(|_| ())
    }
}

/// Discards all working tree and index changes, optionally removing untracked
/// files (`git reset --hard` + `git clean -fd`).
pub fn discard_all(repo: &Path, include_untracked: bool) -> anyhow::Result<()> {
    run(repo, &["reset", "--hard"])?;
    if include_untracked {
        run(repo, &["clean", "-fd"])?;
    }
    Ok(())
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

pub fn stash_branch(repo: &Path, index: usize, name: &str) -> anyhow::Result<()> {
    run(repo, &["stash", "branch", name, &format!("stash@{{{index}}}")]).map(|_| ())
}

// -- Remotes ---------------------------------------------------------------

pub fn remotes(repo: &Path) -> Vec<String> {
    run(repo, &["remote"]).map(|output| lines(&output)).unwrap_or_default()
}

pub fn remote_url(repo: &Path, name: &str) -> Option<String> {
    run(repo, &["remote", "get-url", name])
        .ok()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
}

pub fn add_remote(repo: &Path, name: &str, url: &str) -> anyhow::Result<()> {
    run(repo, &["remote", "add", name, url]).map(|_| ())
}

pub fn remove_remote(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["remote", "remove", name]).map(|_| ())
}

pub fn set_remote_url(repo: &Path, name: &str, url: &str) -> anyhow::Result<()> {
    run(repo, &["remote", "set-url", name, url]).map(|_| ())
}

/// Git arguments that update a local branch from a remote branch without
/// checking it out: `git fetch <remote> <remote_branch>:<local_branch>`.
pub fn fetch_into_args(remote: &str, remote_branch: &str, local_branch: &str) -> Vec<String> {
    vec![
        "fetch".to_string(),
        remote.to_string(),
        format!("{remote_branch}:{local_branch}"),
    ]
}

pub fn fetch_into_branch(
    repo: &Path,
    remote: &str,
    remote_branch: &str,
    local_branch: &str,
) -> anyhow::Result<()> {
    let args = fetch_into_args(remote, remote_branch, local_branch);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(repo, &refs).map(|_| ())
}

pub fn prune_remote(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["remote", "prune", name]).map(|_| ())
}

pub fn fetch_remote(repo: &Path, name: &str) -> anyhow::Result<()> {
    run(repo, &["fetch", name]).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    GitHub,
    GitLab,
    Bitbucket,
}

#[derive(Clone, Debug)]
pub struct RemoteInfo {
    pub provider: Provider,
    pub host: String,
    pub owner: String,
    pub repo: String,
}

pub fn parse_remote(url: &str) -> Option<RemoteInfo> {
    let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        (host.to_string(), path.to_string())
    } else if let Some(rest) = url.split_once("://").map(|(_, rest)| rest) {
        let (host, path) = rest.split_once('/')?;
        (host.to_string(), path.to_string())
    } else {
        return None;
    };

    let path = path.trim_end_matches(".git").to_string();
    let mut segments = path.splitn(2, '/');
    let owner = segments.next()?.to_string();
    let repo = segments.next()?.to_string();

    let provider = if host.contains("github") {
        Provider::GitHub
    } else if host.contains("gitlab") {
        Provider::GitLab
    } else if host.contains("bitbucket") {
        Provider::Bitbucket
    } else {
        return None;
    };

    Some(RemoteInfo {
        provider,
        host,
        owner,
        repo,
    })
}

/// Expands a custom Pull Request URL template. Supported placeholders:
/// `{host}`, `{owner}`, `{repo}`, `{base}` and `{head}`.
pub fn render_pr_template(
    template: &str,
    host: &str,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
) -> String {
    template
        .replace("{host}", host)
        .replace("{owner}", owner)
        .replace("{repo}", repo)
        .replace("{base}", base)
        .replace("{head}", head)
}

/// Expands a custom Issue URL template. Supported placeholders: `{host}`,
/// `{owner}`, `{repo}` and `{issue}`.
pub fn render_issue_template(
    template: &str,
    host: &str,
    owner: &str,
    repo: &str,
    issue: &str,
) -> String {
    template
        .replace("{host}", host)
        .replace("{owner}", owner)
        .replace("{repo}", repo)
        .replace("{issue}", issue)
}

impl RemoteInfo {
    pub fn web_url(&self) -> String {
        format!("https://{}/{}/{}", self.host, self.owner, self.repo)
    }

    pub fn issue_url(&self, issue: &str) -> String {
        format!("{}/issues/{}", self.web_url(), issue)
    }

    pub fn pr_url(&self, base: &str, head: &str, description: &str) -> String {
        let description = urlencode(description);
        match self.provider {
            Provider::GitHub => format!(
                "{}/compare/{}...{}?expand=1&body={}",
                self.web_url(),
                base,
                head,
                description
            ),
            Provider::GitLab => format!(
                "{}/-/merge_requests/new?merge_request[source_branch]={}&merge_request[target_branch]={}&merge_request[description]={}",
                self.web_url(),
                head,
                base,
                description
            ),
            Provider::Bitbucket => format!(
                "{}/pull-requests/new?source={}&dest={}&description={}",
                self.web_url(),
                head,
                base,
                description
            ),
        }
    }

    pub fn commit_url(&self, sha: &str) -> String {
        match self.provider {
            Provider::GitHub => format!("{}/commit/{}", self.web_url(), sha),
            Provider::GitLab => format!("{}/-/commit/{}", self.web_url(), sha),
            Provider::Bitbucket => format!("{}/commits/{}", self.web_url(), sha),
        }
    }
}

fn urlencode(input: &str) -> String {
    let mut output = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            b' ' => output.push_str("%20"),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

pub fn open_url(url: &str) -> anyhow::Result<()> {
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map(|_| ())
        .map_err(Into::into)
}

pub fn open_path(path: &Path) -> anyhow::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .status()
        .map(|_| ())
        .map_err(Into::into)
}

/// Drops a single commit from the current branch with `git rebase --onto`.
pub fn drop_commit(repo: &Path, sha: &str) -> anyhow::Result<()> {
    run(repo, &["rebase", "--onto", &format!("{sha}^"), sha]).map(|_| ())
}

/// First character of `git log -1 --format=%G?`, e.g. `G` for a good signature.
pub fn signature_status(repo: &Path, sha: &str) -> Option<char> {
    let code = run(repo, &["log", "-1", "--format=%G?", sha])
        .ok()?
        .trim()
        .chars()
        .next()?;
    // 'N' means there is no signature at all.
    (code != 'N').then_some(code)
}

/// The verification message for a signed commit (`%GG`), if any.
pub fn signature_details(repo: &Path, sha: &str) -> Option<String> {
    let text = run(repo, &["log", "-1", "--format=%GG", sha]).ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn default_branch(repo: &Path) -> Option<String> {
    if let Ok(output) = run(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        let name = output.trim().trim_start_matches("origin/").to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    for candidate in ["main", "master"] {
        if run(repo, &["show-ref", "--verify", &format!("refs/heads/{candidate}")]).is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// The first remote that looks like a known hosting provider.
pub fn hosting_remote(repo: &Path) -> Option<RemoteInfo> {
    for name in remotes(repo) {
        if let Some(url) = remote_url(repo, &name)
            && let Some(info) = parse_remote(&url)
        {
            return Some(info);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_ssh_remote() {
        let info = parse_remote("git@github.com:senwong/gitviz.git").unwrap();
        assert_eq!(info.provider, Provider::GitHub);
        assert_eq!(info.owner, "senwong");
        assert_eq!(info.repo, "gitviz");
        assert_eq!(
            info.commit_url("abc123"),
            "https://github.com/senwong/gitviz/commit/abc123"
        );
    }

    #[test]
    fn parses_gitlab_https_remote_with_subgroup() {
        let info = parse_remote("https://gitlab.com/group/sub/proj.git").unwrap();
        assert_eq!(info.provider, Provider::GitLab);
        assert_eq!(info.owner, "group");
        assert_eq!(info.repo, "sub/proj");
    }

    #[test]
    fn parses_bitbucket_remote() {
        let info = parse_remote("git@bitbucket.org:team/repo.git").unwrap();
        assert_eq!(info.provider, Provider::Bitbucket);
        assert!(info.issue_url("7").ends_with("/issues/7"));
    }

    #[test]
    fn unknown_host_is_none() {
        assert!(parse_remote("git@example.com:me/repo.git").is_none());
        assert!(parse_remote("not a url").is_none());
    }

    #[test]
    fn urlencode_escapes_specials() {
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
        assert_eq!(urlencode("safe-._~"), "safe-._~");
    }

    #[test]
    fn log_args_include_enabled_refs() {
        let filter = LogFilter {
            branches: true,
            remotes: true,
            tags: true,
            ..LogFilter::default()
        };
        let args = log_ref_args(&filter);
        assert_eq!(args, vec!["--branches", "--remotes", "--tags"]);
        assert!(!args.contains(&"--first-parent".to_string()));
    }

    #[test]
    fn log_args_first_parent_and_remote_heads() {
        let filter = LogFilter {
            first_parent: true,
            branches: true,
            remote_heads: true,
            ..LogFilter::default()
        };
        let args = log_ref_args(&filter);
        assert_eq!(args[0], "--first-parent");
        assert!(args.contains(&"--glob=refs/remotes/*/HEAD".to_string()));
    }

    #[test]
    fn log_args_only_tags() {
        let filter = LogFilter {
            only_tags: true,
            branches: true,
            remotes: true,
            tags: true,
            ..LogFilter::default()
        };
        assert_eq!(
            log_ref_args(&filter),
            vec!["--tags", "--not", "--branches", "--remotes"]
        );
    }

    #[test]
    fn fetch_args_add_prune_flags() {
        assert_eq!(
            fetch_args(false, false),
            vec!["fetch", "--all", "--tags", "--force"]
        );
        assert_eq!(
            fetch_args(true, true),
            vec![
                "fetch",
                "--all",
                "--tags",
                "--force",
                "--prune",
                "--prune-tags"
            ]
        );
    }

    #[test]
    fn decorate_args_full_only_when_requested() {
        assert!(decorate_args(false).is_empty());
        assert_eq!(decorate_args(true), vec!["--decorate=full"]);
    }

    #[test]
    fn push_force_flag_only_when_forced() {
        assert_eq!(push_force_flag(false), None);
        assert_eq!(push_force_flag(true), Some("--force-with-lease"));
    }

    #[test]
    fn fetch_into_args_map_remote_to_local() {
        assert_eq!(
            fetch_into_args("origin", "main", "main"),
            vec!["fetch", "origin", "main:main"]
        );
        assert_eq!(
            fetch_into_args("upstream", "develop", "local-dev"),
            vec!["fetch", "upstream", "develop:local-dev"]
        );
    }

    #[test]
    fn annotated_tag_args_include_message() {
        assert_eq!(
            annotated_tag_args("v1.0", "abc123", "release 1.0"),
            vec!["tag", "-a", "v1.0", "abc123", "-m", "release 1.0"]
        );
    }

    #[test]
    fn pr_template_fills_placeholders() {
        let template = "https://git.example.com/{owner}/{repo}/compare/{base}...{head}?host={host}";
        assert_eq!(
            render_pr_template(template, "git.example.com", "acme", "widget", "main", "feature"),
            "https://git.example.com/acme/widget/compare/main...feature?host=git.example.com"
        );
        // Unknown placeholders are left untouched.
        assert_eq!(
            render_pr_template("{owner}/{unknown}", "h", "acme", "r", "b", "x"),
            "acme/{unknown}"
        );
    }

    #[test]
    fn issue_template_fills_placeholders() {
        assert_eq!(
            render_issue_template(
                "https://bugs.example.com/{owner}/{repo}/issues/{issue}",
                "bugs.example.com",
                "acme",
                "widget",
                "42"
            ),
            "https://bugs.example.com/acme/widget/issues/42"
        );
    }

    #[test]
    fn parses_ahead_behind_counts() {
        assert_eq!(parse_ahead_behind("0\t1\n"), Some((0, 1)));
        assert_eq!(parse_ahead_behind("3 2"), Some((3, 2)));
        assert_eq!(parse_ahead_behind("garbage"), None);
    }
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
