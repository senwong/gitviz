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

use crate::discovery::Repo;
use crate::git::{self, ChangedFile, Commit, CommitDetail, LogFilter, ResetMode, StashEntry, StatusEntry};
use crate::layout;
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
    diff: Option<DiffView>,
    reviewed: HashSet<String>,
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
    theme: Theme,
    focus_handle: FocusHandle,
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
}

struct DiffView {
    title: String,
    text: Arc<String>,
}

#[derive(Clone, Copy)]
enum MenuAction {
    CherryPick,
    Revert,
    Merge,
    Rebase,
    ResetSoft,
    ResetMixed,
    ResetHard,
    Checkout,
    CreateBranch,
    CreateTag,
    Push,
    CopySha,
    CopyMessage,
    StashApply,
    StashPop,
    StashDrop,
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
            MenuAction::Revert => "Revert",
            MenuAction::Merge => "Merge into Current Branch",
            MenuAction::Rebase => "Rebase Current Branch onto This",
            MenuAction::ResetSoft => "Reset to Here (soft)",
            MenuAction::ResetMixed => "Reset to Here (mixed)",
            MenuAction::ResetHard => "Reset to Here (hard)",
            MenuAction::Checkout => "Checkout Commit",
            MenuAction::CreateBranch => "Create Branch Here…",
            MenuAction::CreateTag => "Create Tag Here…",
            MenuAction::Push => "Push Branch",
            MenuAction::CopySha => "Copy SHA",
            MenuAction::CopyMessage => "Copy Commit Message",
            MenuAction::StashApply => "Apply Stash",
            MenuAction::StashPop => "Pop Stash",
            MenuAction::StashDrop => "Drop Stash",
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
    pub fn new(repos: Vec<Repo>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let mut this = Self {
            repos,
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
            diff: None,
            reviewed: HashSet::new(),
            filter: LogFilter {
                branches: true,
                remotes: true,
                tags: true,
                first_parent: false,
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
            theme: Theme::dark(),
            focus_handle,
        };
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
        self.head_ancestors = git::head_ancestors(&repo.path, 50_000);
        self.branch_filter.all = git::local_branches(&repo.path);
        if self.show_uncommitted {
            self.status = git::status(&repo.path, self.include_untracked);
        }
        if self.show_stashes {
            self.stashes = git::stashes(&repo.path);
        }

        match git::log(&repo.path, COMMIT_LIMIT, &self.filter) {
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
        if let Some(repo) = self.active_repo() {
            if let Some(compare) = self.compare.and_then(|index| self.commits.get(index)) {
                let compare_sha = compare.sha.clone();
                self.compare_files = git::compare_files(&repo.path, &compare_sha, &sha);
            } else {
                self.detail = git::commit_detail(&repo.path, &sha).ok();
            }
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

    fn toggle_compare(&mut self, commit_index: usize, cx: &mut Context<Self>) {
        if self.compare == Some(commit_index) {
            self.compare = None;
            self.load_detail(commit_index, cx);
        } else {
            self.compare = Some(commit_index);
            self.load_detail(commit_index, cx);
        }
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
                    self.palette.open = !self.palette.open;
                    self.palette.query.clear();
                    self.palette.selected = 0;
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
            self.active = index;
            self.palette.open = false;
            self.palette.query.clear();
            self.palette.selected = 0;
            self.load(cx);
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
                        MenuAction::Revert,
                        MenuAction::Merge,
                        MenuAction::Rebase,
                        MenuAction::ResetSoft,
                        MenuAction::ResetMixed,
                        MenuAction::ResetHard,
                        MenuAction::Checkout,
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
                ],
            ),
            RowKind::Uncommitted => (MenuContext::Uncommitted, vec![]),
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
                MenuAction::Revert => self.run_op(move |repo| git::revert(&repo.path, &sha), cx),
                MenuAction::Merge => self.run_op(move |repo| git::merge(&repo.path, &sha), cx),
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
                _ => {}
            },
            MenuContext::Uncommitted => {
                if let MenuAction::StashDrop = action {
                    // no-op
                }
            }
        }
    }

    fn toggle_reviewed(&mut self, commit_sha: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{commit_sha}\t{path}");
        if !self.reviewed.remove(&key) {
            self.reviewed.insert(key);
        }
        cx.notify();
    }
}

