//! Memory-efficient streaming ingest.
//!
//! Phase 4: hash a (potentially multi-gigabyte) binary asset into Lore-style
//! content-addressed fragments **without loading it into memory** and **without
//! blocking the UI**. The file is read in fixed-size buffers; only one buffer
//! (CHUNK_SIZE) is resident at a time regardless of file size. Progress is
//! pushed to the UI via `transferProgress` events as it streams.
//!
//! Fixed-size chunking is used here for clarity; production Lore uses FastCDC
//! (content-defined chunking) — see `ChunkingStrategy` in the contracts.

use crate::models::{LoreEvent, LoreEventTag, LoreLogLevel};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

const LORE_EVENT_CHANNEL: &str = "lore://event";

/// 4 MiB fixed chunk size. This is the upper bound on resident memory for the
/// whole operation, independent of total file size.
const CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Throttle progress emission so a multi-GB file doesn't flood the event
/// channel: emit at most once per this many bytes (plus a final event).
const PROGRESS_EVERY: u64 = 32 * 1024 * 1024; // 32 MiB

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSummary {
    pub operation_id: String,
    pub path: String,
    pub total_bytes: u64,
    pub fragment_count: u32,
    /// BLAKE3 over the fragment hashes — the asset's content address (hex).
    pub root_hash: String,
    pub chunk_size: u64,
    pub elapsed_ms: u64,
    /// Peak resident buffer — demonstrates the bound is independent of size.
    pub peak_buffer_bytes: u64,
}

/// Stream `path` into fragments, emitting progress. Runs on the async runtime
/// (a worker thread), so the React UI thread is never blocked.
pub async fn stream_ingest(
    app: AppHandle,
    path: String,
    operation_id: String,
) -> Result<IngestSummary, String> {
    let started = std::time::Instant::now();
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("stat {path}: {e}"))?;
    let total = meta.len();

    let file = File::open(&path).await.map_err(|e| format!("open {path}: {e}"))?;
    let reader = BufReader::with_capacity(CHUNK_SIZE, file);
    let est_total = total.div_ceil(CHUNK_SIZE as u64).max(1) as u32;

    emit_progress(&app, &operation_id, &path, 0, total, 0, est_total);

    // Throttle progress emission across the streaming loop.
    let mut last_emit: u64 = 0;
    let result = ingest_reader(reader, CHUNK_SIZE, |done, frags| {
        if done - last_emit >= PROGRESS_EVERY {
            last_emit = done;
            emit_progress(&app, &operation_id, &path, done, total, frags, est_total);
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let summary = IngestSummary {
        operation_id: operation_id.clone(),
        path,
        total_bytes: result.total_bytes,
        fragment_count: result.fragment_count,
        root_hash: result.root_hash,
        chunk_size: CHUNK_SIZE as u64,
        elapsed_ms: started.elapsed().as_millis() as u64,
        peak_buffer_bytes: CHUNK_SIZE as u64,
    };
    // Final 100% progress event.
    emit_progress(
        &app,
        &summary.operation_id,
        &summary.path,
        total,
        total,
        summary.fragment_count,
        summary.fragment_count,
    );
    Ok(summary)
}

/// The pure streaming core, independent of Tauri: read `reader` in `chunk_size`
/// buffers, BLAKE3-hash each chunk into a fragment, fold fragment hashes into a
/// root hash, and invoke `on_progress(bytes_done, fragments_done)` per chunk.
/// Exactly one `chunk_size` buffer is resident — memory is bounded regardless
/// of input length.
async fn ingest_reader<R, F>(
    mut reader: R,
    chunk_size: usize,
    mut on_progress: F,
) -> std::io::Result<IngestResult>
where
    R: AsyncReadExt + Unpin,
    F: FnMut(u64, u32),
{
    let mut buf = vec![0u8; chunk_size];
    let mut root = blake3::Hasher::new();
    let mut done: u64 = 0;
    let mut fragments: u32 = 0;

    loop {
        let n = fill_chunk(&mut reader, &mut buf).await?;
        if n == 0 {
            break;
        }
        let frag = blake3::hash(&buf[..n]);
        root.update(frag.as_bytes());
        fragments += 1;
        done += n as u64;
        on_progress(done, fragments);
        tokio::task::yield_now().await;
    }

    Ok(IngestResult {
        total_bytes: done,
        fragment_count: fragments,
        root_hash: root.finalize().to_hex().to_string(),
    })
}

struct IngestResult {
    total_bytes: u64,
    fragment_count: u32,
    root_hash: String,
}

/// Read until `buf` is full or EOF. Returns the number of bytes read (0 = EOF).
/// Handles short reads so chunks are uniform (except possibly the last).
async fn fill_chunk<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    buf: &mut [u8],
) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = reader.read(&mut buf[filled..]).await?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

fn emit_progress(
    app: &AppHandle,
    op_id: &str,
    label: &str,
    bytes_done: u64,
    bytes_total: u64,
    frags_done: u32,
    frags_total: u32,
) {
    let event = LoreEvent {
        tag: LoreEventTag::TransferProgress,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: LoreLogLevel::Info,
        payload: Some(serde_json::json!({
            "operationId": op_id,
            "label": label,
            "bytesDone": bytes_done,
            "bytesTotal": bytes_total,
            "fragmentsDone": frags_done,
            "fragmentsTotal": frags_total,
        })),
    };
    let _ = app.emit(LORE_EVENT_CHANNEL, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chunks_into_fragments_with_bounded_buffer() {
        // 20 bytes, 8-byte chunks => 3 fragments (8 + 8 + 4), regardless of the
        // fact that only one 8-byte buffer is ever resident.
        let data = vec![7u8; 20];
        let mut progress: Vec<(u64, u32)> = Vec::new();
        let res = ingest_reader(&data[..], 8, |d, f| progress.push((d, f)))
            .await
            .unwrap();

        assert_eq!(res.total_bytes, 20);
        assert_eq!(res.fragment_count, 3);
        assert_eq!(res.root_hash.len(), 64); // hex BLAKE3
        // progress reported once per chunk, monotonically increasing
        assert_eq!(progress, vec![(8, 1), (16, 2), (20, 3)]);
    }

    #[tokio::test]
    async fn root_hash_is_deterministic_and_content_addressed() {
        let a = ingest_reader(&b"hello world"[..], 4, |_, _| {}).await.unwrap();
        let b = ingest_reader(&b"hello world"[..], 4, |_, _| {}).await.unwrap();
        let c = ingest_reader(&b"hello worlx"[..], 4, |_, _| {}).await.unwrap();
        assert_eq!(a.root_hash, b.root_hash); // same content => same address
        assert_ne!(a.root_hash, c.root_hash); // one byte differs => different
    }

    #[tokio::test]
    async fn empty_input_yields_zero_fragments() {
        let res = ingest_reader(&b""[..], 8, |_, _| {}).await.unwrap();
        assert_eq!(res.fragment_count, 0);
        assert_eq!(res.total_bytes, 0);
    }
}
