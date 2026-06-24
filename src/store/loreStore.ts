/**
 * Zustand store fed by async IPC + daemon events.
 *
 * Holds the active workspace, working-tree status, history, UI selection, and
 * live service state. `subscribeToEvents` wires the daemon event channel so the
 * UI reacts instantly to lock/status/commit changes pushed from the backend.
 */

import { create } from "zustand";
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

interface LoreState {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  status: WorkspaceStatus | null;
  revisions: Revision[];
  serviceState: ServiceState;
  backendMode: ClientMode;
  loreVersion: string | null;
  loading: boolean;
  error: string | null;
  lastEvent: LoreEvent | null;

  // UI state
  activeTab: SidebarTab;
  selectedPath: string | null;
  committing: boolean;

  // Phase 4: streaming + diff tools
  transfer: TransferProgress | null;
  ingestSummary: IngestSummary | null;
  diffTools: DiffTool[];

  // VCS workflow
  branches: Branch[];
  identity: Identity | null;
  busy: string | null;

  // actions
  bootstrap: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshBranches: () => Promise<void>;
  switchBranch: (name: string) => Promise<void>;
  createBranch: (name: string) => Promise<void>;
  syncRepo: () => Promise<void>;
  pushRepo: () => Promise<void>;
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

export const useLoreStore = create<LoreState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: null,
  status: null,
  revisions: [],
  serviceState: "stopped",
  backendMode: "mock",
  loreVersion: null,
  loading: false,
  error: null,
  lastEvent: null,

  activeTab: "changes",
  selectedPath: null,
  committing: false,

  transfer: null,
  ingestSummary: null,
  diffTools: [],

  branches: [],
  identity: null,
  busy: null,

  bootstrap: async () => {
    set({ loading: true, error: null });
    try {
      const [workspaces, serviceState, backendMode, loreVersion] =
        await Promise.all([
          lore.listWorkspaces(),
          lore.serviceState(),
          lore.backendMode(),
          lore.loreVersion().catch(() => null),
        ]);
      const activeWorkspaceId = workspaces[0]?.id ?? null;
      set({ workspaces, activeWorkspaceId, serviceState, backendMode, loreVersion });
      const status = await lore.getWorkspaceStatus();
      // Auto-select the first changed file so the detail pane isn't empty.
      const selectedPath = get().selectedPath ?? status.entries[0]?.path ?? null;
      set({ status, selectedPath });
      await get().refreshHistory();
      await get().refreshBranches();
      lore.listDiffTools().then((diffTools) => set({ diffTools })).catch(() => {});
      lore.currentIdentity().then((identity) => set({ identity })).catch(() => {});
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ loading: false });
    }
  },

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
      set({ error: String(e) });
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
    set({ busy: `Switching to ${name}…`, error: null });
    try {
      await lore.switchBranch(name);
      await get().refreshStatus();
      await get().refreshBranches();
      await get().refreshHistory();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null });
    }
  },

  createBranch: async (name) => {
    set({ busy: `Creating ${name}…`, error: null });
    try {
      await lore.createBranch(name);
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null });
    }
  },

  syncRepo: async () => {
    set({ busy: "Syncing…", error: null });
    try {
      await lore.syncRepository();
      await get().refreshStatus();
      await get().refreshHistory();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null });
    }
  },

  pushRepo: async () => {
    set({ busy: "Pushing…", error: null });
    try {
      await lore.pushRepository();
      await get().refreshHistory();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null });
    }
  },

  setTab: (tab) => set({ activeTab: tab }),

  selectFile: (path) => set({ selectedPath: path }),

  acquireLock: async (path, reason) => {
    try {
      await lore.acquireLock(path, reason);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  releaseLock: async (path) => {
    try {
      await lore.releaseLock(path);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setStaged: async (path, staged) => {
    try {
      if (staged) await lore.stageFiles([path]);
      else await lore.unstageFiles([path]);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  commit: async (message) => {
    set({ committing: true, error: null });
    try {
      await lore.commit(message);
      await get().refreshStatus();
      await get().refreshHistory();
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    } finally {
      set({ committing: false });
    }
  },

  startService: async () => {
    try {
      await lore.startService();
      set({ serviceState: await lore.serviceState() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  stopService: async () => {
    try {
      await lore.stopService();
      set({ serviceState: await lore.serviceState() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  ingestAsset: async (path) => {
    set({ ingestSummary: null, transfer: null, error: null });
    try {
      // Absolute path on disk = repository root + repo-relative path.
      const ws = get().workspaces[0];
      const abs = ws?.path ? `${ws.path}/${path}` : path;
      const summary = await lore.streamIngestFile(abs);
      set({ ingestSummary: summary, transfer: null });
    } catch (e) {
      set({ error: String(e), transfer: null });
    }
  },

  openAssetDiff: async (path, toolId) => {
    try {
      await lore.launchAssetDiff(path, toolId);
    } catch (e) {
      set({ error: String(e) });
    }
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
          if (event.payload && typeof event.payload === "object") {
            const state = (event.payload as { state?: ServiceState }).state;
            if (state) set({ serviceState: state });
          }
          break;
        case "transferProgress":
          if (event.payload && typeof event.payload === "object") {
            set({ transfer: event.payload as TransferProgress });
          }
          break;
        default:
          break;
      }
    }),
}));
