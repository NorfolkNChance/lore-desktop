import { useEffect } from "react";
import { useLoreStore } from "@/store/loreStore";
import { Toolbar } from "@/components/Toolbar";
import { ChangesList } from "@/components/ChangesList";
import { HistoryList } from "@/components/HistoryList";
import { CommitBox } from "@/components/CommitBox";
import { DetailPane } from "@/components/DetailPane";
import { TransferOverlay } from "@/components/TransferOverlay";

/**
 * GitHub Desktop-style shell: top toolbar, a left sidebar (Changes / History +
 * commit box) and a right detail pane that emphasizes binary asset lock state
 * over text diffs. State is driven by IPC + live daemon events.
 */
export default function App() {
  const {
    status,
    activeTab,
    loading,
    error,
    lastEvent,
    bootstrap,
    subscribeToEvents,
    setTab,
    dismissError,
  } = useLoreStore();

  useEffect(() => {
    void bootstrap();
    const unlistenP = subscribeToEvents();
    return () => {
      void unlistenP.then((un) => un());
    };
  }, [bootstrap, subscribeToEvents]);

  const counts = status?.counts;

  return (
    <div className="flex h-full flex-col bg-canvas text-fg">
      <Toolbar />

      {error && (
        <div className="flex items-center gap-2 border-b border-danger/30 bg-danger-subtle px-4 py-1.5 text-[12px] text-danger">
          <span className="flex-1">{error}</span>
          <button onClick={dismissError} aria-label="dismiss" className="px-1">
            ✕
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {/* Sidebar */}
        <aside className="flex w-80 flex-col border-r border-line bg-canvas">
          <div className="flex border-b border-line text-[13px]">
            <Tab
              active={activeTab === "changes"}
              onClick={() => setTab("changes")}
              label={`Changes${counts ? ` (${status!.entries.length})` : ""}`}
            />
            <Tab
              active={activeTab === "history"}
              onClick={() => setTab("history")}
              label="History"
            />
          </div>

          {loading && !status ? (
            <div className="flex flex-1 items-center justify-center text-muted">
              Loading…
            </div>
          ) : activeTab === "changes" ? (
            <>
              <ChangesList />
              <CommitBox />
            </>
          ) : (
            <HistoryList />
          )}
        </aside>

        {/* Detail */}
        <main className="min-w-0 flex-1 bg-canvas">
          <DetailPane />
        </main>
      </div>

      <TransferOverlay />

      <footer className="flex items-center justify-between border-t border-line bg-subtle px-4 py-1 text-[11px] text-muted">
        <span>
          {counts
            ? `${counts.staged} staged · ${counts.modified} modified · ${counts.lockedByMe} locked by you · ${counts.lockedByOther} by others`
            : "—"}
        </span>
        <span>
          {lastEvent
            ? `last event: ${lastEvent.tag} @ ${new Date(lastEvent.timestamp).toLocaleTimeString()}`
            : "no events yet"}
        </span>
      </footer>
    </div>
  );
}

function Tab({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex-1 py-2 text-center font-medium ${
        active
          ? "border-b-2 border-accent text-fg"
          : "text-muted hover:text-fg"
      }`}
    >
      {label}
    </button>
  );
}
