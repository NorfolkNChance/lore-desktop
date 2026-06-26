//! `FfiLoreClient` — binds directly to liblore in-process (Phase D).
//!
//! This is the structural fix for the CLI wrapper's limitations: no text
//! parsing, no subprocess/server hangs, structured data, and real in-process
//! event streaming (liblore delivers results through an event callback —
//! `lore_event_callback_config_t`). Built only under `--features liblore` with
//! the vendored shared library (`scripts/fetch-liblore.sh`).
//!
//! Status: the binding, linking, and a first real call (`lore_version`) are
//! proven end-to-end. The event-callback-driven operations (status, locks,
//! branches, history, …) are mapped incrementally; until then they return a
//! clear "not yet implemented" error and the default build keeps using the CLI
//! wrapper. The `LoreClient` seam means filling these in touches nothing else.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]

/// Raw bindgen output for lore.h.
mod sys {
    include!(concat!(env!("OUT_DIR"), "/liblore_bindings.rs"));
}

use super::{ClientMode, LoreClient, LoreError, LoreResult};
use crate::models::*;
use async_trait::async_trait;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

pub struct FfiLoreClient {
    repository: PathBuf,
}

impl FfiLoreClient {
    pub fn new(repository: PathBuf) -> Self {
        Self { repository }
    }

