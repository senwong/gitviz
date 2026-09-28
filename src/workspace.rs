//! A tiny "workspace file" format for gitviz.
//!
//! A workspace file lists paths (repositories or directories containing
//! repositories), one per line. Blank lines and lines starting with `#` are
//! ignored. This is intentionally dependency-free and easy to commit next to a
//! set of related repositories.

use std::path::{Path, PathBuf};

pub const FILE_SUFFIX: &str = ".gitviz-workspace";

/// Parses a workspace file into a list of raw paths.
pub fn parse(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Serializes a set of roots into the workspace file format.
pub fn serialize(roots: &[PathBuf]) -> String {
    let mut output = String::from("# gitviz workspace\n");
    for root in roots {
        output.push_str(&root.to_string_lossy());
        output.push('\n');
    }
    output
}

/// Expands a leading `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

/// Expands command-line arguments, loading any workspace files into their
/// listed roots.
pub fn expand_roots(args: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for arg in args {
        if arg.to_string_lossy().ends_with(FILE_SUFFIX) {
            if let Ok(text) = std::fs::read_to_string(arg) {
                roots.extend(parse(&text).into_iter().map(|line| expand_tilde(&line)));
            }
        } else {
            roots.push(arg.clone());
        }
    }
    roots
}

/// Where the last-used roots are remembered, so the app can reopen the same
/// repositories without re-scanning (possibly privacy-protected) directories.
fn state_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/gitviz/roots"))
}

/// Loads the roots remembered from the previous session.
pub fn load_default_roots() -> Vec<PathBuf> {
    let Some(path) = state_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse(&text)
        .into_iter()
        .map(|line| expand_tilde(&line))
        .filter(|path| path.exists())
        .collect()
}

/// Remembers the given roots for the next session.
pub fn save_default_roots(roots: &[PathBuf]) {
    let Some(path) = state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serialize(roots));
}

fn recent_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/gitviz/recent"))
}

/// The most recently opened repositories / workspace files (newest first).
pub fn load_recent() -> Vec<PathBuf> {
    let Some(path) = recent_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse(&text)
        .into_iter()
        .map(|line| expand_tilde(&line))
        .filter(|path| path.exists())
        .collect()
}

/// Records `opened` as the most recent entry (deduplicated, capped at 10).
pub fn remember_recent(opened: &Path) {
    let mut entries = load_recent();
    entries.retain(|path| path != opened);
    entries.insert(0, opened.to_path_buf());
    entries.truncate(10);
    let Some(path) = recent_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serialize(&entries));
}

fn settings_path(file: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/gitviz").join(file))
}

/// The theme name chosen last time, if any.
pub fn load_theme() -> Option<String> {
    let path = settings_path("theme")?;
    let text = std::fs::read_to_string(path).ok()?;
    let name = text.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Remembers the chosen theme name.
pub fn save_theme(name: &str) {
    let Some(path) = settings_path("theme") else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paths_ignoring_comments_and_blanks() {
        let text = "# a comment\n\n/one/repo\n  ~/two  \n# trailing\n";
        assert_eq!(parse(text), vec!["/one/repo", "~/two"]);
        assert!(parse("").is_empty());
    }

    #[test]
    fn serializes_with_header() {
        let roots = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(serialize(&roots), "# gitviz workspace\n/a\n/b\n");
    }

    #[test]
    fn round_trips_through_parse() {
        let roots = vec![PathBuf::from("/a"), PathBuf::from("/b/c")];
        let parsed: Vec<PathBuf> = parse(&serialize(&roots)).into_iter().map(PathBuf::from).collect();
        assert_eq!(parsed, roots);
    }

    #[test]
    fn expand_tilde_only_touches_home_prefix() {
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("relative"), PathBuf::from("relative"));
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(expand_tilde("~/x"), PathBuf::from(home).join("x"));
        }
    }

    #[test]
    fn expand_roots_reads_workspace_files() {
        let dir = std::env::temp_dir().join(format!("gitviz-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ws = dir.join(format!("repos{FILE_SUFFIX}"));
        std::fs::write(&ws, "# ws\n/one\n/two\n").unwrap();

        let roots = expand_roots(&[ws.clone(), PathBuf::from("/three")]);
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/one"),
                PathBuf::from("/two"),
                PathBuf::from("/three")
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
