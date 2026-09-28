//! A standalone, multi-repository git graph viewer built on gpui.
//!
//! Usage:
//!   git-graph [PATH ...]
//!
//! Each PATH may be a repository or a directory containing repositories. With
//! no arguments the current directory is used. Press Cmd+P to search and switch
//! between the discovered repositories.

mod discovery;
mod git;
mod layout;
mod view;

use gpui::{App, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

fn main() {
    let roots: Vec<std::path::PathBuf> = {
        let args: Vec<std::path::PathBuf> = std::env::args().skip(1).map(Into::into).collect();
        if args.is_empty() {
            vec![std::env::current_dir().expect("failed to read current directory")]
        } else {
            args
        }
    };

    application().run(move |cx: &mut App| {
        let repos = discovery::discover(&roots);

        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| view::GraphView::new(repos, window, cx)),
        )
        .expect("failed to open window");

        cx.activate(true);
    });
}
