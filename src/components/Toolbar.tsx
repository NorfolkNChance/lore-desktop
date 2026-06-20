import type { ReactNode } from "react";
import { useLoreStore } from "@/store/loreStore";

/**
 * GitHub Desktop-style top toolbar: current repository · current branch ·
 * push, plus the backend-mode badge and daemon indicator.
 */
export function Toolbar() {
  const { workspaces, status, backendMode, loreVersion, serviceState } =
    useLoreStore();
  const ws = workspaces[0];
  const branch = status?.branch;
  const ahead = ws?.stagedFileCount ?? 0;
  const isCli = backendMode === "cli";

  return (
    <header className="flex h-13 items-stretch border-b border-line bg-subtle">
      <Segment className="min-w-52 flex-1">
        <div className="text-[11px] text-muted">Current repository</div>
        <div className="flex items-center gap-1.5 font-medium">
          <span aria-hidden>▸</span>
          <span className="truncate">{ws?.name ?? "—"}</span>
          <span className="text-faint">▾</span>
        </div>
      </Segment>

      <Segment className="min-w-44 flex-1 border-l border-line">
        <div className="text-[11px] text-muted">Current branch</div>
        <div className="flex items-center gap-1.5 font-medium">
          <span aria-hidden>⎇</span>
          <span className="truncate">{branch?.name ?? "—"}</span>
          {branch?.protected && (
            <span className="rounded bg-accent-subtle px-1 text-[10px] text-accent">
              protected
            </span>
          )}
          <span className="text-faint">▾</span>
        </div>
      </Segment>

      <Segment className="min-w-40 border-l border-line">
        <div className="flex items-center gap-2">
          <span aria-hidden className="text-base text-muted">
            ↑
          </span>
          <div>
            <div className="font-medium">Push origin</div>
            <div className="text-[11px] text-muted">
              {ahead > 0 ? `${ahead} staged` : "up to date"}
            </div>
          </div>
        </div>
      </Segment>

      <div className="ml-auto flex items-center gap-3 border-l border-line px-4">
        <span
          title={loreVersion ?? undefined}
          className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${
            isCli
              ? "bg-success-subtle text-success"
              : "bg-attention-subtle text-attention"
          }`}
        >
          {isCli ? "live · lore CLI" : "mock data"}
        </span>
        <span
          className="flex items-center gap-1.5 text-[11px] text-muted"
          title={`daemon: ${serviceState}`}
        >
          <span
            className={`inline-block h-2 w-2 rounded-full ${
              serviceState === "running"
                ? "bg-success"
                : serviceState === "error"
                  ? "bg-danger"
                  : "bg-faint"
            }`}
          />
          {serviceState}
        </span>
      </div>
    </header>
  );
}

function Segment({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`flex flex-col justify-center px-4 py-2 ${className}`}>
      {children}
    </div>
  );
}
