/**
 * Lore data contracts
 * ====================
 * TypeScript interfaces mirroring Lore's core data model.
 *
 * These are grounded in Lore's actual system design (epicgames.github.io/lore),
 * NOT invented. Key vocabulary notes:
 *
 *  - A "Fragment" is the fundamental content-addressed storage unit: a payload of
 *    bytes plus its BLAKE3 hash. Its address is 48 bytes = 32-byte BLAKE3 hash +
 *    16-byte context tag.
 *  - A "Revision" is a frozen, hash-identified Merkle snapshot with one parent
 *    (ordinary) or two (merge), forming an immutable DAG.
 *  - Lore calls a local working directory an "Instance" (UUIDv7). In this UI we
 *    surface it to users as a "Workspace", but the fields model the real Instance.
 *  - Large files are split into chunks (FastCDC or fixed-size), each chunk stored
 *    as a Fragment, ordered by byte offset to enable sparse reads.
 *  - Locks ("locks for unmergeable content") are first-class: `lore lock
 *    acquire | status | release`. This is the backbone of the binary-first UI.
 *
 * All of these are mirrored 1:1 by serde structs in `src-tauri/src/models.rs`
 * (camelCase rename), so the mock IPC handlers and, later, the real liblore
 * bindings serialize directly into these shapes.
 */

// ---------------------------------------------------------------------------
// Content addressing
// ---------------------------------------------------------------------------

/**
 * A Fragment's address: a 32-byte BLAKE3 hash plus a 16-byte opaque context tag
 * (48 bytes total). The context tag carries entity identity (e.g. file IDs for
 * move/copy/obliterate) WITHOUT affecting deduplication.
 */
export interface FragmentAddress {
  /** Lowercase hex of the 32-byte BLAKE3 content hash. */
  hash: string;
  /** Lowercase hex of the 16-byte context tag. */
  context: string;
}

/** How a large file was split into chunks. */
export type ChunkingStrategy = "fastcdc" | "fixed";

/**
 * A content-addressed payload. For small files the file maps to a single
 * fragment; large files are chunked and reference many fragments by offset.
 */
export interface Fragment {
  address: FragmentAddress;
  /** Size of this fragment's payload in bytes. */
  sizeBytes: number;
  /** 16-byte opaque partition id (authorization boundary), lowercase hex. */
  partition: string;
  /** Present when this fragment is a chunk of a larger file. */
  chunking?: ChunkingStrategy;
  /** Byte offset of this chunk within the logical file, when chunked. */
  offset?: number;
}

// ---------------------------------------------------------------------------
// History: revisions & branches
// ---------------------------------------------------------------------------

export interface Author {
  name: string;
  email: string;
}

/**
 * A frozen snapshot, identified by the hash of its serialized state. References
 * one parent (ordinary) or two (merge). `treeRoot` addresses the Merkle root of
 * the node tree for this revision.
 */
export interface Revision {
  /** Content hash identifying this revision (lowercase hex BLAKE3). */
  id: string;
  /** Parent revision hashes. 1 = ordinary, 2 = merge, 0 = root revision. */
  parents: string[];
  message: string;
  author: Author;
  /** ISO-8601 UTC timestamp. */
  timestamp: string;
  /** Address of the Merkle tree root for this revision's state. */
  treeRoot: FragmentAddress;
  /** Convenience flag: parents.length === 2. */
  isMerge: boolean;
}

/**
 * A named, mutable pointer to a latest revision (Lore's equivalent of HEAD).
 * Has a stable opaque UUIDv7 id and a human-readable name mapped through the
 * mutable store.
 */
export interface Branch {
  /** Stable UUIDv7, immutable for the branch's lifetime. */
  id: string;
  /** Human-readable name (mutable mapping). */
  name: string;
  /** Hash of the revision this branch currently points at. */
  latestRevision: string;
  /** True if direct pushes are blocked (`lore branch protect`). */
  protected: boolean;
}

// ---------------------------------------------------------------------------
// Workspace (Lore "Instance")
// ---------------------------------------------------------------------------

/**
 * A local working directory. Lore calls this an "Instance"; the UI label is
 * "Workspace". Multiple instances can share one on-disk Shared Store.
 */
export interface Workspace {
  /** UUIDv7 identifying this instance. */
  id: string;
  /** Friendly display name. */
  name: string;
  /** Absolute path to the working tree on disk. */
  path: string;
  /** Path to the shared immutable+mutable store backing this instance. */
  sharedStorePath: string;
  /** UUIDv7 of the currently checked-out branch. */
  currentBranchId: string;
  /** Hash of the committed revision the working tree diverges from. */
  currentRevision: string;
  /**
   * The sparse "view" filter (.lore/view): the subset of the repository
   * materialized to disk. Empty array = full hydration.
   */
  view: string[];
  /** True if any tracked file differs from the committed revision. */
  dirty: boolean;
  /** Number of files with recorded staging intent. */
  stagedFileCount: number;
}

