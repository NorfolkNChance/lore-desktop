import { useEffect } from "react";
import { useLoreStore } from "@/store/loreStore";
import { assetGlyph, formatBytes, lockBadge, shortHash } from "@/lib/format";
import type { FileEntry } from "@/types/lore";

/**
 * Phase 1 demo shell.
 *
 * This is intentionally a thin vertical slice — not the final UI (that's
 * Phase 3). Its job is to prove the full loop works against the mock backend:
 * bootstrap over IPC, render the binary-first file list, acquire/release locks
 * (which round-trip through Rust and emit a daemon event that refreshes state).
 */
export default function App() {
  const {
    status,
    serviceState,
    backendMode,
    loreVersion,
    loading,
    error,
    lastEvent,
    bootstrap,
    subscribeToEvents,
    acquireLock,
    releaseLock,
  } = useLoreStore();

  useEffect(() => {
    void bootstrap();
    const unlistenP = subscribeToEvents();
    return () => {
      void unlistenP.then((un) => un());
    };
  }, [bootstrap, subscribeToEvents]);

  return (
    <div className="flex h-full flex-col">
      <Header
        serviceState={serviceState}
        backendMode={backendMode}
        loreVersion={loreVersion}
      />
      {error && (
        <div className="border-b border-rose-500/40 bg-rose-500/10 px-4 py-2 text-sm text-rose-300">
          {error}
        </div>
      )}
      <main className="flex-1 overflow-auto p-4">
        {loading && !status ? (
          <p className="text-zinc-400">Loading workspace…</p>
        ) : status ? (
          <>
            <StatusBar />
            <ul className="mt-4 space-y-2">
              {status.entries.map((entry) => (
                <FileRow
                  key={entry.path}
                  entry={entry}
                  onAcquire={() => acquireLock(entry.path)}
                  onRelease={() => releaseLock(entry.path)}
                />
              ))}
            </ul>
          </>
        ) : (
          <p className="text-zinc-400">No workspace.</p>
        )}
      </main>
      <footer className="border-t border-zinc-800 px-4 py-1.5 text-xs text-zinc-500">
        {lastEvent
          ? `last event: ${lastEvent.tag} @ ${lastEvent.timestamp}`
          : "no events yet"}
      </footer>
    </div>
  );
}

function Header({
  serviceState,
  backendMode,
  loreVersion,
}: {
  serviceState: string;
  backendMode: string;
  loreVersion: string | null;
}) {
  const dot =
    serviceState === "running"
      ? "bg-emerald-400"
      : serviceState === "error"
        ? "bg-rose-400"
        : "bg-zinc-500";
  const isCli = backendMode === "cli";
  return (
    <header className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
      <div className="flex items-center gap-3">
        <h1 className="text-sm font-semibold tracking-wide text-zinc-100">
          Lore Desktop
        </h1>
        <span
          title={loreVersion ?? undefined}
          className={`rounded px-1.5 py-0.5 text-xs ring-1 ${
            isCli
              ? "bg-emerald-500/15 text-emerald-300 ring-emerald-500/40"
              : "bg-amber-500/15 text-amber-300 ring-amber-500/40"
          }`}
        >
          {isCli ? "live · lore CLI" : "mock data"}
        </span>
      </div>
      <div className="flex items-center gap-2 text-xs text-zinc-400">
        <span className={`h-2 w-2 rounded-full ${dot}`} />
        daemon: {serviceState}
      </div>
    </header>
  );
}

function StatusBar() {
  const status = useLoreStore((s) => s.status);
  if (!status) return null;
  const { branch, headRevision, counts } = status;
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/50 px-4 py-3 text-sm">
      <span className="font-medium text-zinc-100">⎇ {branch.name}</span>
      {branch.protected && (
        <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-xs text-sky-300 ring-1 ring-sky-500/30">
          protected
        </span>
      )}
      <span className="text-zinc-500">·</span>
      <span className="font-mono text-xs text-zinc-400">
        {shortHash(headRevision.id)} {headRevision.message}
      </span>
      <span className="ml-auto flex gap-2 text-xs">
        <Badge n={counts.staged} label="staged" tone="text-sky-300" />
        <Badge n={counts.modified} label="modified" tone="text-amber-300" />
        <Badge n={counts.lockedByMe} label="locked by you" tone="text-emerald-300" />
        <Badge
          n={counts.lockedByOther}
          label="locked by others"
          tone="text-amber-300"
        />
      </span>
    </div>
  );
}

function Badge({ n, label, tone }: { n: number; label: string; tone: string }) {
  return (
    <span className="rounded bg-zinc-800/60 px-2 py-0.5">
      <span className={`font-semibold ${tone}`}>{n}</span>{" "}
      <span className="text-zinc-400">{label}</span>
    </span>
  );
}

function FileRow({
  entry,
  onAcquire,
  onRelease,
}: {
  entry: FileEntry;
  onAcquire: () => void;
  onRelease: () => void;
}) {
  const badge = lockBadge[entry.lockState];
  const isLockedByOther = entry.lockState === "lockedByOther";
  const isLockedByMe = entry.lockState === "lockedByMe";

  return (
    <li
      className={`flex items-center gap-3 rounded-lg border bg-zinc-900/40 px-3 py-2.5 ${
        isLockedByOther
          ? "border-amber-500/30"
          : entry.dirty
            ? "border-zinc-700"
            : "border-zinc-800"
      }`}
    >
      <span className="text-lg" title={entry.assetKind}>
        {assetGlyph[entry.assetKind]}
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate font-mono text-sm text-zinc-100">
          {entry.path}
        </div>
        <div className="flex gap-3 text-xs text-zinc-500">
          <span className="uppercase tracking-wide">{entry.change}</span>
          <span>{formatBytes(entry.sizeBytes)}</span>
          <span>{entry.fragmentCount} fragments</span>
          {entry.isBinary && <span className="text-amber-400/70">binary</span>}
          {entry.staged && <span className="text-sky-400/80">staged</span>}
        </div>
      </div>

      <span
        className={`rounded-full px-2.5 py-1 text-xs font-medium ${badge.classes}`}
      >
        {badge.label}
      </span>

      {isLockedByMe ? (
        <button
          onClick={onRelease}
          className="rounded-md bg-zinc-800 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-700"
        >
          Release
        </button>
      ) : isLockedByOther ? (
        <button
          disabled
          title={
            entry.lock?.owner
              ? `Held by ${entry.lock.owner.name}`
              : "Locked by another user"
          }
          className="cursor-not-allowed rounded-md bg-zinc-800/50 px-3 py-1.5 text-xs text-zinc-500"
        >
          Locked
        </button>
      ) : (
        <button
          onClick={onAcquire}
          className="rounded-md bg-emerald-600/80 px-3 py-1.5 text-xs text-white hover:bg-emerald-600"
        >
          Lock
        </button>
      )}
    </li>
  );
}
