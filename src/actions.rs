//! Actions triggered from the native menu bar (and key bindings).
//!
//! The menu bar in `main.rs` references these; `view.rs` registers handlers for
//! them on the root element.

use gpui::actions;

actions!(gitviz, [
    OpenRepository,
    OpenWorkspace,
    SaveWorkspace,
    OpenRecent,
    SwitchRepository,
    ToggleTheme,
    RefreshGraph,
    FindCommit,
    Minimize,
    Zoom,
    OpenHomepage,
    Quit,
]);
