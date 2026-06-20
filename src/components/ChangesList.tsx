import { useLoreStore } from "@/store/loreStore";
import { changeMarker, dirName, fileName, lockBadge } from "@/lib/format";
import type { FileEntry } from "@/types/lore";

/** The "Changes" tab: a binary-first list of working-tree files. */
export function ChangesList() {
  const { status, selectedPath, selectFile, setStaged } = useLoreStore();
  const entries = status?.entries ?? [];

  if (entries.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-center text-muted">
        <div>
          <div className="mb-1 text-2xl">✓</div>
          No local changes
        </div>
      </div>
    );
  }

  return (
    <ul className="flex-1 overflow-auto">
      {entries.map((entry) => (
        <FileRow
          key={entry.path}
          entry={entry}
          selected={entry.path === selectedPath}
          onSelect={() => selectFile(entry.path)}
          onToggleStage={() => setStaged(entry.path, !entry.staged)}
        />
      ))}
    </ul>
  );
}

function FileRow({
  entry,
  selected,
  onSelect,
  onToggleStage,
}: {
  entry: FileEntry;
  selected: boolean;
  onSelect: () => void;
  onToggleStage: () => void;
}) {
  const marker = changeMarker[entry.change];
  const badge = lockBadge[entry.lockState];
  const lockedByOther = entry.lockState === "lockedByOther";

  return (
    <li
      onClick={onSelect}
      className={`flex cursor-pointer items-center gap-2 border-l-2 px-3 py-1.5 ${
        selected
          ? "border-accent bg-accent-subtle"
          : "border-transparent hover:bg-subtle"
      }`}
    >
      <input
        type="checkbox"
        checked={entry.staged}
        onClick={(e) => e.stopPropagation()}
        onChange={onToggleStage}
        aria-label={`stage ${entry.path}`}
        className="h-3.5 w-3.5 shrink-0 accent-[var(--color-accent)]"
      />

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1">
          <span className="mono truncate text-[13px] text-fg">
            {fileName(entry.path)}
          </span>
        </div>
        <div className="mono truncate text-[11px] text-faint">
          {dirName(entry.path)}
        </div>
      </div>

      {/* lock dot — the dominant binary-first signal */}
      <span
        title={badge.label}
        className={`rounded-full px-1.5 py-0.5 text-[10px] font-medium ${badge.classes}`}
      >
        {lockedByOther ? "🔒 other" : entry.lockState === "lockedByMe" ? "🔒 you" : "○"}
      </span>

      <span
        title={marker.label}
        className={`mono w-3 text-center text-[13px] font-semibold ${marker.classes}`}
      >
        {marker.letter}
      </span>
    </li>
  );
}
