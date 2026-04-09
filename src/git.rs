use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::types::GitError;

pub struct DiffStat {
    pub files_changed: usize,
}

pub fn is_git_repo(dir: &Path) -> bool {
    if dir.join(".git").exists() {
        return true;
    }
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn has_commits(dir: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Count of files changed between `baseline` and the working tree (committed + unstaged).
pub fn diff_stat(dir: &Path, baseline: &str) -> Result<DiffStat, GitError> {
    let committed = git_stdout(dir, &["diff", "--stat", baseline])?;
    let unstaged = git_stdout(dir, &["diff", "--stat"])?;
    let files_changed = count_unique_files(&committed, &unstaged);
    Ok(DiffStat { files_changed })
}

/// Whether `file` has any changes relative to `baseline` (committed or unstaged).
pub fn file_changed(dir: &Path, baseline: &str, file: &str) -> Result<bool, GitError> {
    let committed = Command::new("git")
        .args(["diff", baseline, "--", file])
        .current_dir(dir)
        .output()
        .map_err(|e| GitError::new(format!("failed to run git diff: {e}")))?;

    if !committed.status.success() {
        return Err(GitError::new(format!(
            "git diff exited with status {}",
            committed.status
        )));
    }
    if !committed.stdout.is_empty() {
        return Ok(true);
    }

    let unstaged = Command::new("git")
        .args(["diff", "--", file])
        .current_dir(dir)
        .output()
        .map_err(|e| GitError::new(format!("failed to run git diff: {e}")))?;

    if !unstaged.status.success() {
        return Err(GitError::new(format!(
            "git diff exited with status {}",
            unstaged.status
        )));
    }
    Ok(!unstaged.stdout.is_empty())
}

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| GitError::new(format!("failed to spawn git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::new(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Union of unique filenames from two `git diff --stat` outputs.
fn count_unique_files(committed: &str, unstaged: &str) -> usize {
    let mut files: HashSet<String> = HashSet::new();

    for output in [committed, unstaged] {
        for line in output.lines() {
            // Per-file lines: " src/foo.rs | 5 ++"
            if let Some(name) = line.split(" | ").next() {
                if line.contains(" | ") {
                    files.insert(name.trim().to_string());
                }
            }
        }
    }

    if files.is_empty() {
        // Fall back to summary line: "3 files changed, ..."
        for output in [committed, unstaged] {
            if let Some(n) = parse_summary_count(output) {
                return n;
            }
        }
        return 0;
    }
    files.len()
}

fn parse_summary_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        let t = line.trim();
        if t.contains("file") && t.contains("changed") {
            t.split_whitespace().next()?.parse().ok()
        } else {
            None
        }
    })
}
