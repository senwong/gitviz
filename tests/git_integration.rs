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
fn reads_commit_body() {
    let repo = TempRepo::new("body");
    std::fs::write(repo.path.join("a.txt"), "1\n").unwrap();
    git_run(&repo.path, &["add", "-A"]);
    git_run(
        &repo.path,
        &[
            "commit",
            "-q",
            "-m",
            "subject line",
            "-m",
            "body line one\nbody line two",
        ],
    );

    let commits = git::log(&repo.path, 10, &LogFilter::default()).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "subject line");
    assert!(
        commits[0].body.contains("body line one") && commits[0].body.contains("body line two"),
        "body was: {:?}",
        commits[0].body
    );
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
fn creates_annotated_tag_with_message() {
    let repo = TempRepo::new("annotated");
    repo.commit("a.txt", "1\n", "one");
    let head = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    git::create_annotated_tag(&repo.path, "v1", &head, "first release").unwrap();
    let tags = git::tags_with_details(&repo.path);
    let tag = tags
        .iter()
        .find(|tag| tag.name == "v1")
        .expect("annotated tag present");
    assert_eq!(tag.message.trim(), "first release");
}

#[test]
fn diffs_working_tree_against_a_commit() {
    let repo = TempRepo::new("worktree");
    repo.commit("a.txt", "one\n", "one");
    let head = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    std::fs::write(repo.path.join("a.txt"), "one\ntwo\n").unwrap();
    let files = git::working_tree_files(&repo.path, &head);
    assert!(files.iter().any(|file| file.path == "a.txt"));

    let diff = git::working_tree_file_diff(&repo.path, &head, "a.txt");
    assert!(diff.contains("+two"), "diff was: {diff}");
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
fn lists_and_diffs_stash_contents() {
    let repo = TempRepo::new("stashdetail");
    repo.commit("a.txt", "1\n", "one");

    std::fs::write(repo.path.join("a.txt"), "1\n2\n").unwrap();
    git::stash_push(&repo.path, true).unwrap();

    let files = git::stash_files(&repo.path, 0);
    assert!(files.iter().any(|file| file.path == "a.txt"));

    let diff = git::stash_file_diff(&repo.path, 0, "a.txt");
    assert!(diff.contains("+2"), "diff was: {diff}");
}

#[test]
fn stashes_containing_reports_ancestor_commits() {
    let repo = TempRepo::new("stashcontains");
    repo.commit("a.txt", "1\n", "one");
    let base = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();

    std::fs::write(repo.path.join("a.txt"), "1\n2\n").unwrap();
    git::stash_push(&repo.path, true).unwrap();

    let contained = git::stashes_containing(&repo.path, &base);
    assert_eq!(contained, vec!["stash@{0}".to_string()]);
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
fn containing_branches_and_tags() {
    let repo = TempRepo::new("contains");
    repo.commit("a.txt", "1\n", "one");
    let first = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();
    repo.commit("b.txt", "2\n", "two");

    git::create_tag(&repo.path, "v1", &first).unwrap();

    assert!(
        git::branches_containing(&repo.path, &first)
            .iter()
            .any(|branch| branch == "main")
    );
    assert!(
        git::tags_containing(&repo.path, &first)
            .iter()
            .any(|tag| tag == "v1")
    );
}

#[test]
fn shows_file_at_revision_and_deletes_tag() {
    let repo = TempRepo::new("revfile");
    repo.commit("a.txt", "old\n", "one");
    let first = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();
    repo.commit("a.txt", "new\n", "two");

    assert_eq!(git::show_file(&repo.path, &first, "a.txt"), "old\n");

    git::create_tag(&repo.path, "v1", &first).unwrap();
    assert!(git::tags(&repo.path).iter().any(|tag| tag == "v1"));
    git::delete_tag(&repo.path, "v1").unwrap();
    assert!(!git::tags(&repo.path).iter().any(|tag| tag == "v1"));
}

#[test]
fn creates_branch_from_stash() {
    let repo = TempRepo::new("stashbranch");
    repo.commit("a.txt", "1\n", "one");
    std::fs::write(repo.path.join("a.txt"), "changed\n").unwrap();
    git::stash_push(&repo.path, true).unwrap();

    git::stash_branch(&repo.path, 0, "from-stash").unwrap();
    assert!(
        git::local_branches(&repo.path)
            .iter()
            .any(|branch| branch == "from-stash")
    );
    // Applying the stash to a branch removes it from the stash list.
    assert!(git::stashes(&repo.path).is_empty());
}

#[test]
fn cherry_pick_and_merge() {
    let repo = TempRepo::new("pickmerge");
    repo.commit("a.txt", "1\n", "one");

    // Create a feature branch with a commit, then cherry-pick it onto main.
    git::create_branch(&repo.path, "feature", None).unwrap();
    repo.commit("f.txt", "feature\n", "feature work");
    let feature_sha = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();
    git::checkout_branch(&repo.path, "main").unwrap();

    git::cherry_pick(&repo.path, &feature_sha).unwrap();
    let subjects: Vec<String> = git::log(&repo.path, 10, &LogFilter::default())
        .unwrap()
        .into_iter()
        .map(|commit| commit.subject)
        .collect();
    assert!(subjects.iter().any(|subject| subject == "feature work"));

    // Merge the feature branch with --no-ff.
    git::merge_no_ff(&repo.path, "feature").unwrap();
    let head = git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0]
        .sha
        .clone();
    let detail = git::commit_detail(&repo.path, &head).unwrap();
    // A merge commit has more than one parent; verify via the raw commit.
    let parents = &git::log(&repo.path, 1, &LogFilter::default()).unwrap()[0].parents;
    let _ = detail;
    assert_eq!(parents.len(), 2);
}

#[test]
fn discards_uncommitted_changes() {
    let repo = TempRepo::new("discard");
    repo.commit("a.txt", "1\n", "one");

    std::fs::write(repo.path.join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.path.join("untracked.txt"), "new\n").unwrap();
    assert!(!git::status(&repo.path, true).is_empty());

    git::discard_all(&repo.path, true).unwrap();
    assert!(git::status(&repo.path, true).is_empty());
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

#[test]
fn edits_a_remote_url() {
    let repo = TempRepo::new("seturl");
    repo.commit("a.txt", "1\n", "one");

    git::add_remote(&repo.path, "origin", "https://github.com/a/b.git").unwrap();
    assert_eq!(
        git::remote_url(&repo.path, "origin").as_deref(),
        Some("https://github.com/a/b.git")
    );

    git::set_remote_url(&repo.path, "origin", "git@github.com:c/d.git").unwrap();
    assert_eq!(
        git::remote_url(&repo.path, "origin").as_deref(),
        Some("git@github.com:c/d.git")
    );
}

#[test]
fn fetches_a_remote_branch_into_a_local_branch() {
    let remote = TempRepo::new("fetchsrc");
    remote.commit("a.txt", "1\n", "one");
    git::create_branch(&remote.path, "release", None).unwrap();
    remote.commit("b.txt", "2\n", "two");

    let bare = std::env::temp_dir().join(format!(
        "gitviz-it-bare-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = std::fs::remove_dir_all(&bare);
    let output = Command::new("git")
        .args(["clone", "--bare", "-q"])
        .arg(&remote.path)
        .arg(&bare)
        .output()
        .expect("failed to clone --bare");
    assert!(output.status.success());

    let local = TempRepo::new("fetchdst");
    git::add_remote(&local.path, "origin", bare.to_str().unwrap()).unwrap();
    assert!(
        !git::local_branches(&local.path)
            .iter()
            .any(|branch| branch == "local-release")
    );

    git::fetch_into_branch(&local.path, "origin", "release", "local-release").unwrap();
    assert!(
        git::local_branches(&local.path)
            .iter()
            .any(|branch| branch == "local-release")
    );

    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn pulls_from_a_remote() {
    let origin = TempRepo::new("pull-src");
    origin.commit("a.txt", "1\n", "one");

    let bare = std::env::temp_dir().join(format!(
        "gitviz-it-pull-bare-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = std::fs::remove_dir_all(&bare);
    let output = Command::new("git")
        .args(["clone", "--bare", "-q"])
        .arg(&origin.path)
        .arg(&bare)
        .output()
        .expect("failed to clone --bare");
    assert!(output.status.success());

    let local = TempRepo::new("pull-dst");
    git::add_remote(&local.path, "origin", bare.to_str().unwrap()).unwrap();
    git_run(&local.path, &["fetch", "origin"]);
    git_run(
        &local.path,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );

    origin.commit("b.txt", "2\n", "two");
    git_run(&origin.path, &["push", "-q", bare.to_str().unwrap(), "main"]);

    git::pull(&local.path).unwrap();
    let subjects: Vec<String> = git::log(&local.path, 10, &LogFilter::default())
        .unwrap()
        .into_iter()
        .map(|commit| commit.subject)
        .collect();
    assert!(subjects.iter().any(|subject| subject == "two"));

    let _ = std::fs::remove_dir_all(&bare);
}
