//! The main gpui view.
//!
//! Aimed at feature parity with mhutchie/vscode-git-graph: a multi-repository
//! commit graph with refs, uncommitted changes, stashes, columns, a commit
//! detail panel, commit comparison, code review tracking, a branch filter, a
//! find box, a repository palette and a right-click menu of git operations.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, ExternalPaths, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, PathPromptOptions, Pixels, Render,
    UniformListScrollHandle, Window, div, point, prelude::*, px, uniform_list,
};

use crate::actions::*;
use crate::config::RepoConfig;
use crate::discovery::Repo;
use crate::emoji;
use crate::git::{
    self, ChangedFile, Commit, CommitDetail, LogFilter, RemoteInfo, ResetMode, StashEntry,
    StatusEntry,
};
use crate::layout;
use crate::markdown::{self, SpanStyle};
use crate::review::ReviewStore;
use crate::theme::Theme;

const COMMIT_LIMIT: usize = 2000;
const INITIAL_LOAD: usize = 300;
const LANE_WIDTH: f32 = 14.0;
const ROW_HEIGHT: f32 = 22.0;
const DOT_SIZE: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Uncommitted,
    Stash(usize),
    Commit(usize),
}

#[derive(Clone, Copy)]
struct Columns {
    date: bool,
    author: bool,
    commit: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            date: true,
            author: true,
            commit: true,
        }
    }
}

/// The result of the (expensive, background) git work for one repository.
struct LoadResult {
    branch: Option<String>,
    head_ancestors: HashSet<String>,
    local_branches: Vec<String>,
    remotes: Vec<String>,
    remote_info: Option<RemoteInfo>,
    tags: Vec<git::TagDetail>,
    status: Vec<StatusEntry>,
    stashes: Vec<StashEntry>,
    commits: Result<Vec<Commit>, String>,
}

