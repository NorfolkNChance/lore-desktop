/**
 * Typed IPC client.
 * =================
 * The single seam between the React UI and the Rust backend. Components call
 * these functions and never touch `invoke`/`listen` directly, so when Phase 2
 * swaps the mock command bodies for liblore the frontend is unaffected.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  LORE_EVENT_CHANNEL,
  type Branch,
  type ClientMode,
  type DiffTool,
  type Identity,
  type IngestSummary,
  type Lock,
  type LockState,
  type LoreEvent,
  type Revision,
  type ServiceState,
  type Workspace,
  type WorkspaceStatus,
} from "@/types/lore";

// ---------------------------------------------------------------------------
// Backend introspection
// ---------------------------------------------------------------------------

/** Which backend is serving data: real `lore` CLI or the stateful mock. */
export const backendMode = (): Promise<ClientMode> => invoke("backend_mode");

export const loreVersion = (): Promise<string> => invoke("lore_version");

// ---------------------------------------------------------------------------
// Read commands (repository comes from backend-managed state)
// ---------------------------------------------------------------------------

export const listWorkspaces = (): Promise<Workspace[]> =>
  invoke("list_workspaces");

export const getWorkspaceStatus = (): Promise<WorkspaceStatus> =>
  invoke("get_workspace_status");

export const listBranches = (): Promise<Branch[]> => invoke("list_branches");

export const listRevisions = (limit?: number): Promise<Revision[]> =>
  invoke("list_revisions", { limit });

export const currentIdentity = (): Promise<Identity> =>
  invoke("current_identity");

// ---------------------------------------------------------------------------
// Repository management (open / clone at runtime)
// ---------------------------------------------------------------------------

export const setRepository = (path: string): Promise<ClientMode> =>
  invoke("set_repository", { path });

export const cloneRepository = (
  url: string,
  path: string,
): Promise<ClientMode> => invoke("clone_repository", { url, path });

// ---------------------------------------------------------------------------
// VCS workflow (branches, sync, push)
// ---------------------------------------------------------------------------

export const switchBranch = (name: string): Promise<void> =>
  invoke("switch_branch", { name });

export const createBranch = (name: string): Promise<void> =>
  invoke("create_branch", { name });

export const syncRepository = (revision?: string): Promise<void> =>
  invoke("sync_repository", { revision });

export const pushRepository = (branch?: string): Promise<void> =>
  invoke("push_repository", { branch });

export const listLocks = (): Promise<Lock[]> => invoke("list_locks");

export const serviceState = (): Promise<ServiceState> =>
  invoke("service_state");

export const startService = (): Promise<void> => invoke("start_service");

export const stopService = (): Promise<void> => invoke("stop_service");

// ---------------------------------------------------------------------------
// Mutating lock commands (mirror `lore lock acquire | status | release`)
// ---------------------------------------------------------------------------

export const acquireLock = (path: string, reason?: string): Promise<Lock> =>
  invoke("acquire_lock", { path, reason });

export const releaseLock = (path: string): Promise<void> =>
  invoke("release_lock", { path });

export const lockStatus = (path: string): Promise<LockState> =>
  invoke("lock_status", { path });

// ---------------------------------------------------------------------------
// Staging & commit (mirror `lore stage | unstage | commit`)
// ---------------------------------------------------------------------------

export const stageFiles = (paths: string[]): Promise<void> =>
  invoke("stage_files", { paths });

export const unstageFiles = (paths: string[]): Promise<void> =>
  invoke("unstage_files", { paths });

export const commit = (message: string): Promise<string> =>
  invoke("commit", { message });

// ---------------------------------------------------------------------------
// Phase 4: streaming ingest + visual diff-tool hooks
// ---------------------------------------------------------------------------

/** Stream a file into fragments (bounded memory). Progress arrives via events. */
export const streamIngestFile = (path: string): Promise<IngestSummary> =>
  invoke("stream_ingest_file", { path });

export const listDiffTools = (): Promise<DiffTool[]> => invoke("list_diff_tools");

/** Launch a native diff tool on two arbitrary paths (the integration hook). */
export const launchDiffTool = (
  left: string,
  right: string,
  toolId?: string,
): Promise<DiffTool> => invoke("launch_diff_tool", { left, right, toolId });

/** Diff a repository asset's working copy vs. its committed base natively. */
export const launchAssetDiff = (
  path: string,
  toolId?: string,
): Promise<DiffTool> => invoke("launch_asset_diff", { path, toolId });

// ---------------------------------------------------------------------------
// Daemon event stream
// ---------------------------------------------------------------------------

/**
 * Subscribe to daemon-pushed events. Returns an unlisten function — call it on
 * unmount. The backend emits on a single channel and consumers narrow on `tag`.
 */
export const onLoreEvent = (
  handler: (event: LoreEvent) => void,
): Promise<UnlistenFn> =>
  listen<LoreEvent>(LORE_EVENT_CHANNEL, (e) => handler(e.payload));
