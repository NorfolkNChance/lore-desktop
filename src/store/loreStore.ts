/**
 * Zustand store fed by async IPC + daemon events.
 *
 * Holds the active workspace, working-tree status, history, UI selection, and
 * live service state. `subscribeToEvents` wires the daemon event channel so the
 * UI reacts instantly to lock/status/commit changes pushed from the backend.
 *
 * Every mutating action funnels through one `run` helper (busy flag, error
 * capture, cleanup), so adding an action is a one-liner and there is exactly
 * one in-flight mechanism: `busy` — `kind` tells components which spinner or
 * disable-state to drive.
 */

import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import * as lore from "@/api/lore";
import type {
  Branch,
  ClientMode,
  DiffTool,
  Identity,
  IngestSummary,
  LoreEvent,
  Revision,
  ServiceState,
  TransferProgress,
  Workspace,
  WorkspaceStatus,
} from "@/types/lore";
import type { UnlistenFn } from "@tauri-apps/api/event";

export type SidebarTab = "changes" | "history";

/** The single in-flight indicator (replaces loading/committing/busy flags). */
export type Busy = {
  /** Human-readable, shown in the toolbar. */
  label: string;
  /** What kind of work — components key off this for spinners/disables. */
  kind: "bootstrap" | "commit" | "vcs";
} | null;

interface LoreState {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  status: WorkspaceStatus | null;
  revisions: Revision[];
  serviceState: ServiceState;
  backendMode: ClientMode;
  loreVersion: string | null;
  /** Set when the CLI backend's version is outside the parser-tested range. */
  compatWarning: string | null;
  busy: Busy;
  error: lore.IpcError | null;
  lastEvent: LoreEvent | null;

  // UI state
  activeTab: SidebarTab;
  selectedPath: string | null;

  // Phase 4: streaming + diff tools
  transfer: TransferProgress | null;
  ingestSummary: IngestSummary | null;
  diffTools: DiffTool[];

  // VCS workflow
  branches: Branch[];
  identity: Identity | null;

  // actions
  bootstrap: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshBranches: () => Promise<void>;
  switchBranch: (name: string) => Promise<void>;
  createBranch: (name: string) => Promise<void>;
  syncRepo: () => Promise<void>;
  pushRepo: () => Promise<void>;
  openRepository: () => Promise<void>;
  cloneRepo: (url: string, path: string) => Promise<void>;
  setTab: (tab: SidebarTab) => void;
  selectFile: (path: string | null) => void;
  acquireLock: (path: string, reason?: string) => Promise<void>;
  releaseLock: (path: string) => Promise<void>;
  setStaged: (path: string, staged: boolean) => Promise<void>;
  commit: (message: string) => Promise<boolean>;
  startService: () => Promise<void>;
  stopService: () => Promise<void>;
  ingestAsset: (path: string) => Promise<void>;
  openAssetDiff: (path: string, toolId?: string) => Promise<void>;
  dismissError: () => void;
  clearIngest: () => void;
  subscribeToEvents: () => Promise<UnlistenFn>;
}

