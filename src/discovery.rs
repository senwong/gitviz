//! Discovers git repositories under the paths passed on the command line.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

/// Scans up to two directory levels below each root (the default depth).
pub fn discover(roots: &[PathBuf]) -> Vec<Repo> {
    discover_with_depth(roots, DEFAULT_DEPTH)
}

pub const DEFAULT_DEPTH: usize = 2;

/// For each root, treat it as a repository if it contains `.git`, and also scan
/// up to `max_depth` directory levels below it. Duplicates (e.g. a repository
/// and one of its nested repositories) are kept only once, keyed by canonical
/// path.
pub fn discover_with_depth(roots: &[PathBuf], max_depth: usize) -> Vec<Repo> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut repos = Vec::new();

    for root in roots {
        for candidate in candidates(root, max_depth) {
            if !candidate.join(".git").exists() {
                continue;
            }
            let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
            if !seen.insert(canonical.clone()) {
                continue;
            }
            let name = canonical
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| canonical.display().to_string());
            repos.push(Repo {
                name,
                path: canonical,
            });
        }
    }

    repos.sort_by_key(|repo| repo.name.to_lowercase());
    repos
}

fn candidates(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut paths = vec![root.to_path_buf()];
    if max_depth == 0 {
        return paths;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return paths;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        paths.extend(candidates(&path, max_depth - 1));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch_git(dir: &Path) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
    }

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gitviz-disc-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_repositories_up_to_depth() {
        let root = temp_root("depth");
        touch_git(&root); // depth 0
        touch_git(&root.join("one")); // depth 1
        touch_git(&root.join("one/two")); // depth 2
        touch_git(&root.join("one/two/three")); // depth 3

        let depth_two = discover_with_depth(std::slice::from_ref(&root), 2);
        assert_eq!(depth_two.len(), 3);

        let depth_three = discover_with_depth(std::slice::from_ref(&root), 3);
        assert_eq!(depth_three.len(), 4);

        let _ = std::fs::remove_dir_all(&root);
    }
}