impl Render for GraphView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rows_dirty {
            self.rebuild_rows();
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
            loaded: self.loaded,
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
            .when_some(diff, |this, diff| this.child(diff))
            .when_some(palette, |this, palette| this.child(palette))
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
            "refresh" => self.run_op(|repo| git::fetch_all_tags(&repo.path), cx),
            "push" => self.run_op(|repo| git::push_current_branch(&repo.path), cx),
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
                .map(|file| render_file_row(file, &weak, &theme, None))
                .collect()
        } else {
            detail
                .files
                .iter()
                .map(|file| {
                    let reviewed = self.reviewed.contains(&format!("{sha}\t{}", file.path));
                    render_file_row(file, &weak, &theme, Some((sha.clone(), reviewed)))
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
                div()
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(format!("{} <{}>", detail.author, detail.email)),
            )
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme.text)
                    .child(detail.message.clone()),
            )
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
                        weak.update(cx, |this, cx| {
                            this.active = repo_index;
                            this.palette.open = false;
                            this.palette.query.clear();
                            this.load(cx);
                        })
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
        let mut items: Vec<AnyElement> = Vec::new();
        for (label, on, id) in [
            ("Date column", self.columns.date, "col-date"),
            ("Author column", self.columns.author, "col-author"),
            ("Commit column", self.columns.commit, "col-commit"),
            ("Include untracked files", self.include_untracked, "untracked"),
        ] {
            let weak = weak.clone();
            let theme = theme.clone();
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
                    .child(
                        div()
                            .text_color(if on { theme.accent } else { theme.text_muted })
                            .child(if on { "[x]" } else { "[ ]" }),
                    )
                    .child(div().text_color(theme.text).child(label))
                    .into_any_element(),
            );
        }
        overlay(theme.clone(), 140., 420., vec![
            div()
                .w_full()
                .px_3()
                .py_2()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child("Settings")
                .into_any_element(),
            v_flex().w_full().children(items).into_any_element(),
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
            _ => {}
        }
        cx.notify();
    }
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
    weak: &gpui::WeakEntity<GraphView>,
    theme: &Theme,
    review: Option<(String, bool)>,
) -> AnyElement {
    let path = file.path.clone();
    let weak_click = weak.clone();
    let weak_review = weak.clone();
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
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.text)
                .child(file.path.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.text_muted)
                .child(format!("+{} -{}", file.added, file.removed)),
        )
        .when(!sha.is_empty(), |this| {
            this.child(
                div()
                    .id("review")
                    .text_sm()
                    .text_color(if reviewed { theme.accent } else { theme.text_muted })
                    .on_click(move |_: &ClickEvent, window, cx| {
                        let _ = window;
                        cx.stop_propagation();
                        weak_review
                            .update(cx, |this, cx| {
                                this.toggle_reviewed(&sha, &file.path, cx)
                            })
                            .ok();
                    })
                    .child(if reviewed { "[x]" } else { "[ ]" }),
            )
        })
        .into_any_element()
}

impl GraphView {
    fn open_diff(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(sha) = self.detail_sha.clone() else {
            return;
        };
        if let Some(repo) = self.active_repo() {
            let text = git::file_diff(&repo.path, &sha, path);
            self.diff = Some(DiffView {
                title: format!("{} — {}", &sha[..sha.len().min(8)], path),
                text: Arc::new(text),
            });
            cx.notify();
        }
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
    loaded: usize,
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
        let colors = theme.lane_colors;
        let lane_area = (self.commits.iter().map(|c| c.lane).max().unwrap_or(0) + 1) as f32
            * LANE_WIDTH
            + 8.;
        let lane_area_px = px(lane_area);
        let commit = commit.clone();
        let is_compare = self.compare == Some(index);
        let is_ancestor = self.head_ancestors.contains(&commit.sha);

        let weak_right = weak.clone();
        let weak_up = weak.clone();
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

        let refs = if commit.refs.is_empty() {
            String::new()
        } else {
            format!("[{}] ", commit.refs.join(", "))
        };

        h_flex()
            .id(("commit", index))
            .h(px(ROW_HEIGHT))
            .w_full()
            .items_center()
            .when(is_selected, |this| this.bg(theme.selected))
            .when(is_compare, |this| this.bg(theme.hover))
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
                    .child(format!("{}{}  {}", refs, commit.short_sha(), commit.subject)),
            )
            .into_any_element()
    }
}

const LAYER_PREFIX: f32 = 24.0;

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
