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

/// Call Ollama's /api/embed endpoint and return the embedding vector.
/// The caller is responsible for choosing the model (we use nomic-embed-text).
pub async fn embed(text: &str, model: &str) -> Result<Vec<f32>, String> {
    #[derive(Serialize)]
    struct Options {
        num_thread: usize,
    }

    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
        input: &'a str,
        options: Options,
    }

    #[derive(Deserialize)]
    struct Resp {
        embeddings: Vec<Vec<f32>>,
    }

    let total = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let num_thread = (total / 2).max(1);

    let resp: Resp = reqwest::Client::new()
        .post("http://localhost:11434/api/embed")
        .json(&Req { model, input: text, options: Options { num_thread } })
        .send()
        .await
        .map_err(|e| format!("Could not reach Ollama: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Unexpected embed response: {e}"))?;

    resp.embeddings
        .into_iter()
        .next()
        .ok_or_else(|| "Empty embedding response from Ollama".to_string())
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

/// A note returned from a semantic search.
#[derive(Debug, Serialize)]
pub struct NoteMatch {
    pub note_id: i64,
    pub title: String,
    /// First ~500 characters of the note content.
    pub excerpt: String,
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
/// Chunks are ordered by ascending cosine distance, so the top MAX_SOURCE_NOTES
/// notes are always the closest matches. The LLM's system prompt instructs it to
/// say "not in your notes" when the context doesn't address the question, so we
/// don't need a distance-gap filter — just cap the sources and let the LLM judge.
const MAX_SOURCE_NOTES: usize = 3;
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

    let mut results = Vec::new();
    // Track which note IDs have already contributed at least one chunk so we
    // can enforce the MAX_SOURCE_NOTES cap.
    let mut seen_notes: std::collections::HashSet<i64> = std::collections::HashSet::new();

    'outer: for batch in &batches {
        let ids = batch
            .column_by_name("note_id")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or("missing note_id column in search results")?;
        let titles = batch
            .column_by_name("title")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing title column in search results")?;
        let contents = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or("missing content column in search results")?;

        for i in 0..batch.num_rows() {
            let raw = contents.value(i);
            let excerpt = if raw.chars().count() > 500 {
                let cutoff = raw
                    .char_indices()
                    .nth(500)
                    .map(|(byte_i, _)| byte_i)
                    .unwrap_or(raw.len());
                format!("{}\u{2026}", &raw[..cutoff])
            } else {
                raw.to_string()
            };

            let note_id = ids.value(i);
            // Enforce distinct-note cap: if this note is new and we've already
            // hit the limit, skip without breaking — a higher-distance chunk
            // from an already-seen note might still appear later in the batch.
            if !seen_notes.contains(&note_id) {
                if seen_notes.len() >= MAX_SOURCE_NOTES {
                    continue;
                }
                seen_notes.insert(note_id);
            }

            results.push(NoteMatch {
                note_id,
                title: titles.value(i).to_string(),
                excerpt,
            });

            if results.len() >= limit {
                break 'outer;
            }
        }
    }

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
