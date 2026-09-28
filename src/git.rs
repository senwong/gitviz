//! Minimal read-only git access by shelling out to `git`.
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
}

impl Commit {
    pub fn short_sha(&self) -> &str {
        let end = self.sha.len().min(8);
        &self.sha[..end]
    }
}

/// Returns the current branch name, or `None` when HEAD is unborn/detached.
pub fn head_branch(repo: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match name.as_str() {
        "" => None,
        "HEAD" => Some("HEAD (detached)".to_string()),
        _ => Some(name),
    }
}

/// Loads up to `limit` commits, newest first, in date order.
pub fn log(repo: &Path, limit: usize) -> anyhow::Result<Vec<Commit>> {
    let format = format!(
        "--format=%H{RECORD_SEP}%P{RECORD_SEP}%D{RECORD_SEP}%an{RECORD_SEP}%at{RECORD_SEP}%s"
    );
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "log",
            "--date-order",
            &format!("-n{limit}"),
            format.as_str(),
        ])
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let text = String::from_utf8_lossy(&output.stdout);
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
        });
    }
    Ok(commits)
}
