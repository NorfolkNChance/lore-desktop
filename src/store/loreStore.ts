/**
 * Zustand store fed by async IPC + daemon events.
 *
 * Holds the active workspace, its status, and live service state. The
 * `subscribeToEvents` action wires the daemon event channel so the UI reacts
 * instantly to lock/status changes pushed from the backend — exactly the
 * event-driven path Phase 3 builds on.
 */

import { create } from "zustand";
import * as lore from "@/api/lore";
import type {
  ClientMode,
  LoreEvent,
  ServiceState,
  Workspace,
  WorkspaceStatus,
} from "@/types/lore";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface LoreState {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  status: WorkspaceStatus | null;
  serviceState: ServiceState;
  backendMode: ClientMode;
  loreVersion: string | null;
  loading: boolean;
  error: string | null;
  lastEvent: LoreEvent | null;

  // actions
  bootstrap: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  acquireLock: (path: string, reason?: string) => Promise<void>;
  releaseLock: (path: string) => Promise<void>;
  subscribeToEvents: () => Promise<UnlistenFn>;
}

export const useLoreStore = create<LoreState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: null,
  status: null,
  serviceState: "stopped",
  backendMode: "mock",
  loreVersion: null,
  loading: false,
  error: null,
  lastEvent: null,

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
      set({ status });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ loading: false });
    }
  },

  refreshStatus: async () => {
    try {
      const status = await lore.getWorkspaceStatus();
      set({ status });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  acquireLock: async (path, reason) => {
    try {
      await lore.acquireLock(path, reason);
      // Event channel will trigger refresh, but refresh now for snappiness.
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

  subscribeToEvents: () =>
    lore.onLoreEvent((event) => {
      set({ lastEvent: event });
      switch (event.tag) {
        case "lockChanged":
        case "statusChanged":
        case "revisionCommitted":
          void get().refreshStatus();
          break;
        case "serviceStateChanged":
          if (event.payload && typeof event.payload === "object") {
            const state = (event.payload as { state?: ServiceState }).state;
            if (state) set({ serviceState: state });
          }
          break;
        default:
          break;
      }
    }),
}));