pub struct GraphView {
    repos: Vec<Repo>,
    roots: Vec<std::path::PathBuf>,
    workspace_path: Option<std::path::PathBuf>,
    recent: Vec<std::path::PathBuf>,
    recent_menu: RecentMenu,
    theme_menu: RecentMenu,
    repo_depth: usize,
    active: usize,
    /// Incremented on every `load`, so stale background results are ignored.
    load_gen: u64,
    loading: bool,
    branch: Option<String>,
    commits: Vec<Commit>,
    rows: Vec<RowKind>,
    rows_dirty: bool,
    error: Option<String>,
    head_ancestors: HashSet<String>,
    status: Vec<StatusEntry>,
    stashes: Vec<StashEntry>,
    selected: Option<RowKind>,
    compare: Option<usize>,
    compare_worktree: bool,
    detail: Option<CommitDetail>,
    detail_sha: Option<String>,
    detail_stash: Option<usize>,
    compare_files: Vec<ChangedFile>,
    tags: Vec<git::TagDetail>,
    file_tree: bool,
    compact_folders: bool,
    show_remote_heads: bool,
    use_full_refs: bool,
    fetch_prune: bool,
    fetch_prune_tags: bool,
    color_preset: usize,
    branch_globs: Vec<String>,
    custom_lane_colors: Vec<String>,
    hidden_actions: Vec<String>,
    custom_emoji: Vec<(String, String)>,
    graph_style: layout::GraphStyle,
    custom_pr_provider: String,
    custom_issue_provider: String,
    date_short: bool,
    relative_dates: bool,
    resize_drag: Option<ResizeDrag>,
    scroll_to_head_on_load: bool,
    scroll_handle: UniformListScrollHandle,
    date_width: f32,
    author_width: f32,
    commit_width: f32,
    diff: Option<DiffView>,
    review: ReviewStore,
    hovered: Option<RowKind>,
    containment_cache: std::collections::HashMap<String, Containment>,
    repo_order: RepoOrder,
    ref_align: RefAlign,
    commands: CommandPalette,
    matches: Vec<usize>,
    match_cursor: usize,
    signature_details: Option<String>,
    branches_containing: Vec<String>,
    tags_containing: Vec<String>,
    filter: LogFilter,
    show_stashes: bool,
    show_uncommitted: bool,
    include_untracked: bool,
    columns: Columns,
    loaded: usize,
    palette: Palette,
    prompt: Option<Prompt>,
    menu: Option<Menu>,
    branch_filter: BranchFilter,
    branch_tracking: std::collections::HashMap<String, (usize, usize)>,
    settings_open: bool,
    search_active: bool,
    search_query: String,
    emoji_enabled: bool,
    markdown_enabled: bool,
    combine_refs: bool,
    use_mailmap: bool,
    include_reflogs: bool,
    date_mode: DateMode,
    remotes: Vec<String>,
    remote_info: Option<RemoteInfo>,
    signature: Option<char>,
    theme: Theme,
    focus_handle: FocusHandle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DateMode {
    Author,
    Commit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoOrder {
    Name,
    Path,
    Given,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RefAlign {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ResizeColumn {
    Date,
    Author,
    Commit,
}

struct ResizeDrag {
    column: ResizeColumn,
    start_x: f32,
    start_width: f32,
}

#[derive(Default)]
struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
}

/// (label, chip id) for the command palette.
const COMMANDS: &[(&str, &str)] = &[
    ("Refresh (fetch tags)", "refresh"),
    ("Push branch", "push"),
    ("Create pull request", "pr"),
    ("Pull current branch", "pull"),
    ("Toggle stashes", "toggle-stashes"),
    ("Toggle uncommitted changes", "toggle-uncommitted"),
    ("Branch filter", "branch-filter"),
    ("Settings", "settings"),
    ("Find", "find"),
    ("Toggle theme", "theme"),
    ("Load more commits", "load-more"),
    ("Export config", "export-config"),
    ("End all code reviews", "end-reviews"),
    ("Resume last code review", "resume-review"),
    ("Stop reviewing this commit", "end-current-review"),
    ("Add branch glob…", "add-glob"),
    ("Fetch into local branch…", "fetch-into"),
    ("Add repository…", "add-repo"),
    ("Remove current repository", "remove-repo"),
    ("Open repository…", "open-repo"),
    ("Open workspace…", "open-workspace"),
    ("Save workspace…", "save-workspace"),
    ("Open recent…", "open-recent"),
    ("Select theme…", "select-theme"),
];

fn filter_commands(query: &str) -> Vec<(&'static str, &'static str)> {
    let query = query.to_lowercase();
    COMMANDS
        .iter()
        .copied()
        .filter(|(label, _)| query.is_empty() || label.to_lowercase().contains(&query))
        .collect()
}

fn find_parent_index(commits: &[Commit], index: usize) -> Option<usize> {
    let parent = commits.get(index)?.parents.first()?.clone();
    commits.iter().position(|commit| commit.sha == parent)
}

fn find_alt_parent_index(commits: &[Commit], index: usize) -> Option<usize> {
    let parent = commits.get(index)?.parents.get(1)?.clone();
    commits.iter().position(|commit| commit.sha == parent)
}

fn find_alt_child_index(commits: &[Commit], index: usize) -> Option<usize> {
    let sha = commits.get(index)?.sha.clone();
    (0..index).rev().find(|&candidate| {
        commits[candidate]
            .parents
            .iter()
            .skip(1)
            .any(|parent| parent == &sha)
    })
}

fn gravatar_url(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    if email.is_empty() {
        return None;
    }
    let hash = format!("{:x}", md5::compute(email.as_bytes()));
    Some(format!(
        "https://www.gravatar.com/avatar/{hash}?s=64&d=identicon"
    ))
}

fn find_child_index(commits: &[Commit], index: usize) -> Option<usize> {
    let sha = commits.get(index)?.sha.clone();
    (0..index)
        .rev()
        .find(|&candidate| {
            commits[candidate]
                .parents
                .iter()
                .any(|parent| parent == &sha)
        })
}

fn near_bottom(offset_y: f32, max_offset_y: f32, threshold: f32) -> bool {
    max_offset_y > 0.0 && -offset_y >= max_offset_y - threshold
}

fn find_head_commit_index(commits: &[Commit], branch: Option<&str>) -> Option<usize> {
    let branch = branch?;
    commits
        .iter()
        .position(|commit| commit.refs.iter().any(|ref_name| ref_name == branch))
}

fn find_matches(commits: &[Commit], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let query = query.to_lowercase();
    commits
        .iter()
        .enumerate()
        .filter(|(_, commit)| {
            commit.subject.to_lowercase().contains(&query)
                || commit.body.to_lowercase().contains(&query)
                || commit.author.to_lowercase().contains(&query)
                || commit.sha.starts_with(&query)
                || commit.author_date.to_lowercase().contains(&query)
                || commit.commit_date.to_lowercase().contains(&query)
                || commit
                    .refs
                    .iter()
                    .any(|ref_name| ref_name.to_lowercase().contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}

fn adjust_width(width: f32, delta: f32) -> f32 {
    (width + delta).clamp(48., 480.)
}

/// Whether navigating to `last_index` in a list of `len` rows is at (or past)
/// the end, and therefore should trigger loading more commits.
fn should_load_more(last_index: usize, len: usize) -> bool {
    len > 0 && last_index + 1 >= len
}

fn format_refs(refs: &[String], combine: bool, align: RefAlign) -> Vec<String> {
    let mut names = if combine {
        combine_refs(refs)
    } else {
        refs.to_vec()
    };
    if align == RefAlign::Right {
        names.reverse();
    }
    names
}

impl RepoOrder {
    fn as_str(self) -> &'static str {
        match self {
            RepoOrder::Name => "name",
            RepoOrder::Path => "path",
            RepoOrder::Given => "given",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "path" => RepoOrder::Path,
            "given" => RepoOrder::Given,
            _ => RepoOrder::Name,
        }
    }
}

#[derive(Default)]
struct Palette {
    open: bool,
    query: String,
    selected: usize,
}

#[derive(Default)]
struct RecentMenu {
    open: bool,
    selected: usize,
}

struct Prompt {
    title: String,
    input: String,
    action: PromptAction,
    sha: String,
}

#[derive(Clone, Copy)]
enum PromptAction {
    CreateBranch,
    CreateTag,
    AddRemote,
    StashBranch,
    RenameBranch,
    AddGlob,
    EditRemote,
    FetchInto,
    CreateAnnotatedTag,
    AddRepository,
}

struct DiffView {
    title: String,
    text: Arc<String>,
}

/// Branches, tags and stashes that include a commit, cached for the hover
/// footer.
#[derive(Clone, Default)]
struct Containment {
    branches: Vec<String>,
    tags: Vec<String>,
    stashes: Vec<String>,
}

impl Containment {
    fn is_empty(&self) -> bool {
        self.branches.is_empty() && self.tags.is_empty() && self.stashes.is_empty()
    }
}

#[derive(Clone, Copy)]
enum MenuAction {
    CherryPick,
    CherryPickEmpty,
    Revert,
    Merge,
    MergeNoFf,
    MergeSquash,
    Rebase,
    ResetSoft,
    ResetMixed,
    ResetHard,
    Checkout,
    Drop,
    CreateBranch,
    CreateTag,
    CreateAnnotatedTag,
    Push,
    CopySha,
    CopyMessage,
    CopyRef,
    StashApply,
    StashPop,
    StashDrop,
    StashBranch,
    StashChanges,
    DiscardChanges,
}

struct Menu {
    x: Pixels,
    y: Pixels,
    context: MenuContext,
    items: Vec<(&'static str, MenuAction)>,
}

#[derive(Clone)]
enum MenuContext {
    Commit(String),
    Stash(usize),
    Uncommitted,
}

impl MenuAction {
    fn label(self) -> &'static str {
        match self {
            MenuAction::CherryPick => "Cherry Pick",
            MenuAction::CherryPickEmpty => "Cherry Pick (allow empty)",
            MenuAction::Revert => "Revert",
            MenuAction::Merge => "Merge into Current Branch",
            MenuAction::MergeNoFf => "Merge (no fast-forward)",
            MenuAction::MergeSquash => "Merge (squash)",
            MenuAction::Rebase => "Rebase Current Branch onto This",
            MenuAction::ResetSoft => "Reset to Here (soft)",
            MenuAction::ResetMixed => "Reset to Here (mixed)",
            MenuAction::ResetHard => "Reset to Here (hard)",
            MenuAction::Checkout => "Checkout Commit",
            MenuAction::Drop => "Drop Commit",
            MenuAction::CreateBranch => "Create Branch Here…",
            MenuAction::CreateTag => "Create Tag Here…",
            MenuAction::CreateAnnotatedTag => "Create Annotated Tag Here…",
            MenuAction::Push => "Push Branch",
            MenuAction::CopySha => "Copy SHA",
            MenuAction::CopyMessage => "Copy Commit Message",
            MenuAction::CopyRef => "Copy Stash Reference",
            MenuAction::StashApply => "Apply Stash",
            MenuAction::StashPop => "Pop Stash",
            MenuAction::StashDrop => "Drop Stash",
            MenuAction::StashBranch => "Create Branch From Stash…",
            MenuAction::StashChanges => "Stash Changes",
            MenuAction::DiscardChanges => "Discard Changes (reset --hard)",
        }
    }

    /// Stable key used to hide actions via `.gitviz.conf`.
    fn key(self) -> &'static str {
        match self {
            MenuAction::CherryPick => "cherry-pick",
            MenuAction::CherryPickEmpty => "cherry-pick-empty",
            MenuAction::Revert => "revert",
            MenuAction::Merge => "merge",
            MenuAction::MergeNoFf => "merge-no-ff",
            MenuAction::MergeSquash => "merge-squash",
            MenuAction::Rebase => "rebase",
            MenuAction::ResetSoft => "reset-soft",
            MenuAction::ResetMixed => "reset-mixed",
            MenuAction::ResetHard => "reset-hard",
            MenuAction::Checkout => "checkout",
            MenuAction::Drop => "drop",
            MenuAction::CreateBranch => "create-branch",
            MenuAction::CreateTag => "create-tag",
            MenuAction::CreateAnnotatedTag => "create-annotated-tag",
            MenuAction::Push => "push",
            MenuAction::CopySha => "copy-sha",
            MenuAction::CopyMessage => "copy-message",
            MenuAction::CopyRef => "copy-ref",
            MenuAction::StashApply => "stash-apply",
            MenuAction::StashPop => "stash-pop",
            MenuAction::StashDrop => "stash-drop",
            MenuAction::StashBranch => "stash-branch",
            MenuAction::StashChanges => "stash-changes",
            MenuAction::DiscardChanges => "discard-changes",
        }
    }
}

fn visible_actions(items: &[MenuAction], hidden: &[String]) -> Vec<MenuAction> {
    items
        .iter()
        .copied()
        .filter(|action| !hidden.iter().any(|key| key == action.key()))
        .collect()
}

#[derive(Default)]
struct BranchFilter {
    open: bool,
    query: String,
    selected: HashSet<String>,
    all: Vec<String>,
}

impl GraphView {
    pub fn new(
        repos: Vec<Repo>,
        roots: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let mut this = Self {
            repos,
            roots,
            workspace_path: None,
            recent: crate::workspace::load_recent(),
            recent_menu: RecentMenu::default(),
            theme_menu: RecentMenu::default(),
            repo_depth: crate::discovery::DEFAULT_DEPTH,
            active: 0,
            load_gen: 0,
            loading: false,
            branch: None,
            commits: Vec::new(),
            rows: Vec::new(),
            rows_dirty: true,
            error: None,
            head_ancestors: HashSet::new(),
            status: Vec::new(),
            stashes: Vec::new(),
            selected: None,
            compare: None,
            compare_worktree: false,
            detail: None,
            detail_sha: None,
            detail_stash: None,
            compare_files: Vec::new(),
            tags: Vec::new(),
            file_tree: false,
            compact_folders: true,
            show_remote_heads: false,
            use_full_refs: false,
            fetch_prune: false,
            fetch_prune_tags: false,
            color_preset: 0,
            branch_globs: Vec::new(),
            custom_lane_colors: Vec::new(),
            hidden_actions: Vec::new(),
            custom_emoji: Vec::new(),
            graph_style: layout::GraphStyle::default(),
            custom_pr_provider: String::new(),
            custom_issue_provider: String::new(),
            date_short: false,
            relative_dates: false,
            resize_drag: None,
            scroll_to_head_on_load: false,
            scroll_handle: UniformListScrollHandle::new(),
            date_width: 150.,
            author_width: 130.,
            commit_width: 80.,
            diff: None,
            review: ReviewStore::load(),
            hovered: None,
            containment_cache: std::collections::HashMap::new(),
            repo_order: RepoOrder::Name,
            ref_align: RefAlign::Left,
            commands: CommandPalette::default(),
            matches: Vec::new(),
            match_cursor: 0,
            signature_details: None,
            branches_containing: Vec::new(),
            tags_containing: Vec::new(),
            filter: LogFilter {
                branches: true,
                remotes: true,
                tags: true,
                first_parent: false,
                use_mailmap: false,
                include_reflogs: false,
                remote_heads: false,
                only_tags: false,
                full_refs: false,
            },
            show_stashes: true,
            show_uncommitted: true,
            include_untracked: true,
            columns: Columns::default(),
            loaded: INITIAL_LOAD,
            palette: Palette::default(),
            prompt: None,
            menu: None,
            branch_filter: BranchFilter::default(),
            branch_tracking: std::collections::HashMap::new(),
            settings_open: false,
            search_active: false,
            search_query: String::new(),
            emoji_enabled: true,
            markdown_enabled: true,
            combine_refs: true,
            use_mailmap: false,
            include_reflogs: false,
            date_mode: DateMode::Author,
            remotes: Vec::new(),
            remote_info: None,
            signature: None,
            theme: crate::workspace::load_theme()
                .and_then(|name| Theme::by_name(&name))
                .unwrap_or_else(Theme::default_theme),
            focus_handle,
        };
        this.apply_repo_config();
        this.sort_repos();
        this.load(cx);
        this
    }

    // -- data -------------------------------------------------------------

    fn load(&mut self, cx: &mut Context<Self>) {
        self.commits.clear();
        self.error = None;
        self.branch = None;
        self.detail = None;
        self.detail_sha = None;
        self.detail_stash = None;
        self.compare = None;
        self.compare_worktree = false;
        self.compare_files.clear();
        self.selected = None;
        self.head_ancestors.clear();
        self.status.clear();
        self.stashes.clear();
        self.branch_tracking.clear();
        self.signature = None;
        self.signature_details = None;
        self.branches_containing.clear();
        self.tags_containing.clear();
        self.containment_cache.clear();
        self.matches.clear();
        self.match_cursor = 0;
        self.rows_dirty = true;

        let Some(repo) = self.active_repo().cloned() else {
            self.loading = false;
            cx.notify();
            return;
        };

        // Keep the log filter in sync with the current settings before the
        // background task reads it.
        self.filter.use_mailmap = self.use_mailmap;
        self.filter.include_reflogs = self.include_reflogs;
        self.filter.remote_heads = self.show_remote_heads;
        self.filter.full_refs = self.use_full_refs;

        let filter = self.filter.clone();
        let include_untracked = self.include_untracked;
        let show_uncommitted = self.show_uncommitted;
        let show_stashes = self.show_stashes;
        let loaded = self.loaded;
        self.load_gen += 1;
        let generation = self.load_gen;
        self.loading = true;
        cx.notify();

        let task = cx.background_spawn(async move {
            let commits = match git::log(&repo.path, loaded, &filter) {
                Ok(mut commits) => {
                    layout::assign_lanes(&mut commits);
                    Ok(commits)
                }
                Err(error) => Err(error.to_string()),
            };
            LoadResult {
                branch: git::head_branch(&repo.path),
                head_ancestors: git::head_ancestors(&repo.path, 50_000),
                local_branches: git::local_branches(&repo.path),
                remotes: git::remotes(&repo.path),
                remote_info: git::hosting_remote(&repo.path),
                tags: git::tags_with_details(&repo.path),
                status: if show_uncommitted {
                    git::status(&repo.path, include_untracked)
                } else {
                    Vec::new()
                },
                stashes: if show_stashes {
                    git::stashes(&repo.path)
                } else {
                    Vec::new()
                },
                commits,
            }
        });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if this.load_gen == generation {
                    this.apply_load(result, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn apply_load(&mut self, result: LoadResult, cx: &mut Context<Self>) {
        self.loading = false;
        self.branch = result.branch;
        self.head_ancestors = result.head_ancestors;
        self.branch_filter.all = result.local_branches;
        self.remotes = result.remotes;
        self.remote_info = result.remote_info;
        self.tags = result.tags;
        self.status = result.status;
        self.stashes = result.stashes;
        match result.commits {
            Ok(commits) => {
                self.commits = commits;
                if self.scroll_to_head_on_load
                    && let Some(index) =
                        find_head_commit_index(&self.commits, self.branch.as_deref())
                {
                    self.selected = Some(RowKind::Commit(index));
                }
            }
            Err(error) => self.error = Some(error),
        }
        self.rows_dirty = true;
        cx.notify();
    }

    /// Computes per-branch ahead/behind counts on demand (they are only shown
    /// in the branch filter panel, and cost one `git rev-list` per branch).
    fn ensure_branch_tracking(&mut self) {
        if !self.branch_tracking.is_empty() {
            return;
        }
        let Some(repo) = self.active_repo().cloned() else {
            return;
        };
        self.branch_tracking = self
            .branch_filter
            .all
            .iter()
            .filter_map(|branch| {
                git::ahead_behind(&repo.path, branch).map(|counts| (branch.clone(), counts))
            })
            .collect();
    }

    fn active_repo(&self) -> Option<&Repo> {
        self.repos.get(self.active)
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        if self.show_uncommitted && !self.status.is_empty() {
            rows.push(RowKind::Uncommitted);
        }
        if self.show_stashes {
            for index in 0..self.stashes.len() {
                rows.push(RowKind::Stash(index));
            }
        }
        let visible = self.filtered_indices();
        for &index in &visible {
            rows.push(RowKind::Commit(index));
        }
        self.rows = rows;
        self.rows_dirty = false;
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let selected: Option<HashSet<&str>> = if self.branch_filter.selected.is_empty() {
            None
        } else {
            Some(
                self.branch_filter
                    .selected
                    .iter()
                    .map(String::as_str)
                    .collect(),
            )
        };
        let query = self.search_query.to_lowercase();
        self.commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| {
                if let Some(selected) = &selected
                    && !commit
                        .refs
                        .iter()
                        .any(|ref_name| selected.contains(ref_name.as_str()))
                {
                    return false;
                }
                query.is_empty()
                    || commit.subject.to_lowercase().contains(&query)
                    || commit.author.to_lowercase().contains(&query)
                    || commit.sha.starts_with(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn load_detail(&mut self, commit_index: usize, cx: &mut Context<Self>) {
        self.detail = None;
        self.detail_sha = None;
        self.detail_stash = None;
        self.compare_files.clear();
        let Some(commit) = self.commits.get(commit_index) else {
            return;
        };
        let sha = commit.sha.clone();
        self.detail_sha = Some(sha.clone());
        if let Some(repo) = self.active_repo().cloned() {
            self.detail = git::commit_detail(&repo.path, &sha).ok();
            if let Some(compare) = self.compare.and_then(|index| self.commits.get(index)) {
                let compare_sha = compare.sha.clone();
                self.compare_files = if self.compare_worktree {
                    git::working_tree_files(&repo.path, &sha)
                } else {
                    git::compare_files(&repo.path, &compare_sha, &sha)
                };
            }
            self.signature = git::signature_status(&repo.path, &sha);
            self.signature_details = git::signature_details(&repo.path, &sha);
            self.branches_containing = git::branches_containing(&repo.path, &sha);
            self.tags_containing = git::tags_containing(&repo.path, &sha);
            // Cache containment so hovering this commit does not shell out.
            self.containment_cache.insert(
                sha.clone(),
                Containment {
                    branches: self.branches_containing.clone(),
                    tags: self.tags_containing.clone(),
                    stashes: git::stashes_containing(&repo.path, &sha),
                },
            );
        }
        cx.notify();
    }

    fn select_row(&mut self, row: RowKind, cx: &mut Context<Self>) {
        match row {
            RowKind::Commit(index) => {
                self.selected = Some(row);
                self.load_detail(index, cx);
            }
            RowKind::Uncommitted => {
                self.selected = Some(row);
                self.compare = None;
                self.compare_worktree = false;
                self.detail = None;
                let files: Vec<ChangedFile> = self
                    .status
                    .iter()
                    .map(|entry| ChangedFile {
                        status: status_entry_letter(entry),
                        added: 0,
                        removed: 0,
                        path: entry.path.clone(),
                    })
                    .collect();
                self.detail = Some(CommitDetail {
                    message: "Uncommitted Changes".to_string(),
                    author: String::new(),
                    email: String::new(),
                    timestamp: 0,
                    files,
                });
                cx.notify();
            }
            RowKind::Stash(index) => {
                self.selected = Some(row);
                self.compare = None;
                self.compare_worktree = false;
                self.detail_sha = None;
                self.detail_stash = Some(index);
                let message = self
                    .stashes
                    .get(index)
                    .map(|stash| stash.message.clone())
                    .unwrap_or_default();
                let files = self
                    .active_repo()
                    .map(|repo| git::stash_files(&repo.path, index))
                    .unwrap_or_default();
                self.detail = Some(CommitDetail {
                    message: format!("stash@{{{index}}}: {message}"),
                    files,
                    ..CommitDetail::default()
                });
                cx.notify();
            }
        }
    }

    fn selected_commit_index(&self) -> Option<usize> {
        match self.selected {
            Some(RowKind::Commit(index)) => Some(index),
            _ => None,
        }
    }

    fn toggle_compare(&mut self, commit_index: usize, cx: &mut Context<Self>) {
        if self.compare.is_some() {
            self.compare = None;
            self.compare_worktree = false;
        } else if matches!(self.selected, Some(RowKind::Uncommitted)) {
            // Comparing the working tree against the clicked commit.
            self.compare = Some(commit_index);
            self.compare_worktree = true;
        } else {
            self.compare = self.selected_commit_index().or(Some(commit_index));
        }
        self.select_row(RowKind::Commit(commit_index), cx);
    }

    fn jump_match(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            self.matches = find_matches(&self.commits, &self.search_query);
        }
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as i32;
        let next = ((self.match_cursor as i32 + delta).rem_euclid(len)) as usize;
        self.match_cursor = next;
        let commit_index = self.matches[next];
        self.select_row(RowKind::Commit(commit_index), cx);
    }

    fn run_op<F>(&mut self, op: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&Repo) -> anyhow::Result<()>,
    {
        let Some(repo) = self.active_repo().cloned() else {
            return;
        };
        match op(&repo) {
            Ok(()) => self.load(cx),
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    // -- keyboard ---------------------------------------------------------

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let cmd = keystroke.modifiers.platform;

        if cmd {
            match keystroke.key.as_str() {
                "p" => {
                    if keystroke.modifiers.shift {
                        // ⇧⌘P: switch repository.
                        self.palette.open = !self.palette.open;
                        self.palette.query.clear();
                        self.palette.selected = 0;
                    } else {
                        // ⌘P: command palette (like most editors).
                        self.commands.open = !self.commands.open;
                        self.commands.query.clear();
                        self.commands.selected = 0;
                    }
                }
                "g" => {
                    self.jump_match(if keystroke.modifiers.shift { -1 } else { 1 }, cx);
                    return;
                }
                "up" => {
                    if let Some(index) = self.selected_commit_index() {
                        let target = if keystroke.modifiers.shift {
                            find_alt_parent_index(&self.commits, index)
                        } else {
                            find_parent_index(&self.commits, index)
                        };
                        if let Some(target) = target {
                            self.select_row(RowKind::Commit(target), cx);
                        }
                    }
                    return;
                }
                "down" => {
                    if let Some(index) = self.selected_commit_index() {
                        let target = if keystroke.modifiers.shift {
                            find_alt_child_index(&self.commits, index)
                        } else {
                            find_child_index(&self.commits, index)
                        };
                        if let Some(target) = target {
                            self.select_row(RowKind::Commit(target), cx);
                        }
                    }
                    return;
                }
                "f" => self.search_active = !self.search_active,
                "r" => {
                    self.load(cx);
                    return;
                }
                "t" => self.theme = self.theme.toggled(),
                "s" => self.step_stash(if keystroke.modifiers.shift { -1 } else { 1 }),
                "h" => self.scroll_to_head(),
                "q" => {
                    cx.quit();
                    return;
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if let Some(prompt) = &mut self.prompt {
            match keystroke.key.as_str() {
                "escape" => self.prompt = None,
                "enter" => self.run_prompt(cx),
                "backspace" => {
                    prompt.input.pop();
                }
                _ => {
                    if let Some(character) = &keystroke.key_char {
                        prompt.input.push_str(character);
                    }
                }
            }
            cx.notify();
            return;
        }

        if self.recent_menu.open {
            let count = self.recent.len();
            match keystroke.key.as_str() {
                "escape" => self.recent_menu.open = false,
                "enter" => {
                    if let Some(path) = self.recent.get(self.recent_menu.selected).cloned() {
                        self.open_recent(path, cx);
                    }
                }
                "up" => {
                    self.recent_menu.selected = self.recent_menu.selected.saturating_sub(1)
                }
                "down" => {
                    if count > 0 && self.recent_menu.selected + 1 < count {
                        self.recent_menu.selected += 1;
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if self.theme_menu.open {
            let themes = Theme::names();
            let count = themes.len();
            match keystroke.key.as_str() {
                "escape" => self.theme_menu.open = false,
                "enter" => {
                    if let Some(name) = themes.get(self.theme_menu.selected).copied() {
                        self.apply_theme(name);
                    }
                }
                "up" => self.theme_menu.selected = self.theme_menu.selected.saturating_sub(1),
                "down" => {
                    if count > 0 && self.theme_menu.selected + 1 < count {
                        self.theme_menu.selected += 1;
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if self.palette.open {
            match keystroke.key.as_str() {
                "escape" => self.palette.open = false,
                "enter" => self.confirm_palette(cx),
                "up" => self.palette.selected = self.palette.selected.saturating_sub(1),
                "down" => {
                    let count = self.filtered_repos().len();
                    if count > 0 && self.palette.selected + 1 < count {
                        self.palette.selected += 1;
                    }
                }
                "backspace" => {
                    self.palette.query.pop();
                    self.palette.selected = 0;
                }
                _ => {
                    if let Some(character) = &keystroke.key_char {
                        self.palette.query.push_str(character);
                        self.palette.selected = 0;
                    }
                }
            }
            cx.notify();
            return;
        }

        if self.commands.open {
            match keystroke.key.as_str() {
                "escape" => self.commands.open = false,
                "enter" => {
                    let commands = filter_commands(&self.commands.query);
                    let index = self.commands.selected.min(commands.len().saturating_sub(1));
                    if let Some((_, id)) = commands.get(index).copied() {
                        self.commands.open = false;
                        self.on_chip(id, cx);
                    }
                    return;
                }
                "up" => self.commands.selected = self.commands.selected.saturating_sub(1),
                "down" => {
                    let count = filter_commands(&self.commands.query).len();
                    if count > 0 && self.commands.selected + 1 < count {
                        self.commands.selected += 1;
                    }
                }
                "backspace" => {
                    self.commands.query.pop();
                    self.commands.selected = 0;
                }
                _ => {
                    if let Some(character) = &keystroke.key_char {
                        self.commands.query.push_str(character);
                        self.commands.selected = 0;
                    }
                }
            }
            cx.notify();
            return;
        }

        if self.branch_filter.open {
            match keystroke.key.as_str() {
                "escape" | "enter" => self.branch_filter.open = false,
                "backspace" => {
                    self.branch_filter.query.pop();
                }
                _ => {
                    if let Some(character) = &keystroke.key_char {
                        self.branch_filter.query.push_str(character);
                    }
                }
            }
            cx.notify();
            return;
        }

        if keystroke.key == "escape" {
            self.menu = None;
            self.settings_open = false;
            self.diff = None;
            cx.notify();
            return;
        }

        if self.search_active {
            match keystroke.key.as_str() {
                "escape" | "enter" => self.search_active = false,
                "backspace" => {
                    self.search_query.pop();
                    self.rows_dirty = true;
                }
                _ => {
                    if let Some(character) = &keystroke.key_char {
                        self.search_query.push_str(character);
                        self.rows_dirty = true;
                    }
                }
            }
            cx.notify();
            return;
        }

        match keystroke.key.as_str() {
            "up" => self.move_selection(-1, cx),
            "down" => self.move_selection(1, cx),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.rows_dirty {
            self.rebuild_rows();
        }
        let current = self
            .selected
            .and_then(|selected| self.rows.iter().position(|row| *row == selected));
        let next = match current {
            Some(index) => (index as i32 + delta).clamp(0, self.rows.len() as i32 - 1) as usize,
            None => 0,
        };
        if let Some(row) = self.rows.get(next).copied() {
            self.select_row(row, cx);
            if should_load_more(next, self.rows.len()) && self.loaded < COMMIT_LIMIT {
                self.loaded = (self.loaded + 500).min(COMMIT_LIMIT);
                self.load(cx);
            }
        }
    }

    fn step_stash(&mut self, delta: i32) {
        if self.stashes.is_empty() {
            return;
        }
        let current = match self.selected {
            Some(RowKind::Stash(index)) => index as i32,
            _ => -1,
        };
        let next = (current + delta).clamp(0, self.stashes.len() as i32 - 1) as usize;
        self.selected = Some(RowKind::Stash(next));
    }

    fn scroll_to_head(&mut self) {
        if let Some(position) = self
            .rows
            .iter()
            .position(|row| match row {
                RowKind::Commit(index) => self
                    .commits
                    .get(*index)
                    .map(|commit| self.branch.as_deref() == Some(commit.short_sha()) || commit.refs.iter().any(|r| Some(r.as_str()) == self.branch.as_deref()))
                    .unwrap_or(false),
                _ => false,
            })
        {
            self.selected = self.rows.get(position).copied();
        }
    }

    // -- overlays ---------------------------------------------------------

    fn on_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((column, start_x, start_width)) = self
            .resize_drag
            .as_ref()
            .map(|drag| (drag.column, drag.start_x, drag.start_width))
        else {
            return;
        };
        let width = (start_width + (event.position.x.as_f32() - start_x)).clamp(48., 480.);
        match column {
            ResizeColumn::Date => self.date_width = width,
            ResizeColumn::Author => self.author_width = width,
            ResizeColumn::Commit => self.commit_width = width,
        }
        cx.notify();
    }

    fn on_mouse_up(
        &mut self,
        _: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resize_drag.take().is_some() {
            cx.notify();
        }
    }

    fn filtered_repos(&self) -> Vec<usize> {
        let query = self.palette.query.to_lowercase();
        self.repos
            .iter()
            .enumerate()
            .filter(|(_, repo)| {
                query.is_empty()
                    || repo.name.to_lowercase().contains(&query)
                    || repo.path.to_string_lossy().to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn confirm_palette(&mut self, cx: &mut Context<Self>) {
        let filtered = self.filtered_repos();
        if let Some(&index) = filtered.get(self.palette.selected) {
            self.switch_to(index, cx);
        }
    }

    fn switch_to(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active = index;
        self.palette.open = false;
        self.palette.query.clear();
        self.palette.selected = 0;
        self.apply_repo_config();
        self.sort_repos();
        self.load(cx);
    }

    fn apply_repo_config(&mut self) {
        let Some(repo) = self.repos.get(self.active) else {
            return;
        };
        let Some(config) = RepoConfig::load(&repo.path) else {
            return;
        };
        self.filter.branches = config.branches;
        self.filter.remotes = config.remotes;
        self.filter.tags = config.tags;
        self.filter.first_parent = config.first_parent;
        self.show_stashes = config.show_stashes;
        self.show_uncommitted = config.show_uncommitted;
        self.include_untracked = config.include_untracked;
        self.combine_refs = config.combine_refs;
        self.emoji_enabled = config.emoji;
        self.markdown_enabled = config.markdown;
        self.date_mode = if config.date_commit {
            DateMode::Commit
        } else {
            DateMode::Author
        };
        self.columns = Columns {
            date: config.columns_date,
            author: config.columns_author,
            commit: config.columns_commit,
        };
        self.repo_order = RepoOrder::from_str(&config.repo_order);
        self.branch_globs = config.branch_globs;
        self.custom_lane_colors = config.lane_colors;
        self.hidden_actions = config.hidden_actions;
        self.custom_emoji = parse_emoji_mappings(&config.emoji_mappings);
        self.graph_style = layout::GraphStyle::parse(&config.graph_style);
        self.custom_pr_provider = config.pr_provider.clone();
        self.custom_issue_provider = config.issue_provider.clone();
    }

    fn sort_repos(&mut self) {
        let active_path = self.repos.get(self.active).map(|repo| repo.path.clone());
        match self.repo_order {
            RepoOrder::Name => self.repos.sort_by_key(|repo| repo.name.to_lowercase()),
            RepoOrder::Path => self
                .repos
                .sort_by_key(|repo| repo.path.to_string_lossy().to_lowercase()),
            RepoOrder::Given => {}
        }
        if let Some(path) = active_path
            && let Some(index) = self.repos.iter().position(|repo| repo.path == path)
        {
            self.active = index;
        }
    }

    fn rediscover(&mut self, cx: &mut Context<Self>) {
        let active_path = self.repos.get(self.active).map(|repo| repo.path.clone());
        self.repos = crate::discovery::discover_with_depth(&self.roots, self.repo_depth);
        self.active = active_path
            .and_then(|path| self.repos.iter().position(|repo| repo.path == path))
            .unwrap_or(0);
        self.sort_repos();
        crate::workspace::save_default_roots(&self.roots);
        self.load(cx);
    }

    /// Removes the active repository from the view (and from the roots if it
    /// was added as one).
    fn remove_active_repo(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active_repo().cloned() else {
            return;
        };
        self.repos.retain(|candidate| candidate.path != repo.path);
        self.roots.retain(|root| root != &repo.path);
        if self.active >= self.repos.len() {
            self.active = self.repos.len().saturating_sub(1);
        }
        self.sort_repos();
        crate::workspace::save_default_roots(&self.roots);
        self.load(cx);
    }

    /// Adds a repository root (idempotently). Returns whether the roots changed.
    fn add_root(&mut self, path: std::path::PathBuf) -> bool {
        if path.exists() && !self.roots.contains(&path) {
            crate::workspace::remember_recent(&path);
            self.recent = crate::workspace::load_recent();
            self.roots.push(path);
            true
        } else {
            false
        }
    }

    /// Loads a `.gitviz-workspace` file, appending its listed roots.
    fn open_workspace(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            self.error = Some(format!("failed to read {}", path.display()));
            cx.notify();
            return;
        };
        for line in crate::workspace::parse(&text) {
            self.add_root(crate::workspace::expand_tilde(&line));
        }
        self.workspace_path = Some(path.to_path_buf());
        crate::workspace::remember_recent(path);
        self.recent = crate::workspace::load_recent();
        self.rediscover(cx);
    }

    /// Writes the current roots to a `.gitviz-workspace` file.
    fn save_workspace(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let text = crate::workspace::serialize(&self.roots);
        match std::fs::write(path, text) {
            Ok(()) => {
                self.workspace_path = Some(path.to_path_buf());
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Opens a path from the "Open Recent" list: a workspace file is loaded,
    /// anything else is added as a repository root.
    fn open_recent(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.recent_menu.open = false;
        if path.to_string_lossy().ends_with(crate::workspace::FILE_SUFFIX) {
            self.open_workspace(&path, cx);
        } else if self.add_root(path) {
            self.rediscover(cx);
        }
    }

    /// Switches to the named theme and remembers the choice.
    fn apply_theme(&mut self, name: &str) {
        if let Some(theme) = Theme::by_name(name) {
            self.theme = theme;
            crate::workspace::save_theme(name);
        }
        self.theme_menu.open = false;
    }

    /// Handles paths dragged onto the window: repository folders are added as
    /// roots, and `.gitviz-workspace` files are loaded.
    fn on_drop_paths(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for path in paths.paths() {
            if path.to_string_lossy().ends_with(crate::workspace::FILE_SUFFIX) {
                self.open_workspace(path, cx);
            } else {
                changed |= self.add_root(path.clone());
            }
        }
        if changed {
            self.rediscover(cx);
        }
    }

    fn export_repo_config(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.active_repo().cloned() else {
            return;
        };
        let config = RepoConfig {
            branches: self.filter.branches,
            remotes: self.filter.remotes,
            tags: self.filter.tags,
            first_parent: self.filter.first_parent,
            show_stashes: self.show_stashes,
            show_uncommitted: self.show_uncommitted,
            include_untracked: self.include_untracked,
            combine_refs: self.combine_refs,
            emoji: self.emoji_enabled,
            markdown: self.markdown_enabled,
            date_commit: self.date_mode == DateMode::Commit,
            columns_date: self.columns.date,
            columns_author: self.columns.author,
            columns_commit: self.columns.commit,
            repo_order: self.repo_order.as_str().to_string(),
            branch_globs: self.branch_globs.clone(),
            lane_colors: self.custom_lane_colors.clone(),
            hidden_actions: self.hidden_actions.clone(),
            emoji_mappings: self
                .custom_emoji
                .iter()
                .map(|(code, emoji)| format!("{code}:{emoji}"))
                .collect(),
            graph_style: self.graph_style.as_str().to_string(),
            pr_provider: self.custom_pr_provider.clone(),
            issue_provider: self.custom_issue_provider.clone(),
        };
        match config.save(&repo.path) {
            Ok(()) => {
                self.error = None;
                cx.notify();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn lane_palette(&self) -> [gpui::Rgba; 8] {
        if !self.custom_lane_colors.is_empty() {
            let parsed: Vec<gpui::Rgba> = self
                .custom_lane_colors
                .iter()
                .filter_map(|color| parse_hex_color(color))
                .collect();
            if !parsed.is_empty() {
                let mut colors = [self.theme.lane_colors[0]; 8];
                for (index, slot) in colors.iter_mut().enumerate() {
                    *slot = parsed[index % parsed.len()];
                }
                return colors;
            }
        }
        match self.color_preset % 3 {
            1 => [
                gpui::rgb(0x1f77b4),
                gpui::rgb(0xff7f0e),
                gpui::rgb(0x2ca02c),
                gpui::rgb(0xd62728),
                gpui::rgb(0x9467bd),
                gpui::rgb(0x8c564b),
                gpui::rgb(0xe377c2),
                gpui::rgb(0x7f7f7f),
            ],
            2 => [gpui::rgb(0x808080); 8],
            _ => self.theme.lane_colors,
        }
    }

    fn run_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let name = prompt.input.trim().to_string();
        if name.is_empty() {
            return;
        }
        let sha = prompt.sha.clone();
        match prompt.action {
            PromptAction::CreateBranch => {
                self.run_op(move |repo| git::create_branch(&repo.path, &name, Some(&sha)), cx)
            }
            PromptAction::CreateTag => {
                self.run_op(move |repo| git::create_tag(&repo.path, &name, &sha), cx)
            }
            PromptAction::AddRemote => {
                let mut parts = name.split_whitespace();
                if let (Some(remote_name), Some(url)) = (parts.next(), parts.next()) {
                    let remote_name = remote_name.to_string();
                    let url = url.to_string();
                    self.run_op(move |repo| git::add_remote(&repo.path, &remote_name, &url), cx);
                }
            }
            PromptAction::StashBranch => {
                if let Ok(index) = sha.parse::<usize>() {
                    self.run_op(move |repo| git::stash_branch(&repo.path, index, &name), cx);
                }
            }
            PromptAction::RenameBranch => {
                let old = sha.clone();
                self.run_op(move |repo| git::rename_branch(&repo.path, &old, &name), cx);
            }
            PromptAction::AddGlob => {
                if !self.branch_globs.iter().any(|glob| glob == &name) {
                    self.branch_globs.push(name);
                }
                self.export_repo_config(cx);
                self.load(cx);
            }
            PromptAction::EditRemote => {
                let remote = sha.clone();
                self.run_op(move |repo| git::set_remote_url(&repo.path, &remote, &name), cx);
            }
            PromptAction::FetchInto => {
                let mut parts = name.split_whitespace();
                if let (Some(remote), Some(remote_branch), Some(local_branch)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    let remote = remote.to_string();
                    let remote_branch = remote_branch.to_string();
                    let local_branch = local_branch.to_string();
                    self.run_op(
                        move |repo| {
                            git::fetch_into_branch(
                                &repo.path,
                                &remote,
                                &remote_branch,
                                &local_branch,
                            )
                        },
                        cx,
                    );
                }
            }
            PromptAction::CreateAnnotatedTag => {
                let (tag, message) = match name.split_once(' ') {
                    Some((tag, message)) => (tag.to_string(), message.trim().to_string()),
                    None => (name.clone(), String::new()),
                };
                let sha = sha.clone();
                self.run_op(
                    move |repo| git::create_annotated_tag(&repo.path, &tag, &sha, &message),
                    cx,
                );
            }
            PromptAction::AddRepository => {
                let path = crate::workspace::expand_tilde(&name);
                if self.add_root(path) {
                    self.rediscover(cx);
                }
            }
        }
    }

    fn open_menu(&mut self, row: RowKind, x: Pixels, y: Pixels) {
        let (context, items) = match row {
            RowKind::Commit(index) => {
                let Some(commit) = self.commits.get(index) else {
                    return;
                };
                (
                    MenuContext::Commit(commit.sha.clone()),
                    vec![
                        MenuAction::CherryPick,
                        MenuAction::CherryPickEmpty,
                        MenuAction::Revert,
                        MenuAction::Merge,
                        MenuAction::MergeNoFf,
                        MenuAction::MergeSquash,
                        MenuAction::Rebase,
                        MenuAction::ResetSoft,
                        MenuAction::ResetMixed,
                        MenuAction::ResetHard,
                        MenuAction::Checkout,
                        MenuAction::Drop,
                        MenuAction::CreateBranch,
                        MenuAction::CreateTag,
                        MenuAction::CreateAnnotatedTag,
                        MenuAction::Push,
                        MenuAction::CopySha,
                        MenuAction::CopyMessage,
                    ],
                )
            }
            RowKind::Stash(index) => (
                MenuContext::Stash(index),
                vec![
                    MenuAction::StashApply,
                    MenuAction::StashPop,
                    MenuAction::StashDrop,
                    MenuAction::StashBranch,
                    MenuAction::CopyRef,
                ],
            ),
            RowKind::Uncommitted => (
                MenuContext::Uncommitted,
                vec![MenuAction::StashChanges, MenuAction::DiscardChanges],
            ),
        };
        let items = visible_actions(&items, &self.hidden_actions);
        self.menu = Some(Menu {
            x,
            y,
            context,
            items: items.into_iter().map(|action| (action.label(), action)).collect(),
        });
    }

    fn run_menu_action(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        match menu.context {
            MenuContext::Commit(sha) => match action {
                MenuAction::CherryPick => {
                    self.run_op(move |repo| git::cherry_pick(&repo.path, &sha), cx)
                }
                MenuAction::CherryPickEmpty => {
                    self.run_op(move |repo| git::cherry_pick_allow_empty(&repo.path, &sha), cx)
                }
                MenuAction::Revert => self.run_op(move |repo| git::revert(&repo.path, &sha), cx),
                MenuAction::Merge => self.run_op(move |repo| git::merge(&repo.path, &sha), cx),
                MenuAction::MergeNoFf => {
                    self.run_op(move |repo| git::merge_no_ff(&repo.path, &sha), cx)
                }
                MenuAction::MergeSquash => {
                    self.run_op(move |repo| git::merge_squash(&repo.path, &sha), cx)
                }
                MenuAction::Rebase => self.run_op(move |repo| git::rebase(&repo.path, &sha), cx),
                MenuAction::ResetSoft => {
                    self.run_op(move |repo| git::reset(&repo.path, &sha, ResetMode::Soft), cx)
                }
                MenuAction::ResetMixed => {
                    self.run_op(move |repo| git::reset(&repo.path, &sha, ResetMode::Mixed), cx)
                }
                MenuAction::ResetHard => {
                    self.run_op(move |repo| git::reset(&repo.path, &sha, ResetMode::Hard), cx)
                }
                MenuAction::Checkout => {
                    self.run_op(move |repo| git::checkout_commit(&repo.path, &sha), cx)
                }
                MenuAction::Drop => self.run_op(move |repo| git::drop_commit(&repo.path, &sha), cx),
                MenuAction::CreateBranch => {
                    self.prompt = Some(Prompt {
                        title: "Create branch".into(),
                        input: String::new(),
                        action: PromptAction::CreateBranch,
                        sha,
                    });
                    cx.notify();
                }
                MenuAction::CreateTag => {
                    self.prompt = Some(Prompt {
                        title: "Create tag".into(),
                        input: String::new(),
                        action: PromptAction::CreateTag,
                        sha,
                    });
                    cx.notify();
                }
                MenuAction::CreateAnnotatedTag => {
                    self.prompt = Some(Prompt {
                        title: "Create annotated tag (name message)".into(),
                        input: String::new(),
                        action: PromptAction::CreateAnnotatedTag,
                        sha,
                    });
                    cx.notify();
                }
                MenuAction::Push => self.run_op(|repo| git::push_current_branch(&repo.path), cx),
                MenuAction::CopySha => {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha));
                }
                MenuAction::CopyMessage => {
                    let message = self
                        .detail
                        .as_ref()
                        .map(|detail| detail.message.clone())
                        .unwrap_or_default();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(message));
                }
                _ => {}
            },
            MenuContext::Stash(index) => match action {
                MenuAction::StashApply => {
                    self.run_op(move |repo| git::stash_apply(&repo.path, index), cx)
                }
                MenuAction::StashPop => {
                    self.run_op(move |repo| git::stash_pop(&repo.path, index), cx)
                }
                MenuAction::StashDrop => {
                    self.run_op(move |repo| git::stash_drop(&repo.path, index), cx)
                }
                MenuAction::StashBranch => {
                    self.prompt = Some(Prompt {
                        title: "Create branch from stash".to_string(),
                        input: String::new(),
                        action: PromptAction::StashBranch,
                        sha: index.to_string(),
                    });
                    cx.notify();
                }
                MenuAction::CopyRef => {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(format!(
                        "stash@{{{index}}}"
                    )));
                }
                _ => {}
            },
            MenuContext::Uncommitted => {
                let include_untracked = self.include_untracked;
                match action {
                    MenuAction::StashChanges => {
                        self.run_op(move |repo| git::stash_push(&repo.path, include_untracked), cx)
                    }
                    MenuAction::DiscardChanges => {
                        self.run_op(move |repo| git::discard_all(&repo.path, include_untracked), cx)
                    }
                    _ => {}
                }
            }
        }
    }

    fn toggle_reviewed(&mut self, commit_sha: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{commit_sha}\t{path}");
        self.review.toggle(&key);
        self.review.save();
        cx.notify();
    }
}

impl Render for GraphView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rows_dirty {
            self.rebuild_rows();
        }
        if !self.search_query.is_empty() {
            self.matches = find_matches(&self.commits, &self.search_query);
            if self.match_cursor >= self.matches.len() {
                self.match_cursor = 0;
            }
        }
        // Automatically load more commits when scrolled near the bottom.
        if self.loaded < COMMIT_LIMIT && self.commits.len() >= self.loaded {
            let at_bottom = {
                let state = self.scroll_handle.0.borrow();
                near_bottom(
                    state.base_handle.offset().y.as_f32(),
                    state.base_handle.max_offset().y.as_f32(),
                    120.,
                )
            };
            if at_bottom {
                self.loaded = (self.loaded + 500).min(COMMIT_LIMIT);
                self.load(cx);
            }
        }
        let theme = self.theme.clone();
        let weak = cx.weak_entity();

        let repo_name = self
            .active_repo()
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| "no repository".to_string());

        let chip = |label: &'static str, on: bool, id: &'static str, weak: gpui::WeakEntity<Self>, theme: Theme| {
            let hover = theme.hover;
            div()
                .id(id)
                .px_2()
                .py_0p5()
                .rounded_full()
                .text_sm()
                .cursor_pointer()
                .when(on, |this| this.bg(theme.accent).text_color(theme.bg))
                .when(!on, |this| {
                    this.text_color(theme.text_muted)
                        .hover(move |this| this.bg(hover))
                })
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    weak.update(cx, |this, cx| this.on_chip(id, cx)).ok();
                })
                .child(label)
        };

        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .bg(theme.panel)
            .border_b_1()
            .border_color(theme.border)
            .child(div().text_color(theme.text).child(repo_name))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(self.branch.clone().unwrap_or_default()),
            )
            .child(div().flex_1())
            .child(chip("Local", self.filter.branches, "filter-local", weak.clone(), theme.clone()))
            .child(chip("Remote", self.filter.remotes, "filter-remote", weak.clone(), theme.clone()))
            .child(chip("Tags", self.filter.tags, "filter-tags", weak.clone(), theme.clone()))
            .child(chip("1st parent", self.filter.first_parent, "filter-first", weak.clone(), theme.clone()))
            .child(chip("Stashes", self.show_stashes, "toggle-stashes", weak.clone(), theme.clone()))
            .child(chip("Changes", self.show_uncommitted, "toggle-uncommitted", weak.clone(), theme.clone()))
            .child(chip("Branches", self.branch_filter.open, "branch-filter", weak.clone(), theme.clone()))
            .child(chip("Settings", self.settings_open, "settings", weak.clone(), theme.clone()))
            .child(chip("Find", self.search_active, "find", weak.clone(), theme.clone()))
            .child(chip("Open", false, "open-repo", weak.clone(), theme.clone()))
            .child(chip("Refresh", false, "refresh", weak.clone(), theme.clone()))
            .child(chip("Push", false, "push", weak.clone(), theme.clone()))
            .child(chip("PR", false, "pr", weak.clone(), theme.clone()))
            .child(chip("Load more", false, "load-more", weak.clone(), theme.clone()))
            .child(chip("Theme", false, "theme", weak.clone(), theme.clone()));

        let search_bar = self.search_active.then(|| {
            h_flex()
                .w_full()
                .px_3()
                .py_1()
                .bg(theme.bg)
                .border_b_1()
                .border_color(theme.border)
                .text_sm()
                .text_color(theme.text)
                .child(if self.search_query.is_empty() {
                    "Search commits…".to_string()
                } else {
                    self.search_query.clone()
                })
        });

        // rows
        let row_ctx = RowRenderContext {
            commits: Arc::new(self.commits.clone()),
            status: Arc::new(self.status.clone()),
            stashes: Arc::new(self.stashes.clone()),
            rows: Arc::new(self.rows.clone()),
            selected: self.selected,
            compare: self.compare,
            head_ancestors: self.head_ancestors.clone(),
            theme: theme.clone(),
            lane_colors: self.lane_palette(),
            emoji_enabled: self.emoji_enabled,
            combine_refs: self.combine_refs,
            columns: self.columns,
            date_mode: self.date_mode,
            date_width: self.date_width,
            author_width: self.author_width,
            commit_width: self.commit_width,
            ref_align: self.ref_align,
            matches: Arc::new(self.matches.clone()),
            match_cursor: self.match_cursor,
            date_short: self.date_short,
            relative_dates: self.relative_dates,
            custom_emoji: Arc::new(self.custom_emoji.clone()),
            graph_style: self.graph_style,
        };

        let body: AnyElement = if let Some(error) = &self.error {
            div().p_4().text_color(theme.error).child(error.clone()).into_any_element()
        } else if self.loading && self.commits.is_empty() {
            div()
                .p_4()
                .text_color(theme.text_muted)
                .child("Loading…")
                .into_any_element()
        } else {
            let ctx = Arc::new(row_ctx);
            let weak = cx.weak_entity();
            uniform_list("commits", ctx.rows.len(), move |range, _window, _cx| {
                range
                    .map(|position| ctx.render_row(position, weak.clone()))
                    .collect::<Vec<_>>()
            })
            .track_scroll(&self.scroll_handle)
            .flex_1()
            .into_any_element()
        };

        let detail = self.selected.is_some().then(|| self.render_detail(weak.clone()));

        let palette = self.palette.open.then(|| self.render_palette(weak.clone()));
        let commands = self.commands.open.then(|| self.render_commands(weak.clone()));
        let prompt = self.prompt.is_some().then(|| self.render_prompt());
        let menu = self.menu.is_some().then(|| self.render_menu(weak.clone()));
        let branch_filter = self.branch_filter.open.then(|| self.render_branch_filter(weak.clone()));
        let settings = self.settings_open.then(|| self.render_settings(weak.clone()));
        let diff = self.diff.as_ref().map(|view| self.render_diff(view));
        let recent = self.recent_menu.open.then(|| self.render_recent(weak.clone()));
        let theme_picker = self.theme_menu.open.then(|| self.render_theme(weak.clone()));

        let content: AnyElement = if self.repos.is_empty() {
            self.render_welcome(weak.clone())
        } else {
            v_flex()
                .size_full()
                .child(header)
                .children(search_bar)
                .child(self.render_column_header(weak.clone()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .child(body)
                        .when_some(detail, |this, detail| this.child(detail)),
                )
                .child(self.render_footer())
                .into_any_element()
        };

        let _ = window;

        v_flex()
            .size_full()
            .bg(theme.bg)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_drop::<ExternalPaths>(cx.listener(Self::on_drop_paths))
            .on_action(cx.listener(|this, _: &OpenRepository, _window, cx| {
                this.on_chip("open-repo", cx)
            }))
            .on_action(cx.listener(|this, _: &OpenWorkspace, _window, cx| {
                this.on_chip("open-workspace", cx)
            }))
            .on_action(cx.listener(|this, _: &SaveWorkspace, _window, cx| {
                this.on_chip("save-workspace", cx)
            }))
            .on_action(cx.listener(|this, _: &OpenRecent, _window, cx| {
                this.recent = crate::workspace::load_recent();
                this.recent_menu = RecentMenu {
                    open: true,
                    selected: 0,
                };
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SwitchRepository, _window, cx| {
                this.palette.open = true;
                this.palette.query.clear();
                this.palette.selected = 0;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _window, cx| {
                this.theme = this.theme.toggled();
                crate::workspace::save_theme(this.theme.name);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectTheme, _window, cx| {
                this.theme_menu = RecentMenu {
                    open: true,
                    selected: 0,
                };
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RefreshGraph, _window, cx| this.load(cx)))
            .on_action(cx.listener(|this, _: &FindCommit, _window, cx| {
                this.search_active = true;
                cx.notify();
            }))
            .on_action(
                cx.listener(|_this, _: &Minimize, window, _cx| window.minimize_window()),
            )
            .on_action(cx.listener(|_this, _: &Zoom, window, _cx| window.zoom_window()))
            .on_action(cx.listener(|_this, _: &OpenHomepage, _window, _cx| {
                let _ = git::open_url("https://github.com/senwong/gitviz");
            }))
            .on_action(cx.listener(|_this, _: &Quit, _window, cx| cx.quit()))
            .child(content)
            .when_some(recent, |this, recent| this.child(recent))
            .when_some(theme_picker, |this, picker| this.child(picker))
            .when_some(diff, |this, diff| this.child(diff))
            .when_some(palette, |this, palette| this.child(palette))
            .when_some(commands, |this, commands| this.child(commands))
            .when_some(branch_filter, |this, filter| this.child(filter))
            .when_some(settings, |this, settings| this.child(settings))
            .when_some(prompt, |this, prompt| this.child(prompt))
            .when_some(menu, |this, menu| this.child(menu))
    }
}

impl GraphView {
    fn on_chip(&mut self, id: &str, cx: &mut Context<Self>) {
        match id {
            "filter-local" => {
                self.filter.branches = !self.filter.branches;
                self.load(cx);
            }
            "filter-remote" => {
                self.filter.remotes = !self.filter.remotes;
                self.load(cx);
            }
            "filter-tags" => {
                self.filter.tags = !self.filter.tags;
                self.load(cx);
            }
            "filter-first" => {
                self.filter.first_parent = !self.filter.first_parent;
                self.load(cx);
            }
            "toggle-stashes" => {
                self.show_stashes = !self.show_stashes;
                self.rows_dirty = true;
                cx.notify();
            }
            "toggle-uncommitted" => {
                self.show_uncommitted = !self.show_uncommitted;
                self.rows_dirty = true;
                cx.notify();
            }
            "branch-filter" => {
                self.branch_filter.open = !self.branch_filter.open;
                if self.branch_filter.open {
                    self.ensure_branch_tracking();
                }
                cx.notify();
            }
            "settings" => {
                self.settings_open = !self.settings_open;
                cx.notify();
            }
            "find" => {
                self.search_active = !self.search_active;
                cx.notify();
            }
            "refresh" => {
                let prune = self.fetch_prune;
                let prune_tags = self.fetch_prune_tags;
                self.run_op(
                    move |repo| git::fetch_with(&repo.path, prune, prune_tags),
                    cx,
                )
            }
            "push" => self.run_op(|repo| git::push_current_branch(&repo.path), cx),
            "resume-review" => {
                if let Some(sha) = self.review.latest_commit()
                    && let Some(index) = self.commits.iter().position(|commit| commit.sha == sha)
                {
                    self.select_row(RowKind::Commit(index), cx);
                }
            }
            "end-current-review" => {
                if let Some(sha) = self.detail_sha.clone() {
                    self.review.remove_commit(&sha);
                    self.review.save();
                    cx.notify();
                }
            }
            "add-glob" => {
                self.prompt = Some(Prompt {
                    title: "Branch glob (e.g. heads/feature/*)".to_string(),
                    input: String::new(),
                    action: PromptAction::AddGlob,
                    sha: String::new(),
                });
                cx.notify();
            }
            "fetch-into" => {
                self.prompt = Some(Prompt {
                    title: "Fetch into local branch (remote remote-branch local-branch)".to_string(),
                    input: String::new(),
                    action: PromptAction::FetchInto,
                    sha: String::new(),
                });
                cx.notify();
            }
            "add-repo" => {
                self.prompt = Some(Prompt {
                    title: "Add repository (path, ~ allowed)".to_string(),
                    input: String::new(),
                    action: PromptAction::AddRepository,
                    sha: String::new(),
                });
                cx.notify();
            }
            "remove-repo" => {
                self.remove_active_repo(cx);
                return;
            }
            "open-repo" => {
                let receiver = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: true,
                    prompt: Some("Open repository or folder".into()),
                });
                cx.spawn(async move |this, cx| {
                    if let Ok(Ok(Some(paths))) = receiver.await {
                        this.update(cx, |this, cx| {
                            let mut changed = false;
                            for path in paths {
                                changed |= this.add_root(path);
                            }
                            if changed {
                                this.rediscover(cx);
                            }
                        })
                        .ok();
                    }
                })
                .detach();
            }
            "open-workspace" => {
                let receiver = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Open gitviz workspace".into()),
                });
                cx.spawn(async move |this, cx| {
                    if let Ok(Ok(Some(paths))) = receiver.await
                        && let Some(path) = paths.into_iter().next()
                    {
                        this.update(cx, |this, cx| this.open_workspace(&path, cx)).ok();
                    }
                })
                .detach();
            }
            "save-workspace" => {
                let directory = std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let receiver = cx
                    .prompt_for_new_path(&directory, Some("repos.gitviz-workspace"));
                cx.spawn(async move |this, cx| {
                    if let Ok(Ok(Some(path))) = receiver.await {
                        this.update(cx, |this, cx| this.save_workspace(&path, cx)).ok();
                    }
                })
                .detach();
            }
            "open-recent" => {
                self.recent = crate::workspace::load_recent();
                self.recent_menu = RecentMenu {
                    open: true,
                    selected: 0,
                };
                cx.notify();
            }
            "pr" => {
                let branch = self.branch.clone().unwrap_or_default();
                let base = self
                    .active_repo()
                    .and_then(|repo| git::default_branch(&repo.path))
                    .unwrap_or_else(|| "main".to_string());
                if let Some(info) = self.remote_info.clone() {
                    let url = if self.custom_pr_provider.trim().is_empty() {
                        info.pr_url(&base, &branch, &format!("Merge {branch} into {base}"))
                    } else {
                        git::render_pr_template(
                            &self.custom_pr_provider,
                            &info.host,
                            &info.owner,
                            &info.repo,
                            &base,
                            &branch,
                        )
                    };
                    let _ = git::open_url(&url);
                }
            }
            "pull" => self.run_op(|repo| git::pull(&repo.path), cx),
            "load-more" => {
                self.loaded = (self.loaded + 500).min(COMMIT_LIMIT);
                self.load(cx);
            }
            "theme" => {
                self.theme = self.theme.toggled();
                crate::workspace::save_theme(self.theme.name);
                cx.notify();
            }
            "select-theme" => {
                self.theme_menu = RecentMenu {
                    open: true,
                    selected: 0,
                };
                cx.notify();
            }
            _ => {}
        }
    }

    fn render_detail(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let Some(detail) = &self.detail else {
            return div().into_any_element();
        };
        let sha = self.detail_sha.clone().unwrap_or_default();
        let comparing = self.compare.is_some();
        let review_active = !comparing && self.review.has_commit(&sha);
        // The uncommitted-changes view is the only one with no commit or stash.
        let can_discard = self.detail_sha.is_none() && self.detail_stash.is_none();

        let files: Vec<AnyElement> = if comparing {
            self.compare_files
                .iter()
                .map(|file| render_file_row(file, &file.path, &weak, &theme, None, false))
                .collect()
        } else if self.file_tree {
            build_tree_rows(&detail.files, self.compact_folders)
                .into_iter()
                .map(|row| {
                    if row.is_dir {
                        indent_wrap(
                            row.depth,
                            div()
                                .text_sm()
                                .text_color(theme.text_muted)
                                .child(format!("{}/", row.name))
                                .into_any_element(),
                        )
                    } else {
                        let file = row.file.unwrap_or_default();
                        let reviewed = self.review.is_reviewed(&format!("{sha}\t{}", file.path));
                        indent_wrap(
                            row.depth,
                            render_file_row(
                                &file,
                                &row.name,
                                &weak,
                                &theme,
                                Some((sha.clone(), reviewed, review_active)),
                                can_discard,
                            ),
                        )
                    }
                })
                .collect()
        } else {
            detail
                .files
                .iter()
                .map(|file| {
                    let reviewed = self.review.is_reviewed(&format!("{sha}\t{}", file.path));
                    render_file_row(
                        file,
                        &file.path,
                        &weak,
                        &theme,
                        Some((sha.clone(), reviewed, review_active)),
                        can_discard,
                    )
                })
                .collect()
        };

        v_flex()
            .w(px(440.))
            .h_full()
            .bg(theme.panel)
            .border_l_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_color(theme.text)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(if comparing {
                        format!("Comparing with {}", &sha[..sha.len().min(8)])
                    } else {
                        sha.chars().take(8).collect::<String>()
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .child(avatar_circle(&detail.author, &detail.email, weak.clone(), &theme))
                    .child(div().text_sm().text_color(theme.text_muted).child({
                        let mut meta = format!("{} <{}>", detail.author, detail.email);
                        if let Some(signature) = self.signature {
                            meta.push_str(&format!("  ·  signature {signature}"));
                        }
                        meta
                    })),
            )
            .when_some(self.signature_details.clone(), |this, details| {
                this.child(
                    div()
                        .w_full()
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(theme.text_muted)
                        .child(details),
                )
            })
            .when(
                !self.branches_containing.is_empty() || !self.tags_containing.is_empty(),
                |this| {
                    let mut parts: Vec<String> = self.branches_containing.clone();
                    parts.extend(self.tags_containing.iter().map(|tag| format!("tag:{tag}")));
                    this.child(
                        div()
                            .w_full()
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child(format!("contained in: {}", parts.join(", "))),
                    )
                },
            )
            .child(self.render_message(&detail.message))
            .child(self.render_tags(&sha, weak.clone()))
            .child(self.render_detail_actions(&sha, weak))
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(format!("{} files changed", files.len())),
            )
            .child(
                v_flex()
                    .id("detail-file-list")
                    .w_full()
                    .overflow_y_scroll()
                    .children(files),
            )
            .into_any_element()
    }

    fn render_tags(&self, sha: &str, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let matching: Vec<git::TagDetail> = self
            .tags
            .iter()
            .filter(|tag| tag.commit == sha)
            .cloned()
            .collect();
        if matching.is_empty() {
            return div().into_any_element();
        }
        let rows: Vec<AnyElement> = matching
            .iter()
            .map(|tag| {
                let name_push = tag.name.clone();
                let name_delete = tag.name.clone();
                let name_copy = tag.name.clone();
                let weak_push = weak.clone();
                let weak_delete = weak.clone();
                let theme_row = theme.clone();
                v_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_0p5()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.accent)
                                    .child(format!("tag {}", tag.name)),
                            )
                            .child(div().flex_1())
                            .child(action_button(
                                format!("tag-copy-{}", tag.name),
                                "Copy",
                                &theme_row,
                                move |cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        name_copy.clone(),
                                    ));
                                },
                            ))
                            .child(action_button(
                                format!("tag-push-{}", tag.name),
                                "Push",
                                &theme_row,
                                move |cx| {
                                    let name = name_push.clone();
                                    weak_push
                                        .update(cx, |this, cx| {
                                            this.run_op(
                                                move |repo| git::push_tag(&repo.path, &name),
                                                cx,
                                            )
                                        })
                                        .ok();
                                },
                            ))
                            .child(action_button(
                                format!("tag-delete-{}", tag.name),
                                "Delete",
                                &theme_row,
                                move |cx| {
                                    let name = name_delete.clone();
                                    weak_delete
                                        .update(cx, |this, cx| {
                                            this.run_op(
                                                move |repo| git::delete_tag(&repo.path, &name),
                                                cx,
                                            )
                                        })
                                        .ok();
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child(format!("{}  {}", tag.tagger, tag.date)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text)
                            .child(tag.message.clone()),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex().w_full().children(rows).into_any_element()
    }

    fn render_message(&self, message: &str) -> AnyElement {
        let theme = self.theme.clone();
        let lines: Vec<AnyElement> = if self.markdown_enabled {
            markdown::parse(message)
                .into_iter()
                .map(|spans| {
                    let spans: Vec<AnyElement> = spans
                        .into_iter()
                        .map(|span| {
                            let text = if self.emoji_enabled {
                                emoji::replace_with(&span.text, &self.custom_emoji)
                            } else {
                                span.text
                            };
                            span_element(text, span.style, &theme)
                        })
                        .collect();
                    h_flex().flex_wrap().children(spans).into_any_element()
                })
                .collect()
        } else {
            message
                .lines()
                .map(|line| {
                    let text = if self.emoji_enabled {
                        emoji::replace_with(line, &self.custom_emoji)
                    } else {
                        line.to_string()
                    };
                    div()
                        .text_sm()
                        .text_color(theme.text)
                        .child(text)
                        .into_any_element()
                })
                .collect()
        };
        v_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_0p5()
            .children(lines)
            .into_any_element()
    }

    fn render_detail_actions(&self, sha: &str, _weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let sha_string = sha.to_string();
        let message = self
            .detail
            .as_ref()
            .map(|detail| detail.message.clone())
            .unwrap_or_default();
        let remote = self.remote_info.clone();
        let branch = self.branch.clone().unwrap_or_default();
        let mut buttons: Vec<AnyElement> = Vec::new();

        let sha_for_copy = sha_string.clone();
        buttons.push(
            action_button("copy-sha", "Copy SHA", &theme, move |cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha_for_copy.clone()));
            })
            .into_any_element(),
        );

        let message_for_copy = message.clone();
        buttons.push(
            action_button("copy-message", "Copy Message", &theme, move |cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(message_for_copy.clone()));
            })
            .into_any_element(),
        );

        let email_for_copy = self
            .detail
            .as_ref()
            .map(|detail| detail.email.clone())
            .unwrap_or_default();
        if !email_for_copy.is_empty() {
            buttons.push(
                action_button("copy-email", "Copy Email", &theme, move |cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(email_for_copy.clone()));
                })
                .into_any_element(),
            );
        }

        if let Some(info) = remote {
            let url = info.commit_url(&sha_string);
            buttons.push(
                action_button("open-remote", "Open on Remote", &theme, move |_cx| {
                    let _ = git::open_url(&url);
                })
                .into_any_element(),
            );
            if !branch.is_empty() {
                let base = self
                    .active_repo()
                    .and_then(|repo| git::default_branch(&repo.path))
                    .unwrap_or_else(|| "main".to_string());
                let pr = if self.custom_pr_provider.trim().is_empty() {
                    info.pr_url(&base, &branch, &format!("Merge {branch} into {base}"))
                } else {
                    git::render_pr_template(
                        &self.custom_pr_provider,
                        &info.host,
                        &info.owner,
                        &info.repo,
                        &base,
                        &branch,
                    )
                };
                buttons.push(
                    action_button("create-pr", "Create PR", &theme, move |_cx| {
                        let _ = git::open_url(&pr);
                    })
                    .into_any_element(),
                );
            }
            for issue in find_issues(&message) {
                let url = if self.custom_issue_provider.trim().is_empty() {
                    info.issue_url(&issue)
                } else {
                    git::render_issue_template(
                        &self.custom_issue_provider,
                        &info.host,
                        &info.owner,
                        &info.repo,
                        &issue,
                    )
                };
                let id = format!("issue-{issue}");
                buttons.push(
                    action_button(id, "Open Issue", &theme, move |_cx| {
                        let _ = git::open_url(&url);
                    })
                    .into_any_element(),
                );
            }
        }

        for (index, url) in find_urls(&message).into_iter().enumerate() {
            let id = format!("url-{index}");
            buttons.push(
                action_button(id, "Open Link", &theme, move |_cx| {
                    let _ = git::open_url(&url);
                })
                .into_any_element(),
            );
        }

        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_2()
            .flex_wrap()
            .children(buttons)
            .into_any_element()
    }

    fn render_commands(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let commands = filter_commands(&self.commands.query);
        let selected = self.commands.selected.min(commands.len().saturating_sub(1));

        let items: Vec<AnyElement> = commands
            .iter()
            .enumerate()
            .map(|(position, (label, id))| {
                let is_selected = position == selected;
                let weak = weak.clone();
                let theme = theme.clone();
                let label = *label;
                let id = *id;
                h_flex()
                    .id(id)
                    .w_full()
                    .px_3()
                    .py_1()
                    .when(is_selected, |this| {
                        this.bg(theme.selected)
                            .border_l_2()
                            .border_color(theme.accent)
                    })
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        weak.update(cx, |this, cx| {
                            this.commands.open = false;
                            this.on_chip(id, cx);
                        })
                        .ok();
                    })
                    .child(div().text_color(theme.text).child(label))
                    .into_any_element()
            })
            .collect();

        let query_line = if self.commands.query.is_empty() {
            "Command palette…".to_string()
        } else {
            self.commands.query.clone()
        };

        overlay(theme.clone(), 120., 520., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text)
                .border_b_1()
                .border_color(theme.border)
                .child(query_line)
                .into_any_element(),
            v_flex()
                .id("command-list")
                .w_full()
                .overflow_y_scroll()
                .children(items)
                .into_any_element(),
        ])
    }

    fn render_column_header(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let lane_area = (self.commits.iter().map(|commit| commit.lane).max().unwrap_or(0) + 1)
            as f32
            * LANE_WIDTH
            + 8.;

        let handle = |column: ResizeColumn, width: f32, weak: gpui::WeakEntity<Self>| {
            let id = match column {
                ResizeColumn::Date => "resize-date",
                ResizeColumn::Author => "resize-author",
                ResizeColumn::Commit => "resize-commit",
            };
            div()
                .id(id)
                .w(px(6.))
                .h_full()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, _window, cx| {
                        let start_x = event.position.x.as_f32();
                        weak.update(cx, |this, cx| {
                            this.resize_drag = Some(ResizeDrag {
                                column,
                                start_x,
                                start_width: width,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        })
                        .ok();
                    },
                )
        };

        let cell = |label: &'static str,
                    width: f32,
                    column: ResizeColumn,
                    weak: gpui::WeakEntity<Self>| {
            h_flex()
                .w(px(width))
                .h_full()
                .items_center()
                .text_sm()
                .text_color(theme.text_muted)
                .child(div().flex_1().truncate().child(label))
                .child(handle(column, width, weak))
        };

        h_flex()
            .w_full()
            .px_2()
            .h(px(20.))
            .items_center()
            .bg(theme.panel)
            .border_b_1()
            .border_color(theme.border)
            .child(div().w(px(lane_area)))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child("Description"),
            )
            .when(self.columns.date, |this| {
                this.child(cell(
                    "Date",
                    self.date_width,
                    ResizeColumn::Date,
                    weak.clone(),
                ))
            })
            .when(self.columns.author, |this| {
                this.child(cell(
                    "Author",
                    self.author_width,
                    ResizeColumn::Author,
                    weak.clone(),
                ))
            })
            .when(self.columns.commit, |this| {
                this.child(cell(
                    "Commit",
                    self.commit_width,
                    ResizeColumn::Commit,
                    weak.clone(),
                ))
            })
            .into_any_element()
    }

    fn render_palette(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let filtered = self.filtered_repos();
        let selected = self.palette.selected.min(filtered.len().saturating_sub(1));

        let items: Vec<AnyElement> = filtered
            .iter()
            .enumerate()
            .map(|(position, &repo_index)| {
                let repo = &self.repos[repo_index];
                let is_selected = position == selected;
                let weak = weak.clone();
                let theme = theme.clone();
                h_flex()
                    .id(("repo", repo_index))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .when(is_selected, |this| {
                        this.bg(theme.selected)
                            .border_l_2()
                            .border_color(theme.accent)
                    })
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        weak.update(cx, |this, cx| this.switch_to(repo_index, cx))
                            .ok();
                    })
                    .child(div().text_color(theme.text).child(repo.name.clone()))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child(repo.path.display().to_string()),
                    )
                    .into_any_element()
            })
            .collect();

        let query_line = if self.palette.query.is_empty() {
            "Search repositories…".to_string()
        } else {
            self.palette.query.clone()
        };

        overlay(theme.clone(), 120., 560., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text)
                .border_b_1()
                .border_color(theme.border)
                .child(query_line)
                .into_any_element(),
            v_flex()
                .id("repo-list")
                .w_full()
                .overflow_y_scroll()
                .children(items)
                .into_any_element(),
        ])
    }

    fn render_welcome(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();

        let button = |id: &'static str,
                      label: &'static str,
                      theme: &Theme,
                      weak: gpui::WeakEntity<Self>| {
            let bg = theme.accent;
            let fg = theme.bg;
            div()
                .id(id)
                .px_4()
                .py_2()
                .rounded_full()
                .bg(bg)
                .text_color(fg)
                .cursor_pointer()
                .hover(move |this| this.opacity(0.88))
                .on_click(move |_: &ClickEvent, _window, cx| {
                    weak.update(cx, |this, cx| this.on_chip(id, cx)).ok();
                })
                .child(label)
        };

        let recents: Vec<AnyElement> = self
            .recent
            .iter()
            .map(|path| {
                let label = path.display().to_string();
                let target = path.clone();
                let weak = weak.clone();
                let text = theme.text;
                let hover = theme.hover;
                h_flex()
                    .id(format!("welcome-recent-{label}"))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover))
                    .on_click(move |_: &ClickEvent, _window, cx| {
                        let target = target.clone();
                        weak.update(cx, |this, cx| this.open_recent(target, cx)).ok();
                    })
                    .child(div().text_sm().text_color(text).child(label))
                    .into_any_element()
            })
            .collect();

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(theme.bg)
            .child(div().text_lg().text_color(theme.text).child("gitviz"))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child("Open a repository or workspace to see its graph"),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(button("open-repo", "Open Repository…", &theme, weak.clone()))
                    .child(button("open-workspace", "Open Workspace…", &theme, weak.clone()))
                    .when(!self.roots.is_empty(), |this| {
                        this.child(button(
                            "save-workspace",
                            "Save Workspace…",
                            &theme,
                            weak.clone(),
                        ))
                    }),
            )
            .child(
                v_flex()
                    .w(px(560.))
                    .max_h(px(360.))
                    .bg(theme.panel)
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .border_b_1()
                            .border_color(theme.border)
                            .child("Recent"),
                    )
                    .child(if recents.is_empty() {
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("No recent repositories")
                            .into_any_element()
                    } else {
                        v_flex()
                            .id("welcome-recent-list")
                            .w_full()
                            .overflow_y_scroll()
                            .children(recents)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn render_recent(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let selected = self
            .recent_menu
            .selected
            .min(self.recent.len().saturating_sub(1));

        let items: Vec<AnyElement> = self
            .recent
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let is_selected = index == selected;
                let label = path.display().to_string();
                let target = path.clone();
                let weak = weak.clone();
                let text = theme.text;
                let hover = theme.hover;
                let selected_bg = theme.selected;
                let accent = theme.accent;
                h_flex()
                    .id(format!("recent-item-{index}"))
                    .w_full()
                    .px_3()
                    .py_1()
                    .when(is_selected, |this| {
                        this.bg(selected_bg)
                            .border_l_2()
                            .border_color(accent)
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover))
                    .on_click(move |_: &ClickEvent, _window, cx| {
                        let target = target.clone();
                        weak.update(cx, |this, cx| this.open_recent(target, cx)).ok();
                    })
                    .child(div().text_sm().text_color(text).child(label))
                    .into_any_element()
            })
            .collect();

        overlay(theme.clone(), 120., 560., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text)
                .border_b_1()
                .border_color(theme.border)
                .child("Open Recent")
                .into_any_element(),
            if items.is_empty() {
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child("No recent repositories")
                    .into_any_element()
            } else {
                v_flex()
                    .id("recent-list")
                    .w_full()
                    .overflow_y_scroll()
                    .children(items)
                    .into_any_element()
            },
        ])
    }

    fn render_theme(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let names = Theme::names();
        let selected = self.theme_menu.selected.min(names.len().saturating_sub(1));

        let items: Vec<AnyElement> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let name: &'static str = *name;
                let is_selected = index == selected;
                let is_current = name == theme.name;
                let weak = weak.clone();
                let text = theme.text;
                let muted = theme.text_muted;
                let hover = theme.hover;
                let selected_bg = theme.selected;
                let accent = theme.accent;
                h_flex()
                    .id(format!("theme-item-{index}"))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .when(is_selected, |this| {
                        this.bg(selected_bg)
                            .border_l_2()
                            .border_color(accent)
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover))
                    .on_click(move |_: &ClickEvent, _window, cx| {
                        weak.update(cx, |this, cx| {
                            this.apply_theme(name);
                            cx.notify();
                        })
                        .ok();
                    })
                    .child(div().flex_1().text_sm().text_color(text).child(name))
                    .when(is_current, |this| {
                        this.child(div().text_sm().text_color(muted).child("current"))
                    })
                    .into_any_element()
            })
            .collect();

        overlay(theme.clone(), 120., 480., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text)
                .border_b_1()
                .border_color(theme.border)
                .child("Select Theme")
                .into_any_element(),
            v_flex()
                .id("theme-list")
                .w_full()
                .overflow_y_scroll()
                .children(items)
                .into_any_element(),
        ])
    }

    fn render_prompt(&self) -> AnyElement {
        let theme = self.theme.clone();
        let (title, input) = self
            .prompt
            .as_ref()
            .map(|prompt| (prompt.title.clone(), prompt.input.clone()))
            .unwrap_or_default();
        overlay(theme.clone(), 160., 480., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child(title)
                .into_any_element(),
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text)
                .child(if input.is_empty() {
                    "Type a name…".to_string()
                } else {
                    input
                })
                .into_any_element(),
        ])
    }

    fn render_branch_filter(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let query = self.branch_filter.query.to_lowercase();

        let show_all = {
            let weak = weak.clone();
            let theme = theme.clone();
            h_flex()
                .id("branch-show-all")
                .w_full()
                .px_3()
                .py_1()
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    weak.update(cx, |this, cx| {
                        this.branch_filter.selected.clear();
                        this.rows_dirty = true;
                        cx.notify();
                    })
                    .ok();
                })
                .child(div().text_color(theme.accent).child("Show All"))
                .into_any_element()
        };

        let branches = filter_by_globs(&self.branch_filter.all, &self.branch_globs);
        let items: Vec<AnyElement> = branches
            .iter()
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .map(|name| {
                let checked = self.branch_filter.selected.contains(name);
                let name_toggle = name.clone();
                let name_co = name.clone();
                let name_rn = name.clone();
                let name_del = name.clone();
                let name_copy = name.clone();
                let name_merge = name.clone();
                let name_rebase = name.clone();
                let weak_toggle = weak.clone();
                let weak_co = weak.clone();
                let weak_rn = weak.clone();
                let weak_del = weak.clone();
                let weak_merge = weak.clone();
                let weak_rebase = weak.clone();
                let theme_row = theme.clone();
                h_flex()
                    .id(format!("branch-{}", name))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .child(
                        div()
                            .id(format!("branch-toggle-{}", name))
                            .text_color(if checked { theme.accent } else { theme.text_muted })
                            .on_click(move |_: &ClickEvent, window, cx| {
                                let _ = window;
                                let name = name_toggle.clone();
                                weak_toggle
                                    .update(cx, |this, cx| {
                                        if !this.branch_filter.selected.remove(&name) {
                                            this.branch_filter.selected.insert(name);
                                        }
                                        this.rows_dirty = true;
                                        cx.notify();
                                    })
                                    .ok();
                            })
                            .child(if checked { "[x]" } else { "[ ]" }),
                    )
                    .child(div().flex_1().text_color(theme.text).child(name.clone()))
                    .when_some(self.branch_tracking.get(name).copied(), |this, (ahead, behind)| {
                        let mut parts = Vec::new();
                        if ahead > 0 {
                            parts.push(format!("↑{ahead}"));
                        }
                        if behind > 0 {
                            parts.push(format!("↓{behind}"));
                        }
                        this.child(
                            div()
                                .text_sm()
                                .text_color(theme_row.text_muted)
                                .child(parts.join(" ")),
                        )
                    })
                    .child(action_button(
                        format!("branch-copy-{}", name),
                        "Copy",
                        &theme_row,
                        move |cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                name_copy.clone(),
                            ));
                        },
                    ))
                    .child(action_button(
                        format!("branch-co-{}", name),
                        "Checkout",
                        &theme_row,
                        move |cx| {
                            let name = name_co.clone();
                            weak_co
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::checkout_branch(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("branch-rn-{}", name),
                        "Rename",
                        &theme_row,
                        move |cx| {
                            let name = name_rn.clone();
                            weak_rn
                                .update(cx, |this, cx| {
                                    this.prompt = Some(Prompt {
                                        title: "Rename branch".to_string(),
                                        input: name.clone(),
                                        action: PromptAction::RenameBranch,
                                        sha: name,
                                    });
                                    cx.notify();
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("branch-del-{}", name),
                        "Delete",
                        &theme_row,
                        move |cx| {
                            let name = name_del.clone();
                            weak_del
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::delete_branch(&repo.path, &name, false),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("branch-merge-{}", name),
                        "Merge",
                        &theme_row,
                        move |cx| {
                            let name = name_merge.clone();
                            weak_merge
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::merge(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("branch-rebase-{}", name),
                        "Rebase",
                        &theme_row,
                        move |cx| {
                            let name = name_rebase.clone();
                            weak_rebase
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::rebase(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .into_any_element()
            })
            .collect();

        overlay(theme.clone(), 120., 520., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child("Filter branches (click to toggle, or act on one)")
                .into_any_element(),
            show_all,
            v_flex()
                .id("branch-filter-list")
                .w_full()
                .overflow_y_scroll()
                .children(items)
                .into_any_element(),
        ])
    }

    fn render_settings(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let date_label = match self.date_mode {
            DateMode::Author => "Show author dates",
            DateMode::Commit => "Show commit dates",
        };
        let toggles = [
            ("Date column", self.columns.date, "col-date"),
            ("Author column", self.columns.author, "col-author"),
            ("Commit column", self.columns.commit, "col-commit"),
            ("Include untracked files", self.include_untracked, "untracked"),
            ("Emoji shortcodes", self.emoji_enabled, "emoji"),
            ("Markdown in messages", self.markdown_enabled, "markdown"),
            ("Combine local + remote refs", self.combine_refs, "combine"),
            (date_label, self.date_mode == DateMode::Commit, "date"),
            ("Respect .mailmap", self.use_mailmap, "mailmap"),
            ("Include reflog commits", self.include_reflogs, "reflogs"),
            ("Short date format", self.date_short, "date-short"),
            ("Relative dates", self.relative_dates, "relative-dates"),
            ("Scroll to HEAD on load", self.scroll_to_head_on_load, "load-scroll-head"),
            ("Show remote HEAD refs", self.show_remote_heads, "remote-heads"),
            ("Show full ref names", self.use_full_refs, "full-refs"),
            ("Only tag commits", self.filter.only_tags, "only-tags"),
            ("Fetch: prune", self.fetch_prune, "fetch-prune"),
            ("Fetch: prune tags", self.fetch_prune_tags, "fetch-prune-tags"),
            ("File tree in details", self.file_tree, "file-tree"),
            ("Compact folders", self.compact_folders, "compact-folders"),
            (
                "Angular graph connectors",
                self.graph_style == layout::GraphStyle::Angular,
                "graph-style",
            ),
        ];

        let mut items: Vec<AnyElement> = toggles
            .into_iter()
            .map(|(label, on, id)| {
                let weak = weak.clone();
                let theme = theme.clone();
                h_flex()
                    .id(id)
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        weak.update(cx, |this, cx| this.toggle_setting(id, cx)).ok();
                    })
                    .child(
                        div()
                            .text_color(if on { theme.accent } else { theme.text_muted })
                            .child(if on { "[x]" } else { "[ ]" }),
                    )
                    .child(div().text_color(theme.text).child(label))
                    .into_any_element()
            })
            .collect();

        for (label, id) in [
            ("Cycle repository order (name/path/given)", "repo-order"),
            ("Cycle reference alignment", "ref-align"),
            ("Cycle lane colours", "color-preset"),
            ("Date width −", "width-date-minus"),
            ("Date width +", "width-date-plus"),
            ("Author width −", "width-author-minus"),
            ("Author width +", "width-author-plus"),
            ("Commit width −", "width-commit-minus"),
            ("Commit width +", "width-commit-plus"),
            ("Discovery depth −", "depth-minus"),
            ("Discovery depth +", "depth-plus"),
            ("Export configuration to .gitviz.conf", "export-config"),
            ("Add branch glob…", "add-glob"),
            ("Fetch into local branch…", "fetch-into"),
            ("Add repository…", "add-repo"),
            ("Remove current repository", "remove-repo"),
            ("Open repository…", "open-repo"),
            ("Open workspace…", "open-workspace"),
            ("Save workspace…", "save-workspace"),
            ("Pull current branch", "pull"),
            ("Clear branch globs", "clear-globs"),
            ("End all code reviews", "end-reviews"),
        ] {
            let weak = weak.clone();
            let theme_row = theme.clone();
            items.push(
                h_flex()
                    .id(id)
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        weak.update(cx, |this, cx| this.toggle_setting(id, cx)).ok();
                    })
                    .child(div().text_color(theme_row.accent).child(label))
                    .into_any_element(),
            );
        }

        items.push(
            div()
                .w_full()
                .px_3()
                .pt_2()
                .pb_1()
                .text_sm()
                .text_color(theme.text_muted)
                .border_t_1()
                .border_color(theme.border)
                .child("Remotes")
                .into_any_element(),
        );

        for name in &self.remotes {
            let weak_fetch = weak.clone();
            let weak_prune = weak.clone();
            let weak_remove = weak.clone();
            let weak_edit = weak.clone();
            let theme_row = theme.clone();
            let name_fetch = name.clone();
            let name_prune = name.clone();
            let name_remove = name.clone();
            let name_edit = name.clone();
            let url_edit = self
                .active_repo()
                .and_then(|repo| git::remote_url(&repo.path, name))
                .unwrap_or_default();
            items.push(
                h_flex()
                    .id(format!("remote-{}", name))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .child(div().flex_1().text_color(theme.text).child(name.clone()))
                    .child(action_button(
                        format!("remote-edit-{name}"),
                        "Edit URL",
                        &theme_row,
                        move |cx| {
                            let remote = name_edit.clone();
                            let input = url_edit.clone();
                            weak_edit
                                .update(cx, |this, cx| {
                                    this.prompt = Some(Prompt {
                                        title: format!("URL for remote {remote}"),
                                        input,
                                        action: PromptAction::EditRemote,
                                        sha: remote,
                                    });
                                    cx.notify();
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("remote-fetch-{name}"),
                        "Fetch",
                        &theme_row,
                        move |cx| {
                            let name = name_fetch.clone();
                            weak_fetch
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::fetch_remote(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("remote-prune-{name}"),
                        "Prune",
                        &theme_row,
                        move |cx| {
                            let name = name_prune.clone();
                            weak_prune
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::prune_remote(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .child(action_button(
                        format!("remote-remove-{name}"),
                        "Remove",
                        &theme_row,
                        move |cx| {
                            let name = name_remove.clone();
                            weak_remove
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::remove_remote(&repo.path, &name),
                                        cx,
                                    )
                                })
                                .ok();
                        },
                    ))
                    .into_any_element(),
            );
        }

        let weak_add = weak.clone();
        let theme_add = theme.clone();
        items.push(
            h_flex()
                .id("add-remote")
                .w_full()
                .px_3()
                .py_1()
                .gap_2()
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    weak_add
                        .update(cx, |this, cx| {
                            this.prompt = Some(Prompt {
                                title: "Add remote (name url)".to_string(),
                                input: String::new(),
                                action: PromptAction::AddRemote,
                                sha: String::new(),
                            });
                            cx.notify();
                        })
                        .ok();
                })
                .child(
                    div()
                        .text_color(theme_add.accent)
                        .child("+ Add remote…"),
                )
                .into_any_element(),
        );

        overlay(theme.clone(), 100., 460., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child("Settings")
                .into_any_element(),
            v_flex()
                .id("settings-list")
                .w_full()
                .overflow_y_scroll()
                .children(items)
                .into_any_element(),
        ])
    }

    fn render_menu(&self, weak: gpui::WeakEntity<Self>) -> AnyElement {
        let theme = self.theme.clone();
        let Some(menu) = &self.menu else {
            return div().into_any_element();
        };
        let items: Vec<AnyElement> = menu
            .items
            .iter()
            .map(|(label, action)| {
                let label = label.to_string();
                let action = *action;
                let weak = weak.clone();
                let theme = theme.clone();
                div()
                    .id(format!("menu-{label}"))
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(theme.text)
                    .hover(move |this| this.bg(theme.hover))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        weak.update(cx, |this, cx| this.run_menu_action(action, cx)).ok();
                    })
                    .child(label)
                    .into_any_element()
            })
            .collect();

        div()
            .absolute()
            .left(menu.x)
            .top(menu.y)
            .w(px(240.))
            .bg(theme.panel)
            .rounded_md()
            .shadow_lg()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .children(items)
            .into_any_element()
    }

    fn render_diff(&self, view: &DiffView) -> AnyElement {
        let theme = self.theme.clone();
        let lines: Vec<AnyElement> = view
            .text
            .lines()
            .map(|line| {
                let color = if line.starts_with('+') && !line.starts_with("+++") {
                    theme.accent
                } else if line.starts_with('-') && !line.starts_with("---") {
                    theme.error
                } else {
                    theme.text_muted
                };
                div()
                    .w_full()
                    .text_sm()
                    .text_color(color)
                    .child(line.to_string())
                    .into_any_element()
            })
            .collect();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(theme.overlay)
            .flex()
            .justify_center()
            .pt(px(60.))
            .child(
                v_flex()
                    .w(px(900.))
                    .max_h(px(640.))
                    .bg(theme.panel)
                    .rounded_md()
                    .shadow_lg()
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .px_3()
                            .py_2()
                            .text_color(theme.text)
                            .border_b_1()
                            .border_color(theme.border)
                            .child(view.title.clone()),
                    )
                    .child(
                        v_flex()
                            .id("diff-lines")
                            .w_full()
                            .overflow_y_scroll()
                            .children(lines),
                    ),
            )
            .into_any_element()
    }

    fn toggle_setting(&mut self, id: &str, cx: &mut Context<Self>) {
        match id {
            "col-date" => self.columns.date = !self.columns.date,
            "col-author" => self.columns.author = !self.columns.author,
            "col-commit" => self.columns.commit = !self.columns.commit,
            "untracked" => {
                self.include_untracked = !self.include_untracked;
        let Some(repo) = self.active_repo().cloned() else {
            self.rows_dirty = true;
            cx.notify();
            return;
        };
                self.status = git::status(&repo.path, self.include_untracked);
                self.rows_dirty = true;
            }
            "emoji" => self.emoji_enabled = !self.emoji_enabled,
            "markdown" => self.markdown_enabled = !self.markdown_enabled,
            "combine" => self.combine_refs = !self.combine_refs,
            "date" => {
                self.date_mode = match self.date_mode {
                    DateMode::Author => DateMode::Commit,
                    DateMode::Commit => DateMode::Author,
                }
            }
            "mailmap" => {
                self.use_mailmap = !self.use_mailmap;
                self.load(cx);
                return;
            }
            "reflogs" => {
                self.include_reflogs = !self.include_reflogs;
                self.load(cx);
                return;
            }
            "repo-order" => {
                self.repo_order = match self.repo_order {
                    RepoOrder::Name => RepoOrder::Path,
                    RepoOrder::Path => RepoOrder::Given,
                    RepoOrder::Given => RepoOrder::Name,
                };
                self.sort_repos();
            }
            "color-preset" => self.color_preset = (self.color_preset + 1) % 3,
            "width-date-minus" => self.date_width = adjust_width(self.date_width, -20.),
            "width-date-plus" => self.date_width = adjust_width(self.date_width, 20.),
            "width-author-minus" => self.author_width = adjust_width(self.author_width, -20.),
            "width-author-plus" => self.author_width = adjust_width(self.author_width, 20.),
            "width-commit-minus" => self.commit_width = adjust_width(self.commit_width, -20.),
            "width-commit-plus" => self.commit_width = adjust_width(self.commit_width, 20.),
            "depth-minus" => {
                self.repo_depth = self.repo_depth.saturating_sub(1);
                self.rediscover(cx);
                return;
            }
            "depth-plus" => {
                self.repo_depth += 1;
                self.rediscover(cx);
                return;
            }
            "file-tree" => self.file_tree = !self.file_tree,
            "compact-folders" => self.compact_folders = !self.compact_folders,
            "graph-style" => self.graph_style = self.graph_style.toggled(),
            "remote-heads" => {
                self.show_remote_heads = !self.show_remote_heads;
                self.load(cx);
                return;
            }
            "full-refs" => {
                self.use_full_refs = !self.use_full_refs;
                self.load(cx);
                return;
            }
            "only-tags" => {
                self.filter.only_tags = !self.filter.only_tags;
                self.load(cx);
                return;
            }
            "fetch-prune" => self.fetch_prune = !self.fetch_prune,
            "fetch-prune-tags" => self.fetch_prune_tags = !self.fetch_prune_tags,
            "date-short" => self.date_short = !self.date_short,
            "relative-dates" => self.relative_dates = !self.relative_dates,
            "load-scroll-head" => self.scroll_to_head_on_load = !self.scroll_to_head_on_load,
            "clear-globs" => self.branch_globs.clear(),
            "add-glob" => {
                self.prompt = Some(Prompt {
                    title: "Branch glob (e.g. heads/feature/*)".to_string(),
                    input: String::new(),
                    action: PromptAction::AddGlob,
                    sha: String::new(),
                });
            }
            "fetch-into" => {
                self.prompt = Some(Prompt {
                    title: "Fetch into local branch (remote remote-branch local-branch)".to_string(),
                    input: String::new(),
                    action: PromptAction::FetchInto,
                    sha: String::new(),
                });
            }
            "add-repo" => {
                self.prompt = Some(Prompt {
                    title: "Add repository (path, ~ allowed)".to_string(),
                    input: String::new(),
                    action: PromptAction::AddRepository,
                    sha: String::new(),
                });
            }
            "remove-repo" => {
                self.remove_active_repo(cx);
                return;
            }
            "open-repo" | "open-workspace" | "save-workspace" | "open-recent" | "select-theme" => {
                self.on_chip(id, cx);
                return;
            }
            "pull" => {
                self.run_op(|repo| git::pull(&repo.path), cx);
                return;
            }
            "ref-align" => {
                self.ref_align = match self.ref_align {
                    RefAlign::Left => RefAlign::Right,
                    RefAlign::Right => RefAlign::Left,
                };
            }
            "export-config" => {
                self.export_repo_config(cx);
                return;
            }
            "end-reviews" => {
                self.review.end_all();
                self.review.save();
            }
            _ => {}
        }
        cx.notify();
    }
}

/// Horizontally stacks children (standalone replacement for zed's `ui::h_flex`).
fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// Vertically stacks children (standalone replacement for zed's `ui::v_flex`).
fn v_flex() -> Div {
    div().flex().flex_col()
}

fn action_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    theme: &Theme,
    handler: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let hover_color = theme.hover;
    let text_color = theme.text_muted;
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_sm()
        .text_color(text_color)
        .hover(move |this| this.bg(hover_color))
        .on_click(move |_: &ClickEvent, _window, cx| handler(cx))
        .child(label)
}

fn overlay(theme: Theme, top: f32, width: f32, children: Vec<AnyElement>) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .justify_center()
        .pt(px(top))
        .bg(theme.overlay)
        .child(
            v_flex()
                .w(px(width))
                .max_h(px(460.))
                .bg(theme.panel)
                .rounded_lg()
                .shadow_lg()
                .border_1()
                .border_color(theme.border)
                .overflow_hidden()
                .children(children),
        )
        .into_any_element()
}

fn render_file_row(
    file: &ChangedFile,
    display: &str,
    weak: &gpui::WeakEntity<GraphView>,
    theme: &Theme,
    review: Option<(String, bool, bool)>,
    can_discard: bool,
) -> AnyElement {
    let path = file.path.clone();
    let weak_click = weak.clone();
    let weak_review = weak.clone();
    let weak_copy = weak.clone();
    let weak_open = weak.clone();
    let weak_rev = weak.clone();
    let weak_discard = weak.clone();
    let path_copy = file.path.clone();
    let path_open = file.path.clone();
    let path_rev = file.path.clone();
    let path_discard = file.path.clone();
    let review_path = file.path.clone();
    let theme = theme.clone();
    let (sha, reviewed, review_active) = review.unwrap_or_default();
    let needs_review = review_active && !reviewed;
    h_flex()
        .id(format!("file-{}", file.path))
        .w_full()
        .px_2()
        .py_0p5()
        .gap_2()
        .on_click(move |_: &ClickEvent, window, cx| {
            let _ = window;
            let path = path.clone();
            weak_click
                .update(cx, |this, cx| this.open_diff(&path, cx))
                .ok();
        })
        .child(status_letter(file.status, &theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.text)
                .when(needs_review, |this| this.font_weight(gpui::FontWeight::BOLD))
                .child(display.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.text_muted)
                .child(format!("+{} -{}", file.added, file.removed)),
        )
        .when(!sha.is_empty(), |this| {
            let review_id = format!("review-{}", file.path);
            this.child(
                div()
                    .id(review_id)
                    .text_sm()
                    .text_color(if reviewed { theme.accent } else { theme.text_muted })
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        cx.stop_propagation();
                        weak_review
                            .update(cx, |this, cx| this.toggle_reviewed(&sha, &review_path, cx))
                            .ok();
                    })
                    .child(if reviewed { "[x]" } else { "[ ]" }),
            )
        })
        .child(
            div()
                .id(format!("copy-path-{}", file.path))
                .text_sm()
                .text_color(theme.text_muted)
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    cx.stop_propagation();
                    weak_copy
                        .update(cx, |this, cx| this.copy_path(&path_copy, cx))
                        .ok();
                })
                .child("Copy"),
        )
        .child(
            div()
                .id(format!("open-file-{}", file.path))
                .text_sm()
                .text_color(theme.text_muted)
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    cx.stop_propagation();
                    weak_open
                        .update(cx, |this, cx| this.open_file(&path_open, cx))
                        .ok();
                })
                .child("Open"),
        )
        .child(
            div()
                .id(format!("rev-file-{}", file.path))
                .text_sm()
                .text_color(theme.text_muted)
                .on_click(move |_: &ClickEvent, window, cx| {
                    let _ = window;
                    cx.stop_propagation();
                    weak_rev
                        .update(cx, |this, cx| this.open_file_at_revision(&path_rev, cx))
                        .ok();
                })
                .child("Rev"),
        )
        .when(can_discard, |this| {
            this.child(
                div()
                    .id(format!("discard-file-{}", file.path))
                    .text_sm()
                    .text_color(theme.text_muted)
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        cx.stop_propagation();
                        weak_discard
                            .update(cx, |this, cx| this.discard_uncommitted_file(&path_discard, cx))
                            .ok();
                    })
                    .child("Discard"),
            )
        })
        .into_any_element()
}

