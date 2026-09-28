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
    App, AppContext as _, Bounds, Menu, MenuItem, WindowBounds, WindowOptions, point, px, size,
};
use gpui_platform::application;

fn main() {
    // `--head` selects the HEAD commit on load (handy for screencasts/scripts).
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }
    if raw_args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("gitviz {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let select_head = raw_args.iter().any(|arg| arg == "--head");
    let roots: Vec<PathBuf> = {
        // macOS passes `-psn_<...>` when an app is launched from Finder/`open`;
        // it is not a path.
        let args: Vec<PathBuf> = raw_args
            .iter()
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
        let repos = discovery::discover_with_depth(&roots, gitviz::workspace::load_depth());

        let bounds = window_bounds(cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(560.))),
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| view::GraphView::new(repos, search_roots, select_head, window, cx))
            },
        );
        if let Err(error) = window {
            eprintln!("gitviz: failed to open window: {error}");
            std::process::exit(1);
        }

        cx.set_menus(app_menus());
        cx.activate(true);
    });
}

/// The initial window bounds: restore the last geometry (clamped so it stays
/// on the display), else a centered default window.
fn window_bounds(cx: &App) -> Bounds<gpui::Pixels> {
    const DEFAULT: (f32, f32) = (1320., 860.);
    let Some((x, y, width, height)) = gitviz::workspace::load_window_geometry() else {
        return Bounds::centered(None, size(px(DEFAULT.0), px(DEFAULT.1)), cx);
    };
    let Some(visible) = cx.primary_display().map(|display| display.visible_bounds()) else {
        return Bounds::centered(None, size(px(width), px(height)), cx);
    };
    let vx = visible.origin.x.as_f32();
    let vy = visible.origin.y.as_f32();
    let vw = visible.size.width.as_f32();
    let vh = visible.size.height.as_f32();
    // Keep at least ~120px of the window on screen.
    let ox = x.clamp(vx - width + 120., vx + vw - 120.);
    let oy = y.clamp(vy, vy + vh - 60.);
    Bounds {
        origin: point(px(ox), px(oy)),
        size: size(px(width), px(height)),
    }
}

fn print_help() {
    println!(
        r#"gitviz {version} — a standalone, multi-repository git graph viewer

USAGE:
    gitviz [OPTIONS] [PATH ...]

ARGS:
    PATH ...    Repositories, directories containing repositories, or a
                `.gitviz-workspace` file. Defaults to the current directory
                (or the last session's repositories when launched from Finder).

OPTIONS:
    --head          Select the HEAD commit on load
    -h, --help      Print this help and exit
    -V, --version   Print the version and exit"#,
        version = env!("CARGO_PKG_VERSION")
    );
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
