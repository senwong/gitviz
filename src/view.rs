//! The main gpui view.
//!
//! Aimed at feature parity with mhutchie/vscode-git-graph: a multi-repository
//! commit graph with refs, uncommitted changes, stashes, columns, a commit
//! detail panel, commit comparison, code review tracking, a branch filter, a
//! find box, a repository palette and a right-click menu of git operations.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, Pixels, Render, Window, div, h_flex, point, prelude::*, px,
    uniform_list, v_flex,
};

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

pub struct GraphView {
    repos: Vec<Repo>,
    roots: Vec<std::path::PathBuf>,
    repo_depth: usize,
    active: usize,
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
    detail: Option<CommitDetail>,
    detail_sha: Option<String>,
    compare_files: Vec<ChangedFile>,
    tags: Vec<git::TagDetail>,
    file_tree: bool,
    compact_folders: bool,
    show_remote_heads: bool,
    fetch_prune: bool,
    fetch_prune_tags: bool,
    color_preset: usize,
    date_width: f32,
    author_width: f32,
    commit_width: f32,
    diff: Option<DiffView>,
    review: ReviewStore,
    hovered: Option<RowKind>,
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
    ("Toggle stashes", "toggle-stashes"),
    ("Toggle uncommitted changes", "toggle-uncommitted"),
    ("Branch filter", "branch-filter"),
    ("Settings", "settings"),
    ("Find", "find"),
    ("Toggle theme", "theme"),
    ("Load more commits", "load-more"),
    ("Export config", "export-config"),
    ("End all code reviews", "end-reviews"),
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
}