impl GraphView {
    fn open_diff(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.detail_stash {
            if let Some(repo) = self.active_repo() {
                let text = git::stash_file_diff(&repo.path, index, path);
                self.diff = Some(DiffView {
                    title: format!("stash@{{{index}}} — {path}"),
                    text: Arc::new(text),
                });
                cx.notify();
            }
            return;
        }
        let Some(sha) = self.detail_sha.clone() else {
            return;
        };
        let compare_sha = self
            .compare
            .and_then(|index| self.commits.get(index))
            .map(|commit| commit.sha.clone());
        if let Some(repo) = self.active_repo() {
            let (text, title) = match compare_sha {
                Some(_) if self.compare_worktree => (
                    git::working_tree_file_diff(&repo.path, &sha, path),
                    format!("Working tree vs {} — {}", &sha[..sha.len().min(8)], path),
                ),
                Some(from) => (
                    git::compare_file_diff(&repo.path, &from, &sha, path),
                    format!(
                        "{}..{} — {}",
                        &from[..from.len().min(8)],
                        &sha[..sha.len().min(8)],
                        path
                    ),
                ),
                None => (
                    git::file_diff(&repo.path, &sha, path),
                    format!("{} — {}", &sha[..sha.len().min(8)], path),
                ),
            };
            self.diff = Some(DiffView {
                title,
                text: Arc::new(text),
            });
            self.mark_reviewed_if_active(&sha, path);
            cx.notify();
        }
    }

