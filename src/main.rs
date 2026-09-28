//! A standalone, multi-repository git graph viewer built on gpui.
//!
//! Usage:
//!   gitviz [PATH ...]
//!
//! Each PATH may be a repository, a directory containing repositories, or a
//! `.gitviz-workspace` file. With no arguments the current directory is used
//! (or, when launched from Finder/`open`, the home directory). Press Cmd+P to
//! search and switch between the discovered repositories.

use std::path::PathBuf;

use gitviz::actions::{
    FindCommit, Minimize, OpenHomepage, OpenRecent, OpenRepository, OpenWorkspace, Quit,
    RefreshGraph, SaveWorkspace, SelectTheme, SwitchRepository, ToggleTheme, Zoom,
};
use gitviz::{discovery, view};
use gpui::{
    App, AppContext as _, Bounds, Menu, MenuItem, WindowBounds, WindowOptions, px, size,
};
use gpui_platform::application;

fn main() {
    // `--head` selects the HEAD commit on load (handy for screencasts/scripts).
    let select_head = std::env::args().any(|arg| arg == "--head");
    let roots: Vec<PathBuf> = {
        // macOS passes `-psn_<...>` when an app is launched from Finder/`open`;
        // it is not a path.
        let args: Vec<PathBuf> = std::env::args()
            .skip(1)
            .filter(|arg| !arg.starts_with("-psn_") && !arg.starts_with("--"))
            .map(Into::into)
            .collect();
        if args.is_empty() {
            default_root()
        } else {
            // Paths may include `.gitviz-workspace` files, which are expanded
            // into their listed roots.
            gitviz::workspace::expand_roots(&args)
        }
    };

    application().run(move |cx: &mut App| {
        let search_roots = roots.clone();
        let repos = discovery::discover(&roots);

        let (width, height) = gitviz::workspace::load_window_size().unwrap_or((1320., 860.));
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(560.))),
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| view::GraphView::new(repos, search_roots, select_head, window, cx))
            },
        )
        .expect("failed to open window");

        cx.set_menus(app_menus());
        cx.activate(true);
    });
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu::new("gitviz").items([
            MenuItem::action("About gitviz", OpenHomepage),
            MenuItem::separator(),
            MenuItem::action("Quit gitviz", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open Repository…", OpenRepository),
            MenuItem::action("Open Workspace…", OpenWorkspace),
            MenuItem::action("Save Workspace…", SaveWorkspace),
            MenuItem::separator(),
            MenuItem::action("Open Recent…", OpenRecent),
            MenuItem::separator(),
            MenuItem::action("Switch Repository…", SwitchRepository),
        ]),
        Menu::new("View").items([
            MenuItem::action("Select Theme…", SelectTheme),
            MenuItem::action("Next Theme", ToggleTheme),
            MenuItem::separator(),
            MenuItem::action("Refresh", RefreshGraph),
            MenuItem::action("Find", FindCommit),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
        Menu::new("Help").items([MenuItem::action("gitviz on GitHub", OpenHomepage)]),
    ]
}

/// The root to scan when no paths are given.
///
/// Order: the roots remembered from the previous session, then the current
/// directory (terminal launch). A Finder/`open` launch has `/` as its working
/// directory and no remembered roots on first run, so it starts on the welcome
/// screen. We never scan the whole home directory because that walks into
/// macOS-protected folders and network volumes, which triggers prompts.
fn default_root() -> Vec<PathBuf> {
    let remembered = gitviz::workspace::load_default_roots();
    if !remembered.is_empty() {
        return remembered;
    }
    match std::env::current_dir() {
        Ok(cwd) if cwd != PathBuf::from("/") => vec![cwd],
        _ => Vec::new(),
    }
}
