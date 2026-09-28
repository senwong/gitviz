//! Discovers git repositories under the paths passed on the command line.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

/// For each root, treat it as a repository if it contains `.git`, and also scan
/// up to two directory levels below it. Duplicates (e.g. a repository and one of
/// its nested repositories) are kept only once, keyed by canonical path.
pub fn discover(roots: &[PathBuf]) -> Vec<Repo> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut repos = Vec::new();

    for root in roots {
        for candidate in candidates(root) {
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

fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.to_path_buf()];
    let Ok(entries) = std::fs::read_dir(root) else {
        return paths;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        paths.push(path.clone());
        if let Ok(nested) = std::fs::read_dir(&path) {
            for nested_entry in nested.flatten() {
                let nested_path = nested_entry.path();
                if nested_path.is_dir() {
                    paths.push(nested_path);
                }
            }
        }
    }
    paths
}