    /// The liblore library version (proves the FFI binding + linking work).
    pub fn library_version() -> String {
        // SAFETY: `lore_version` returns a pointer to a static, NUL-terminated
        // C string owned by the library; we only borrow it.
        unsafe {
            let p = sys::lore_version();
            if p.is_null() {
                "unknown".to_string()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }
}

// Event tag values (bindgen prefixes C enum variants with the enum type name).
const EV_ERROR: u32 = sys::lore_event_id_t_LORE_EVENT_ERROR as u32;
const EV_COMPLETE: u32 = sys::lore_event_id_t_LORE_EVENT_COMPLETE as u32;
const EV_END: u32 = sys::lore_event_id_t_LORE_EVENT_END as u32;
const EV_BRANCH_LIST_ENTRY: u32 = sys::lore_event_id_t_LORE_EVENT_BRANCH_LIST_ENTRY as u32;
const EV_AUTH_USER_INFO: u32 = sys::lore_event_id_t_LORE_EVENT_AUTH_USER_INFO as u32;
const EV_STATUS_FILE: u32 = sys::lore_event_id_t_LORE_EVENT_REPOSITORY_STATUS_FILE as u32;
const EV_STATUS_REVISION: u32 = sys::lore_event_id_t_LORE_EVENT_REPOSITORY_STATUS_REVISION as u32;
const EV_LOCK_QUERY: u32 = sys::lore_event_id_t_LORE_EVENT_LOCK_FILE_QUERY as u32;
const EV_HISTORY_ENTRY: u32 = sys::lore_event_id_t_LORE_EVENT_REVISION_HISTORY_ENTRY as u32;
const EV_METADATA: u32 = sys::lore_event_id_t_LORE_EVENT_METADATA as u32;
const META_STRING: u32 = sys::lore_metadata_tag_t_LORE_METADATA_STRING as u32;
const META_NUMERIC: u32 = sys::lore_metadata_tag_t_LORE_METADATA_NUMERIC as u32;

/// Borrow a `&str` as a `lore_string_t` (ptr + len). The source must outlive
/// the returned value (and any FFI call using it).
fn lstr(s: &str) -> sys::lore_string_t {
    sys::lore_string_t {
        string: s.as_ptr() as *const std::os::raw::c_char,
        length: s.len(),
    }
}

/// Run an action-style op (no per-entry data, only success/error). Returns Ok
/// on `COMPLETE { status: 0 }`, else the `ERROR`/status message.
fn ffi_action<C>(op: &str, call: C) -> LoreResult<()>
where
    C: FnOnce(sys::lore_event_callback_config_t) -> i32,
{
    let mut outcome = OpOutcome::default();
    let rc = run_event_op(call, |event| unsafe {
        let _ = outcome.absorb(event);
    });
    outcome.into_result(rc, op)
}

// lore_file_action_t values.
const ACTION_ADD: u32 = sys::lore_file_action_t_LORE_FILE_ACTION_ADD as u32;
const ACTION_DELETE: u32 = sys::lore_file_action_t_LORE_FILE_ACTION_DELETE as u32;
const ACTION_MOVE: u32 = sys::lore_file_action_t_LORE_FILE_ACTION_MOVE as u32;
const ACTION_COPY: u32 = sys::lore_file_action_t_LORE_FILE_ACTION_COPY as u32;
const NODE_DIRECTORY: u32 = sys::lore_node_type_t_LORE_NODE_TYPE_DIRECTORY as u32;

/// Render a 32-byte `lore_hash_t` as lowercase hex.
fn hash_to_hex(h: &sys::lore_hash_t) -> String {
    h.data.iter().map(|b| format!("{b:02x}")).collect()
}

/// Classify an asset by extension (FFI-local copy of the CLI heuristic).
fn asset_kind_for(path: &str) -> AssetKind {
    let ext = std::path::Path::new(path)
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
        _ => AssetKind::Binary,
    }
}

// ---------------------------------------------------------------------------
// FFI plumbing: event-collector pattern
// ---------------------------------------------------------------------------
//
// Every liblore operation is `lore_X(globals, args, callback) -> i32` and
// streams results synchronously through the callback as `lore_event_t`s
// (typed entries, then `COMPLETE { status }`, then `END`). We pass a Rust
// closure across the C boundary via the callback's `user_context` and collect
// into Rust structures.

/// Read a borrowed `lore_string_t` (ptr + length, not NUL-terminated).
///
/// SAFETY: `s.string` must be valid for `s.length` bytes (it is, for the
/// duration of the callback that produced it).
unsafe fn lore_string(s: &sys::lore_string_t) -> String {
    if s.string.is_null() || s.length == 0 {
        return String::new();
    }
    let bytes = std::slice::from_raw_parts(s.string as *const u8, s.length);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Build `lore_global_args_t` for `repo`. `repo_c` must outlive the call (the
/// struct borrows its pointer). `offline` keeps reads local — never blocking on
/// the remote, matching the CLI client's local-first behaviour.
fn make_globals(repo_c: &CStr, offline: bool) -> sys::lore_global_args_t {
    // SAFETY: the struct is plain-old-data (pointers, ints, flags); zeroing it
    // yields empty strings (null ptr + 0 len) and false flags, then we set the
    // fields we care about.
    let mut g: sys::lore_global_args_t = unsafe { std::mem::zeroed() };
    g.repository_path = sys::lore_string_t {
        string: repo_c.as_ptr(),
        length: repo_c.to_bytes().len(),
    };
    g.offline = offline as u8;
    g.local = offline as u8;
    g
}

fn path_cstring(repo: &Path) -> CString {
    CString::new(repo.to_string_lossy().as_bytes().to_vec())
        .unwrap_or_else(|_| CString::new("").unwrap())
}

/// Invoke a liblore op, routing each event to `handler`. Returns the op's i32
/// return code. `call` receives the configured callback and performs the actual
/// `lore_X(...)` call.
fn run_event_op<C, H>(call: C, mut handler: H) -> i32
where
    C: FnOnce(sys::lore_event_callback_config_t) -> i32,
    H: FnMut(&sys::lore_event_t),
{
    let mut handler_dyn: &mut dyn FnMut(&sys::lore_event_t) = &mut handler;
    let ctx = (&mut handler_dyn) as *mut &mut dyn FnMut(&sys::lore_event_t) as u64;
    let cfg = sys::lore_event_callback_config_t {
        user_context: ctx,
        func: Some(trampoline),
    };
    call(cfg)
}

/// C callback: recover the Rust handler from `user_context` and dispatch.
unsafe extern "C" fn trampoline(event: *const sys::lore_event_t, user_context: u64) {
    if event.is_null() || user_context == 0 {
        return;
    }
    let handler = &mut *(user_context as *mut &mut dyn FnMut(&sys::lore_event_t));
    handler(&*event);
}

/// Collects the `COMPLETE { status }` and any `ERROR` message for an operation.
#[derive(Default)]
struct OpOutcome {
    error: Option<String>,
    status: i32,
}

impl OpOutcome {
    /// Handle the common terminal events; returns true if it consumed the event.
    unsafe fn absorb(&mut self, event: &sys::lore_event_t) -> bool {
        match event.tag {
            t if t == EV_ERROR => {
                self.error = Some(lore_string(&event.__bindgen_anon_1.error.error_inner));
                true
            }
            t if t == EV_COMPLETE => {
                self.status = event.__bindgen_anon_1.complete.status;
                true
            }
            t if t == EV_END => true,
            _ => false,
        }
    }

    fn into_result(self, rc: i32, op: &str) -> LoreResult<()> {
        if let Some(e) = self.error {
            return Err(LoreError::Cli(e));
        }
        if rc != 0 || self.status != 0 {
            return Err(LoreError::Cli(format!(
                "liblore {op} failed (rc={rc}, status={})",
                self.status
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Concrete operations (run on a blocking pool — liblore calls are synchronous)
// ---------------------------------------------------------------------------

fn ffi_list_branches(repo: &Path) -> LoreResult<Vec<Branch>> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let args = sys::lore_branch_list_args_t { archived: 0 };

    let mut branches: Vec<Branch> = Vec::new();
    let mut outcome = OpOutcome::default();
    let rc = run_event_op(
        |cfg| unsafe { sys::lore_branch_list(&globals, &args, cfg) },
        |event| unsafe {
            if outcome.absorb(event) {
                return;
            }
            if event.tag == EV_BRANCH_LIST_ENTRY {
                let e = &event.__bindgen_anon_1.branch_list_entry;
                branches.push(Branch {
                    id: String::new(),
                    name: lore_string(&e.name),
                    latest_revision: String::new(),
                    protected: false,
                });
            }
        },
    );
    outcome.into_result(rc, "branch_list")?;
    Ok(branches)
}

fn ffi_current_identity(repo: &Path) -> LoreResult<Identity> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    // Empty user_ids => resolve the current user locally.
    let args = sys::lore_auth_user_info_args_t {
        user_ids: sys::lore_string_array_t { ptr: std::ptr::null(), count: 0 },
    };

    let mut identity: Option<Identity> = None;
    let mut outcome = OpOutcome::default();
    let rc = run_event_op(
        |cfg| unsafe { sys::lore_auth_user_info(&globals, &args, cfg) },
        |event| unsafe {
            if outcome.absorb(event) {
                return;
            }
            if event.tag == EV_AUTH_USER_INFO {
                let e = &event.__bindgen_anon_1.auth_user_info;
                identity = Some(Identity {
                    user_id: lore_string(&e.id),
                    name: lore_string(&e.name),
                    authenticated: true,
                });
            }
        },
    );
    // Not being logged in isn't a hard error — return an unauthenticated identity.
    match outcome.into_result(rc, "auth_user_info") {
        Ok(()) => Ok(identity.unwrap_or(Identity {
            user_id: String::new(),
            name: String::new(),
            authenticated: false,
        })),
        Err(_) => Ok(Identity {
            user_id: String::new(),
            name: String::new(),
            authenticated: false,
        }),
    }
}

/// Working-tree status via `lore_repository_status` (offline = local-first, with
/// `scan` so brand-new files are detected). Locks are merged separately by the
/// trait method; here every entry starts as `Unknown`.
fn ffi_repository_status(repo: &Path) -> LoreResult<WorkspaceStatus> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let mut args: sys::lore_repository_status_args_t = unsafe { std::mem::zeroed() };
    args.scan = 1; // walk the filesystem so new untracked files appear
    args.staged = 1;

    let mut entries: Vec<FileEntry> = Vec::new();
    let mut branch = Branch {
        id: String::new(),
        name: "main".into(),
        latest_revision: String::new(),
        protected: false,
    };
    let mut head = Revision {
        id: String::new(),
        parents: vec![],
        message: String::new(),
        author: Author { name: String::new(), email: String::new() },
        timestamp: String::new(),
        tree_root: FragmentAddress { hash: String::new(), context: String::new() },
        is_merge: false,
    };
    let mut workspace_id = String::new();
    let mut outcome = OpOutcome::default();

    let rc = run_event_op(
        |cfg| unsafe { sys::lore_repository_status(&globals, &args, cfg) },
        |event| unsafe {
            if outcome.absorb(event) {
                return;
            }
            match event.tag {
                t if t == EV_STATUS_FILE => {
                    let f = &event.__bindgen_anon_1.repository_status_file;
                    // Skip directories; the UI lists files.
                    if f.type_ as u32 == NODE_DIRECTORY {
                        return;
                    }
                    let path = lore_string(&f.path);
                    let action = f.action as u32;
                    let dirty = f.flag_dirty != 0;
                    let change = if action == ACTION_ADD {
                        FileChange::Added
                    } else if action == ACTION_DELETE {
                        FileChange::Deleted
                    } else if action == ACTION_MOVE {
                        FileChange::Renamed
                    } else if action == ACTION_COPY {
                        FileChange::Added
                    } else if dirty {
                        FileChange::Modified // KEEP + dirty content
                    } else {
                        FileChange::Unchanged
                    };
                    let kind = asset_kind_for(&path);
                    entries.push(FileEntry {
                        path,
                        file_id: String::new(),
                        change,
                        staged: f.flag_staged != 0,
                        dirty,
                        is_binary: !matches!(kind, AssetKind::Text),
                        asset_kind: kind,
                        size_bytes: f.size,
                        fragment_count: 0,
                        lock_state: LockState::Unknown,
                        lock: None,
                    });
                }
                t if t == EV_STATUS_REVISION => {
                    let r = &event.__bindgen_anon_1.repository_status_revision;
                    branch.name = lore_string(&r.branch_name);
                    let rev = hash_to_hex(&r.revision);
                    branch.latest_revision = rev.clone();
                    head.id = rev;
                    workspace_id = format!("{:x?}", r.repository); // opaque id
                }
                _ => {}
            }
        },
    );
    outcome.into_result(rc, "repository_status")?;

    let counts = StatusCounts {
        staged: entries.iter().filter(|e| e.staged).count() as u32,
        modified: entries
            .iter()
            .filter(|e| matches!(e.change, FileChange::Modified))
            .count() as u32,
        locked_by_me: 0,
        locked_by_other: 0,
    };
    Ok(WorkspaceStatus {
        workspace_id,
        branch,
        head_revision: head,
        entries,
        counts,
        locks_available: false, // filled by the lock merge in status()
    })
}

/// All locks on the current branch via `lore_lock_file_query`.
fn ffi_query_locks(repo: &Path, identity: &Identity) -> LoreResult<Vec<Lock>> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, false); // locks are server-authoritative
    let empty = sys::lore_string_t { string: std::ptr::null(), length: 0 };
    let args = sys::lore_lock_file_query_args_t {
        branch: empty,
        owner: empty,
        path: empty,
    };

    let mut locks: Vec<Lock> = Vec::new();
    let mut outcome = OpOutcome::default();
    let rc = run_event_op(
        |cfg| unsafe { sys::lore_lock_file_query(&globals, &args, cfg) },
        |event| unsafe {
            if outcome.absorb(event) {
                return;
            }
            if event.tag == EV_LOCK_QUERY {
                let q = &event.__bindgen_anon_1.lock_file_query;
                let owner = lore_string(&q.owner);
                let state = if identity.authenticated
                    && !owner.is_empty()
                    && owner != identity.user_id
                    && owner != identity.name
                {
                    LockState::LockedByOther
                } else {
                    LockState::LockedByMe
                };
                locks.push(Lock {
                    path: lore_string(&q.path),
                    state,
                    owner: Some(Author { name: owner, email: String::new() }),
                    instance_id: None,
                    acquired_at: None,
                    reason: None,
                });
            }
        },
    );
    outcome.into_result(rc, "lock_file_query")?;
    Ok(locks)
}

fn ffi_history(repo: &Path, limit: Option<u32>) -> LoreResult<Vec<Revision>> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let mut args: sys::lore_revision_history_args_t = unsafe { std::mem::zeroed() };
    args.length = limit.unwrap_or(0);

    let mut revs: Vec<Revision> = Vec::new();
    let mut outcome = OpOutcome::default();
    let rc = run_event_op(
        |cfg| unsafe { sys::lore_revision_history(&globals, &args, cfg) },
        |event| unsafe {
            if outcome.absorb(event) {
                return;
            }
            match event.tag {
                t if t == EV_HISTORY_ENTRY => {
                    let h = &event.__bindgen_anon_1.revision_history_entry;
                    // Parents: non-zero hashes (the second is a merge parent).
                    let parents: Vec<String> = h
                        .parent
                        .iter()
                        .filter(|p| p.data.iter().any(|&b| b != 0))
                        .map(hash_to_hex)
                        .collect();
                    let is_merge = parents.len() == 2;
                    revs.push(Revision {
                        id: hash_to_hex(&h.revision),
                        parents,
                        // Filled by the METADATA events that follow this entry.
                        message: String::new(),
                        author: Author { name: String::new(), email: String::new() },
                        timestamp: String::new(),
                        tree_root: FragmentAddress {
                            hash: String::new(),
                            context: String::new(),
                        },
                        is_merge,
                    });
                }
                // liblore emits the message/timestamp/author as METADATA events
                // immediately after each history entry.
                t if t == EV_METADATA => {
                    if let Some(rev) = revs.last_mut() {
                        let m = &event.__bindgen_anon_1.metadata;
                        let key = lore_string(&m.key);
                        match key.as_str() {
                            "message" if m.value.tag == META_STRING => {
                                rev.message = lore_string(&m.value.__bindgen_anon_1.string);
                            }
                            // liblore reports the commit time as Unix milliseconds.
                            "timestamp" if m.value.tag == META_NUMERIC => {
                                let millis = m.value.__bindgen_anon_1.numeric as i64;
                                if let Some(dt) = chrono::DateTime::from_timestamp_millis(millis) {
                                    rev.timestamp = dt.to_rfc3339();
                                }
                            }
                            // `lore_revision_history` only emits branch/timestamp/
                            // message — author isn't carried here (kept for the
                            // day a future key appears).
                            "author" | "creator" if m.value.tag == META_STRING => {
                                rev.author.name = lore_string(&m.value.__bindgen_anon_1.string);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        },
    );
    outcome.into_result(rc, "revision_history")?;
    Ok(revs)
}

fn ffi_branch_switch(repo: &Path, name: &str) -> LoreResult<()> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let mut args: sys::lore_branch_switch_args_t = unsafe { std::mem::zeroed() };
    args.branch = lstr(name);
    ffi_action("branch_switch", |cfg| unsafe {
        sys::lore_branch_switch(&globals, &args, cfg)
    })
}

fn ffi_branch_create(repo: &Path, name: &str) -> LoreResult<()> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let mut args: sys::lore_branch_create_args_t = unsafe { std::mem::zeroed() };
    args.branch = lstr(name);
    ffi_action("branch_create", |cfg| unsafe {
        sys::lore_branch_create(&globals, &args, cfg)
    })
}

fn ffi_sync(repo: &Path, revision: Option<&str>) -> LoreResult<()> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, false); // sync talks to the remote
    let mut args: sys::lore_revision_sync_args_t = unsafe { std::mem::zeroed() };
    if let Some(rev) = revision {
        args.revision = lstr(rev);
    }
    ffi_action("revision_sync", |cfg| unsafe {
        sys::lore_revision_sync(&globals, &args, cfg)
    })
}

fn ffi_push(repo: &Path, branch: Option<&str>) -> LoreResult<()> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, false); // push talks to the remote
    let mut args: sys::lore_branch_push_args_t = unsafe { std::mem::zeroed() };
    if let Some(b) = branch {
        args.branch = lstr(b);
    }
    ffi_action("branch_push", |cfg| unsafe {
        sys::lore_branch_push(&globals, &args, cfg)
    })
}

fn ffi_stage(repo: &Path, paths: &[String]) -> LoreResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let lstrings: Vec<sys::lore_string_t> = paths.iter().map(|p| lstr(p)).collect();
    let mut args: sys::lore_file_stage_args_t = unsafe { std::mem::zeroed() };
    args.paths = sys::lore_string_array_t { ptr: lstrings.as_ptr(), count: lstrings.len() };
    ffi_action("file_stage", |cfg| unsafe {
        sys::lore_file_stage(&globals, &args, cfg)
    })
}

fn ffi_unstage(repo: &Path, paths: &[String]) -> LoreResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true);
    let lstrings: Vec<sys::lore_string_t> = paths.iter().map(|p| lstr(p)).collect();
    let mut args: sys::lore_file_unstage_args_t = unsafe { std::mem::zeroed() };
    args.paths = sys::lore_string_array_t { ptr: lstrings.as_ptr(), count: lstrings.len() };
    ffi_action("file_unstage", |cfg| unsafe {
        sys::lore_file_unstage(&globals, &args, cfg)
    })
}

