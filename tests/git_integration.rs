//! Integration tests that exercise `gitviz::git` against a real, temporary
//! git repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use gitviz::git::{self, LogFilter};

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "gitviz-it-{tag}-{}-{}",
            std::process::id(),
            nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        git_run(&path, &["init", "-q"]);
        git_run(&path, &["config", "user.email", "test@example.com"]);
        git_run(&path, &["config", "user.name", "Test"]);
        git_run(&path, &["config", "commit.gpgsign", "false"]);
        git_run(&path, &["checkout", "-q", "-b", "main"]);
        Self { path }
    }

    fn commit(&self, file: &str, contents: &str, message: &str) {
        std::fs::write(self.path.join(file), contents).unwrap();
        git_run(&self.path, &["add", "-A"]);
        git_run(&self.path, &["commit", "-q", "-m", message]);
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn git_run(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn reads_log_status_and_branches() {
    let repo = TempRepo::new("log");
    repo.commit("a.txt", "hello\n", "initial :sparkles:");
    repo.commit("b.txt", "world\n", "second");

    let commits = git::log(&repo.path, 10, &LogFilter::default()).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].subject, "second");
    assert_eq!(commits[1].subject, "initial :sparkles:");
    assert_eq!(commits[0].parents.len(), 1);

    assert_eq!(git::head_branch(&repo.path).as_deref(), Some("main"));
    assert!(git::local_branches(&repo.path).iter().any(|b| b == "main"));
    assert!(git::status(&repo.path, true).is_empty());
}

#[test]
fn branch_tag_and_commit_operations() {
    let repo = TempRepo::new("ops");
    repo.commit("a.txt", "1\n", "one");
    repo.commit("b.txt", "2\n", "two");

    let head = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    git::create_branch(&repo.path, "feature", Some(&head)).unwrap();
    assert!(git::local_branches(&repo.path).iter().any(|b| b == "feature"));

    git::create_tag(&repo.path, "v1", &head).unwrap();
    assert!(git::tags(&repo.path).iter().any(|t| t == "v1"));

    git::checkout_branch(&repo.path, "main").unwrap();
    assert_eq!(git::head_branch(&repo.path).as_deref(), Some("main"));
}

#[test]
fn stash_workflow() {
    let repo = TempRepo::new("stash");
    repo.commit("a.txt", "1\n", "one");

    std::fs::write(repo.path.join("a.txt"), "changed\n").unwrap();
    assert!(!git::status(&repo.path, true).is_empty());

    git::stash_push(&repo.path, true).unwrap();
    assert!(git::status(&repo.path, true).is_empty());
    assert_eq!(git::stashes(&repo.path).len(), 1);

    git::stash_pop(&repo.path, 0).unwrap();
    assert!(git::stashes(&repo.path).is_empty());
}

#[test]
fn commit_detail_reports_changed_files() {
    let repo = TempRepo::new("detail");
    repo.commit("a.txt", "1\n", "one");
    repo.commit("a.txt", "1\n2\n", "two");

    let head = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();
    let detail = git::commit_detail(&repo.path, &head).unwrap();
    assert_eq!(detail.message.trim(), "two");
    assert_eq!(detail.files.len(), 1);
    assert_eq!(detail.files[0].path, "a.txt");
    assert_eq!(detail.files[0].status, 'M');
    assert!(detail.files[0].added >= 1);
}

#[test]
fn compare_and_diff_between_commits() {
    let repo = TempRepo::new("compare");
    repo.commit("a.txt", "one\n", "one");
    let first = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    repo.commit("a.txt", "one\ntwo\n", "two");
    let second = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    let files = git::compare_files(&repo.path, &first, &second);
    assert!(files.iter().any(|file| file.path == "a.txt"));

    let diff = git::compare_file_diff(&repo.path, &first, &second, "a.txt");
    assert!(diff.contains("+two"), "diff was: {diff}");

    // Unsigned commits report no signature.
    assert_eq!(git::signature_status(&repo.path, &second), None);
}

#[test]
fn remotes_are_readable() {
    let repo = TempRepo::new("remote");
    repo.commit("a.txt", "1\n", "one");

    git_run(
        &repo.path,
        &["remote", "add", "origin", "git@github.com:senwong/gitviz.git"],
    );
    assert!(git::remotes(&repo.path).iter().any(|name| name == "origin"));

    let info = git::hosting_remote(&repo.path).expect("hosting remote");
    assert_eq!(info.owner, "senwong");
    assert_eq!(info.repo, "gitviz");
}
