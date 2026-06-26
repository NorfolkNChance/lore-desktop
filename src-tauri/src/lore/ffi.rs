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

fn pending(op: &str) -> LoreError {
    LoreError::Cli(format!(
        "liblore FFI: `{op}` not yet implemented — build without `--features liblore` for the full CLI-backed client"
    ))
}

// Event tag values (bindgen prefixes C enum variants with the enum type name).
const EV_ERROR: u32 = sys::lore_event_id_t_LORE_EVENT_ERROR as u32;
const EV_COMPLETE: u32 = sys::lore_event_id_t_LORE_EVENT_COMPLETE as u32;
const EV_END: u32 = sys::lore_event_id_t_LORE_EVENT_END as u32;
const EV_BRANCH_LIST_ENTRY: u32 = sys::lore_event_id_t_LORE_EVENT_BRANCH_LIST_ENTRY as u32;
const EV_AUTH_USER_INFO: u32 = sys::lore_event_id_t_LORE_EVENT_AUTH_USER_INFO as u32;

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

#[async_trait]
impl LoreClient for FfiLoreClient {
    fn mode(&self) -> ClientMode {
        ClientMode::Ffi
    }

    async fn version(&self) -> LoreResult<String> {
        Ok(format!("liblore {}", Self::library_version()))
    }

    async fn status(&self) -> LoreResult<WorkspaceStatus> {
        let _ = &self.repository;
        Err(pending("status"))
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        Err(pending("query_locks"))
    }

    async fn lock_status(&self, _path: &str) -> LoreResult<LockState> {
        Err(pending("lock_status"))
    }

    async fn acquire_lock(&self, _path: &str, _reason: Option<String>) -> LoreResult<Lock> {
        Err(pending("acquire_lock"))
    }

    async fn release_lock(&self, _path: &str) -> LoreResult<()> {
        Err(pending("release_lock"))
    }

    async fn stage(&self, _paths: &[String]) -> LoreResult<()> {
        Err(pending("stage"))
    }

    async fn unstage(&self, _paths: &[String]) -> LoreResult<()> {
        Err(pending("unstage"))
    }

    async fn commit(&self, _message: &str) -> LoreResult<String> {
        Err(pending("commit"))
    }

    async fn list_branches(&self) -> LoreResult<Vec<Branch>> {
        let repo = self.repository.clone();
        tokio::task::spawn_blocking(move || ffi_list_branches(&repo))
            .await
            .map_err(|e| LoreError::Io(e.to_string()))?
    }

    async fn switch_branch(&self, _name: &str) -> LoreResult<()> {
        Err(pending("switch_branch"))
    }

    async fn create_branch(&self, _name: &str) -> LoreResult<()> {
        Err(pending("create_branch"))
    }

    async fn history(&self, _limit: Option<u32>) -> LoreResult<Vec<Revision>> {
        Err(pending("history"))
    }

    async fn sync(&self, _revision: Option<String>) -> LoreResult<()> {
        Err(pending("sync"))
    }

    async fn push(&self, _branch: Option<String>) -> LoreResult<()> {
        Err(pending("push"))
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
}
