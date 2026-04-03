use std::sync::Arc;

use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, Int32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray,
};
use arrow_schema::{ArrowError, DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use serde::{Deserialize, Serialize};
use tauri::Manager;

// nomic-embed-text produces 768-dimensional vectors.
const DIMS: i32 = 768;
const TABLE: &str = "notes";

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Wraps the LanceDB connection so Tauri can manage it as app state.
/// Connection is Arc-backed and cheap to clone.
pub struct VectorDb(pub Connection);

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

fn note_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("note_id", DataType::Int64, false),
        Field::new("chunk_index", DataType::Int32, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                DIMS,
            ),
            false,
        ),
    ]))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Open the notes table, creating it with the correct schema if it doesn't exist yet.
/// If the table exists but has the old pre-chunking schema (no chunk_index column),
/// it is dropped and recreated automatically. Notes will be re-indexed on their next save.
async fn open_table(conn: &Connection) -> Result<lancedb::Table, String> {
    match conn.open_table(TABLE).execute().await {
        Ok(t) => {
            let schema = t.schema().await.map_err(|e| e.to_string())?;
            if schema.field_with_name("chunk_index").is_err() {
                conn.drop_table(TABLE).await.map_err(|e| e.to_string())?;
                conn.create_empty_table(TABLE, note_schema())
                    .execute()
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Ok(t)
            }
        }
        Err(_) => conn
            .create_empty_table(TABLE, note_schema())
            .execute()
            .await
            .map_err(|e| e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Connect to LanceDB, storing data in the same app-data directory as SQLite.
pub async fn init(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("lancedb");

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir
        .to_str()
        .ok_or("Database path contains non-UTF8 characters")?;

    let conn = lancedb::connect(path)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    // Pre-create the table so the first write doesn't pay the schema-creation cost.
    open_table(&conn).await?;

    Ok(conn)
}

/// Evict all currently loaded Ollama models *except* the one we're about to use.
/// On AMD RDNA4 with Vulkan, running two models simultaneously causes GPU crashes.
/// Skipping the target model avoids the cost of unloading and reloading it when
/// it is already resident from the previous call (e.g. during a bulk reindex).
async fn evict_other_models(client: &reqwest::Client, keep_model: &str) {
    #[derive(Deserialize)]
    struct RunningModel { name: String }
    #[derive(Deserialize)]
    struct PsResp { models: Vec<RunningModel> }
    #[derive(Serialize)]
    struct UnloadReq<'a> { model: &'a str, keep_alive: i32, stream: bool }

    let Ok(resp) = client.get("http://localhost:11434/api/ps").send().await else { return };
    let Ok(ps) = resp.json::<PsResp>().await else { return };
    for m in ps.models {
        if m.name == keep_model { continue; }
        let _ = client
            .post("http://localhost:11434/api/generate")
            .json(&UnloadReq { model: &m.name, keep_alive: 0, stream: false })
            .send()
            .await;
    }
}

/// Call Ollama's /api/embeddings endpoint and return the embedding vector.
/// Evicts all running models first to prevent Vulkan GPU context conflicts.
/// Uses keep_alive=0 so the embed model is unloaded immediately after use.
/// Retries once on failure — evicting again before the second attempt clears
/// any GPU state that was corrupted by the first crash.
pub async fn embed(text: &str, model: &str) -> Result<Vec<f32>, String> {
    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
        prompt: &'a str,
        keep_alive: i32,
    }

    #[derive(Deserialize)]
    struct Resp {
        embedding: Vec<f32>,
    }

    // 120-second timeout — embedding a single chunk should never take this long.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut last_err = String::new();
    for attempt in 1u32..=2 {
        // Evict competing models before every attempt. Skips the embed model itself
        // so it stays resident across consecutive calls (e.g. during bulk reindex).
        evict_other_models(&client, model).await;

        let result: Result<Vec<f32>, String> = async {
            let response = client
                .post("http://localhost:11434/api/embeddings")
                .json(&Req { model, prompt: text, keep_alive: 0 })
                .send()
                .await
                .map_err(|e| format!("Could not reach Ollama (embedding): {e}"))?;

            let text_body = response
                .text()
                .await
                .map_err(|e| format!("Could not read embed response: {e}"))?;

            let resp: Resp = serde_json::from_str(&text_body)
                .map_err(|e| format!("Unexpected embed response: {e}\nBody: {text_body}"))?;

            if resp.embedding.is_empty() {
                return Err("Empty embedding response from Ollama".to_string());
            }

            // Sanity check: nomic-embed-text returns approximately unit-normalized
            // vectors (norm ≈ 1.0). A near-zero norm means Ollama returned garbage
            // from a crashed inference — treat it as an error and retry.
            let norm: f32 = resp.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm < 0.1 {
                return Err(format!("Degenerate embedding vector (norm={norm:.4}) — Ollama inference likely crashed"));
            }

            Ok(resp.embedding)
        }.await;

        match result {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    // Wait for Ollama to finish cleaning up the crashed runner.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
    Err(last_err)
}

/// Insert or replace all chunks for a note in the vector index.
/// Deletes any existing rows for this note_id first, then inserts one row per chunk.
/// Each chunk is a tuple of (chunk_index, chunk_text, embedding).
pub async fn upsert(
    conn: &Connection,
    note_id: i64,
    title: &str,
    chunks: Vec<(i32, String, Vec<f32>)>,
) -> Result<(), String> {
    let table = open_table(conn).await?;

    // Remove all existing chunks for this note.
    table
        .delete(&format!("note_id = {note_id}"))
        .await
        .map_err(|e| e.to_string())?;

    if chunks.is_empty() {
        return Ok(());
    }

    let n = chunks.len();

    // Flatten all embedding vectors into one contiguous array for the FixedSizeList column.
    let all_floats: Vec<f32> = chunks
        .iter()
        .flat_map(|(_, _, emb)| emb.iter().copied())
        .collect();

    let vector_col = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        DIMS,
        Arc::new(Float32Array::from(all_floats)) as ArrayRef,
        None,
    )
    .map_err(|e| e.to_string())?;

    let schema = note_schema();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![note_id; n])) as ArrayRef,
            Arc::new(Int32Array::from(
                chunks.iter().map(|(i, _, _)| *i).collect::<Vec<i32>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(vec![title; n])) as ArrayRef,
            Arc::new(StringArray::from(
                chunks
                    .iter()
                    .map(|(_, text, _)| text.as_str())
                    .collect::<Vec<&str>>(),
            )) as ArrayRef,
            Arc::new(vector_col) as ArrayRef,
        ],
    )
    .map_err(|e| e.to_string())?;

    // RecordBatchIterator wraps an iterator of Result<RecordBatch> and implements
    // RecordBatchReader, which is what LanceDB's add() expects.
    let items: Vec<Result<RecordBatch, ArrowError>> = vec![Ok(batch)];
    let reader = RecordBatchIterator::new(items, schema);

    table.add(reader).execute().await.map_err(|e| e.to_string())
}

