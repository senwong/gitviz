//! Assigns a lane (column) to every commit so that a commit and its first
//! parent share a column, and merge parents get their own columns.
//!
//! Commits must be ordered newest first (as `git log` emits them), so parents
//! only ever appear at a later index than their children.

use crate::git::Commit;

/// Assigns `lane` on every commit. Returns the number of lanes used.
pub fn assign_lanes(commits: &mut [Commit]) -> usize {
    // `lanes[i]` is the SHA that lane `i` is currently waiting for, i.e. the
    // next commit that should be drawn in that column.
    let mut lanes: Vec<Option<String>> = Vec::new();

    for commit in commits.iter_mut() {
        let lane = lanes
            .iter()
            .position(|waiting| waiting.as_deref() == Some(commit.sha.as_str()))
            .unwrap_or_else(|| first_free_lane(&mut lanes));

        commit.lane = lane;

        // The first parent continues in the same lane.
        lanes[lane] = commit.parents.first().cloned();

        // Additional parents (merges) get their own lane if not already present.
        for parent in commit.parents.iter().skip(1) {
            if lanes
                .iter()
                .any(|waiting| waiting.as_deref() == Some(parent.as_str()))
            {
                continue;
            }
            let parent_lane = first_free_lane(&mut lanes);
            lanes[parent_lane] = Some(parent.clone());
        }
    }

    lanes.len().max(1)
}

fn first_free_lane(lanes: &mut Vec<Option<String>>) -> usize {
    if let Some(index) = lanes.iter().position(Option::is_none) {
        index
    } else {
        lanes.push(None);
        lanes.len() - 1
    }
}