    /// In an ongoing code review, viewing a file marks it as reviewed so it is
    /// no longer shown in bold.
    fn mark_reviewed_if_active(&mut self, sha: &str, path: &str) {
        let key = format!("{sha}\t{path}");
        if self.review.has_commit(sha) && !self.review.is_reviewed(&key) {
            self.review.mark(&key);
            self.review.save();
        }
    }

    fn copy_path(&mut self, path: &str, cx: &mut App) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.to_string()));
    }

    /// Discards a single uncommitted file (removing it when untracked).
    fn discard_uncommitted_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let untracked = self
            .status
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.is_untracked())
            .unwrap_or(false);
        let path = path.to_string();
        self.run_op(move |repo| git::discard_file(&repo.path, &path, untracked), cx);
    }

    fn open_file(&mut self, path: &str, _cx: &mut Context<Self>) {
        if let Some(repo) = self.active_repo().cloned() {
            let _ = git::open_path(&repo.path.join(path));
            if let Some(sha) = self.detail_sha.clone() {
                self.mark_reviewed_if_active(&sha, path);
            }
        }
    }

    /// Writes the file as it was at the selected commit to a temp file and
    /// opens it, mirroring git-graph's "Open at this revision".
    fn open_file_at_revision(&mut self, path: &str, _cx: &mut Context<Self>) {
        let Some(sha) = self.detail_sha.clone() else {
            return;
        };
        let Some(repo) = self.active_repo().cloned() else {
            return;
        };
        let content = git::show_file(&repo.path, &sha, path);
        let file_name = path.rsplit('/').next().unwrap_or("file").to_string();
        let dir = std::env::temp_dir().join(format!("gitviz-rev-{}", &sha[..sha.len().min(8)]));
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join(file_name);
        if std::fs::write(&target, content).is_ok() {
            let _ = git::open_path(&target);
            self.mark_reviewed_if_active(&sha, path);
        }
    }

    fn render_footer(&self) -> AnyElement {
        let theme = self.theme.clone();
        let base = match self.hovered {
            Some(RowKind::Commit(index)) => self.commits.get(index).map(|commit| {
                let refs = if commit.refs.is_empty() {
                    "no refs".to_string()
                } else {
                    commit.refs.join(", ")
                };
                let ancestor = if self.head_ancestors.contains(&commit.sha) {
                    "in HEAD"
                } else {
                    "not in HEAD"
                };
                format!("{}  ·  {}  ·  {}", commit.short_sha(), refs, ancestor)
            }),
            Some(RowKind::Stash(index)) => self
                .stashes
                .get(index)
                .map(|stash| format!("stash@{{{index}}}: {}", stash.message)),
            Some(RowKind::Uncommitted) => {
                Some(format!("Uncommitted changes: {} files", self.status.len()))
            }
            None => None,
        }
        .unwrap_or_else(|| "Hover a commit to see its refs".to_string());

        let text = match self.hovered {
            Some(RowKind::Commit(index)) => {
                let containment = self
                    .commits
                    .get(index)
                    .and_then(|commit| self.containment_cache.get(&commit.sha))
                    .map(|containment| {
                        if containment.is_empty() {
                            "not in any branch/tag/stash".to_string()
                        } else {
                            let mut parts = containment.branches.clone();
                            parts
                                .extend(containment.tags.iter().map(|tag| format!("tag:{tag}")));
                            parts.extend(
                                containment.stashes.iter().map(|stash| stash.to_string()),
                            );
                            format!("contained in: {}", parts.join(", "))
                        }
                    });
                match containment {
                    Some(extra) => format!("{base}   ·   {extra}"),
                    None => base,
                }
            }
            _ => base,
        };

        let text = if !self.search_query.is_empty() && !self.matches.is_empty() {
            format!(
                "{text}   ·   match {}/{}  (⌘G / ⇧⌘G)",
                self.match_cursor + 1,
                self.matches.len()
            )
        } else {
            text
        };

        div()
            .w_full()
            .px_3()
            .py_0p5()
            .bg(theme.panel)
            .border_t_1()
            .border_color(theme.border)
            .text_sm()
            .text_color(theme.text_muted)
            .truncate()
            .child(text)
            .into_any_element()
    }
}