/// Remove a note from the vector index (called on delete).
pub async fn remove(conn: &Connection, note_id: i64) -> Result<(), String> {
    let table = open_table(conn).await?;
    table
        .delete(&format!("note_id = {note_id}"))
        .await
        .map_err(|e| e.to_string())
}

/// Delete all rows from the vector index.
/// Called when a vault password is set — encrypted notes must not remain searchable.
pub async fn purge_all(conn: &Connection) -> Result<(), String> {
    let table = open_table(conn).await?;
    let count = table.count_rows(None).await.map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(());
    }
    // LanceDB delete with a condition that matches every row.
    table
        .delete("note_id >= 0")
        .await
        .map_err(|e| e.to_string())
}

/// A note returned from a semantic search. May include multiple excerpts
/// from different chunks of the same note.
#[derive(Debug, Serialize)]
pub struct NoteMatch {
    pub note_id: i64,
    pub title: String,
    pub excerpts: Vec<String>,
}

/// A raw search hit including the distance score. Used for debugging threshold calibration.
#[derive(Debug, Serialize)]
pub struct RawMatch {
    pub note_id: i64,
    pub title: String,
    pub excerpt: String,
    pub distance: f32,
}

/// Maximum number of distinct notes to include in search results.
const MAX_SOURCE_NOTES: usize = 5;
/// How many raw chunks to retrieve from LanceDB per search.
/// Must be larger than MAX_SOURCE_NOTES to allow deduplication to work — if a
/// long note contributes many top-ranked chunks they will all count as one note,
/// leaving room for shorter/newer notes to appear in the final result set.
pub const CHUNK_FETCH_LIMIT: usize = 100;
/// Search the vector index for notes semantically similar to the query embedding.
/// Returns up to `limit` individual chunk results ordered by similarity.
/// Chunks whose cosine distance exceeds MAX_DISTANCE are silently dropped, so
/// the returned list may be shorter than `limit` when few notes are relevant.
/// Multiple chunks from the same note may be returned if they are all relevant —
/// the caller is responsible for grouping them by note_id/title.
pub async fn search(
    conn: &Connection,
    query: Vec<f32>,
    limit: usize,
) -> Result<Vec<NoteMatch>, String> {
    let table = open_table(conn).await?;

    // LanceDB errors when searching an empty table in some versions — short-circuit.
    let count = table
        .count_rows(None)
        .await
        .map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(vec![]);
    }

    let stream = table
        .vector_search(query)
        .map_err(|e| e.to_string())?
        .limit(limit)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;

    // Pass 1: find the best distance per note across all chunks.
    // This is used to rank notes and select the top MAX_SOURCE_NOTES.
    let mut best_dist: std::collections::HashMap<i64, (f32, String)> = std::collections::HashMap::new();

    for batch in &batches {
        let ids = batch
            .column_by_name("note_id")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or("missing note_id column in search results")?;
        let titles = batch
            .column_by_name("title")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing title column in search results")?;
        let distances = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
            .ok_or("missing _distance column in search results")?;

        for i in 0..batch.num_rows() {
            let note_id = ids.value(i);
            let distance = distances.value(i);
            let entry = best_dist.entry(note_id).or_insert((f32::MAX, titles.value(i).to_string()));
            if distance < entry.0 {
                entry.0 = distance;
            }
        }
    }

    // Pick the top MAX_SOURCE_NOTES notes by best chunk distance.
    let mut ranked: Vec<(i64, f32, String)> = best_dist
        .into_iter()
        .map(|(id, (dist, title))| (id, dist, title))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(MAX_SOURCE_NOTES);
    let top_ids: std::collections::HashSet<i64> = ranked.iter().map(|(id, _, _)| *id).collect();

    // Pass 2: collect all chunks that belong to the top notes, preserving chunk order.
    // We keep a map from note_id → list of (chunk_index, excerpt).
    let mut note_chunks: std::collections::HashMap<i64, Vec<(i32, String)>> =
        std::collections::HashMap::new();

    for batch in &batches {
        let ids = batch
            .column_by_name("note_id")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or("missing note_id column in search results")?;
        let contents = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing content column in search results")?;
        let chunk_indices = batch
            .column_by_name("chunk_index")
            .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
            .ok_or("missing chunk_index column in search results")?;

        for i in 0..batch.num_rows() {
            let note_id = ids.value(i);
            if !top_ids.contains(&note_id) {
                continue;
            }
            let raw = contents.value(i);
            let excerpt = if raw.chars().count() > 500 {
                let cutoff = raw.char_indices().nth(500).map(|(b, _)| b).unwrap_or(raw.len());
                format!("{}\u{2026}", &raw[..cutoff])
            } else {
                raw.to_string()
            };
            let chunks = note_chunks.entry(note_id).or_default();
            let ci = chunk_indices.value(i);
            if !chunks.iter().any(|(c, _)| *c == ci) {
                chunks.push((ci, excerpt));
            }
        }
    }

    // Assemble final results in ranked order.
    let results = ranked
        .into_iter()
        .map(|(note_id, _, title)| {
            let mut chunks = note_chunks.remove(&note_id).unwrap_or_default();
            chunks.sort_by_key(|(ci, _)| *ci);
            NoteMatch {
                note_id,
                title,
                excerpts: chunks.into_iter().map(|(_, e)| e).collect(),
            }
        })
        .collect();

    Ok(results)
}

