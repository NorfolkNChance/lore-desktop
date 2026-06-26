/** Presentation helpers shared across components. */

import type { AssetKind, FileChange, LockState } from "@/types/lore";

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

export function shortHash(hash: string, len = 8): string {
  return hash ? hash.slice(0, len) : "—";
}

/** Compact relative time from an ISO-8601 timestamp. */
export function relativeTime(iso: string): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const secs = Math.round((Date.now() - then) / 1000);
  if (secs < 60) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.round(hrs / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(then).toLocaleDateString();
}

/** Tabler-style monogram glyph per asset kind (we use simple letters/emoji-free). */
export const assetGlyph: Record<AssetKind, string> = {
  uasset: "▣",
  umap: "◈",
  blueprint: "❖",
  material: "◐",
  texture: "▦",
  audio: "◧",
  binary: "▪",
  text: "≡",
};

export const assetLabel: Record<AssetKind, string> = {
  uasset: "Unreal asset",
  umap: "Level / map",
  blueprint: "Blueprint",
  material: "Material",
  texture: "Texture",
  audio: "Audio",
  binary: "Binary",
  text: "Text",
};

/** Single-letter change marker + its color classes (GitHub-style gutter). */
export const changeMarker: Record<
  FileChange,
  { letter: string; classes: string; label: string }
> = {
  added: { letter: "A", classes: "text-success", label: "Added" },
  modified: { letter: "M", classes: "text-attention", label: "Modified" },
  deleted: { letter: "D", classes: "text-danger", label: "Deleted" },
  renamed: { letter: "R", classes: "text-accent", label: "Renamed" },
  unchanged: { letter: "·", classes: "text-faint", label: "Unchanged" },
};

/** Lock state -> label + light-theme pill classes. The UI's primary signal. */
export const lockBadge: Record<
  LockState,
  { label: string; classes: string; icon: string }
> = {
  unlocked: {
    label: "Unlocked",
    classes: "bg-inset text-muted",
    icon: "○",
  },
  lockedByMe: {
    label: "Locked by you",
    classes: "bg-success-subtle text-success",
    icon: "●",
  },
  lockedByOther: {
    label: "Locked by other",
    classes: "bg-attention-subtle text-attention",
    icon: "●",
  },
  stale: {
    label: "Stale lock",
    classes: "bg-danger-subtle text-danger",
    icon: "●",
  },
  unknown: {
    label: "Lock state unavailable",
    classes: "bg-attention-subtle text-attention",
    icon: "?",
  },
};

export function fileName(path: string): string {
  const i = path.lastIndexOf("/");
  return i >= 0 ? path.slice(i + 1) : path;
}

export function dirName(path: string): string {
  const i = path.lastIndexOf("/");
  return i >= 0 ? path.slice(0, i + 1) : "";
}