/// Row rendering context so the `uniform_list` closure can be `'static`.
struct RowRenderContext {
    commits: Arc<Vec<Commit>>,
    status: Arc<Vec<StatusEntry>>,
    stashes: Arc<Vec<StashEntry>>,
    rows: Arc<Vec<RowKind>>,
    selected: Option<RowKind>,
    compare: Option<usize>,
    head_ancestors: HashSet<String>,
    theme: Theme,
    lane_colors: [gpui::Rgba; 8],
    emoji_enabled: bool,
    combine_refs: bool,
    columns: Columns,
    date_mode: DateMode,
    date_width: f32,
    author_width: f32,
    commit_width: f32,
    ref_align: RefAlign,
    matches: Arc<Vec<usize>>,
    match_cursor: usize,
    date_short: bool,
    relative_dates: bool,
    custom_emoji: Arc<Vec<(String, String)>>,
    graph_style: layout::GraphStyle,
}

impl RowRenderContext {
    fn render_row(&self, position: usize, weak: gpui::WeakEntity<GraphView>) -> AnyElement {
        let Some(&row) = self.rows.get(position) else {
            return div().into_any_element();
        };
        let theme = &self.theme;
        let is_selected = self.selected == Some(row);
        match row {
            RowKind::Uncommitted => self.render_synthetic(
                "Uncommitted Changes",
                format!("{} changes", self.status.len()),
                is_selected,
                theme,
                weak,
                position,
                None,
                None,
            ),
            RowKind::Stash(index) => {
                let message = self
                    .stashes
                    .get(index)
                    .map(|stash| stash.message.clone())
                    .unwrap_or_default();
                self.render_synthetic(
                    &format!("stash@{{{index}}}"),
                    message,
                    is_selected,
                    theme,
                    weak,
                    position,
                    None,
                    Some(row),
                )
            }
            RowKind::Commit(index) => {
                let Some(commit) = self.commits.get(index) else {
                    return div().into_any_element();
                };
                self.render_commit(index, commit, is_selected, theme, weak, position)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_synthetic(
        &self,
        primary: &str,
        secondary: String,
        is_selected: bool,
        theme: &Theme,
        weak: gpui::WeakEntity<GraphView>,
        _position: usize,
        _commit: Option<usize>,
        row: Option<RowKind>,
    ) -> AnyElement {
        let row_kind = row.unwrap_or(RowKind::Uncommitted);
        let weak_click = weak.clone();
        let weak_right = weak;
        let theme = theme.clone();
        h_flex()
            .id(format!("synthetic-{}", primary))
            .h(px(ROW_HEIGHT))
            .w_full()
            .items_center()
            .px_2()
            .gap_2()
            .when(is_selected, |this| this.bg(theme.selected))
            .on_click(move |_: &ClickEvent, window, cx| {
                let _ = window;
                weak_click.update(cx, |this, cx| this.select_row(row_kind, cx)).ok();
            })
            .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, window, cx| {
                let _ = window;
                weak_right
                    .update(cx, |this, _cx| this.open_menu(row_kind, event.position.x, event.position.y))
                    .ok();
            })
            .child(
                div()
                    .w(px(LAYER_PREFIX))
                    .flex()
                    .justify_center()
                    .text_color(theme.text_muted)
                    .child("○"),
            )
            .child(div().text_color(theme.text).child(primary.to_string()))
            .child(div().flex_1())
            .child(div().text_sm().text_color(theme.text_muted).child(secondary))
            .into_any_element()
    }

    fn render_commit(
        &self,
        index: usize,
        commit: &Commit,
        is_selected: bool,
        theme: &Theme,
        weak: gpui::WeakEntity<GraphView>,
        _position: usize,
    ) -> AnyElement {
        let colors = self.lane_colors;
        let graph_style = self.graph_style;
        let lane_area = (self.commits.iter().map(|c| c.lane).max().unwrap_or(0) + 1) as f32
            * LANE_WIDTH
            + 8.;
        let lane_area_px = px(lane_area);
        let commit = commit.clone();
        let is_compare = self.compare == Some(index);
        let is_ancestor = self.head_ancestors.contains(&commit.sha);

        let weak_right = weak.clone();
        let weak_up = weak.clone();
        let weak_hover = weak.clone();
        let weak_down = weak;

        let for_paint = commit.clone();
        let canvas = gpui::canvas(
            move |_bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| for_paint.clone(),
            move |bounds: Bounds<Pixels>, commit: Commit, window: &mut Window, _cx: &mut App| {
                paint_lanes(&mut *window, bounds, &commit, &colors, graph_style);
            },
        )
        .w(lane_area_px)
        .h(px(ROW_HEIGHT));

        let dot_left = px(commit.lane as f32 * LANE_WIDTH + (LANE_WIDTH - DOT_SIZE) / 2.0);
        let dot_top = px((ROW_HEIGHT - DOT_SIZE) / 2.0);

        let ref_names = format_refs(&commit.refs, self.combine_refs, self.ref_align);
        let is_current_match = self.matches.get(self.match_cursor) == Some(&index);
        let is_match = self.matches.contains(&index);
        let refs = if ref_names.is_empty() {
            String::new()
        } else {
            format!("[{}] ", ref_names.join(", "))
        };
        let subject = if self.emoji_enabled {
            emoji::replace_with(&commit.subject, &self.custom_emoji)
        } else {
            commit.subject.clone()
        };
        let date = if self.relative_dates {
            let timestamp = match self.date_mode {
                DateMode::Author => commit.timestamp,
                DateMode::Commit => commit.commit_timestamp,
            };
            relative_time(timestamp, now_secs())
        } else {
            let date = match self.date_mode {
                DateMode::Author => commit.author_date.clone(),
                DateMode::Commit => commit.commit_date.clone(),
            };
            format_date(&date, self.date_short)
        };

        h_flex()
            .id(("commit", index))
            .h(px(ROW_HEIGHT))
            .w_full()
            .items_center()
            .when(is_selected, |this| this.bg(theme.selected))
            .when(is_compare, |this| this.bg(theme.hover))
            .when(is_current_match, |this| this.bg(theme.selected))
            .when(is_match && !is_current_match, |this| this.bg(theme.hover))
            .on_hover(move |hovered, _window, cx| {
                let target = if *hovered { Some(index) } else { None };
                weak_hover
                    .update(cx, |this, cx| {
                        let current = match this.hovered {
                            Some(RowKind::Commit(current)) => Some(current),
                            _ => None,
                        };
                        if current != target {
                            this.hovered = target.map(RowKind::Commit);
                            cx.notify();
                        }
                    })
                    .ok();
            })
            .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, window, cx| {
                let _ = window;
                weak_right
                    .update(cx, |this, _cx| this.open_menu(RowKind::Commit(index), event.position.x, event.position.y))
                    .ok();
            })
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, window, cx| {
                let _ = window;
                if event.modifiers.secondary() {
                    weak_down
                        .update(cx, |this, cx| this.toggle_compare(index, cx))
                        .ok();
                } else {
                    weak_up
                        .update(cx, |this, cx| this.select_row(RowKind::Commit(index), cx))
                        .ok();
                }
            })
            .child(
                div()
                    .relative()
                    .w(lane_area_px)
                    .h(px(ROW_HEIGHT))
                    .child(canvas)
                    .child(
                        div()
                            .absolute()
                            .left(dot_left)
                            .top(dot_top)
                            .w(px(DOT_SIZE))
                            .h(px(DOT_SIZE))
                            .rounded_full()
                            .bg(colors[commit.lane % colors.len()]),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(if is_ancestor { theme.text } else { theme.text_muted })
                    .child(format!("{}{}  {}", refs, commit.short_sha(), subject)),
            )
            .when(self.columns.date, |this| {
                this.child(column_cell(&date, theme, self.date_width))
            })
            .when(self.columns.author, |this| {
                this.child(column_cell(&commit.author, theme, self.author_width))
            })
            .when(self.columns.commit, |this| {
                this.child(column_cell(commit.short_sha(), theme, self.commit_width))
            })
            .into_any_element()
    }
}

const LAYER_PREFIX: f32 = 24.0;

fn parse_hex_color(value: &str) -> Option<gpui::Rgba> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()? as u32;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()? as u32;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()? as u32;
    Some(gpui::rgb((red << 16) | (green << 8) | blue))
}

