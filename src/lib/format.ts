/** Small presentation helpers shared across components. */

import type { AssetKind, LockState } from "@/types/lore";

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[i]}`;
}

export function shortHash(hash: string, len = 10): string {
  return hash.slice(0, len);
}

export const assetGlyph: Record<AssetKind, string> = {
  uasset: "▣",
  umap: "🗺",
  blueprint: "🧩",
  material: "🎨",
  texture: "🖼",
  audio: "🔊",
  binary: "📦",
  text: "📄",
};

/** Tailwind utility classes for each lock state — the UI's primary signal. */
export const lockBadge: Record<
  LockState,
  { label: string; classes: string }
> = {
  unlocked: {
    label: "Unlocked",
    classes: "bg-zinc-700/40 text-zinc-300 ring-1 ring-zinc-600",
  },
  lockedByMe: {
    label: "Locked by you",
    classes: "bg-emerald-500/15 text-emerald-300 ring-1 ring-emerald-500/40",
  },
  lockedByOther: {
    label: "Locked by other",
    classes: "bg-amber-500/15 text-amber-300 ring-1 ring-amber-500/40",
  },
  stale: {
    label: "Stale lock",
    classes: "bg-rose-500/15 text-rose-300 ring-1 ring-rose-500/40",
  },
};