fn ffi_commit(repo: &Path, message: &str) -> LoreResult<String> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, true); // commit creates a local revision
    let mut args: sys::lore_revision_commit_args_t = unsafe { std::mem::zeroed() };
    args.message = lstr(message);
    ffi_action("revision_commit", |cfg| unsafe {
        sys::lore_revision_commit(&globals, &args, cfg)
    })?;
    Ok("committed".to_string())
}

fn ffi_acquire_lock(repo: &Path, path: &str, reason: Option<String>) -> LoreResult<Lock> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, false); // locks are server-side
    let one = [lstr(path)];
    let mut args: sys::lore_lock_file_acquire_args_t = unsafe { std::mem::zeroed() };
    args.paths = sys::lore_string_array_t { ptr: one.as_ptr(), count: 1 };
    ffi_action("lock_file_acquire", |cfg| unsafe {
        sys::lore_lock_file_acquire(&globals, &args, cfg)
    })?;
    Ok(Lock {
        path: path.to_string(),
        state: LockState::LockedByMe,
        owner: None,
        instance_id: None,
        acquired_at: Some(chrono::Utc::now().to_rfc3339()),
        reason,
    })
}

fn ffi_release_lock(repo: &Path, path: &str) -> LoreResult<()> {
    let repo_c = path_cstring(repo);
    let globals = make_globals(&repo_c, false);
    let one = [lstr(path)];
    let mut args: sys::lore_lock_file_release_args_t = unsafe { std::mem::zeroed() };
    args.paths = sys::lore_string_array_t { ptr: one.as_ptr(), count: 1 };
    ffi_action("lock_file_release", |cfg| unsafe {
        sys::lore_lock_file_release(&globals, &args, cfg)
    })
}