fn filter_by_globs(branches: &[String], globs: &[String]) -> Vec<String> {
    if globs.is_empty() {
        return branches.to_vec();
    }
    branches
        .iter()
        .filter(|branch| globs.iter().any(|pattern| glob_match(pattern, branch)))
        .cloned()
        .collect()
}

/// Parses `.gitviz.conf` `emoji_mappings` entries of the form `code:emoji`.
fn parse_emoji_mappings(mappings: &[String]) -> Vec<(String, String)> {
    mappings
        .iter()
        .filter_map(|mapping| mapping.split_once(':'))
        .filter(|(code, emoji)| !code.trim().is_empty() && !emoji.is_empty())
        .map(|(code, emoji)| (code.trim().to_string(), emoji.trim().to_string()))
        .collect()
}

/// Picks the single-letter status to show for an uncommitted file.
fn status_entry_letter(entry: &StatusEntry) -> char {
    if entry.is_untracked() {
        'A'
    } else if entry.worktree_status != ' ' && entry.worktree_status != '?' {
        entry.worktree_status
    } else {
        entry.index_status
    }
}

fn format_date(iso: &str, short: bool) -> String {
    if short {
        iso.chars().take(10).collect()
    } else {
        iso.to_string()
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn plural(count: i64) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// Human-readable relative time, e.g. `3 days ago`.
fn relative_time(past: i64, now: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;

    let delta = now.saturating_sub(past);
    if delta < 0 {
        return "in the future".to_string();
    }
    match delta {
        d if d < MINUTE => "just now".to_string(),
        d if d < HOUR => format!("{} minute{} ago", d / MINUTE, plural(d / MINUTE)),
        d if d < DAY => format!("{} hour{} ago", d / HOUR, plural(d / HOUR)),
        d if d < WEEK => format!("{} day{} ago", d / DAY, plural(d / DAY)),
        d if d < MONTH => format!("{} week{} ago", d / WEEK, plural(d / WEEK)),
        d if d < YEAR => format!("{} month{} ago", d / MONTH, plural(d / MONTH)),
        d => format!("{} year{} ago", d / YEAR, plural(d / YEAR)),
    }
}

/// Minimal glob matching supporting `*` (any run) and `?` (one character).
fn glob_match(pattern: &str, text: &str) -> bool {
    fn helper(pattern: &[char], text: &[char]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some(('*', rest)) => (0..=text.len()).any(|skip| helper(rest, &text[skip..])),
            Some(('?', rest)) => !text.is_empty() && helper(rest, &text[1..]),
            Some((character, rest)) => text.first() == Some(character) && helper(rest, &text[1..]),
        }
    }
    helper(
        &pattern.chars().collect::<Vec<_>>(),
        &text.chars().collect::<Vec<_>>(),
    )
}

fn combine_refs(refs: &[String]) -> Vec<String> {
    let locals: HashSet<&str> = refs.iter().map(String::as_str).collect();
    refs.iter()
        .filter(|name| match name.split_once('/') {
            Some((_, rest)) => !locals.contains(rest),
            None => true,
        })
        .cloned()
        .collect()
}

struct TreeNode {
    dirs: std::collections::BTreeMap<String, TreeNode>,
    files: Vec<ChangedFile>,
}

impl TreeNode {
    fn new() -> Self {
        Self {
            dirs: std::collections::BTreeMap::new(),
            files: Vec::new(),
        }
    }
}

struct TreeRow {
    depth: usize,
    is_dir: bool,
    name: String,
    file: Option<ChangedFile>,
}

fn build_tree_rows(files: &[ChangedFile], compact: bool) -> Vec<TreeRow> {
    let mut root = TreeNode::new();
    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let mut node = &mut root;
        for part in &parts[..parts.len().saturating_sub(1)] {
            node = node.dirs.entry(part.to_string()).or_insert_with(TreeNode::new);
        }
        node.files.push(file.clone());
    }
    let mut rows = Vec::new();
    flatten_tree(&root, 0, compact, &mut rows);
    rows
}

