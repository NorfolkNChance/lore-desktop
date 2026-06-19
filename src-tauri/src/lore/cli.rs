//! `CliLoreClient` — drives the real `lore` binary as a subprocess.
//!
//! Hard-won facts from running the real CLI (encoded here so they don't
//! regress):
//!   * Relative lock paths resolve against the process **cwd**, not
//!     `--repository`. We set `current_dir` to the working tree.
//!   * `--non-interactive` prevents auth prompts from hanging a spawned child.
//!   * There is no JSON output; success/failure is `[Error] …` + exit code.

use super::parse::{self, FileMarker};
use super::{ClientMode, LoreClient, LoreError, LoreResult};
use crate::models::*;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct CliLoreClient {
    binary: PathBuf,
    repository: PathBuf,
    /// Serializes subprocess invocations. The `lore` CLI isn't built for
    /// concurrent processes against one repo — parallel calls contend on the
    /// local store lock and can deadlock. A GUI fires several reads at once
    /// (status + locks on bootstrap), so we funnel them through one at a time.
    gate: tokio::sync::Mutex<()>,
}

impl CliLoreClient {
    pub fn new(binary: PathBuf, repository: PathBuf) -> Self {
        Self { binary, repository, gate: tokio::sync::Mutex::new(()) }
    }

    /// Run `lore <args>` with the repository as cwd, returning stdout on
    /// success. Errors come from a nonzero exit or an `[Error] …` line.
    ///
    /// Repository discovery is via cwd, not `--repository` (see
    /// `LoreConfig::discover`). A timeout guards against a server-side call
    /// (e.g. `lock query`) hanging and freezing the UI thread waiting on IPC.
    async fn run(&self, args: &[&str]) -> LoreResult<String> {
        self.run_inner(args, false, std::time::Duration::from_secs(20)).await
    }

    /// `--offline`: use only local data. Reads like `status` are served from the
    /// local store, which is instant and never blocks on a slow/unreachable
    /// remote — the right default for a local-first desktop UI.
    async fn run_offline(&self, args: &[&str]) -> LoreResult<String> {
        self.run_inner(args, true, std::time::Duration::from_secs(20)).await
    }

    async fn run_inner(
        &self,
        args: &[&str],
        offline: bool,
        timeout: std::time::Duration,
    ) -> LoreResult<String> {
        // One `lore` process at a time per repository.
        let _guard = self.gate.lock().await;

        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(&self.repository).arg("--non-interactive");
        if offline {
            cmd.arg("--offline");
        }
        // kill_on_drop so a cancelled (timed-out) call doesn't leave an orphan
        // holding the gate.
        cmd.args(args).kill_on_drop(true);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                LoreError::Cli(format!(
                    "lore {} timed out after {}s",
                    args.join(" "),
                    timeout.as_secs()
                ))
            })?
            .map_err(|e| LoreError::BinaryUnavailable(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let combined = format!("{stdout}\n{stderr}");

        if let Some(msg) = parse::parse_error(&combined) {
            return Err(classify(&msg));
        }
        if !output.status.success() {
            let msg = if stderr.trim().is_empty() { stdout.clone() } else { stderr };
            return Err(LoreError::Cli(msg.trim().to_string()));
        }
        Ok(stdout)
    }
}

/// Map a parsed `[Error]` message to a typed error.
fn classify(msg: &str) -> LoreError {
    if msg.to_ascii_lowercase().contains("repository not found") {
        LoreError::RepositoryNotFound(msg.to_string())
    } else {
        LoreError::Cli(msg.to_string())
    }
}

#[async_trait]
impl LoreClient for CliLoreClient {
    fn mode(&self) -> ClientMode {
        ClientMode::Cli
    }

    async fn version(&self) -> LoreResult<String> {
        let out = self.run(&["--version"]).await?;
        Ok(out.trim().to_string())
    }

