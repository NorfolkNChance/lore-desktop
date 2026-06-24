import { useState, type ReactNode } from "react";
import { useLoreStore } from "@/store/loreStore";

/**
 * GitHub Desktop-style top toolbar: current repository · branch menu (switch /
 * create) · sync + push, plus identity, backend-mode badge, and daemon
 * (watcher) indicator.
 */
export function Toolbar() {
  const {
    workspaces,
    status,
    identity,
    backendMode,
    loreVersion,
    serviceState,
    busy,
    syncRepo,
    pushRepo,
  } = useLoreStore();
  const ws = workspaces[0];
  const branch = status?.branch;
  const isCli = backendMode === "cli";

  return (
    <header className="flex h-13 items-stretch border-b border-line bg-subtle">
      <Segment className="min-w-48 flex-1">
        <div className="text-[11px] text-muted">Current repository</div>
        <div className="flex items-center gap-1.5 font-medium">
          <span aria-hidden>▸</span>
          <span className="truncate">{ws?.name ?? "—"}</span>
        </div>
      </Segment>

      <div className="min-w-44 flex-1 border-l border-line">
        <BranchMenu currentName={branch?.name} protectedBranch={branch?.protected} />
      </div>

      <Segment className="min-w-44 border-l border-line">
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => syncRepo()}
            disabled={!isCli || !!busy}
            title="Sync (lore sync)"
            className="flex items-center gap-1 rounded-md border border-line bg-canvas px-2 py-1 text-[12px] hover:bg-inset disabled:opacity-50"
          >
            <span aria-hidden>↓</span> Sync
          </button>
          <button
            onClick={() => pushRepo()}
            disabled={!isCli || !!busy}
            title="Push origin (lore push)"
            className="flex items-center gap-1 rounded-md border border-line bg-canvas px-2 py-1 text-[12px] hover:bg-inset disabled:opacity-50"
          >
            <span aria-hidden>↑</span> Push
          </button>
        </div>
      </Segment>

      <div className="ml-auto flex items-center gap-3 border-l border-line px-4">
        {busy && <span className="text-[11px] text-muted">{busy}</span>}
        {identity?.authenticated && (
          <span className="text-[11px] text-muted" title={identity.userId}>
            {identity.name}
          </span>
        )}
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
          title={`watcher: ${serviceState}`}
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
          watcher
        </span>
      </div>
    </header>
  );
}

function BranchMenu({
  currentName,
  protectedBranch,
}: {
  currentName?: string;
  protectedBranch?: boolean;
}) {
  const { branches, switchBranch, createBranch } = useLoreStore();
  const [open, setOpen] = useState(false);

  const onCreate = async () => {
    const name = window.prompt("New branch name:");
    setOpen(false);
    if (name && name.trim()) await createBranch(name.trim());
  };

  return (
    <div className="relative h-full">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex h-full w-full flex-col justify-center px-4 py-2 text-left hover:bg-inset"
      >
        <span className="text-[11px] text-muted">Current branch</span>
        <span className="flex items-center gap-1.5 font-medium">
          <span aria-hidden>⎇</span>
          <span className="truncate">{currentName ?? "—"}</span>
          {protectedBranch && (
            <span className="rounded bg-accent-subtle px-1 text-[10px] text-accent">
              protected
            </span>
          )}
          <span className="text-faint">▾</span>
        </span>
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute left-2 top-full z-20 mt-1 w-60 rounded-md border border-line bg-canvas py-1 shadow-lg">
            <div className="px-3 py-1 text-[11px] text-muted">Branches</div>
            {branches.length === 0 && (
              <div className="px-3 py-1.5 text-[12px] text-faint">No branches</div>
            )}
            {branches.map((b) => (
              <button
                key={b.name}
                onClick={() => {
                  setOpen(false);
                  if (b.name !== currentName) switchBranch(b.name);
                }}
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-subtle"
              >
                <span className="w-3 text-accent">
                  {b.name === currentName ? "✓" : ""}
                </span>
                <span className="truncate">{b.name}</span>
                {b.protected && (
                  <span className="ml-auto text-[10px] text-muted">protected</span>
                )}
              </button>
            ))}
            <div className="my-1 border-t border-line-muted" />
            <button
              onClick={onCreate}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-accent hover:bg-subtle"
            >
              <span aria-hidden>+</span> New branch…
            </button>
          </div>
        </>
      )}
    </div>
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