fn flatten_tree(node: &TreeNode, depth: usize, compact: bool, rows: &mut Vec<TreeRow>) {
    for (dir_name, child) in &node.dirs {
        let mut name = dir_name.clone();
        let mut current = child;
        if compact {
            while current.files.is_empty() && current.dirs.len() == 1 {
                let (next_name, next) = current.dirs.iter().next().expect("one child");
                name = format!("{name}/{next_name}");
                current = next;
            }
        }
        rows.push(TreeRow {
            depth,
            is_dir: true,
            name,
            file: None,
        });
        flatten_tree(current, depth + 1, compact, rows);
    }
    for file in &node.files {
        let name = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string();
        rows.push(TreeRow {
            depth,
            is_dir: false,
            name,
            file: Some(file.clone()),
        });
    }
}

fn indent_wrap(depth: usize, child: AnyElement) -> AnyElement {
    h_flex()
        .w_full()
        .child(div().w(px(depth as f32 * 12.)))
        .child(child)
        .into_any_element()
}

fn avatar_circle(
    name: &str,
    email: &str,
    _weak: gpui::WeakEntity<GraphView>,
    theme: &Theme,
) -> AnyElement {
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let hash = name
        .bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));
    let color = theme.lane_colors[(hash as usize) % theme.lane_colors.len()];
    let url = gravatar_url(email);
    div()
        .id(format!("avatar-{}", name))
        .w(px(20.))
        .h(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(color)
        .text_sm()
        .text_color(theme.bg)
        .when_some(url, |this, url| {
            this.cursor_pointer().on_click(move |_: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                let _ = git::open_url(&url);
            })
        })
        .child(initial)
        .into_any_element()
}

