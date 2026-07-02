/**
 * Lore data contracts
 * ====================
 * The types in `./bindings/` are GENERATED from the Rust serde structs
 * (`src-tauri/src/models.rs` and friends) by ts-rs. To change a contract,
 * edit the Rust type, then regenerate with:
 *
 *   (cd src-tauri && cargo test --lib export_typescript_bindings)
 *
 * CI fails when the committed bindings drift from the Rust structs — never
 * edit the generated files by hand.
 *
 * Vocabulary (grounded in Lore's system design, epicgames.github.io/lore):
 *  - A "Fragment" is the content-addressed storage unit; its 48-byte address
 *    is a 32-byte BLAKE3 hash + 16-byte context tag.
 *  - A "Revision" is a frozen Merkle snapshot with 1 parent (ordinary) or
 *    2 (merge), forming an immutable DAG.
 *  - Lore calls a local working directory an "Instance" (UUIDv7); the UI
 *    surfaces it as a "Workspace".
 *  - Locks on unmergeable content are first-class — the backbone of the
 *    binary-first UI.
 */

export type { FragmentAddress } from "./bindings/FragmentAddress";
export type { ChunkingStrategy } from "./bindings/ChunkingStrategy";
export type { Fragment } from "./bindings/Fragment";
export type { Author } from "./bindings/Author";
export type { Revision } from "./bindings/Revision";
export type { Branch } from "./bindings/Branch";
export type { Workspace } from "./bindings/Workspace";
export type { LockState } from "./bindings/LockState";
export type { Identity } from "./bindings/Identity";
export type { Lock } from "./bindings/Lock";
export type { FileChange } from "./bindings/FileChange";
export type { AssetKind } from "./bindings/AssetKind";
export type { FileEntry } from "./bindings/FileEntry";
export type { StatusCounts } from "./bindings/StatusCounts";
export type { WorkspaceStatus } from "./bindings/WorkspaceStatus";
export type { ServiceState } from "./bindings/ServiceState";
export type { LoreLogLevel } from "./bindings/LoreLogLevel";
export type { TransferProgress } from "./bindings/TransferProgress";
export type { ClientMode } from "./bindings/ClientMode";
export type { DiffTool } from "./bindings/DiffTool";
export type { IngestSummary } from "./bindings/IngestSummary";

import type { LockState } from "./bindings/LockState";
import type { LoreLogLevel } from "./bindings/LoreLogLevel";
import type { ServiceState } from "./bindings/ServiceState";
import type { TransferProgress } from "./bindings/TransferProgress";

/**
 * A single event pushed from the daemon to the UI. Hand-written rather than
 * generated: it mirrors Rust `events::LoreEventKind`, whose wire format is
 * pinned by unit tests in `src-tauri/src/events.rs`. The payload is typed per
 * tag, so consumers narrow on `tag` without casting.
 */
export type LoreEvent = {
  /** ISO-8601 UTC timestamp the event was emitted. */
  timestamp: string;
  level: LoreLogLevel;
} & (
  | {
      /** A lock changed on `path`, or a periodic re-query tick (fields absent). */
      tag: "lockChanged";
      payload: { path?: string; state?: LockState };
    }
  | {
      /** The working tree changed; `source` says who noticed. */
      tag: "statusChanged";
      payload: { source: string };
    }
  | {
      /** A revision landed (commit/push/clone); `message` for commits. */
      tag: "revisionCommitted";
      payload: { message?: string };
    }
  | { tag: "branchSwitched"; payload: { branch: string } }
  | { tag: "serviceStateChanged"; payload: { state: ServiceState } }
  | { tag: "transferProgress"; payload: TransferProgress }
);

/** The set of event tags, derived from the union. */
export type LoreEventTag = LoreEvent["tag"];

/** The Tauri event channel name daemon events are emitted on. */
export const LORE_EVENT_CHANNEL = "lore://event" as const;