/// Clone a remote repository into `dest` (`lore_repository_clone`). Standalone
/// (not bound to an existing client) so the command layer can call it directly
/// for an FFI-native clone.
pub fn clone(url: &str, dest: &Path) -> LoreResult<()> {
    let dest_c = path_cstring(dest);
    // The destination is the repository path; clone needs the remote (online).
    let globals = make_globals(&dest_c, false);
    let mut args: sys::lore_repository_clone_args_t = unsafe { std::mem::zeroed() };
    args.repository_url = lstr(url);
    ffi_action("repository_clone", |cfg| unsafe {
        sys::lore_repository_clone(&globals, &args, cfg)
    })
}

/// Run an FFI op on the blocking pool (liblore calls are synchronous).
async fn blocking<T, F>(f: F) -> LoreResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> LoreResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| LoreError::Io(e.to_string()))?
}

#[async_trait]
impl LoreClient for FfiLoreClient {
    fn mode(&self) -> ClientMode {
        ClientMode::Ffi
    }

    async fn version(&self) -> LoreResult<String> {
        Ok(format!("liblore {}", Self::library_version()))
    }

    async fn status(&self) -> LoreResult<WorkspaceStatus> {
        let repo = self.repository.clone();
        let mut status = tokio::task::spawn_blocking(move || ffi_repository_status(&repo))
            .await
            .map_err(|e| LoreError::Io(e.to_string()))??;

        // Merge server-side lock state, bounded so a slow/unreachable remote
        // degrades to "unknown" rather than stalling (mirrors the CLI client).
        let lock_result = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            self.query_locks(),
        )
        .await
        .ok()
        .and_then(|r| r.ok());

