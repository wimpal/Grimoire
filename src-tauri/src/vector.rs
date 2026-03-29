use std::sync::Arc;

use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray,
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
async fn open_table(conn: &Connection) -> Result<lancedb::Table, String> {
    match conn.open_table(TABLE).execute().await {
        Ok(t) => Ok(t),
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
    struct Req<'a> {
        model: &'a str,
        input: &'a str,
    }

    #[derive(Deserialize)]
    struct Resp {
        embeddings: Vec<Vec<f32>>,
    }

    let resp: Resp = reqwest::Client::new()
        .post("http://localhost:11434/api/embed")
        .json(&Req { model, input: text })
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

/// Insert or replace a note's embedding in the vector index.
/// We delete any existing record for this note_id first (upsert pattern).
pub async fn upsert(
    conn: &Connection,
    note_id: i64,
    title: &str,
    content: &str,
    embedding: Vec<f32>,
) -> Result<(), String> {
    let table = open_table(conn).await?;

    // Remove any existing record to avoid duplicates.
    table
        .delete(&format!("note_id = {note_id}"))
        .await
        .map_err(|e| e.to_string())?;

    // Build the FixedSizeList column for the vector.
    let flat_values = Float32Array::from(embedding);
    let vector_col = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        DIMS,
        Arc::new(flat_values) as ArrayRef,
        None,
    )
    .map_err(|e| e.to_string())?;

    // Build a one-row Arrow RecordBatch.
    let schema = note_schema();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![note_id])) as ArrayRef,
            Arc::new(StringArray::from(vec![title])) as ArrayRef,
            Arc::new(StringArray::from(vec![content])) as ArrayRef,
            Arc::new(vector_col) as ArrayRef,
        ],
    )
    .map_err(|e| e.to_string())?;

    // RecordBatchIterator wraps an iterator of Result<RecordBatch> and implements
    // RecordBatchReader, which is what LanceDB's add() expects.
    let items: Vec<Result<RecordBatch, ArrowError>> = vec![Ok(batch)];
    let reader = RecordBatchIterator::new(items, schema);

    table
        .add(reader)
        .execute()
        .await
        .map_err(|e| e.to_string())
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

/// Search the vector index for notes semantically similar to the query embedding.
/// Returns up to `limit` results ordered by similarity.
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

    // Collect the async stream of RecordBatches into a Vec.
    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for batch in &batches {
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
            // Truncate at a character boundary, not a byte boundary.
            let excerpt = if raw.chars().count() > 500 {
                let cutoff = raw
                    .char_indices()
                    .nth(500)
                    .map(|(byte_i, _)| byte_i)
                    .unwrap_or(raw.len());
                format!("{}…", &raw[..cutoff])
            } else {
                raw.to_string()
            };

            results.push(NoteMatch {
                note_id: ids.value(i),
                title: titles.value(i).to_string(),
                excerpt,
            });
        }
    }

    Ok(results)
}
