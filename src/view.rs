//! The main gpui view: a commit graph for the active repository plus a
//! command palette (Cmd+P) to search and switch between repositories.

use gpui::{
    AnyElement, App, Context, FocusHandle, IntoElement, KeyDownEvent, Render, Window, div, h_flex,
    prelude::*, px, rgb, rgba, uniform_list, v_flex,
};

use crate::discovery::Repo;
use crate::git::{self, Commit};
use crate::layout;

const COMMIT_LIMIT: usize = 2000;
const LANE_WIDTH: f32 = 14.0;

/// Colors cycled through for lanes, mirroring a typical git graph palette.
const LANE_COLORS: [u32; 8] = [
    0xe06c75, 0x61afef, 0x98c379, 0xe5c07b, 0xc678dd, 0x56b6c2, 0xd19a66, 0xabb2bf,
];

pub struct GraphView {
    repos: Vec<Repo>,
    active: usize,
    commits: Vec<Commit>,
    branch: Option<String>,
    error: Option<String>,
    palette: Palette,
    focus_handle: FocusHandle,
}

#[derive(Default)]
struct Palette {
    open: bool,
    query: String,
    selected: usize,
}

impl GraphView {
    pub fn new(repos: Vec<Repo>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let mut this = Self {
            repos,
            active: 0,
            commits: Vec::new(),
            branch: None,
            error: None,
            palette: Palette::default(),
            focus_handle,
        };
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.commits.clear();
        self.error = None;
        self.branch = None;

        let Some(repo) = self.repos.get(self.active) else {
            return;
        };

        self.branch = git::head_branch(&repo.path);
        match git::log(&repo.path, COMMIT_LIMIT) {
            Ok(mut commits) => {
                layout::assign_lanes(&mut commits);
                self.commits = commits;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn filtered_repos(&self) -> Vec<usize> {
        let query = self.palette.query.to_lowercase();
        self.repos
            .iter()
            .enumerate()
            .filter(|(_, repo)| {
                query.is_empty()
                    || repo.name.to_lowercase().contains(&query)
                    || repo
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
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

    fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let cmd = keystroke.modifiers.platform;

        if cmd && keystroke.key == "p" {
            self.palette.open = !self.palette.open;
            self.palette.query.clear();
            self.palette.selected = 0;
            cx.notify();
            return;
        }
        if cmd && keystroke.key == "q" {
            cx.quit();
            return;
        }
        if cmd && keystroke.key == "r" {
            self.load(cx);
            return;
        }

        if !self.palette.open {
            return;
        }

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
                if !cmd
                    && !keystroke.modifiers.control
                    && let Some(character) = &keystroke.key_char
                {
                    self.palette.query.push_str(character);
                    self.palette.selected = 0;
                }
            }
        }
        cx.notify();
    }

    fn render_palette(&self) -> AnyElement {
        let filtered = self.filtered_repos();
        let selected = self.palette.selected.min(filtered.len().saturating_sub(1));

        let items: Vec<AnyElement> = filtered
            .iter()
            .enumerate()
            .map(|(position, &repo_index)| {
                let repo = &self.repos[repo_index];
                let is_selected = position == selected;
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .when(is_selected, |this| this.bg(rgb(0x2f4f6f)))
                    .child(
                        div()
                            .text_color(rgb(0xf0f0f0))
                            .child(repo.name.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x9aa0a6))
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

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(px(120.))
            .bg(rgba(0x000000cc))
            .child(
                v_flex()
                    .w(px(560.))
                    .max_h(px(420.))
                    .bg(rgb(0x1f1f1f))
                    .rounded_md()
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .px_3()
                            .py_2()
                            .text_color(rgb(0xe0e0e0))
                            .border_b_1()
                            .border_color(rgb(0x333333))
                            .child(query_line),
                    )
                    .child(v_flex().w_full().overflow_y_scroll().children(items)),
            )
            .into_any_element()
    }
}

impl Render for GraphView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let repo_name = self
            .repos
            .get(self.active)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| "no repository".to_string());

        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_3()
            .bg(rgb(0x1e1e1e))
            .border_b_1()
            .border_color(rgb(0x333333))
            .child(div().text_color(rgb(0xf0f0f0)).child(repo_name))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9aa0a6))
                    .child(self.branch.clone().unwrap_or_default()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6b7280))
                    .child(format!("{} repos · ⌘P switch · ⌘R reload", self.repos.len())),
            );

        let lane_area = (self.commits.iter().map(|c| c.lane).max().unwrap_or(0) + 1) as f32
            * LANE_WIDTH
            + 8.;
        let lane_area_px = px(lane_area);

        let body: AnyElement = if let Some(error) = &self.error {
            div()
                .p_4()
                .text_color(rgb(0xff6b6b))
                .child(error.clone())
                .into_any_element()
        } else {
            let rows = self.commits.clone();
            uniform_list("commits", rows.len(), move |range, _window, _cx| {
                range
                    .map(|index| {
                        let commit = &rows[index];
                        let color = rgb(LANE_COLORS[commit.lane % LANE_COLORS.len()]);
                        h_flex()
                            .h(px(22.))
                            .w_full()
                            .items_center()
                            .child(
                                div().w(lane_area_px).flex().items_center().child(
                                    div()
                                        .ml(px(commit.lane as f32 * LANE_WIDTH))
                                        .w(px(8.))
                                        .h(px(8.))
                                        .rounded_full()
                                        .bg(color),
                                ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .text_color(rgb(0xd0d0d0))
                                    .child(format!("{}  {}", commit.short_sha(), commit.subject)),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            })
            .flex_1()
            .into_any_element()
        };

        let palette = self.palette.open.then(|| self.render_palette());

        v_flex()
            .size_full()
            .bg(rgb(0x141414))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(header)
            .child(body)
            .when_some(palette, |this, palette| this.child(palette))
    }
}
