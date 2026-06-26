//! Lore integration layer.
//!
//! Phase 2 decision: wrap the `lore` CLI now, keep an FFI path to `liblore`
//! reserved behind the `liblore` cargo feature. All backends implement the
//! [`LoreClient`] trait, so the command layer and UI never learn which one is
//! active and the future FFI swap is a one-line change in [`build_client`].
//!
//! The CLI surface here is grounded in the real `lore 0.8.3` binary (verified
//! by running it against a live local repo), not guessed — see `parse.rs` for
//! the captured output formats the parsers are tested against.

pub mod cli;
#[cfg(feature = "liblore")]
pub mod ffi;
pub mod mock;
pub mod parse;

use crate::models::*;
use async_trait::async_trait;
use std::path::PathBuf;

/// Errors crossing the integration boundary. Kept dependency-free; the command
/// layer flattens these to `String` for the frontend.
#[derive(Debug, Clone)]
pub enum LoreError {
    /// No repository at the configured path.
    RepositoryNotFound(String),
    /// The `lore` binary could not be located/spawned.
    BinaryUnavailable(String),
    /// `lore` ran but reported an error (parsed from `[Error] …`).
    Cli(String),
    /// Output could not be parsed into the expected shape.
    Parse(String),
    /// I/O failure spawning or communicating with the process.
    Io(String),
}

impl std::fmt::Display for LoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoreError::RepositoryNotFound(p) => write!(f, "repository not found: {p}"),
            LoreError::BinaryUnavailable(m) => write!(f, "lore binary unavailable: {m}"),
            LoreError::Cli(m) => write!(f, "{m}"),
            LoreError::Parse(m) => write!(f, "could not parse lore output: {m}"),
            LoreError::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

impl std::error::Error for LoreError {}

impl From<std::io::Error> for LoreError {
    fn from(e: std::io::Error) -> Self {
        LoreError::Io(e.to_string())
    }
}

pub type LoreResult<T> = Result<T, LoreError>;

/// Which backend is serving requests — surfaced to the UI so it can show a
/// "mock data" banner when `lore` isn't available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientMode {
    /// Wrapping the real `lore` CLI against a live repository.
    Cli,
    /// Binding directly to liblore in-process (Phase D).
    Ffi,
    /// Static/stateful mock (no repository configured or `lore` not found).
    Mock,
}

/// The single seam every backend implements. Async because the CLI client
/// shells out and the future FFI client will do blocking work on a pool.
#[async_trait]
pub trait LoreClient: Send + Sync {
    fn mode(&self) -> ClientMode;

    /// `lore --version`.
    async fn version(&self) -> LoreResult<String>;

    /// Full working-tree status for the configured repository.
    async fn status(&self) -> LoreResult<WorkspaceStatus>;

    /// All locks on the current branch (`lore lock query`).
    async fn query_locks(&self) -> LoreResult<Vec<Lock>>;

    /// Lock disposition for one path (`lore lock status <path>`).
    async fn lock_status(&self, path: &str) -> LoreResult<LockState>;

    /// Acquire an exclusive lock (`lore lock acquire <path>`). `reason` is
    /// UI-side metadata — the 0.8.3 CLI has no `--reason` flag, so it's stored
    /// in the manager's cache, not passed through.
    async fn acquire_lock(&self, path: &str, reason: Option<String>) -> LoreResult<Lock>;

    /// Release a lock (`lore lock release <path>`).
    async fn release_lock(&self, path: &str) -> LoreResult<()>;

    /// Stage files for the next commit (`lore stage <paths>`).
    async fn stage(&self, paths: &[String]) -> LoreResult<()>;

    /// Unstage files (`lore unstage <paths>`).
    async fn unstage(&self, paths: &[String]) -> LoreResult<()>;

    /// Commit the staged revision (`lore commit <message>`). Returns the CLI's
    /// confirmation text.
    async fn commit(&self, message: &str) -> LoreResult<String>;

    // -- VCS workflow (Phase A) --------------------------------------------

    /// All branches (`lore branch list`).
    async fn list_branches(&self) -> LoreResult<Vec<Branch>>;

    /// Switch the working tree to a branch (`lore branch switch <name>`).
    async fn switch_branch(&self, name: &str) -> LoreResult<()>;

    /// Create a new branch (`lore branch create <name>`).
    async fn create_branch(&self, name: &str) -> LoreResult<()>;

    /// Revision history, newest first (`lore history`).
    async fn history(&self, limit: Option<u32>) -> LoreResult<Vec<Revision>>;

    /// Synchronize the working tree to a revision (`lore sync [revision]`).
    async fn sync(&self, revision: Option<String>) -> LoreResult<()>;

    /// Push commits to the remote (`lore push [branch]`).
    async fn push(&self, branch: Option<String>) -> LoreResult<()>;

    // -- Identity (Phase B1) -----------------------------------------------

    /// The authenticated user, used to attribute locks to me vs. others
    /// (`lore auth info`). Returns `authenticated: false` when not logged in.
    async fn current_identity(&self) -> LoreResult<Identity>;
}

/// Resolved configuration for talking to Lore.
#[derive(Debug, Clone)]
pub struct LoreConfig {
    /// Absolute path to the `lore` executable, if found.
    pub binary: Option<PathBuf>,
    /// Working-tree path passed as `--repository` and used as the child cwd
    /// (relative lock paths resolve against cwd, not `--repository`).
    pub repository: Option<PathBuf>,
}

impl LoreConfig {
    /// Discover configuration from the environment: `LORE_BIN` / PATH for the
    /// binary, `LORE_REPOSITORY` for the working tree.
    ///
    /// The repository path is used as the child process cwd and `lore`
    /// discovers the repository by walking up from there. We deliberately do
    /// NOT pass `--repository` or canonicalize the path: the repo instance is
    /// registered under whatever path it was created with, and forcing a
    /// canonical (`/tmp` -> `/private/tmp`) root makes server-side calls like
    /// `lore lock query` fail to resolve the instance. cwd-based discovery
    /// matches the path `lore` already knows.
    pub fn discover() -> Self {
        let repository = std::env::var_os("LORE_REPOSITORY").map(PathBuf::from);
        LoreConfig { binary: locate_binary(), repository }
    }
}

/// Look for the `lore` binary: `LORE_BIN`, then common install locations, then
/// rely on PATH resolution by the OS at spawn time.
fn locate_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LORE_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = vec![
        "/usr/local/bin/lore".into(),
        "/opt/homebrew/bin/lore".into(),
    ];
    if let Some(h) = home {
        candidates.push(h.join(".local/bin/lore"));
        candidates.push(h.join(".cargo/bin/lore"));
    }
    candidates.into_iter().find(|p| p.exists())
}

/// Build the active backend. Uses the real CLI when both a binary and a
/// repository are configured; otherwise falls back to the stateful mock so the
/// UI is always functional. Swapping in an FFI client later happens here.
pub fn build_client(config: &LoreConfig) -> Box<dyn LoreClient> {
    // Prefer the in-process liblore backend when built with `--features liblore`
    // and a repository is configured.
    #[cfg(feature = "liblore")]
    {
        if let Some(repository) = &config.repository {
            if repository.exists() {
                return Box::new(ffi::FfiLoreClient::new(repository.clone()));
            }
        }
    }

    match (&config.binary, &config.repository) {
        (Some(binary), Some(repository)) if repository.exists() => {
            Box::new(cli::CliLoreClient::new(binary.clone(), repository.clone()))
        }
        _ => Box::new(mock::MockLoreClient::new()),
    }
}