        if let Some(locks) = lock_result {
            let by_path: std::collections::HashMap<&str, &Lock> =
                locks.iter().map(|l| (l.path.as_str(), l)).collect();
            for e in &mut status.entries {
                match by_path.get(e.path.as_str()) {
                    Some(l) => {
                        e.lock_state = l.state;
                        e.lock = Some((*l).clone());
                    }
                    None => e.lock_state = LockState::Unlocked,
                }
            }
            status.locks_available = true;
            status.counts.locked_by_me = status
                .entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByMe)
                .count() as u32;
            status.counts.locked_by_other = status
                .entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByOther)
                .count() as u32;
        }
        // else: entries stay Unknown, locks_available stays false.
        Ok(status)
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        let repo = self.repository.clone();
        tokio::task::spawn_blocking(move || {
            let identity = ffi_current_identity(&repo).unwrap_or(Identity {
                user_id: String::new(),
                name: String::new(),
                authenticated: false,
            });
            ffi_query_locks(&repo, &identity)
        })
        .await
        .map_err(|e| LoreError::Io(e.to_string()))?
    }

    async fn lock_status(&self, path: &str) -> LoreResult<LockState> {
        let target = path.to_string();
        let locks = self.query_locks().await?;
        Ok(locks
            .into_iter()
            .find(|l| l.path == target)
            .map(|l| l.state)
            .unwrap_or(LockState::Unlocked))
    }

    async fn acquire_lock(&self, path: &str, reason: Option<String>) -> LoreResult<Lock> {
        let repo = self.repository.clone();
        let path = path.to_string();
        blocking(move || ffi_acquire_lock(&repo, &path, reason)).await
    }

    async fn release_lock(&self, path: &str) -> LoreResult<()> {
        let repo = self.repository.clone();
        let path = path.to_string();
        blocking(move || ffi_release_lock(&repo, &path)).await
    }

    async fn stage(&self, paths: &[String]) -> LoreResult<()> {
        let repo = self.repository.clone();
        let paths = paths.to_vec();
        blocking(move || ffi_stage(&repo, &paths)).await
    }

    async fn unstage(&self, paths: &[String]) -> LoreResult<()> {
        let repo = self.repository.clone();
        let paths = paths.to_vec();
        blocking(move || ffi_unstage(&repo, &paths)).await
    }

    async fn commit(&self, message: &str) -> LoreResult<String> {
        let repo = self.repository.clone();
        let message = message.to_string();
        blocking(move || ffi_commit(&repo, &message)).await
    }

    async fn list_branches(&self) -> LoreResult<Vec<Branch>> {
        let repo = self.repository.clone();
        blocking(move || ffi_list_branches(&repo)).await
    }

    async fn switch_branch(&self, name: &str) -> LoreResult<()> {
        let repo = self.repository.clone();
        let name = name.to_string();
        blocking(move || ffi_branch_switch(&repo, &name)).await
    }

    async fn create_branch(&self, name: &str) -> LoreResult<()> {
        let repo = self.repository.clone();
        let name = name.to_string();
        blocking(move || ffi_branch_create(&repo, &name)).await
    }

    async fn history(&self, limit: Option<u32>) -> LoreResult<Vec<Revision>> {
        let repo = self.repository.clone();
        blocking(move || ffi_history(&repo, limit)).await
    }

    async fn sync(&self, revision: Option<String>) -> LoreResult<()> {
        let repo = self.repository.clone();
        blocking(move || ffi_sync(&repo, revision.as_deref())).await
    }

    async fn push(&self, branch: Option<String>) -> LoreResult<()> {
        let repo = self.repository.clone();
        blocking(move || ffi_push(&repo, branch.as_deref())).await
    }

    async fn current_identity(&self) -> LoreResult<Identity> {
        let repo = self.repository.clone();
        tokio::task::spawn_blocking(move || ffi_current_identity(&repo))
            .await
            .map_err(|e| LoreError::Io(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_version_is_nonempty() {
        // Proves the binding links and a real liblore call returns data.
        let v = FfiLoreClient::library_version();
        assert!(!v.is_empty());
        assert_ne!(v, "unknown");
    }

    /// Live FFI test against a real repository. Ignored by default (needs a repo
    /// + library); run with: LORE_TEST_REPO=/path cargo test --features liblore
    /// -- --ignored
    #[test]
    #[ignore]
    fn ffi_branches_against_live_repo() {
        let repo = std::env::var("LORE_TEST_REPO").unwrap_or_else(|_| "/tmp/lore-wf".into());
        let branches = ffi_list_branches(Path::new(&repo)).expect("branch_list");
        eprintln!("FFI branches: {:?}", branches.iter().map(|b| &b.name).collect::<Vec<_>>());
        assert!(branches.iter().any(|b| b.name == "main"), "expected a 'main' branch");
    }

    #[test]
    #[ignore]
    fn ffi_status_against_live_repo() {
        let repo = std::env::var("LORE_TEST_REPO").unwrap_or_else(|_| "/tmp/lore-wf".into());
        let status = ffi_repository_status(Path::new(&repo)).expect("status");
        eprintln!(
            "FFI status: branch={} rev={} entries={}",
            status.branch.name,
            &status.head_revision.id[..status.head_revision.id.len().min(10)],
            status.entries.len()
        );
        for e in &status.entries {
            eprintln!("  {:?} {} ({} B)", e.change, e.path, e.size_bytes);
        }
        assert_eq!(status.branch.name, "main");
        assert!(!status.head_revision.id.is_empty(), "expected a head revision");
    }

    #[test]
    #[ignore]
    fn ffi_history_against_live_repo() {
        let repo = std::env::var("LORE_TEST_REPO").unwrap_or_else(|_| "/tmp/lore-wf".into());
        let revs = ffi_history(Path::new(&repo), Some(50)).expect("history");
        eprintln!("FFI history: {} revisions", revs.len());
        for r in &revs {
            eprintln!("  {} [{}] {}", &r.id[..r.id.len().min(10)], r.timestamp, r.message);
        }
        assert!(!revs.is_empty(), "expected at least one revision");
        assert!(
            revs[0].timestamp.starts_with("202"),
            "expected an ISO timestamp, got {:?}",
            revs[0].timestamp
        );
    }
}
