//! Parsers for `lore` CLI human-readable output.
//!
//! The 0.8.3 CLI has no `--json`/`--format` flag, so we parse text. To keep
//! that brittleness contained and honest, every parser is unit-tested against
//! output captured verbatim from the real binary running on a live repo (see
//! the `tests` module). When an FFI/native-crate backend lands, this whole file
//! is deleted, not migrated.

/// A line beginning with `[Error]` anywhere in stdout/stderr indicates failure.
/// Returns the message after the marker, if present.
pub fn parse_error(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("[Error]").map(|rest| rest.trim().to_string())
    })
}

/// Parsed `lore status` output.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedStatus {
    pub repository_id: Option<String>,
    pub branch_name: Option<String>,
    pub revision_hash: Option<String>,
    /// (change marker, repo-relative path) for each file entry. Directory-only
    /// entries (trailing `/`) are filtered out.
    pub files: Vec<(FileMarker, String)>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FileMarker {
    Added,
    Modified,
    Deleted,
}

pub fn parse_status(output: &str) -> ParsedStatus {
    let mut status = ParsedStatus::default();
    for line in output.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("Repository ") {
            status.repository_id = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("On branch ") {
            // "main revision 0 -> 0000…"
            if let Some((name, tail)) = rest.split_once(" revision ") {
                status.branch_name = Some(name.trim().to_string());
                if let Some((_, hash)) = tail.split_once("-> ") {
                    status.revision_hash = Some(hash.trim().to_string());
                }
            } else {
                status.branch_name = Some(rest.trim().to_string());
            }
        } else if let Some((marker, path)) = parse_file_line(line) {
            if !path.ends_with('/') {
                status.files.push((marker, path.to_string()));
            }
        }
    }
    status
}

/// Match a status file line: a single change marker, a space, then a path.
fn parse_file_line(line: &str) -> Option<(FileMarker, &str)> {
    let (marker, rest) = line.split_at(line.char_indices().nth(1)?.0);
    let path = rest.strip_prefix(' ')?;
    if path.is_empty() {
        return None;
    }
    let marker = match marker {
        "A" => FileMarker::Added,
        "M" => FileMarker::Modified,
        "D" => FileMarker::Deleted,
        _ => return None,
    };
    Some((marker, path))
}

/// One parsed lock entry. `when` is the raw timestamp/branch token following
/// the owner; the caller normalizes it.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedLock {
    pub path: String,
    pub owner: String,
    /// For `lock status`: an RFC-2822 timestamp. For `lock query`: a branch id.
    pub trailer: String,
}

/// Parse `lore lock status <path>`:
///   "Files locked for edit:"
///   "<path> by <owner> on <RFC-2822 date>"
/// Empty / no header => no locks.
pub fn parse_lock_status(output: &str) -> Vec<ParsedLock> {
    parse_lock_block(output, "Files locked for edit:", " on ")
}

/// Parse `lore lock query`:
///   "Locks found:"
///   "<path> by <owner> on branch <branch-id>"
pub fn parse_lock_query(output: &str) -> Vec<ParsedLock> {
    parse_lock_block(output, "Locks found:", " on branch ")
}

fn parse_lock_block(output: &str, header: &str, on_sep: &str) -> Vec<ParsedLock> {
    let mut locks = Vec::new();
    let mut in_block = false;
    for line in output.lines() {
        let line = line.trim();
        if line == header {
            in_block = true;
            continue;
        }
        if !in_block || line.is_empty() {
            continue;
        }
        // "<path> by <owner> on[ branch] <trailer>"
        let Some((path, rest)) = line.split_once(" by ") else {
            continue;
        };
        // rsplit so an owner containing the separator is unlikely to confuse us;
        // the trailer (date/branch id) never contains " on ".
        let Some((owner, trailer)) = rest.rsplit_once(on_sep) else {
            continue;
        };
        locks.push(ParsedLock {
            path: path.trim().to_string(),
            owner: owner.trim().to_string(),
            trailer: trailer.trim().to_string(),
        });
    }
    locks
}

/// Parse the paths echoed by `lore lock acquire`:
///   "Lock acquired on files:"
///   "<path>"
pub fn parse_lock_acquire(output: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_block = false;
    for line in output.lines() {
        let line = line.trim();
        if line == "Lock acquired on files:" {
            in_block = true;
            continue;
        }
        if in_block && !line.is_empty() {
            paths.push(line.to_string());
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured verbatim from `lore 0.8.3+201` on a live local repo.

    #[test]
    fn parses_error_marker() {
        assert_eq!(
            parse_error("[Error] Repository not found: /tmp/lore-test"),
            Some("Repository not found: /tmp/lore-test".to_string())
        );
        assert_eq!(parse_error("all good"), None);
    }

    #[test]
    fn parses_real_status() {
        let out = "Repository 019ee051a87f799390c6dcd69d0c4486\n\
                   On branch main revision 0 -> 0000000000000000000000000000000000000000000000000000000000000000\n\
                   Remote revision 0 -> 0000000000000000000000000000000000000000000000000000000000000000\n\
                   Local branch in sync with remote\n\
                   Untracked files:\n\
                   A Content/\n\
                   A Content/Maps/\n\
                   A Content/Maps/Volcano.umap\n";
        let s = parse_status(out);
        assert_eq!(s.repository_id.as_deref(), Some("019ee051a87f799390c6dcd69d0c4486"));
        assert_eq!(s.branch_name.as_deref(), Some("main"));
        assert_eq!(
            s.revision_hash.as_deref(),
            Some("0000000000000000000000000000000000000000000000000000000000000000")
        );
        // Directories filtered; only the real file remains.
        assert_eq!(s.files, vec![(FileMarker::Added, "Content/Maps/Volcano.umap".to_string())]);
    }

    #[test]
    fn parses_real_lock_status() {
        let out = "Files locked for edit:\n\
                   Content/Maps/Volcano.umap by <unknown> on Fri, 19 Jun 2026 14:41:23 +0000\n";
        let locks = parse_lock_status(out);
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].path, "Content/Maps/Volcano.umap");
        assert_eq!(locks[0].owner, "<unknown>");
        assert_eq!(locks[0].trailer, "Fri, 19 Jun 2026 14:41:23 +0000");
    }

    #[test]
    fn unlocked_status_is_empty() {
        assert!(parse_lock_status("").is_empty());
        assert!(parse_lock_status("\n").is_empty());
    }

    #[test]
    fn parses_real_lock_query() {
        let out = "Locks found:\n\
                   Content/Maps/Volcano.umap by <unknown> on branch e726318bbc3fd75ac8733a7e030cc35b\n";
        let locks = parse_lock_query(out);
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].path, "Content/Maps/Volcano.umap");
        assert_eq!(locks[0].owner, "<unknown>");
        assert_eq!(locks[0].trailer, "e726318bbc3fd75ac8733a7e030cc35b");
    }

    #[test]
    fn parses_real_lock_acquire() {
        let out = "Lock acquired on files:\nContent/Maps/Volcano.umap\n";
        assert_eq!(parse_lock_acquire(out), vec!["Content/Maps/Volcano.umap".to_string()]);
    }
}
