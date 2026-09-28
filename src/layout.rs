//! Assigns a lane (column) to every commit and computes the line segments to
//! draw for each row.
//!
//! Commits must be ordered newest first (as `git log` emits them), so parents
//! only ever appear at a later index than their children. Each row is drawn
//! independently: `through` lanes are vertical lines spanning the whole row,
//! `incoming` lanes curve into the commit dot, and `outgoing` lanes curve out
//! of the dot to the commit's non-first parents.

use std::collections::HashSet;

use crate::git::Commit;

/// Assigns lanes and drawing metadata on every commit. Returns lanes used.
pub fn assign_lanes(commits: &mut [Commit]) -> usize {
    // `lanes[i]` is the SHA lane `i` is currently waiting for.
    let mut lanes: Vec<Option<String>> = Vec::new();

    for commit in commits.iter_mut() {
        let before: Vec<usize> = active_lanes(&lanes);
        let arriving: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, waiting)| waiting.as_deref() == Some(commit.sha.as_str()))
            .map(|(index, _)| index)
            .collect();

        let lane = arriving
            .iter()
            .min()
            .copied()
            .unwrap_or_else(|| first_free_lane(&mut lanes));
        commit.lane = lane;

        let arriving_set: HashSet<usize> = arriving.iter().copied().collect();
        commit.incoming = arriving.iter().copied().filter(|&l| l != lane).collect();
        commit.top_line = arriving_set.contains(&lane);

        // The first parent continues in this lane.
        lanes[lane] = commit.parents.first().cloned();
        commit.bottom_line = commit.parents.first().is_some();

        // Additional parents get their own lanes.
        let mut outgoing = Vec::new();
        for parent in commit.parents.iter().skip(1) {
            if lanes
                .iter()
                .any(|waiting| waiting.as_deref() == Some(parent.as_str()))
            {
                continue;
            }
            let parent_lane = first_free_lane(&mut lanes);
            lanes[parent_lane] = Some(parent.clone());
            outgoing.push(parent_lane);
        }
        commit.outgoing = outgoing;

        let after: HashSet<usize> = active_lanes(&lanes).into_iter().collect();
        commit.through = before
            .into_iter()
            .filter(|l| *l != lane && !arriving_set.contains(l) && after.contains(l))
            .collect();
    }

    lanes.len().max(1)
}

fn active_lanes(lanes: &[Option<String>]) -> Vec<usize> {
    lanes
        .iter()
        .enumerate()
        .filter(|(_, waiting)| waiting.is_some())
        .map(|(index, _)| index)
        .collect()
}

fn first_free_lane(lanes: &mut Vec<Option<String>>) -> usize {
    if let Some(index) = lanes.iter().position(Option::is_none) {
        index
    } else {
        lanes.push(None);
        lanes.len() - 1
    }
}
