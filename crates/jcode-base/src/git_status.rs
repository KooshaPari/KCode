//! Git summary for the session context block.
//!
//! The summary feeds prompts, and it is requested on refresh paths that can tick
//! several times a second. Every probe forks `git`, and on Windows each fork is
//! scanned by Defender, so an unmemoised probe burned ~12 logical cores across
//! idle sessions. Two things prevent that: repo detection is a filesystem walk,
//! and the summary is cached per working directory for a short TTL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// TTL for the per-directory git summary below. The summary feeds the once-per-session
/// context block, so it does not need second-level freshness, and each probe forks up to
/// three `git` processes (branch, status). On Windows every fork is scanned by Defender,
/// so re-forking on a poll tick was burning ~12 logical cores across idle sessions.
const GIT_INFO_TTL: Duration = Duration::from_secs(10);

struct GitInfoCacheEntry {
    at: Instant,
    value: Option<String>,
}

fn git_info_cache() -> &'static Mutex<HashMap<PathBuf, GitInfoCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, GitInfoCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// True when `dir` (or any ancestor) holds a `.git` entry.
///
/// A worktree or submodule stores `.git` as a file, so `exists()` covers both
/// shapes. This is a pure filesystem walk: it replaces the previous
/// `git rev-parse --is-inside-work-tree` fork, which was the single most
/// frequent process spawn on a polling client.
pub fn is_inside_git_work_tree(dir: &Path) -> bool {
    let mut current = Some(dir);
    while let Some(path) = current {
        if path.join(".git").exists() {
            return true;
        }
        current = path.parent();
    }
    false
}

/// Get git branch and status summary
pub(crate) fn get_git_info(working_dir: Option<&Path>) -> Option<String> {
    if let Some(dir) = working_dir {
        return get_git_info_cached(dir, GIT_INFO_TTL, get_git_info_uncached);
    }
    let Ok(current_dir) = std::env::current_dir() else {
        return None;
    };
    get_git_info_cached(&current_dir, GIT_INFO_TTL, get_git_info_uncached)
}

/// Per-directory TTL cache around the git probe.
///
/// `probe` is injectable so tests can count how many times a real fork would
/// occur without running git.
pub(crate) fn get_git_info_cached(
    dir: &Path,
    ttl: Duration,
    probe: impl Fn(&Path) -> Option<String>,
) -> Option<String> {
    if let Ok(cache) = git_info_cache().lock()
        && let Some(entry) = cache.get(dir)
        && entry.at.elapsed() < ttl
    {
        return entry.value.clone();
    }

    let value = probe(dir);
    if let Ok(mut cache) = git_info_cache().lock() {
        cache.insert(
            dir.to_path_buf(),
            GitInfoCacheEntry {
                at: Instant::now(),
                value: value.clone(),
            },
        );
    }
    value
}

fn get_git_info_uncached(working_dir: &Path) -> Option<String> {
    // Cheap, fork-free pre-check: outside a repo there is nothing to ask git.
    if !is_inside_git_work_tree(working_dir) {
        return None;
    }

    let mut info = vec!["Git:".to_string()];

    // Current branch
    let mut branch_command = Command::new("git");
    branch_command.current_dir(working_dir);
    if let Ok(output) = branch_command.args(["branch", "--show-current"]).output()
        && output.status.success()
    {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            info.push(format!("  Branch: {}", branch));
        }
    }

    // Short status (modified files count)
    let mut status_command = Command::new("git");
    status_command.current_dir(working_dir);
    if let Ok(output) = status_command.args(["status", "--porcelain"]).output()
        && output.status.success()
    {
        let status = String::from_utf8_lossy(&output.stdout);
        let modified: Vec<&str> = status.lines().take(5).collect();
        if !modified.is_empty() {
            info.push(format!("  Modified: {} files", status.lines().count()));
            for file in modified {
                info.push(format!("    {}", file));
            }
            if status.lines().count() > 5 {
                info.push("    ...".to_string());
            }
        }
    }

    if info.len() > 1 {
        Some(info.join("\n"))
    } else {
        None
    }
}