fn status_letter(status: char, theme: &Theme) -> AnyElement {
    let color = match status {
        'A' => theme.accent,
        'D' => theme.error,
        'R' => theme.accent,
        'U' => theme.error,
        _ => theme.text_muted,
    };
    div()
        .w(px(12.))
        .text_sm()
        .text_color(color)
        .child(status.to_string())
        .into_any_element()
}

fn column_cell(text: &str, theme: &Theme, width: f32) -> AnyElement {
    div()
        .w(px(width))
        .truncate()
        .text_sm()
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

fn span_element(text: String, style: SpanStyle, theme: &Theme) -> AnyElement {
    let element = match style {
        SpanStyle::Normal => div().text_sm().text_color(theme.text),
        SpanStyle::Bold => div()
            .text_sm()
            .text_color(theme.text)
            .font_weight(gpui::FontWeight::BOLD),
        SpanStyle::Italic => div().text_sm().text_color(theme.text).italic(),
        SpanStyle::BoldItalic => div()
            .text_sm()
            .text_color(theme.text)
            .font_weight(gpui::FontWeight::BOLD)
            .italic(),
        SpanStyle::Code => div()
            .text_sm()
            .text_color(theme.accent)
            .bg(theme.hover)
            .px_1()
            .rounded_md(),
    };
    element.child(text).into_any_element()
}

fn find_urls(message: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for token in message.split_whitespace() {
        let token = token.trim_matches(|c: char| {
            matches!(c, '(' | ')' | ',' | '.' | '"' | '\'' | '<' | '>')
        });
        if token.starts_with("http://") || token.starts_with("https://") {
            urls.push(token.to_string());
        }
    }
    urls
}

fn find_issues(message: &str) -> Vec<String> {
    let mut issues = Vec::new();
    let mut chars = message.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '#' {
            let mut number = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    number.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if !number.is_empty() {
                issues.push(number);
            }
        }
    }
    issues
}

fn paint_lanes(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    commit: &Commit,
    colors: &[gpui::Rgba; 8],
    style: layout::GraphStyle,
) {
    let x = |lane: f32| bounds.origin.x + px(lane * LANE_WIDTH + LANE_WIDTH / 2.0);
    let y = |fraction: f32| bounds.origin.y + px(bounds.size.height.as_f32() * fraction);

    for segment in layout::row_segments(commit, style) {
        let color = colors[segment.color_lane % colors.len()];
        match segment.kind {
            layout::SegmentKind::Line { from, to } => {
                if let Ok(path) = {
                    let mut builder = gpui::PathBuilder::stroke(px(1.5));
                    builder.move_to(point(x(from.0), y(from.1)));
                    builder.line_to(point(x(to.0), y(to.1)));
                    builder.build()
                } {
                    window.paint_path(path, color);
                }
            }
            layout::SegmentKind::Curve { from, to, control } => {
                if let Ok(path) = {
                    let mut builder = gpui::PathBuilder::stroke(px(1.5));
                    builder.move_to(point(x(from.0), y(from.1)));
                    builder.curve_to(point(x(to.0), y(to.1)), point(x(control.0), y(control.1)));
                    builder.build()
                } {
                    window.paint_path(path, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str, subject: &str, author: &str) -> Commit {
        Commit {
            sha: sha.to_string(),
            parents: Vec::new(),
            refs: Vec::new(),
            author: author.to_string(),
            timestamp: 0,
            commit_timestamp: 0,
            author_date: String::new(),
            commit_date: String::new(),
            subject: subject.to_string(),
            body: String::new(),
            lane: 0,
            through: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
            top_line: false,
            bottom_line: false,
        }
    }

    #[test]
    fn format_refs_combines_and_aligns() {
        let refs = vec![
            "main".to_string(),
            "origin/main".to_string(),
            "v1".to_string(),
        ];
        assert_eq!(format_refs(&refs, false, RefAlign::Left), refs);
        assert_eq!(
            format_refs(&refs, true, RefAlign::Left),
            vec!["main".to_string(), "v1".to_string()]
        );
        assert_eq!(
            format_refs(&refs, false, RefAlign::Right),
            vec![
                "v1".to_string(),
                "origin/main".to_string(),
                "main".to_string()
            ]
        );
    }

    #[test]
    fn command_filter_matches_case_insensitively() {
        assert_eq!(filter_commands("").len(), COMMANDS.len());
        let filtered = filter_commands("push");
        assert!(filtered.iter().any(|(label, _)| *label == "Push branch"));
        assert!(filter_commands("zzz").is_empty());
    }

    #[test]
    fn extracts_urls_and_issues_from_messages() {
        let message = "See (https://example.com/a). Also http://b.test/x, and fixes #123 #45.";
        let urls = find_urls(message);
        assert_eq!(
            urls,
            vec!["https://example.com/a".to_string(), "http://b.test/x".to_string()]
        );
        assert_eq!(find_issues(message), vec!["123".to_string(), "45".to_string()]);
        assert!(find_urls("no links here").is_empty());
        assert!(find_issues("no issues").is_empty());
    }

    #[test]
    fn combine_refs_prefers_local_over_remote() {
        let refs = vec![
            "main".to_string(),
            "origin/main".to_string(),
            "origin/feature".to_string(),
        ];
        assert_eq!(combine_refs(&refs), vec!["main", "origin/feature"]);
        assert!(combine_refs(&[]).is_empty());
    }

    #[test]
    fn find_matches_by_subject_author_and_sha() {
        let mut first = commit("abc123", "Fix SEO", "Sen");
        first.author_date = "2024-05-01 10:00".to_string();
        first.refs = vec!["v1.2.0".to_string()];
        first.body = "Closes the indexing regression".to_string();
        let commits = vec![first, commit("def456", "Add graph", "Alice")];

        assert_eq!(find_matches(&commits, "seo"), vec![0]);
        assert_eq!(find_matches(&commits, "alice"), vec![1]);
        assert_eq!(find_matches(&commits, "abc"), vec![0]);
        assert_eq!(find_matches(&commits, "2024-05-01"), vec![0]);
        assert_eq!(find_matches(&commits, "v1.2"), vec![0]);
        assert_eq!(find_matches(&commits, "indexing"), vec![0]);
        assert!(find_matches(&commits, "").is_empty());
    }

    #[test]
    fn column_widths_are_clamped() {
        assert_eq!(adjust_width(200., 20.), 220.);
        assert_eq!(adjust_width(40., -20.), 48.);
        assert_eq!(adjust_width(470., 40.), 480.);
    }

    #[test]
    fn load_more_near_end() {
        assert!(should_load_more(4, 5));
        assert!(!should_load_more(2, 5));
        assert!(!should_load_more(0, 0));
    }

    #[test]
    fn glob_matching() {
        assert!(glob_match("heads/feature/*", "heads/feature/login"));
        assert!(!glob_match("heads/feature/*", "heads/bugfix/login"));
        assert!(glob_match("main", "main"));
        assert!(glob_match("v?.*", "v1.2"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn hex_colors_parse() {
        assert!(parse_hex_color("#e06c75").is_some());
        assert!(parse_hex_color("e06c75").is_some());
        assert!(parse_hex_color("#fff").is_none());
        assert!(parse_hex_color("nope").is_none());
    }

    #[test]
    fn filters_branches_by_globs() {
        let branches = vec![
            "main".to_string(),
            "heads/feature/login".to_string(),
            "heads/fix/x".to_string(),
        ];
        assert_eq!(filter_by_globs(&branches, &[]), branches);
        let filtered = filter_by_globs(&branches, &["heads/feature/*".to_string()]);
        assert_eq!(filtered, vec!["heads/feature/login".to_string()]);
    }

    #[test]
    fn short_date_format() {
        assert_eq!(format_date("2024-05-01 10:00:00 +0000", true), "2024-05-01");
        assert_eq!(
            format_date("2024-05-01 10:00:00 +0000", false),
            "2024-05-01 10:00:00 +0000"
        );
    }

    #[test]
    fn relative_time_formats_each_bucket() {
        let now = 10_000_000_000_i64;
        assert_eq!(relative_time(now - 10, now), "just now");
        assert_eq!(relative_time(now - 60, now), "1 minute ago");
        assert_eq!(relative_time(now - 120, now), "2 minutes ago");
        assert_eq!(relative_time(now - 3 * 3600, now), "3 hours ago");
        assert_eq!(relative_time(now - 2 * 86_400, now), "2 days ago");
        assert_eq!(relative_time(now - 3 * 604_800, now), "3 weeks ago");
        assert_eq!(relative_time(now - 5 * 2_592_000, now), "5 months ago");
        assert_eq!(relative_time(now - 2 * 31_536_000, now), "2 years ago");
        assert_eq!(relative_time(now + 60, now), "in the future");
    }

    #[test]
    fn gravatar_url_hashes_email() {
        let url = gravatar_url("Test@Example.com ").unwrap();
        assert!(
            url.contains("55502f40dc8b7c769880b10874abc9d0"),
            "url was {url}"
        );
        assert!(gravatar_url("").is_none());
    }

    #[test]
    fn alt_parent_and_child_navigation() {
        let mut merge = commit("m", "M", "x");
        merge.parents = vec!["a".to_string(), "b".to_string()];
        let mut a = commit("a", "A", "x");
        a.parents = vec!["c".to_string()];
        let mut b = commit("b", "B", "x");
        b.parents = vec!["c".to_string()];
        let c = commit("c", "C", "x");
        let commits = vec![merge, a, b, c];

        assert_eq!(find_alt_parent_index(&commits, 0), Some(2));
        assert_eq!(find_alt_child_index(&commits, 2), Some(0));
    }

    #[test]
    fn near_bottom_detection() {
        assert!(near_bottom(-500.0, 500.0, 120.0));
        assert!(!near_bottom(-100.0, 500.0, 120.0));
        assert!(!near_bottom(0.0, 0.0, 120.0));
    }

    #[test]
    fn finds_head_commit() {
        let mut with_ref = commit("a", "A", "x");
        with_ref.refs = vec!["main".to_string()];
        let commits = vec![commit("b", "B", "x"), with_ref];
        assert_eq!(find_head_commit_index(&commits, Some("main")), Some(1));
        assert_eq!(find_head_commit_index(&commits, None), None);
    }

    #[test]
    fn menu_visibility_filters_hidden() {
        let items = vec![
            MenuAction::CherryPick,
            MenuAction::Revert,
            MenuAction::Merge,
        ];
        let hidden = vec!["revert".to_string()];
        let visible = visible_actions(&items, &hidden);
        assert_eq!(visible.len(), 2);
        assert!(!visible.iter().any(|action| action.key() == "revert"));
    }

    #[test]
    fn parent_and_child_navigation() {
        let mut a = commit("a", "A", "x");
        a.parents = vec!["b".to_string()];
        let mut b = commit("b", "B", "x");
        b.parents = vec!["c".to_string()];
        let c = commit("c", "C", "x");
        let commits = vec![a, b, c];

        assert_eq!(find_parent_index(&commits, 0), Some(1));
        assert_eq!(find_parent_index(&commits, 2), None);
        assert_eq!(find_child_index(&commits, 1), Some(0));
        assert_eq!(find_child_index(&commits, 0), None);
    }

    #[test]
    fn parses_custom_emoji_mappings() {
        let mappings = vec![
            "shipit:🚢".to_string(),
            "  party : 🎉 ".to_string(),
            "broken".to_string(),
            ":nope".to_string(),
        ];
        let parsed = parse_emoji_mappings(&mappings);
        assert_eq!(
            parsed,
            vec![
                ("shipit".to_string(), "🚢".to_string()),
                ("party".to_string(), "🎉".to_string()),
            ]
        );
    }

    #[test]
    fn command_palette_includes_review_and_glob_commands() {
        assert!(
            COMMANDS
                .iter()
                .any(|(_, id)| *id == "end-current-review")
        );
        assert!(COMMANDS.iter().any(|(_, id)| *id == "add-glob"));
        assert!(COMMANDS.iter().any(|(_, id)| *id == "fetch-into"));
    }

    #[test]
    fn containment_reports_empty() {
        let empty = Containment::default();
        assert!(empty.is_empty());
        let mut filled = Containment::default();
        filled.branches.push("main".to_string());
        assert!(!filled.is_empty());
    }

    #[test]
    fn command_palette_includes_repo_commands() {
        assert!(COMMANDS.iter().any(|(_, id)| *id == "add-repo"));
        assert!(COMMANDS.iter().any(|(_, id)| *id == "remove-repo"));
    }

    #[test]
    fn uncommitted_status_letter_prefers_worktree_change() {
        let untracked = StatusEntry {
            index_status: '?',
            worktree_status: '?',
            path: "new.txt".to_string(),
        };
        assert_eq!(status_entry_letter(&untracked), 'A');

        let modified = StatusEntry {
            index_status: ' ',
            worktree_status: 'M',
            path: "a.txt".to_string(),
        };
        assert_eq!(status_entry_letter(&modified), 'M');

        let staged = StatusEntry {
            index_status: 'D',
            worktree_status: ' ',
            path: "gone.txt".to_string(),
        };
        assert_eq!(status_entry_letter(&staged), 'D');
    }

    #[test]
    fn file_tree_groups_and_optionally_compacts_folders() {
        let files = vec![
            changed("a/b/c.txt"),
            changed("a/b/d.txt"),
            changed("e.txt"),
        ];

        let rows = build_tree_rows(&files, false);
        let names: Vec<(usize, bool, &str)> = rows
            .iter()
            .map(|row| (row.depth, row.is_dir, row.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                (0, true, "a"),
                (1, true, "b"),
                (2, false, "c.txt"),
                (2, false, "d.txt"),
                (0, false, "e.txt"),
            ]
        );

        let compact = build_tree_rows(&files, true);
        let compact_names: Vec<(usize, bool, &str)> = compact
            .iter()
            .map(|row| (row.depth, row.is_dir, row.name.as_str()))
            .collect();
        assert_eq!(
            compact_names,
            vec![
                (0, true, "a/b"),
                (1, false, "c.txt"),
                (1, false, "d.txt"),
                (0, false, "e.txt"),
            ]
        );
    }

    fn changed(path: &str) -> ChangedFile {
        ChangedFile {
            status: 'M',
            added: 1,
            removed: 0,
            path: path.to_string(),
        }
    }
}
