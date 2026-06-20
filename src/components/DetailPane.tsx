import { useLoreStore } from "@/store/loreStore";
import {
  assetGlyph,
  assetLabel,
  changeMarker,
  formatBytes,
  lockBadge,
  relativeTime,
} from "@/lib/format";
import type { FileEntry } from "@/types/lore";

/**
 * The right pane. Where GitHub Desktop shows a text diff, a binary-first VCS
 * shows the asset's lock state and metadata with a launcher into a native
 * visual diff tool. Text files fall back to a (placeholder) text view.
 */
export function DetailPane() {
  const { status, selectedPath } = useLoreStore();
  const entry = status?.entries.find((e) => e.path === selectedPath) ?? null;

  if (!entry) {
    return (
      <div className="flex h-full items-center justify-center text-muted">
        Select a file to view details
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <FileHeader entry={entry} />
      <LockPanel entry={entry} />
      <div className="flex-1 overflow-auto p-4">
        {entry.isBinary ? <BinaryBody entry={entry} /> : <TextBody entry={entry} />}
      </div>
    </div>
  );
}

function FileHeader({ entry }: { entry: FileEntry }) {
  const marker = changeMarker[entry.change];
  return (
    <div className="border-b border-line px-4 py-3">
      <div className="flex items-center gap-2">
        <span className="text-lg text-muted" aria-hidden>
          {assetGlyph[entry.assetKind]}
        </span>
        <span className="mono truncate text-[13px] font-medium">{entry.path}</span>
      </div>
      <div className="mt-1 flex items-center gap-3 text-[11px] text-muted">
        <span className={marker.classes}>{marker.label}</span>
        <span>·</span>
        <span>{assetLabel[entry.assetKind]}</span>
        <span>·</span>
        <span>{formatBytes(entry.sizeBytes)}</span>
        {entry.fragmentCount > 0 && (
          <>
            <span>·</span>
            <span>{entry.fragmentCount} fragments</span>
          </>
        )}
      </div>
    </div>
  );
}

function LockPanel({ entry }: { entry: FileEntry }) {
  const { acquireLock, releaseLock } = useLoreStore();
  const badge = lockBadge[entry.lockState];
  const lock = entry.lock;

  return (
    <div className="flex items-center gap-3 border-b border-line px-4 py-3">
      <span
        className={`flex h-8 w-8 items-center justify-center rounded-full text-sm ${badge.classes}`}
        aria-hidden
      >
        {badge.icon}
      </span>
      <div className="flex-1">
        <div className={`text-[13px] font-medium ${badge.classes.split(" ")[1]}`}>
          {badge.label}
        </div>
        <div className="text-[11px] text-muted">
          {lock?.owner
            ? `${lock.owner.name}${
                lock.acquiredAt ? ` · ${relativeTime(lock.acquiredAt)}` : ""
              }${lock.reason ? ` · "${lock.reason}"` : ""}`
            : "No exclusive lock held"}
        </div>
      </div>

      {entry.lockState === "lockedByMe" ? (
        <button
          onClick={() => releaseLock(entry.path)}
          className="rounded-md border border-line bg-canvas px-3 py-1.5 text-[12px] font-medium hover:bg-subtle"
        >
          Release lock
        </button>
      ) : entry.lockState === "lockedByOther" ? (
        <button
          disabled
          title={lock?.owner ? `Held by ${lock.owner.name}` : undefined}
          className="cursor-not-allowed rounded-md border border-line px-3 py-1.5 text-[12px] text-faint"
        >
          Locked
        </button>
      ) : (
        <button
          onClick={() => acquireLock(entry.path)}
          className="rounded-md bg-accent px-3 py-1.5 text-[12px] font-medium text-white hover:brightness-95"
        >
          Lock for edit
        </button>
      )}
    </div>
  );
}

function BinaryBody({ entry }: { entry: FileEntry }) {
  const { diffTools, openAssetDiff, ingestAsset } = useLoreStore();
  const available = diffTools.filter((t) => t.available);

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-line p-8 text-center">
      <div className="text-4xl text-faint" aria-hidden>
        {assetGlyph[entry.assetKind]}
      </div>
      <div className="text-[13px] font-medium">Binary asset — no text diff</div>
      <p className="max-w-sm text-[12px] text-muted">
        Lore stores this as deduplicated, content-addressed fragments. Compare
        revisions with a native visual diff tool, or stream it into fragments.
      </p>

      <div className="mt-1 flex flex-wrap items-center justify-center gap-2">
        <button
          onClick={() => openAssetDiff(entry.path)}
          disabled={available.length === 0}
          className="rounded-md border border-line bg-canvas px-3 py-1.5 text-[12px] font-medium hover:bg-subtle disabled:opacity-50"
        >
          ⇄ Open in visual diff tool
        </button>
        <button
          onClick={() => ingestAsset(entry.path)}
          className="rounded-md bg-accent px-3 py-1.5 text-[12px] font-medium text-white hover:brightness-95"
        >
          ⤓ Stream into fragments
        </button>
      </div>

      {available.length > 0 ? (
        <div className="flex flex-wrap items-center justify-center gap-1.5 text-[11px] text-muted">
          <span>diff via:</span>
          {available.map((t) => (
            <button
              key={t.id}
              onClick={() => openAssetDiff(entry.path, t.id)}
              title={t.path}
              className="rounded bg-inset px-1.5 py-0.5 hover:bg-line-muted"
            >
              {t.name}
            </button>
          ))}
        </div>
      ) : (
        <div className="text-[11px] text-faint">
          No diff tool detected (FileMerge / P4Merge / Beyond Compare / VS Code)
        </div>
      )}
    </div>
  );
}

function TextBody({ entry }: { entry: FileEntry }) {
  return (
    <div className="rounded-lg border border-line">
      <div className="border-b border-line bg-subtle px-3 py-1.5 text-[11px] text-muted">
        Text file · {formatBytes(entry.sizeBytes)}
      </div>
      <div className="mono p-3 text-[12px] text-muted">
        Text diff view — line-by-line diff renders here (secondary to the
        binary-first workflow).
      </div>
    </div>
  );
}
