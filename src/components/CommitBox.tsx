import { useState } from "react";
import { useLoreStore } from "@/store/loreStore";

/** The pinned commit box at the bottom of the sidebar (GitHub Desktop staple). */
export function CommitBox() {
  const { status, committing, commit } = useLoreStore();
  const [summary, setSummary] = useState("");
  const [description, setDescription] = useState("");

  const stagedCount = status?.counts.staged ?? 0;
  const branchName = status?.branch.name ?? "main";
  const canCommit = stagedCount > 0 && summary.trim().length > 0 && !committing;

  const onCommit = async () => {
    const message = description.trim()
      ? `${summary.trim()}\n\n${description.trim()}`
      : summary.trim();
    const ok = await commit(message);
    if (ok) {
      setSummary("");
      setDescription("");
    }
  };

  return (
    <div className="border-t border-line bg-subtle p-3">
      <input
        value={summary}
        onChange={(e) => setSummary(e.target.value)}
        placeholder={
          stagedCount > 0 ? "Summary (required)" : "Stage files to commit"
        }
        disabled={stagedCount === 0}
        className="mb-2 w-full rounded-md border border-line bg-canvas px-2 py-1.5 text-[13px] outline-none focus:border-accent disabled:opacity-60"
      />
      <textarea
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description"
        rows={2}
        disabled={stagedCount === 0}
        className="mb-2 w-full resize-none rounded-md border border-line bg-canvas px-2 py-1.5 text-[13px] outline-none focus:border-accent disabled:opacity-60"
      />
      <button
        onClick={onCommit}
        disabled={!canCommit}
        className="w-full rounded-md bg-success px-3 py-1.5 text-[13px] font-medium text-white enabled:hover:brightness-95 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {committing
          ? "Committing…"
          : `Commit ${stagedCount > 0 ? stagedCount : ""} to ${branchName}`}
      </button>
    </div>
  );
}
