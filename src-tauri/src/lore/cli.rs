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
    /// Cached identity (resolved once via `lore auth info`).
    identity: tokio::sync::OnceCell<Identity>,
}

impl CliLoreClient {
    pub fn new(binary: PathBuf, repository: PathBuf) -> Self {
        Self {
            binary,
            repository,
            gate: tokio::sync::Mutex::new(()),
            identity: tokio::sync::OnceCell::new(),
        }
    }

    /// Resolve and cache the current identity. `lore auth info` is empty / errors
    /// when not logged in → `authenticated: false`.
    async fn identity(&self) -> Identity {
        self.identity
            .get_or_init(|| async {
                match self.run(&["auth", "info"]).await {
                    Ok(out) if !out.trim().is_empty() => parse_identity(&out),
                    _ => Identity {
                        user_id: String::new(),
                        name: String::new(),
                        authenticated: false,
                    },
                }
            })
            .await
            .clone()
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
        // Read status from the local store (offline = never blocks on the
        // remote). `--scan` walks the working tree so brand-new untracked files
        // are detected — essential for the file-watcher to surface edits a user
        // just made, since plain `status` only reports already-dirty files.
        let status_out = self.run_offline(&["status", "--scan"]).await?;
        let mut parsed = parse::parse_status(&status_out);
        // `lore status` can list a path more than once (e.g. untracked + dirty);
        // collapse to one entry per path, keeping first-seen order.
        {
            let mut seen = std::collections::HashSet::new();
            parsed.files.retain(|(_, p)| seen.insert(p.clone()));
        }

        // Lock state is server-authoritative (no offline form). Bound it tightly
        // so a slow/unreachable remote degrades to "unknown lock" rather than
        // stalling. Distinguish a successful (possibly empty) result from a
        // failure/timeout: on failure, locks are *unknown*, not *unlocked*.
        let lock_result = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            self.query_locks(),
        )
        .await
        .ok()
        .and_then(|r| r.ok());
        let locks_available = lock_result.is_some();
        let locks = lock_result.unwrap_or_default();
        let lock_by_path: std::collections::HashMap<&str, &Lock> =
            locks.iter().map(|l| (l.path.as_str(), l)).collect();

        let entries: Vec<FileEntry> = parsed
            .files
            .iter()
            .map(|(marker, path)| {
                let found = lock_by_path.get(path.as_str());
                let lock_state = if !locks_available {
                    LockState::Unknown
                } else {
                    match found {
                        Some(l) => l.state,
                        None => LockState::Unlocked,
                    }
                };
                let lock = found.map(|l| (*l).clone());
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
            locks_available,
        })
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        let out = self.run(&["lock", "query"]).await?;
        let identity = self.identity().await;
        let locks = parse::parse_lock_query(&out)
            .into_iter()
            .map(|p| {
                // Attribute me vs. other by comparing the lock owner to the
                // authenticated identity. Without login we can't tell, so fall
                // back to "locked by me" (single-user assumption).
                let state = if identity.authenticated && !owner_is_me(&p.owner, &identity) {
                    LockState::LockedByOther
                } else {
                    LockState::LockedByMe
                };
                Lock {
                    path: p.path,
                    state,
                    owner: Some(Author { name: p.owner, email: String::new() }),
                    instance_id: None,
                    acquired_at: None,
                    reason: None,
                }
            })
            .collect();
        Ok(locks)
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

    async fn stage(&self, paths: &[String]) -> LoreResult<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["stage"];
        args.extend(paths.iter().map(|s| s.as_str()));
        self.run(&args).await?;
        Ok(())
    }

    async fn unstage(&self, paths: &[String]) -> LoreResult<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["unstage"];
        args.extend(paths.iter().map(|s| s.as_str()));
        self.run(&args).await?;
        Ok(())
    }

    async fn commit(&self, message: &str) -> LoreResult<String> {
        let out = self.run(&["commit", message]).await?;
        Ok(out.trim().to_string())
    }

    async fn list_branches(&self) -> LoreResult<Vec<Branch>> {
        let out = self.run_offline(&["branch", "list"]).await?;
        Ok(parse::parse_branch_list(&out)
            .into_iter()
            .map(|(name, _current)| Branch {
                id: String::new(),
                name,
                latest_revision: String::new(),
                protected: false,
            })
            .collect())
    }

    async fn switch_branch(&self, name: &str) -> LoreResult<()> {
        self.run(&["branch", "switch", name]).await?;
        Ok(())
    }

    async fn create_branch(&self, name: &str) -> LoreResult<()> {
        self.run(&["branch", "create", name]).await?;
        Ok(())
    }

    async fn history(&self, limit: Option<u32>) -> LoreResult<Vec<Revision>> {
        let limit_s;
        let mut args = vec!["history"];
        if let Some(n) = limit {
            limit_s = n.to_string();
            args.push(&limit_s);
        }
        let out = self.run_offline(&args).await?;
        Ok(parse::parse_history(&out)
            .into_iter()
            .map(|r| Revision {
                id: r.signature,
                parents: vec![],
                message: r.message,
                author: Author { name: String::new(), email: String::new() },
                timestamp: rfc2822_to_iso(&r.date),
                tree_root: FragmentAddress { hash: String::new(), context: String::new() },
                is_merge: false,
            })
            .collect())
    }

    async fn sync(&self, revision: Option<String>) -> LoreResult<()> {
        let mut args = vec!["sync"];
        if let Some(rev) = &revision {
            args.push(rev);
        }
        self.run(&args).await?;
        Ok(())
    }

    async fn push(&self, branch: Option<String>) -> LoreResult<()> {
        let mut args = vec!["push"];
        if let Some(b) = &branch {
            args.push(b);
        }
        self.run(&args).await?;
        Ok(())
    }

    async fn current_identity(&self) -> LoreResult<Identity> {
        Ok(self.identity().await)
    }
}

/// True if a lock owner string refers to the authenticated identity.
fn owner_is_me(owner: &str, identity: &Identity) -> bool {
    let o = owner.trim();
    o == "<unknown>"
        || (!identity.user_id.is_empty() && o.contains(&identity.user_id))
        || (!identity.name.is_empty() && o.contains(&identity.name))
}

/// Parse `lore auth info` into an `Identity`. The exact layout depends on the
/// auth backend; we extract a user id and a display name best-effort and treat
/// any non-empty output as authenticated.
fn parse_identity(output: &str) -> Identity {
    let mut user_id = String::new();
    let mut name = String::new();
    for line in output.lines() {
        let l = line.trim();
        if let Some((k, v)) = l.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if key.contains("id") && user_id.is_empty() {
                user_id = val;
            } else if (key.contains("name") || key.contains("user") || key.contains("email"))
                && name.is_empty()
            {
                name = val;
            }
        }
    }
    if user_id.is_empty() && name.is_empty() {
        name = output.trim().lines().next().unwrap_or("").to_string();
    }
    Identity { user_id, name, authenticated: true }
}

/// RFC-2822 (`lore history` Date) -> ISO-8601; falls back to the raw string.
fn rfc2822_to_iso(date: &str) -> String {
    chrono::DateTime::parse_from_rfc2822(date)
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|_| date.to_string())
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
