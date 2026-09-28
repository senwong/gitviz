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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Commit;

    fn commit(sha: &str, parents: &[&str]) -> Commit {
        Commit {
            sha: sha.to_string(),
            parents: parents.iter().map(|parent| parent.to_string()).collect(),
            refs: Vec::new(),
            author: "Test".to_string(),
            timestamp: 0,
            commit_timestamp: 0,
            author_date: String::new(),
            commit_date: String::new(),
            subject: "subject".to_string(),
            lane: 0,
            through: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
            top_line: false,
            bottom_line: false,
        }
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let mut commits = vec![
            commit("c", &["b"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ];
        let lanes = assign_lanes(&mut commits);
        assert_eq!(lanes, 1);
        assert_eq!(
            commits.iter().map(|commit| commit.lane).collect::<Vec<_>>(),
            vec![0, 0, 0]
        );
        // the tip has no line above it, and a line below it
        assert!(!commits[0].top_line);
        assert!(commits[0].bottom_line);
        // the middle commit has both
        assert!(commits[1].top_line);
        assert!(commits[1].bottom_line);
        // the root has no line below
        assert!(!commits[2].bottom_line);
    }

    #[test]
    fn merge_allocates_a_new_lane() {
        let mut commits = vec![
            commit("m", &["a1", "b1"]),
            commit("a1", &["root"]),
            commit("b1", &["root"]),
            commit("root", &[]),
        ];
        assign_lanes(&mut commits);
        assert_eq!(commits[0].lane, 0);
        assert_eq!(commits[0].outgoing, vec![1]);
    }

    #[test]
    fn every_commit_gets_a_lane() {
        let mut commits = vec![
            commit("c1", &["c2", "c3"]),
            commit("c2", &["c4"]),
            commit("c3", &["c4"]),
            commit("c4", &[]),
        ];
        let lanes = assign_lanes(&mut commits);
        assert!(lanes >= 1);
        assert!(commits.iter().all(|commit| commit.lane < lanes));
    }
}