struct DiffView {
    title: String,
    text: Arc<String>,
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
    Push,
    CopySha,
    CopyMessage,
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
            MenuAction::Push => "Push Branch",
            MenuAction::CopySha => "Copy SHA",
            MenuAction::CopyMessage => "Copy Commit Message",
            MenuAction::StashApply => "Apply Stash",
            MenuAction::StashPop => "Pop Stash",
            MenuAction::StashDrop => "Drop Stash",
            MenuAction::StashBranch => "Create Branch From Stash…",
            MenuAction::StashChanges => "Stash Changes",
            MenuAction::DiscardChanges => "Discard Changes (reset --hard)",
        }
    }
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
            repo_depth: crate::discovery::DEFAULT_DEPTH,
            active: 0,
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
            detail: None,
            detail_sha: None,
            compare_files: Vec::new(),
            tags: Vec::new(),
            file_tree: false,
            compact_folders: true,
            show_remote_heads: false,
            fetch_prune: false,
            fetch_prune_tags: false,
            color_preset: 0,
            date_width: 150.,
            author_width: 130.,
            commit_width: 80.,
            diff: None,
            review: ReviewStore::load(),
            hovered: None,
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
            theme: Theme::dark(),
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
        self.compare = None;
        self.compare_files.clear();
        self.selected = None;
        self.head_ancestors.clear();
        self.status.clear();
        self.stashes.clear();
        self.rows_dirty = true;

        let Some(repo) = self.active_repo().cloned() else {
            return;
        };

        self.branch = git::head_branch(&repo.path);
        self.signature = None;
        self.signature_details = None;
        self.branches_containing.clear();
        self.tags_containing.clear();
        self.matches.clear();
        self.match_cursor = 0;
        self.head_ancestors = git::head_ancestors(&repo.path, 50_000);
        self.branch_filter.all = git::local_branches(&repo.path);
        self.remotes = git::remotes(&repo.path);
        self.remote_info = git::hosting_remote(&repo.path);
        self.filter.use_mailmap = self.use_mailmap;
        self.filter.include_reflogs = self.include_reflogs;
        self.filter.remote_heads = self.show_remote_heads;
        self.tags = git::tags_with_details(&repo.path);
        if self.show_uncommitted {
            self.status = git::status(&repo.path, self.include_untracked);
        }
        if self.show_stashes {
            self.stashes = git::stashes(&repo.path);
        }

        match git::log(&repo.path, self.loaded, &self.filter) {
            Ok(mut commits) => {
                layout::assign_lanes(&mut commits);
                self.commits = commits;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
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
        self.compare_files.clear();
        let Some(commit) = self.commits.get(commit_index) else {
            return;
        };
        let sha = commit.sha.clone();
        self.detail_sha = Some(sha.clone());
        if let Some(repo) = self.active_repo().cloned() {
            if let Some(compare) = self.compare.and_then(|index| self.commits.get(index)) {
                let compare_sha = compare.sha.clone();
                self.compare_files = git::compare_files(&repo.path, &compare_sha, &sha);
            } else {
                self.detail = git::commit_detail(&repo.path, &sha).ok();
            }
            self.signature = git::signature_status(&repo.path, &sha);
            self.signature_details = git::signature_details(&repo.path, &sha);
            self.branches_containing = git::branches_containing(&repo.path, &sha);
            self.tags_containing = git::tags_containing(&repo.path, &sha);
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
                self.detail = None;
                let files: Vec<ChangedFile> = self
                    .status
                    .iter()
                    .map(|entry| ChangedFile {
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
                let message = self
                    .stashes
                    .get(index)
                    .map(|stash| stash.message.clone())
                    .unwrap_or_default();
                self.detail = Some(CommitDetail {
                    message: format!("stash@{{{index}}}: {message}"),
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
                        self.commands.open = !self.commands.open;
                        self.commands.query.clear();
                        self.commands.selected = 0;
                    } else {
                        self.palette.open = !self.palette.open;
                        self.palette.query.clear();
                        self.palette.selected = 0;
                    }
                }
                "g" => {
                    self.jump_match(if keystroke.modifiers.shift { -1 } else { 1 }, cx);
                    return;
                }
                "up" => {
                    if let Some(index) = self.selected_commit_index()
                        && let Some(parent) = find_parent_index(&self.commits, index)
                    {
                        self.select_row(RowKind::Commit(parent), cx);
                    }
                    return;
                }
                "down" => {
                    if let Some(index) = self.selected_commit_index()
                        && let Some(child) = find_child_index(&self.commits, index)
                    {
                        self.select_row(RowKind::Commit(child), cx);
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
        self.load(cx);
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
                ],
            ),
            RowKind::Uncommitted => (
                MenuContext::Uncommitted,
                vec![MenuAction::StashChanges, MenuAction::DiscardChanges],
            ),
        };
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
        let theme = self.theme.clone();
        let weak = cx.weak_entity();

        let repo_name = self
            .active_repo()
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| "no repository".to_string());

        let chip = |label: &'static str, on: bool, id: &'static str, weak: gpui::WeakEntity<Self>, theme: Theme| {
            div()
                .id(id)
                .px_2()
                .py_0p5()
                .rounded_md()
                .text_sm()
                .when(on, |this| this.bg(theme.accent).text_color(theme.bg))
                .when(!on, |this| this.text_color(theme.text_muted))
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
            .child(chip("Refresh", false, "refresh", weak.clone(), theme.clone()))
            .child(chip("Push", false, "push", weak.clone(), theme.clone()))
            .child(chip("PR", false, "pr", weak.clone(), theme.clone()))
            .child(chip("Load more", false, "load-more", weak.clone(), theme.clone()))
            .child(chip("Theme", false, "theme", weak, theme.clone()));

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
        };

        let body: AnyElement = if let Some(error) = &self.error {
            div().p_4().text_color(theme.error).child(error.clone()).into_any_element()
        } else {
            let ctx = Arc::new(row_ctx);
            let weak = cx.weak_entity();
            uniform_list("commits", ctx.rows.len(), move |range, _window, _cx| {
                range
                    .map(|position| ctx.render_row(position, weak.clone()))
                    .collect::<Vec<_>>()
            })
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

        let _ = window;

        v_flex()
            .size_full()
            .bg(theme.bg)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(header)
            .children(search_bar)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(body)
                    .when_some(detail, |this, detail| this.child(detail)),
            )
            .child(self.render_footer())
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
            "pr" => {
                let branch = self.branch.clone().unwrap_or_default();
                let base = self
                    .active_repo()
                    .and_then(|repo| git::default_branch(&repo.path))
                    .unwrap_or_else(|| "main".to_string());
                if let Some(info) = self.remote_info.clone() {
                    let url = info.pr_url(
                        &base,
                        &branch,
                        &format!("Merge {branch} into {base}"),
                    );
                    let _ = git::open_url(&url);
                }
            }
            "load-more" => {
                self.loaded = (self.loaded + 500).min(COMMIT_LIMIT);
                self.load(cx);
            }
            "theme" => {
                self.theme = self.theme.toggled();
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

        let files: Vec<AnyElement> = if comparing {
            self.compare_files
                .iter()
                .map(|file| render_file_row(file, &file.path, &weak, &theme, None))
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
                                Some((sha.clone(), reviewed)),
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
                    render_file_row(file, &file.path, &weak, &theme, Some((sha.clone(), reviewed)))
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
                    .child(avatar_circle(&detail.author, &theme))
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
            .child(v_flex().w_full().overflow_y_scroll().children(files))
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
                                emoji::replace_shortcodes(&span.text)
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
                        emoji::replace_shortcodes(line)
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
                let pr = info.pr_url(&base, &branch, &format!("Merge {branch} into {base}"));
                buttons.push(
                    action_button("create-pr", "Create PR", &theme, move |_cx| {
                        let _ = git::open_url(&pr);
                    })
                    .into_any_element(),
                );
            }
            for issue in find_issues(&message) {
                let url = info.issue_url(&issue);
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
                    .id(("command", id))
                    .w_full()
                    .px_3()
                    .py_1()
                    .when(is_selected, |this| this.bg(theme.selected))
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
            v_flex().w_full().overflow_y_scroll().children(items).into_any_element(),
        ])
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
                    .when(is_selected, |this| this.bg(theme.selected))
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
            v_flex().w_full().overflow_y_scroll().children(items).into_any_element(),
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
        let items: Vec<AnyElement> = self
            .branch_filter
            .all
            .iter()
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .map(|name| {
                let checked = self.branch_filter.selected.contains(name);
                let name = name.clone();
                let weak = weak.clone();
                let theme = theme.clone();
                h_flex()
                    .id(format!("branch-{}", name))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        let name = name.clone();
                        weak.update(cx, |this, cx| {
                            if !this.branch_filter.selected.remove(&name) {
                                this.branch_filter.selected.insert(name);
                            }
                            this.rows_dirty = true;
                            cx.notify();
                        })
                        .ok();
                    })
                    .child(
                        div()
                            .text_color(if checked { theme.accent } else { theme.text_muted })
                            .child(if checked { "[x]" } else { "[ ]" }),
                    )
                    .child(div().text_color(theme.text).child(name))
                    .into_any_element()
            })
            .collect();

        overlay(theme.clone(), 120., 420., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child("Filter branches (click to toggle)")
                .into_any_element(),
            v_flex().w_full().overflow_y_scroll().children(items).into_any_element(),
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
            ("Show remote HEAD refs", self.show_remote_heads, "remote-heads"),
            ("Only tag commits", self.filter.only_tags, "only-tags"),
            ("Fetch: prune", self.fetch_prune, "fetch-prune"),
            ("Fetch: prune tags", self.fetch_prune_tags, "fetch-prune-tags"),
            ("File tree in details", self.file_tree, "file-tree"),
            ("Compact folders", self.compact_folders, "compact-folders"),
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
            let theme_row = theme.clone();
            let name_fetch = name.clone();
            let name_prune = name.clone();
            let name_remove = name.clone();
            items.push(
                h_flex()
                    .id(format!("remote-{}", name))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .child(div().flex_1().text_color(theme.text).child(name.clone()))
                    .child(action_button(
                        format!("remote-fetch-{name}"),
                        "Fetch",
                        &theme_row,
                        move |cx| {
                            weak_fetch
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::fetch_remote(&repo.path, &name_fetch),
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
                            weak_prune
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::prune_remote(&repo.path, &name_prune),
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
                            weak_remove
                                .update(cx, |this, cx| {
                                    this.run_op(
                                        move |repo| git::remove_remote(&repo.path, &name_remove),
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
            v_flex().w_full().overflow_y_scroll().children(items).into_any_element(),
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
                    .id(("menu", label.clone()))
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
            "remote-heads" => {
                self.show_remote_heads = !self.show_remote_heads;
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
                .rounded_md()
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
    review: Option<(String, bool)>,
) -> AnyElement {
    let path = file.path.clone();
    let weak_click = weak.clone();
    let weak_review = weak.clone();
    let weak_copy = weak.clone();
    let weak_open = weak.clone();
    let weak_rev = weak.clone();
    let path_copy = file.path.clone();
    let path_open = file.path.clone();
    let path_rev = file.path.clone();
    let theme = theme.clone();
    let (sha, reviewed) = review.map(|(sha, reviewed)| (sha, reviewed)).unwrap_or_default();
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
        .child(status_letter(file.status, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.text)
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
                            .update(cx, |this, cx| this.toggle_reviewed(&sha, &file.path, cx))
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
        .into_any_element()
}

impl GraphView {
    fn open_diff(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(sha) = self.detail_sha.clone() else {
            return;
        };
        let compare_sha = self
            .compare
            .and_then(|index| self.commits.get(index))
            .map(|commit| commit.sha.clone());
        if let Some(repo) = self.active_repo() {
            let (text, title) = match compare_sha {
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
            cx.notify();
        }
    }

    fn copy_path(&mut self, path: &str, cx: &mut App) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.to_string()));
    }

    fn open_file(&mut self, path: &str, _cx: &mut Context<Self>) {
        if let Some(repo) = self.active_repo().cloned() {
            let _ = git::open_path(&repo.path.join(path));
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
        }
    }

    fn render_footer(&self) -> AnyElement {
        let theme = self.theme.clone();
        let text = match self.hovered {
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
                    .update(cx, |this, cx| this.open_menu(row_kind, event.position.x, event.position.y))
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
                paint_lanes(&mut *window, bounds, &commit, &colors);
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
            emoji::replace_shortcodes(&commit.subject)
        } else {
            commit.subject.clone()
        };
        let date = match self.date_mode {
            DateMode::Author => commit.author_date.clone(),
            DateMode::Commit => commit.commit_date.clone(),
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
                    .update(cx, |this, cx| this.open_menu(RowKind::Commit(index), event.position.x, event.position.y))
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

fn avatar_circle(name: &str, theme: &Theme) -> AnyElement {
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let hash = name
        .bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));
    let color = theme.lane_colors[(hash as usize) % theme.lane_colors.len()];
    div()
        .w(px(20.))
        .h(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(color)
        .text_sm()
        .text_color(theme.bg)
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

fn paint_lanes(window: &mut Window, bounds: Bounds<Pixels>, commit: &Commit, colors: &[gpui::Rgba; 8]) {
    let x = |lane: usize| bounds.origin.x + px(lane as f32 * LANE_WIDTH + LANE_WIDTH / 2.0);
    let top = bounds.origin.y;
    let middle = top + bounds.size.height / 2.0;
    let bottom = top + bounds.size.height;
    let color = colors[commit.lane % colors.len()];

    for &lane in &commit.through {
        if let Ok(path) = {
            let mut builder = gpui::PathBuilder::stroke(px(1.5));
            builder.move_to(point(x(lane), top));
            builder.line_to(point(x(lane), bottom));
            builder.build()
        } {
            window.paint_path(path, colors[lane % colors.len()]);
        }
    }
    if commit.top_line {
        if let Ok(path) = {
            let mut builder = gpui::PathBuilder::stroke(px(1.5));
            builder.move_to(point(x(commit.lane), top));
            builder.line_to(point(x(commit.lane), middle));
            builder.build()
        } {
            window.paint_path(path, color);
        }
    }
    if commit.bottom_line {
        if let Ok(path) = {
            let mut builder = gpui::PathBuilder::stroke(px(1.5));
            builder.move_to(point(x(commit.lane), middle));
            builder.line_to(point(x(commit.lane), bottom));
            builder.build()
        } {
            window.paint_path(path, color);
        }
    }
    for &lane in &commit.incoming {
        if let Ok(path) = {
            let mut builder = gpui::PathBuilder::stroke(px(1.5));
            let from = point(x(lane), top);
            let to = point(x(commit.lane), middle);
            builder.move_to(from);
            builder.curve_to(to, point(from.x, to.y));
            builder.build()
        } {
            window.paint_path(path, colors[lane % colors.len()]);
        }
    }
    for &lane in &commit.outgoing {
        if let Ok(path) = {
            let mut builder = gpui::PathBuilder::stroke(px(1.5));
            let from = point(x(commit.lane), middle);
            let to = point(x(lane), bottom);
            builder.move_to(from);
            builder.curve_to(to, point(to.x, from.y));
            builder.build()
        } {
            window.paint_path(path, color);
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
    fn find_matches_by_subject_author_and_sha() {
        let mut first = commit("abc123", "Fix SEO", "Sen");
        first.author_date = "2024-05-01 10:00".to_string();
        first.refs = vec!["v1.2.0".to_string()];
        let commits = vec![first, commit("def456", "Add graph", "Alice")];

        assert_eq!(find_matches(&commits, "seo"), vec![0]);
        assert_eq!(find_matches(&commits, "alice"), vec![1]);
        assert_eq!(find_matches(&commits, "abc"), vec![0]);
        assert_eq!(find_matches(&commits, "2024-05-01"), vec![0]);
        assert_eq!(find_matches(&commits, "v1.2"), vec![0]);
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
}
