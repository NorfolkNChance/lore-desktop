import { useLoreStore } from "@/store/loreStore";
import { relativeTime, shortHash } from "@/lib/format";

/** The "History" tab: the revision DAG as a flat list (newest first). */
export function HistoryList() {
  const revisions = useLoreStore((s) => s.revisions);

  if (revisions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-6 text-muted">
        No revisions yet
      </div>
    );
  }

  return (
    <ul className="flex-1 overflow-auto">
      {revisions.map((rev) => (
        <li
          key={rev.id}
          className="border-b border-line-muted px-3 py-2 hover:bg-subtle"
        >
          <div className="flex items-center gap-2">
            <span className="text-faint" aria-hidden>
              {rev.isMerge ? "⑃" : "●"}
            </span>
            <span className="truncate text-[13px] text-fg">
              {rev.message || "(no message)"}
            </span>
          </div>
          <div className="mt-0.5 flex items-center gap-2 pl-6 text-[11px] text-muted">
            <span className="mono">{shortHash(rev.id)}</span>
            <span>·</span>
            <span>{rev.author.name || "unknown"}</span>
            {rev.timestamp && (
              <>
                <span>·</span>
                <span>{relativeTime(rev.timestamp)}</span>
              </>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}
