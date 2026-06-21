import { useLoreStore } from "@/store/loreStore";
import { formatBytes, shortHash } from "@/lib/format";

/**
 * Live progress for streaming ingest (Phase 4). Shows a bounded-memory stream's
 * progress as `transferProgress` events arrive, then a result card with the
 * computed fragment breakdown. Docked above the footer.
 */
export function TransferOverlay() {
  const { transfer, ingestSummary, clearIngest } = useLoreStore();
  if (!transfer && !ingestSummary) return null;

  if (ingestSummary) {
    const s = ingestSummary;
    const mbps =
      s.elapsedMs > 0 ? (s.totalBytes / 1024 / 1024 / (s.elapsedMs / 1000)) : 0;
    return (
      <div className="flex items-center gap-3 border-t border-success/30 bg-success-subtle px-4 py-2 text-[12px]">
        <span className="text-success" aria-hidden>
          ✓
        </span>
        <span className="text-success">
          Streamed <strong>{formatBytes(s.totalBytes)}</strong> into{" "}
          <strong>{s.fragmentCount.toLocaleString()}</strong> fragments
        </span>
        <span className="text-muted">·</span>
        <span className="mono text-muted">blake3 {shortHash(s.rootHash, 12)}</span>
        <span className="text-muted">·</span>
        <span className="text-muted">
          {mbps.toFixed(0)} MiB/s · peak RAM {formatBytes(s.peakBufferBytes)} ·{" "}
          {s.elapsedMs} ms
        </span>
        <button onClick={clearIngest} className="ml-auto px-1 text-muted" aria-label="dismiss">
          ✕
        </button>
      </div>
    );
  }

  const t = transfer!;
  const pct = t.bytesTotal > 0 ? Math.min(100, (t.bytesDone / t.bytesTotal) * 100) : 0;
  return (
    <div className="border-t border-line bg-subtle px-4 py-2">
      <div className="mb-1 flex items-center gap-2 text-[12px]">
        <span className="font-medium">Streaming into fragments…</span>
        <span className="text-muted">
          {formatBytes(t.bytesDone)} / {formatBytes(t.bytesTotal)}
        </span>
        <span className="text-muted">·</span>
        <span className="text-muted">{t.fragmentsDone.toLocaleString()} fragments</span>
        <span className="ml-auto font-medium text-accent">{pct.toFixed(0)}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-inset">
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-150"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