/// Like search() but returns all top-N hits with their raw distance scores,
/// ignoring MAX_DISTANCE. Used by the debug_search command to calibrate the threshold.
pub async fn raw_search(
    conn: &Connection,
    query: Vec<f32>,
    limit: usize,
) -> Result<Vec<RawMatch>, String> {
    let table = open_table(conn).await?;
    let count = table.count_rows(None).await.map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(vec![]);
    }

    let stream = table
        .vector_search(query)
        .map_err(|e| e.to_string())?
        .limit(limit)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let batches: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for batch in &batches {
        let ids = batch
            .column_by_name("note_id")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or("missing note_id column")?;
        let titles = batch
            .column_by_name("title")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing title column")?;
        let contents = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing content column")?;
        let distances = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
            .ok_or("missing _distance column")?;

        for i in 0..batch.num_rows() {
            let raw = contents.value(i);
            let excerpt = if raw.chars().count() > 200 {
                let cutoff = raw
                    .char_indices()
                    .nth(200)
                    .map(|(b, _)| b)
                    .unwrap_or(raw.len());
                format!("{}\u{2026}", &raw[..cutoff])
            } else {
                raw.to_string()
            };
            results.push(RawMatch {
                note_id: ids.value(i),
                title: titles.value(i).to_string(),
                excerpt,
                distance: distances.value(i),
            });
        }
    }
    Ok(results)
}