export const useLoreStore = create<LoreState>((set, get) => {
  /**
   * Run an async action with the shared busy/error plumbing. Pass `busy` for
   * actions that should block the toolbar (branch ops, commit, clone…) or
   * `null` for quiet ones (locks, staging). Returns whether `fn` succeeded.
   */
  const run = async (busy: Busy, fn: () => Promise<void>): Promise<boolean> => {
    set(busy ? { busy, error: null } : { error: null });
    try {
      await fn();
      return true;
    } catch (e) {
      set({ error: lore.toIpcError(e) });
      return false;
    } finally {
      if (busy) set({ busy: null });
    }
  };

  /** Refresh status, branches, and history concurrently. */
  const refreshAll = async () => {
    await Promise.all([
      get().refreshStatus(),
      get().refreshBranches(),
      get().refreshHistory(),
    ]);
  };

  return {
    workspaces: [],
    activeWorkspaceId: null,
    status: null,
    revisions: [],
    serviceState: "stopped",
    backendMode: "mock",
    loreVersion: null,
    compatWarning: null,
    busy: null,
    error: null,
    lastEvent: null,

    activeTab: "changes",
    selectedPath: null,

    transfer: null,
    ingestSummary: null,
    diffTools: [],

    branches: [],
    identity: null,

    bootstrap: () =>
      run({ label: "Loading…", kind: "bootstrap" }, async () => {
        const [workspaces, serviceState, info] = await Promise.all([
          lore.listWorkspaces(),
          lore.serviceState(),
          lore.backendInfo().catch(() => null),
        ]);
        const activeWorkspaceId = workspaces[0]?.id ?? null;
        set({
          workspaces,
          activeWorkspaceId,
          serviceState,
          backendMode: info?.mode ?? "mock",
          loreVersion: info?.version ?? null,
          compatWarning: info?.compatWarning ?? null,
        });
        const status = await lore.getWorkspaceStatus();
        // Auto-select the first changed file so the detail pane isn't empty.
        const selectedPath = get().selectedPath ?? status.entries[0]?.path ?? null;
        set({ status, selectedPath });
        await Promise.all([get().refreshHistory(), get().refreshBranches()]);
        lore.listDiffTools().then((diffTools) => set({ diffTools })).catch(() => {});
        lore.currentIdentity().then((identity) => set({ identity })).catch(() => {});
      }).then(() => {}),

    refreshStatus: async () => {
      try {
        const status = await lore.getWorkspaceStatus();
        // Keep selection valid; fall back to the first entry.
        const cur = get().selectedPath;
        const stillThere = status.entries.some((e) => e.path === cur);
        set({
          status,
          selectedPath: stillThere ? cur : status.entries[0]?.path ?? null,
        });
      } catch (e) {
        set({ error: lore.toIpcError(e) });
      }
    },

    refreshHistory: async () => {
      try {
        const revisions = await lore.listRevisions(50);
        set({ revisions });
      } catch (e) {
        // History is non-critical; don't surface as a blocking error.
        console.warn("history load failed", e);
      }
    },

    refreshBranches: async () => {
      try {
        const branches = await lore.listBranches();
        set({ branches });
      } catch (e) {
        console.warn("branch load failed", e);
      }
    },

    switchBranch: async (name) => {
      await run({ label: `Switching to ${name}…`, kind: "vcs" }, async () => {
        await lore.switchBranch(name);
        await refreshAll();
      });
    },

    createBranch: async (name) => {
      await run({ label: `Creating ${name}…`, kind: "vcs" }, async () => {
        await lore.createBranch(name);
        await get().refreshBranches();
      });
    },

    syncRepo: async () => {
      await run({ label: "Syncing…", kind: "vcs" }, async () => {
        await lore.syncRepository();
        await Promise.all([get().refreshStatus(), get().refreshHistory()]);
      });
    },

    pushRepo: async () => {
      await run({ label: "Pushing…", kind: "vcs" }, async () => {
        await lore.pushRepository();
        await get().refreshHistory();
      });
    },

    openRepository: async () => {
      try {
        const dir = await open({
          directory: true,
          title: "Open a Lore repository",
        });
        if (typeof dir === "string" && dir) {
          set({ selectedPath: null });
          await run({ label: "Opening repository…", kind: "vcs" }, async () => {
            await lore.setRepository(dir);
            await get().bootstrap();
          });
        }
      } catch (e) {
        // Dialog errors (the IPC ones are handled inside `run`).
        set({ error: lore.toIpcError(e) });
      }
    },

    cloneRepo: async (url, path) => {
      set({ selectedPath: null });
      await run({ label: "Cloning…", kind: "vcs" }, async () => {
        await lore.cloneRepository(url, path);
        await get().bootstrap();
      });
    },

    setTab: (tab) => set({ activeTab: tab }),

    selectFile: (path) => set({ selectedPath: path }),

    acquireLock: async (path, reason) => {
      await run(null, async () => {
        await lore.acquireLock(path, reason);
        await get().refreshStatus();
      });
    },

    releaseLock: async (path) => {
      await run(null, async () => {
        await lore.releaseLock(path);
        await get().refreshStatus();
      });
    },

    setStaged: async (path, staged) => {
      await run(null, async () => {
        if (staged) await lore.stageFiles([path]);
        else await lore.unstageFiles([path]);
        await get().refreshStatus();
      });
    },

    commit: (message) =>
      run({ label: "Committing…", kind: "commit" }, async () => {
        await lore.commit(message);
        await Promise.all([get().refreshStatus(), get().refreshHistory()]);
      }),

    startService: async () => {
      await run(null, async () => {
        await lore.startService();
        set({ serviceState: await lore.serviceState() });
      });
    },

    stopService: async () => {
      await run(null, async () => {
        await lore.stopService();
        set({ serviceState: await lore.serviceState() });
      });
    },

    ingestAsset: async (path) => {
      set({ ingestSummary: null, transfer: null });
      // Repo-relative path; the backend resolves (and validates) it against
      // the repository root — the boundary check lives on the Rust side.
      await run(null, async () => {
        const summary = await lore.streamIngestFile(path);
        set({ ingestSummary: summary });
      });
      set({ transfer: null });
    },

    openAssetDiff: async (path, toolId) => {
      await run(null, () => lore.launchAssetDiff(path, toolId).then(() => {}));
    },

    dismissError: () => set({ error: null }),

    clearIngest: () => set({ ingestSummary: null, transfer: null }),

    subscribeToEvents: () =>
      lore.onLoreEvent((event) => {
        set({ lastEvent: event });
        switch (event.tag) {
          case "lockChanged":
          case "statusChanged":
            void get().refreshStatus();
            break;
          case "revisionCommitted":
            void get().refreshStatus();
            void get().refreshHistory();
            break;
          case "branchSwitched":
            void get().refreshBranches();
            void get().refreshStatus();
            break;
          case "serviceStateChanged":
            set({ serviceState: event.payload.state });
            break;
          case "transferProgress":
            set({ transfer: event.payload });
            break;
          default:
            break;
        }
      }),
  };
});