    async fn status(&self) -> LoreResult<WorkspaceStatus> {
        // Read status from the local store: instant, and never blocks on the
        // remote. Everything the UI needs (branch, revision, file list) is local.
        let status_out = self.run_offline(&["status"]).await?;
        let mut parsed = parse::parse_status(&status_out);
        // `lore status` can list a path more than once (e.g. untracked + dirty);
        // collapse to one entry per path, keeping first-seen order.
        {
            let mut seen = std::collections::HashSet::new();
            parsed.files.retain(|(_, p)| seen.insert(p.clone()));
        }

        // Lock state is server-authoritative (no offline form). Bound it tightly
        // so a slow/unreachable remote degrades the file list to "unknown lock"
        // rather than stalling it. status() never fails on the lock merge.
        let locks = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            self.query_locks(),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
        let locked_paths: std::collections::HashSet<&str> =
            locks.iter().map(|l| l.path.as_str()).collect();

        let entries: Vec<FileEntry> = parsed
            .files
            .iter()
            .map(|(marker, path)| {
                let lock_state = if locked_paths.contains(path.as_str()) {
                    // The local single-identity CLI can't yet distinguish
                    // "me" vs "other"; treat any held lock as ours. Identity
                    // resolution arrives with `lore login` / the FFI client.
                    LockState::LockedByMe
                } else {
                    LockState::Unlocked
                };
                let lock = locks.iter().find(|l| l.path == *path).cloned();
                let kind = asset_kind_for(path);
                FileEntry {
                    path: path.clone(),
                    file_id: String::new(),
                    change: match marker {
                        FileMarker::Added => FileChange::Added,
                        FileMarker::Modified => FileChange::Modified,
                        FileMarker::Deleted => FileChange::Deleted,
                    },
                    staged: false,
                    dirty: true,
                    is_binary: !matches!(kind, AssetKind::Text),
                    asset_kind: kind,
                    size_bytes: file_size(&self.repository, path),
                    // Fragment count isn't exposed by the CLI; 0 = unknown until
                    // the FFI client can read the store index.
                    fragment_count: 0,
                    lock_state,
                    lock,
                }
            })
            .collect();

        let branch = Branch {
            id: String::new(),
            name: parsed.branch_name.unwrap_or_else(|| "main".into()),
            latest_revision: parsed.revision_hash.clone().unwrap_or_default(),
            protected: false,
        };
        let head_revision = Revision {
            id: parsed.revision_hash.unwrap_or_default(),
            parents: vec![],
            message: String::new(),
            author: Author { name: String::new(), email: String::new() },
            timestamp: String::new(),
            tree_root: FragmentAddress { hash: String::new(), context: String::new() },
            is_merge: false,
        };
        let counts = StatusCounts {
            staged: entries.iter().filter(|e| e.staged).count() as u32,
            modified: entries
                .iter()
                .filter(|e| matches!(e.change, FileChange::Modified))
                .count() as u32,
            locked_by_me: entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByMe)
                .count() as u32,
            locked_by_other: entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByOther)
                .count() as u32,
        };

        Ok(WorkspaceStatus {
            workspace_id: parsed.repository_id.unwrap_or_default(),
            branch,
            head_revision,
            entries,
            counts,
        })
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        let out = self.run(&["lock", "query"]).await?;
        Ok(parse::parse_lock_query(&out)
            .into_iter()
            .map(|p| Lock {
                path: p.path,
                state: LockState::LockedByMe,
                owner: Some(Author { name: p.owner, email: String::new() }),
                instance_id: None,
                acquired_at: None,
                reason: None,
            })
            .collect())
    }

    async fn lock_status(&self, path: &str) -> LoreResult<LockState> {
        let out = self.run(&["lock", "status", path]).await?;
        if parse::parse_lock_status(&out).is_empty() {
            Ok(LockState::Unlocked)
        } else {
            Ok(LockState::LockedByMe)
        }
    }

    async fn acquire_lock(&self, path: &str, reason: Option<String>) -> LoreResult<Lock> {
        let out = self.run(&["lock", "acquire", path]).await?;
        let acquired = parse::parse_lock_acquire(&out);
        if !acquired.iter().any(|p| p == path) {
            return Err(LoreError::Cli(format!(
                "lock acquire did not confirm {path} (got: {acquired:?})"
            )));
        }
        Ok(Lock {
            path: path.to_string(),
            state: LockState::LockedByMe,
            owner: None,
            instance_id: None,
            acquired_at: Some(now_iso8601()),
            reason,
        })
    }

    async fn release_lock(&self, path: &str) -> LoreResult<()> {
        self.run(&["lock", "release", path]).await?;
        Ok(())
    }
}

/// Best-effort file size from the working tree.
fn file_size(repo: &Path, rel: &str) -> u64 {
    std::fs::metadata(repo.join(rel)).map(|m| m.len()).unwrap_or(0)
}

/// Classify an asset by extension for the binary-first UI.
fn asset_kind_for(path: &str) -> AssetKind {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "umap" => AssetKind::Umap,
        "uasset" => AssetKind::Uasset,
        "png" | "tga" | "jpg" | "jpeg" | "exr" | "dds" => AssetKind::Texture,
        "wav" | "ogg" | "mp3" => AssetKind::Audio,
        "txt" | "ini" | "json" | "xml" | "cpp" | "h" | "rs" | "ts" | "md" | "toml" => {
            AssetKind::Text
        }
        "" => AssetKind::Binary,
        _ => AssetKind::Binary,
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}
