//! The single backend→frontend event channel.
//!
//! Every push to the UI goes through [`emit`] on [`LORE_EVENT_CHANNEL`]. Each
//! tag's payload is typed by [`LoreEventKind`]; the wire format is
//! `{ tag, payload, timestamp, level }` and the frontend narrows it with a
//! discriminated union (`LoreEvent` in `src/types/lore.ts`). Adding an event
//! means adding a variant here and a union arm there — this replaces the
//! ad-hoc `json!` payloads and per-module copies of the channel constant that
//! previously had to agree by convention.

use crate::models::{LockState, LoreLogLevel, ServiceState, TransferProgress};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const LORE_EVENT_CHANNEL: &str = "lore://event";

/// Tag + typed payload for every event the backend can push.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tag", content = "payload", rename_all = "camelCase")]
pub enum LoreEventKind {
    /// A lock was acquired/released on `path`, or — with both fields absent —
    /// the watcher's periodic "re-query server locks" tick.
    LockChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<LockState>,
    },
    /// The working tree changed; `source` records who noticed (watcher,
    /// stage/unstage, sync, …) for debugging.
    StatusChanged { source: String },
    /// A revision landed locally or remotely (commit / push / clone);
    /// `message` is present for commits.
    RevisionCommitted {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A branch was created or switched to.
    BranchSwitched { branch: String },
    ServiceStateChanged { state: ServiceState },
    TransferProgress(TransferProgress),
}

/// The envelope crossing the Tauri event channel: `kind` flattens to
/// `tag` + `payload` alongside the metadata fields.
#[derive(Debug, Clone, Serialize)]
pub struct LoreEvent {
    #[serde(flatten)]
    pub kind: LoreEventKind,
    pub timestamp: String,
    pub level: LoreLogLevel,
}

/// Emit `kind` to the frontend, stamped with the current time at Info level.
pub fn emit(app: &AppHandle, kind: LoreEventKind) {
    let event = LoreEvent {
        kind,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: LoreLogLevel::Info,
    };
    let _ = app.emit(LORE_EVENT_CHANNEL, event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(kind: LoreEventKind) -> serde_json::Value {
        serde_json::to_value(LoreEvent {
            kind,
            timestamp: "2026-07-02T00:00:00Z".into(),
            level: LoreLogLevel::Info,
        })
        .unwrap()
    }

    // These pin the wire format the TS discriminated union relies on
    // (src/types/lore.ts). If a variant or field changes shape, fail here —
    // not silently at runtime in the webview.

    #[test]
    fn wire_format_is_tag_plus_payload_envelope() {
        let v = envelope(LoreEventKind::LockChanged {
            path: Some("Content/A.umap".into()),
            state: Some(LockState::LockedByMe),
        });
        assert_eq!(
            v,
            json!({
                "tag": "lockChanged",
                "payload": { "path": "Content/A.umap", "state": "lockedByMe" },
                "timestamp": "2026-07-02T00:00:00Z",
                "level": "info",
            })
        );
    }

    #[test]
    fn poll_tick_lock_event_omits_absent_fields() {
        let v = envelope(LoreEventKind::LockChanged { path: None, state: None });
        assert_eq!(v["payload"], json!({}));
    }

    #[test]
    fn transfer_progress_payload_matches_the_ts_interface() {
        let v = envelope(LoreEventKind::TransferProgress(TransferProgress {
            operation_id: "op-1".into(),
            label: "file.uasset".into(),
            bytes_done: 8,
            bytes_total: 16,
            fragments_done: 1,
            fragments_total: 2,
        }));
        assert_eq!(v["tag"], "transferProgress");
        assert_eq!(
            v["payload"],
            json!({
                "operationId": "op-1",
                "label": "file.uasset",
                "bytesDone": 8,
                "bytesTotal": 16,
                "fragmentsDone": 1,
                "fragmentsTotal": 2,
            })
        );
    }

    #[test]
    fn service_state_payload_is_camel_cased() {
        let v = envelope(LoreEventKind::ServiceStateChanged { state: ServiceState::Running });
        assert_eq!(v["tag"], "serviceStateChanged");
        assert_eq!(v["payload"], json!({ "state": "running" }));
    }
}