// ---------------------------------------------------------------------------
// Locks (binary-first core)
// ---------------------------------------------------------------------------

/**
 * Lock disposition from the perspective of the current instance — the single
 * most important signal in a binary-first UI.
 */
export type LockState =
  | "unlocked"
  | "lockedByMe"
  | "lockedByOther"
  | "stale"
  /** Server unreachable / query timed out — distinct from "unlocked". */
  | "unknown";

/** The authenticated user (for me-vs-others lock attribution). */
export interface Identity {
  userId: string;
  name: string;
  authenticated: boolean;
}

/** A lock held on an unmergeable (typically binary) file. */
export interface Lock {
  /** Repository-relative path of the locked file. */
  path: string;
  state: LockState;
  /** Who holds the lock (absent when unlocked). */
  owner?: Author;
  /** UUIDv7 of the instance that acquired the lock. */
  instanceId?: string;
  /** ISO-8601 UTC time the lock was acquired. */
  acquiredAt?: string;
  /** Optional human note supplied at acquire time. */
  reason?: string;
}

// ---------------------------------------------------------------------------
// File / working-tree status (drives the binary-first file list)
// ---------------------------------------------------------------------------

export type FileChange =
  | "unchanged"
  | "added"
  | "modified"
  | "deleted"
  | "renamed";

/**
 * Recognized Unreal/binary asset kinds, used by the UI to pick icons and to
 * decide whether a text diff is even meaningful.
 */
export type AssetKind =
  | "uasset" // generic Unreal asset
  | "umap" // level / map
  | "blueprint"
  | "material"
  | "texture"
  | "audio"
  | "binary" // other binary
  | "text"; // mergeable text

export interface FileEntry {
  /** Repository-relative path. */
  path: string;
  /** 16-byte context tag acting as a stable file id (hex), for move tracking. */
  fileId: string;
  change: FileChange;
  /** Recorded intent to include in the next revision. */
  staged: boolean;
  /** Differs from committed revision (orthogonal to `staged`). */
  dirty: boolean;
  /** True for unmergeable content; flips the UI from "diff" to "lock" mode. */
  isBinary: boolean;
  assetKind: AssetKind;
  sizeBytes: number;
  /** Number of fragments (chunks) this file resolves to. */
  fragmentCount: number;
  /** Current lock disposition for this path. */
  lockState: LockState;
  /** Full lock record when locked. */
  lock?: Lock;
}

/** Aggregate working-tree status for a workspace. */
export interface WorkspaceStatus {
  workspaceId: string;
  branch: Branch;
  headRevision: Revision;
  entries: FileEntry[];
  /** Convenience counts for header badges. */
  counts: {
    staged: number;
    modified: number;
    lockedByMe: number;
    lockedByOther: number;
  };
  /** False when lock state couldn't be resolved (entries are `unknown`). */
  locksAvailable: boolean;
}

// ---------------------------------------------------------------------------
// Daemon events (pushed from the Rust service over Tauri events)
// ---------------------------------------------------------------------------

/** Mirrors liblore's event tagging used by the SDKs. */
export type LoreEventTag =
  | "lockChanged"
  | "statusChanged"
  | "revisionCommitted"
  | "branchSwitched"
  | "serviceStateChanged"
  | "transferProgress"
  | "log";

export type LoreLogLevel = "trace" | "debug" | "info" | "warn" | "error";

/** Lifecycle state of the local `lore service` daemon. */
export type ServiceState =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "error";

/** Progress for multi-gigabyte streaming transfers (Phase 4). */
export interface TransferProgress {
  /** Stable id for the operation (e.g. a commit or sync). */
  operationId: string;
  label: string;
  bytesDone: number;
  bytesTotal: number;
  /** Fragments processed so far. */
  fragmentsDone: number;
  fragmentsTotal: number;
}

/** A single event pushed from the daemon to the UI. */
export interface LoreEvent {
  tag: LoreEventTag;
  /** ISO-8601 UTC timestamp the event was emitted. */
  timestamp: string;
  level: LoreLogLevel;
  /** Tag-specific payload; consumers narrow on `tag`. */
  payload?: unknown;
}

/** Which backend is serving data. Mirrors Rust `ClientMode`. */
export type ClientMode = "cli" | "ffi" | "mock";

/** Result of a streaming ingest (Phase 4). Mirrors Rust `IngestSummary`. */
export interface IngestSummary {
  operationId: string;
  path: string;
  totalBytes: number;
  fragmentCount: number;
  /** BLAKE3 over the fragment hashes — the asset's content address. */
  rootHash: string;
  chunkSize: number;
  elapsedMs: number;
  /** Peak resident buffer — bounded regardless of file size. */
  peakBufferBytes: number;
}

/** A detected native visual diff tool (Phase 4). Mirrors Rust `DiffToolInfo`. */
export interface DiffTool {
  id: string;
  name: string;
  available: boolean;
  path?: string;
}

/** The Tauri event channel name daemon events are emitted on. */
export const LORE_EVENT_CHANNEL = "lore://event" as const;
