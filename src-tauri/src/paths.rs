//! Phase 5: path validation at the IPC boundary.
//!
//! Caller-supplied paths arrive from the webview and must never reach the
//! filesystem un-checked. Every command that touches a repository file resolves
//! it through [`resolve_repo_path`], which confines the result to the
//! repository root.

use std::path::{Component, Path, PathBuf};

/// Resolve a caller-supplied, repository-relative path against `root`,
/// rejecting anything that would escape it.
///
/// Rules:
/// - the path must be relative (no absolute paths, no Windows drive prefixes);
/// - no `..` components — lexical escapes are rejected before touching disk;
/// - the file must exist, and its canonicalized form must still live under the
///   canonicalized root — this catches symlinks inside the repository that
///   point outside it.
///
/// Only the *comparison* uses canonicalized paths; the returned path is the
/// plain `root.join(requested)` so downstream consumers (the `lore` CLI is
/// picky about canonicalized roots) see the path as configured.
pub fn resolve_repo_path(root: &Path, requested: &str) -> Result<PathBuf, String> {
    if requested.is_empty() {
        return Err("empty path".to_string());
    }
    let rel = Path::new(requested);
    if rel.is_absolute() {
        return Err(format!("absolute paths are not allowed: {requested}"));
    }
    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!("path escapes the repository: {requested}"));
            }
            // RootDir / Prefix (e.g. `C:` on Windows) even when `is_absolute()`
            // was false, as with `C:foo` drive-relative paths.
            _ => return Err(format!("invalid path: {requested}")),
        }
    }

    let joined = root.join(rel);
    if !joined.exists() {
        return Err(format!("not found in repository: {requested}"));
    }

    // Symlink check: the real location must still be inside the real root.
    let canon_root = root
        .canonicalize()
        .map_err(|e| format!("repository root unavailable: {e}"))?;
    let canon = joined
        .canonicalize()
        .map_err(|e| format!("cannot resolve {requested}: {e}"))?;
    if !canon.starts_with(&canon_root) {
        return Err(format!("path escapes the repository: {requested}"));
    }

    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Assets")).unwrap();
        std::fs::write(dir.path().join("Assets/Hero.uasset"), b"binary").unwrap();
        dir
    }

    #[test]
    fn accepts_a_normal_relative_path() {
        let repo = repo();
        let resolved = resolve_repo_path(repo.path(), "Assets/Hero.uasset").unwrap();
        assert_eq!(resolved, repo.path().join("Assets/Hero.uasset"));
    }

    #[test]
    fn rejects_absolute_paths() {
        let repo = repo();
        let outside = repo.path().join("Assets/Hero.uasset");
        let err = resolve_repo_path(repo.path(), outside.to_str().unwrap()).unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn rejects_parent_dir_escapes() {
        let repo = repo();
        for evil in ["../secrets", "Assets/../../secrets", ".."] {
            let err = resolve_repo_path(repo.path(), evil).unwrap_err();
            assert!(err.contains("escapes"), "{evil}: {err}");
        }
    }

    #[test]
    fn rejects_missing_files() {
        let repo = repo();
        let err = resolve_repo_path(repo.path(), "Assets/Nope.uasset").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn rejects_empty_path() {
        let repo = repo();
        assert!(resolve_repo_path(repo.path(), "").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_leave_the_repository() {
        let repo = repo();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("sneaky")).unwrap();
        let err = resolve_repo_path(repo.path(), "sneaky").unwrap_err();
        assert!(err.contains("escapes"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn accepts_symlinks_that_stay_inside_the_repository() {
        let repo = repo();
        std::os::unix::fs::symlink(
            repo.path().join("Assets/Hero.uasset"),
            repo.path().join("alias"),
        )
        .unwrap();
        assert!(resolve_repo_path(repo.path(), "alias").is_ok());
    }
}
