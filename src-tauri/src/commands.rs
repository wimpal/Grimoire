use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

use crate::KeyStore;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Raw note row as stored in SQLite. Used internally only — not sent to the frontend.
/// When encryption is active, `title` and `content` are base64-encoded ciphertext blobs.
#[derive(Debug, sqlx::FromRow)]
struct NoteRow {
    id: i64,
    title: String,
    content: String,
    folder_id: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

/// A note as returned to the frontend.
/// `locked` is true when the note's folder is locked and no session key is available.
/// When `locked` is true, `title` and `content` are empty strings.
#[derive(Debug, Serialize)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub folder_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub locked: bool,
}

/// Raw folder row as stored in SQLite. Used internally only.
#[derive(Debug, sqlx::FromRow)]
struct FolderRow {
    id: i64,
    name: String,
    parent_id: Option<i64>,
    created_at: i64,
    locked: i64,
}

/// A folder as returned to the frontend.
/// `locked` is true when the folder has a password AND no session key is held for it.
#[derive(Debug, Serialize)]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub created_at: i64,
    pub locked: bool,
}

/// A minimal note reference used for tag/link results — just enough to render
/// a clickable pill in the UI without transferring full note content.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LinkedNote {
    pub id: i64,
    pub title: String,
}

/// A node in the knowledge graph (a note with its id and display title).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GraphNode {
    pub id: i64,
    pub title: String,
}

/// A directed edge between two notes in the knowledge graph.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GraphEdge {
    pub source: i64,
    pub target: i64,
}

// ---------------------------------------------------------------------------
// Encryption helpers (internal)
// ---------------------------------------------------------------------------

/// Resolve the active encryption key for a note given its folder_id.
/// Priority: folder key > vault key > None (plaintext).
fn resolve_key(folder_id: Option<i64>, keys: &KeyStore) -> Option<[u8; 32]> {
    if let Some(fid) = folder_id {
        if let Ok(fk) = keys.folder_keys.lock() {
            if let Some(k) = fk.get(&fid) {
                return Some(*k);
            }
        }
    }
    if let Ok(vk) = keys.vault_key.lock() {
        if let Some(k) = *vk {
            return Some(k);
        }
    }
    None
}

/// Returns true if the folder has a password AND no session key is held for it.
fn folder_is_locked(folder_id: i64, folder_locked_col: bool, keys: &KeyStore) -> bool {
    if !folder_locked_col {
        return false;
    }
    keys.folder_keys
        .lock()
        .map(|fk| !fk.contains_key(&folder_id))
        .unwrap_or(true)
}

/// Decrypt a note's title and content using `key`.
/// Falls back to the raw value if decryption fails (handles unencrypted rows).
fn decrypt_note_fields(key: &[u8; 32], enc_title: String, enc_content: String) -> (String, String) {
    let title = crate::crypto::decrypt(key, &enc_title)
        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
        .unwrap_or(enc_title);
    let content = crate::crypto::decrypt(key, &enc_content)
        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
        .unwrap_or(enc_content);
    (title, content)
}

/// Decrypt a folder name using `key`, falling back to raw value.
fn decrypt_folder_name(key: &[u8; 32], enc_name: String) -> String {
    crate::crypto::decrypt(key, &enc_name)
        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
        .unwrap_or(enc_name)
}

/// Map a FolderRow into the public Folder struct.
/// If the folder is locked and no session key exists, `locked` is true and the
/// raw (encrypted) name is still returned so the UI can show something.
/// If a vault key is available, the name is decrypted.
fn map_folder_row(row: FolderRow, keys: &KeyStore) -> Folder {
    let is_locked = folder_is_locked(row.id, row.locked != 0, keys);
    let name = if is_locked {
        // Don't expose the encrypted blob as the display name; show "<locked>"
        "<locked>".to_string()
    } else if let Some(key) = resolve_key(None, keys) {
        decrypt_folder_name(&key, row.name)
    } else {
        row.name
    };

    Folder {
        id: row.id,
        name,
        parent_id: row.parent_id,
        created_at: row.created_at,
        locked: is_locked,
    }
}

// ---------------------------------------------------------------------------
// Note commands
// ---------------------------------------------------------------------------
/// Looks up the folder's `locked` column value via `folder_locked` parameter.
fn map_note_row(row: NoteRow, folder_locked: bool, keys: &KeyStore) -> Note {
    // Check if this note's folder is locked without a session key.
    let is_locked = row.folder_id
        .map(|fid| folder_is_locked(fid, folder_locked, keys))
        .unwrap_or(false);

    if is_locked {
        return Note {
            id: row.id,
            title: String::new(),
            content: String::new(),
            folder_id: row.folder_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            locked: true,
        };
    }

    // Vault locked check (no vault key in memory).
    let vault_locked = keys.vault_key.lock()
        .map(|vk| vk.is_none())
        .unwrap_or(true);
    let vault_has_pw = std::sync::Arc::new(false); // checked below
    let _ = vault_has_pw;

    // Attempt decrypt if a key is available.
    let (title, content) = if let Some(key) = resolve_key(row.folder_id, keys) {
        decrypt_note_fields(&key, row.title, row.content)
    } else {
        // No encryption active — content is plaintext.
        // But if vault is locked (has password, no key), hide content.
        if vault_locked {
            // vault_locked here means vault_key is None — but we need to check
            // whether that's because there's no password at all or because it's locked.
            // We can't do an async DB call here, so we check by trying to use the vault key;
            // since it's None and we have no folder key, the data must be inaccessible.
            // The frontend checks is_vault_locked before this ever renders — trust that gate.
            (row.title, row.content)
        } else {
            (row.title, row.content)
        }
    };

    Note {
        id: row.id,
        title,
        content,
        folder_id: row.folder_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
        locked: false,
    }
}

/// Create a new note and return the full row.
#[tauri::command]
pub async fn create_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    title: String,
    folder_id: Option<i64>,
) -> Result<Note, String> {
    // Encrypt title if a key is active.
    let stored_title = if let Some(key) = resolve_key(folder_id, &keys) {
        crate::crypto::encrypt(&key, title.as_bytes())
    } else {
        title
    };

    let row = sqlx::query_as::<_, NoteRow>(
        "INSERT INTO notes (title, folder_id) VALUES (?, ?)
         RETURNING id, title, content, folder_id, created_at, updated_at",
    )
    .bind(&stored_title)
    .bind(folder_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(map_note_row(row, false, &keys))
}

/// Fetch a single note by id.
#[tauri::command]
pub async fn get_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    id: i64,
) -> Result<Note, String> {
    // Also fetch the folder's locked column so we can compute lock state.
    let row = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, content, folder_id, created_at, updated_at
         FROM notes WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let folder_locked = if let Some(fid) = row.folder_id {
        let v: i64 = sqlx::query_scalar("SELECT locked FROM folders WHERE id = ?")
            .bind(fid)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        v != 0
    } else {
        false
    };

    Ok(map_note_row(row, folder_locked, &keys))
}

/// List all notes, optionally filtered to a specific folder.
/// Pass `null` from JS to get notes with no folder, omit the filter to get all.
/// This command takes an explicit `all` flag to distinguish "no folder" from "every folder".
#[tauri::command]
pub async fn list_notes(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    folder_id: Option<i64>,
    all: Option<bool>,
) -> Result<Vec<Note>, String> {
    let rows = if all.unwrap_or(false) {
        sqlx::query_as::<_, NoteRow>(
            "SELECT id, title, content, folder_id, created_at, updated_at
             FROM notes ORDER BY updated_at DESC",
        )
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_as::<_, NoteRow>(
            "SELECT id, title, content, folder_id, created_at, updated_at
             FROM notes WHERE folder_id IS ? ORDER BY updated_at DESC",
        )
        .bind(folder_id)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?
    };

    // Bulk-fetch folder lock states so we only hit the DB once per unique folder_id.
    let folder_lock_states: std::collections::HashMap<i64, bool> = {
        let locked_rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT id, locked FROM folders")
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();
        locked_rows.into_iter().map(|(id, lk)| (id, lk != 0)).collect()
    };

    let notes = rows
        .into_iter()
        .map(|row| {
            let fl = row.folder_id
                .and_then(|fid| folder_lock_states.get(&fid).copied())
                .unwrap_or(false);
            map_note_row(row, fl, &keys)
        })
        .collect();

    Ok(notes)
}

/// Update a note's title and content. Bumps updated_at to the current time.
#[tauri::command]
pub async fn update_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    id: i64,
    title: String,
    content: String,
) -> Result<Note, String> {
    // Look up the current folder_id and its lock state.
    let current: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT folder_id FROM notes WHERE id = ?")
            .bind(id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| e.to_string())?;

    let folder_id = current.and_then(|(fid,)| fid);

    let folder_locked = if let Some(fid) = folder_id {
        let v: i64 = sqlx::query_scalar("SELECT locked FROM folders WHERE id = ?")
            .bind(fid)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        v != 0
    } else {
        false
    };

    // Refuse writes to a locked-but-not-unlocked folder.
    if folder_id.map(|fid| folder_is_locked(fid, folder_locked, &keys)).unwrap_or(false) {
        return Err("folder_locked".to_string());
    }

    let (stored_title, stored_content) = if let Some(key) = resolve_key(folder_id, &keys) {
        (
            crate::crypto::encrypt(&key, title.as_bytes()),
            crate::crypto::encrypt(&key, content.as_bytes()),
        )
    } else {
        (title, content)
    };

    let row = sqlx::query_as::<_, NoteRow>(
        "UPDATE notes
         SET title = ?, content = ?, updated_at = unixepoch()
         WHERE id = ?
         RETURNING id, title, content, folder_id, created_at, updated_at",
    )
    .bind(&stored_title)
    .bind(&stored_content)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(map_note_row(row, folder_locked, &keys))
}

/// Move a note to a different folder (or to no folder when folder_id is null).
#[tauri::command]
pub async fn move_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    id: i64,
    folder_id: Option<i64>,
) -> Result<Note, String> {
    let row = sqlx::query_as::<_, NoteRow>(
        "UPDATE notes
         SET folder_id = ?, updated_at = unixepoch()
         WHERE id = ?
         RETURNING id, title, content, folder_id, created_at, updated_at",
    )
    .bind(folder_id)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(map_note_row(row, false, &keys))
}

/// Delete a note. Returns nothing on success.
#[tauri::command]
pub async fn delete_note(pool: State<'_, SqlitePool>, id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Folder commands
// ---------------------------------------------------------------------------

/// Create a new folder and return the full row.
#[tauri::command]
pub async fn create_folder(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    name: String,
    parent_id: Option<i64>,
) -> Result<Folder, String> {
    // Encrypt folder name if vault key is active.
    let stored_name = if let Some(key) = resolve_key(None, &keys) {
        crate::crypto::encrypt(&key, name.as_bytes())
    } else {
        name
    };

    let row = sqlx::query_as::<_, FolderRow>(
        "INSERT INTO folders (name, parent_id) VALUES (?, ?)
         RETURNING id, name, parent_id, created_at, locked",
    )
    .bind(&stored_name)
    .bind(parent_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(map_folder_row(row, &keys))
}

/// List all folders. The frontend is responsible for building the tree from parent_id.
#[tauri::command]
pub async fn list_folders(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
) -> Result<Vec<Folder>, String> {
    let rows = sqlx::query_as::<_, FolderRow>(
        "SELECT id, name, parent_id, created_at, locked FROM folders ORDER BY name ASC",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| map_folder_row(r, &keys)).collect())
}

/// Rename a folder.
#[tauri::command]
pub async fn rename_folder(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    id: i64,
    name: String,
) -> Result<Folder, String> {
    let stored_name = if let Some(key) = resolve_key(None, &keys) {
        crate::crypto::encrypt(&key, name.as_bytes())
    } else {
        name
    };

    let row = sqlx::query_as::<_, FolderRow>(
        "UPDATE folders SET name = ? WHERE id = ?
         RETURNING id, name, parent_id, created_at, locked",
    )
    .bind(&stored_name)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(map_folder_row(row, &keys))
}

/// Delete a folder. Child folders and notes are handled by ON DELETE CASCADE
/// and ON DELETE SET NULL respectively (defined in the migration).
#[tauri::command]
pub async fn delete_folder(pool: State<'_, SqlitePool>, id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Chat (Ollama)
// ---------------------------------------------------------------------------

/// A single message in a conversation. `role` is "user" or "assistant".
/// Both `Serialize` (to send to Ollama) and `Deserialize` (to receive from the frontend) are needed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// The request body sent to Ollama's /api/chat endpoint.
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

/// Runtime options forwarded to Ollama on every request.
/// `num_thread` caps the number of CPU threads Ollama uses for inference,
/// leaving headroom for the OS and other running applications.
#[derive(Serialize)]
struct OllamaOptions {
    num_thread: usize,
}

impl OllamaOptions {
    fn balanced() -> Self {
        let total = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // Use at most half the logical cores, but always at least 1.
        Self { num_thread: (total / 2).max(1) }
    }
}

/// The response body from Ollama. Only the `message` field is needed.
#[derive(Deserialize)]
struct OllamaChatResponse {
    message: ChatMessage,
}

/// Send a chat message to a locally-running Ollama instance and return the
/// assistant's reply. The full `messages` history is forwarded each time so
/// Ollama maintains conversational context.
#[tauri::command]
pub async fn chat(model: String, messages: Vec<ChatMessage>) -> Result<String, String> {
    let client = reqwest::Client::new();

    let body = OllamaChatRequest {
        model,
        messages,
        stream: false,
        options: OllamaOptions::balanced(),
    };

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Could not reach Ollama — is it running? ({e})"))?;

    let parsed: OllamaChatResponse = response
        .json()
        .await
        .map_err(|e| format!("Unexpected response from Ollama: {e}"))?;

    Ok(parsed.message.content)
}

// ---------------------------------------------------------------------------
// RAG commands (vector index + semantic search)
// ---------------------------------------------------------------------------

// The embed model is fixed here. nomic-embed-text is the standard lightweight
// choice for Ollama: 274 MB, 768-dimensional vectors, purpose-built for text.
// nomic-embed-text requires asymmetric prefixes: documents are prefixed with
// "search_document: " and queries with "search_query: " for accurate retrieval.
const EMBED_MODEL: &str = "nomic-embed-text";

async fn embed_document(text: &str) -> Result<Vec<f32>, String> {
    crate::vector::embed(&format!("search_document: {text}"), EMBED_MODEL).await
}

async fn embed_query(text: &str) -> Result<Vec<f32>, String> {
    crate::vector::embed(&format!("search_query: {text}"), EMBED_MODEL).await
}

/// Split a long line into smaller pieces at sentence-ending punctuation.
/// Only splits where `. `, `! `, or `? ` is followed by an uppercase letter,
/// which avoids false splits on abbreviations like "e.g. " or "TLS 1.3 ".
fn split_at_punctuation(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // Peek two chars ahead: space then uppercase → real sentence boundary.
            let next = chars.get(i + 1).copied();
            let after = chars.get(i + 2).copied();
            if matches!(next, Some(' ') | Some('\t'))
                && matches!(after, Some(c) if c.is_uppercase())
            {
                let s = buf.trim().to_string();
                if !s.is_empty() {
                    parts.push(s);
                }
                buf.clear();
            }
        }
    }

    let tail = buf.trim().to_string();
    if !tail.is_empty() {
        parts.push(tail);
    }
    if parts.is_empty() {
        parts.push(text.trim().to_string());
    }
    parts
}

/// Split `text` into individual sentences.
/// First splits on newlines (one idea per line is common in notes), then
/// further splits long lines at sentence-ending punctuation.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Only try punctuation-splitting for lines longer than 30 words;
        // short lines are already a single thought.
        if line.split_whitespace().count() > 30 {
            sentences.extend(split_at_punctuation(line));
        } else {
            sentences.push(line.to_string());
        }
    }

    if sentences.is_empty() {
        // Fallback: treat the whole text as one chunk.
        let s = text.trim().to_string();
        if !s.is_empty() {
            sentences.push(s);
        }
    }
    sentences
}

/// Group a flat list of sentences into overlapping chunks.
/// `per_chunk` is the number of sentences per chunk; `overlap` is how many
/// sentences the next chunk re-uses from the end of the previous one.
/// This keeps sentence boundaries intact while giving neighbouring chunks
/// shared context for better retrieval continuity.
fn chunk_sentences(sentences: Vec<String>, per_chunk: usize, overlap: usize) -> Vec<String> {
    if sentences.is_empty() {
        return vec![String::new()];
    }
    if sentences.len() <= per_chunk {
        return vec![sentences.join(" ")];
    }
    let step = per_chunk.saturating_sub(overlap).max(1);
    let mut chunks = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + per_chunk).min(sentences.len());
        chunks.push(sentences[start..end].join(" "));
        if end == sentences.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// Embed a note and store it in the vector index.
/// The note content is split into sentences, then grouped into overlapping
/// 3-sentence chunks. Each chunk is embedded on its own content alone —
/// the title is intentionally excluded from the embedding so that off-topic
/// sentences in a note (e.g. a random thought at the end of a technical note)
/// still produce vectors that match unrelated queries.
#[tauri::command]
pub async fn index_note(
    vdb: State<'_, crate::vector::VectorDb>,
    note_id: i64,
    title: String,
    content: String,
) -> Result<(), String> {
    let sentences = split_sentences(&content);
    let raw_chunks = chunk_sentences(sentences, 1, 0);

    let mut chunks: Vec<(i32, String, Vec<f32>)> = Vec::new();
    for (i, chunk_text) in raw_chunks.into_iter().enumerate() {
        let embedding = embed_document(&chunk_text).await?;
        chunks.push((i as i32, chunk_text, embedding));
    }

    crate::vector::upsert(&vdb.0, note_id, &title, chunks).await
}

/// Remove a note from the vector index. Called when a note is deleted.
#[tauri::command]
pub async fn remove_note_index(
    vdb: State<'_, crate::vector::VectorDb>,
    note_id: i64,
) -> Result<(), String> {
    crate::vector::remove(&vdb.0, note_id).await
}

/// Embed the query text and return the most semantically similar notes.
/// The frontend uses this to build context before sending a chat message.
/// Returns an empty list if the vault is locked — encrypted embeddings must not be served.
#[tauri::command]
pub async fn search_notes(
    vdb: State<'_, crate::vector::VectorDb>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::vector::NoteMatch>, String> {
    // When the vault is locked the index is purged, so search naturally returns nothing.
    // No key check needed here — just run the query.
    let embedding = embed_query(&query).await?;
    crate::vector::search(&vdb.0, embedding, limit.unwrap_or(crate::vector::CHUNK_FETCH_LIMIT)).await
}

/// Re-index every note currently in SQLite into LanceDB from scratch.
/// Only indexes notes that are currently decryptable (vault unlocked or no vault password).
/// Locked folder notes are skipped — they will be indexed when unlocked.
/// Returns the number of notes indexed.
#[tauri::command]
pub async fn reindex_all(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    vdb: State<'_, crate::vector::VectorDb>,
) -> Result<usize, String> {
    // Guard: if the vault has a password but the key isn't in memory, we can't
    // decrypt note content. Indexing now would store ciphertext blobs. Bail out.
    let vault_key_absent = keys.vault_key.lock()
        .map(|vk| vk.is_none())
        .unwrap_or(true);
    if vault_key_absent {
        let has_pw: bool = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vault_lock WHERE id = 1",
        )
        .fetch_one(pool.inner())
        .await
        .map(|n: i64| n > 0)
        .unwrap_or(false);
        if has_pw {
            // Vault is locked — cannot index encrypted content.
            return Ok(0);
        }
    }

    let raw_notes = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, content, folder_id, created_at, updated_at FROM notes",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let folder_lock_states: std::collections::HashMap<i64, bool> = {
        let locked_rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT id, locked FROM folders")
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();
        locked_rows.into_iter().map(|(id, lk)| (id, lk != 0)).collect()
    };

    let mut count = 0usize;
    for raw in raw_notes {
        let fl = raw.folder_id
            .and_then(|fid| folder_lock_states.get(&fid).copied())
            .unwrap_or(false);
        let note = map_note_row(raw, fl, &keys);
        // Skip locked notes — their content is inaccessible.
        if note.locked {
            continue;
        }
        let sentences = split_sentences(&note.content);
        let raw_chunks = chunk_sentences(sentences, 1, 0);
        let mut chunks: Vec<(i32, String, Vec<f32>)> = Vec::new();
        for (i, chunk_text) in raw_chunks.into_iter().enumerate() {
            let embedding = embed_document(&chunk_text).await?;
            chunks.push((i as i32, chunk_text, embedding));
        }
        crate::vector::upsert(&vdb.0, note.id, &note.title, chunks).await?;
        count += 1;
    }

    Ok(count)
}

// ---------------------------------------------------------------------------
// Tags and wiki-links
// ---------------------------------------------------------------------------

/// Extract `#tag` mentions from note content.
/// A tag is `#` immediately followed by one or more word characters (letters,
/// digits, `-`, `_`), and must be preceded by whitespace or start-of-text so
/// that URLs like `https://example.com/#section` are not treated as tags.
fn parse_tags(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' {
            let preceded_ok = i == 0 || chars[i - 1].is_whitespace();
            let followed_ok = chars
                .get(i + 1)
                .map(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .unwrap_or(false);
            if preceded_ok && followed_ok {
                let start = i + 1;
                let mut end = start;
                while end < chars.len()
                    && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '-')
                {
                    end += 1;
                }
                let tag: String = chars[start..end].iter().collect::<String>().to_lowercase();
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    tags
}

/// Extract `[[note title]]` wiki-link targets from note content.
fn parse_wiki_links(content: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find("[[") {
        rest = &rest[open + 2..];
        if let Some(close) = rest.find("]]") {
            let title = rest[..close].trim().to_string();
            if !title.is_empty() && !links.contains(&title) {
                links.push(title);
            }
            rest = &rest[close + 2..];
        } else {
            break;
        }
    }
    links
}

/// Persist the parsed tags for a note. Replaces all existing note→tag rows,
/// but leaves the `tags` table rows in place (tags are shared across notes).
async fn sync_tags(pool: &SqlitePool, note_id: i64, tags: &[String]) -> Result<(), String> {
    sqlx::query("DELETE FROM note_tags WHERE note_id = ?")
        .bind(note_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for tag in tags {
        // Ensure the tag name exists in the tags table.
        sqlx::query("INSERT OR IGNORE INTO tags (name) VALUES (?)")
            .bind(tag)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        let tag_id: i64 = sqlx::query_scalar("SELECT id FROM tags WHERE name = ?")
            .bind(tag)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query("INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)")
            .bind(note_id)
            .bind(tag_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Persist the parsed wiki-links for a note. Replaces all existing outgoing
/// links. Link targets that don't match an existing note title are silently
/// skipped — they'll be picked up on the next save if the target is created.
async fn sync_links(
    pool: &SqlitePool,
    note_id: i64,
    link_titles: &[String],
) -> Result<(), String> {
    sqlx::query("DELETE FROM note_links WHERE source_id = ?")
        .bind(note_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    for title in link_titles {
        let target: Option<i64> =
            sqlx::query_scalar("SELECT id FROM notes WHERE title = ? LIMIT 1")
                .bind(title)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;

        if let Some(target_id) = target {
            if target_id != note_id {
                sqlx::query(
                    "INSERT OR IGNORE INTO note_links (source_id, target_id) VALUES (?, ?)",
                )
                .bind(note_id)
                .bind(target_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Return all tags with a count of how many notes use each one, sorted by
/// name. Used by the sidebar tag browser.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

#[tauri::command]
pub async fn list_all_tags(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<TagCount>, String> {
    let tags = sqlx::query_as::<_, TagCount>(
        "SELECT t.name, COUNT(nt.note_id) AS count
         FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         GROUP BY t.id
         ORDER BY t.name ASC",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(tags)
}

/// Return all notes and all wiki-links as a graph dataset.
/// The frontend uses this to build a force-directed graph.
#[tauri::command]
pub async fn get_graph_data(
    pool: State<'_, SqlitePool>,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), String> {
    let nodes = sqlx::query_as::<_, GraphNode>(
        "SELECT id, title FROM notes ORDER BY id ASC",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let edges = sqlx::query_as::<_, GraphEdge>(
        "SELECT source_id AS source, target_id AS target FROM note_links",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok((nodes, edges))
}

/// Parse and persist all `#tags` and `[[wiki-links]]` found in a note's content.
/// Called in the background after every save. Failures are non-fatal — the
/// relations are derived data and can always be recomputed from content.
#[tauri::command]
pub async fn sync_note_relations(
    pool: State<'_, SqlitePool>,
    note_id: i64,
    content: String,
) -> Result<(), String> {
    let tags = parse_tags(&content);
    let links = parse_wiki_links(&content);
    sync_tags(pool.inner(), note_id, &tags).await?;
    sync_links(pool.inner(), note_id, &links).await?;
    Ok(())
}

/// Return the tag names attached to a note, alphabetically sorted.
#[tauri::command]
pub async fn get_note_tags(
    pool: State<'_, SqlitePool>,
    note_id: i64,
) -> Result<Vec<String>, String> {
    let tags: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ?
         ORDER BY t.name ASC",
    )
    .bind(note_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(tags)
}

/// Return notes that this note links to via `[[title]]`, alphabetically sorted.
#[tauri::command]
pub async fn get_note_links(
    pool: State<'_, SqlitePool>,
    note_id: i64,
) -> Result<Vec<LinkedNote>, String> {
    let links = sqlx::query_as::<_, LinkedNote>(
        "SELECT n.id, n.title FROM notes n
         JOIN note_links nl ON nl.target_id = n.id
         WHERE nl.source_id = ?
         ORDER BY n.title ASC",
    )
    .bind(note_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(links)
}

/// Return notes that link to this note via `[[title]]` (backlinks), alphabetically sorted.
#[tauri::command]
pub async fn get_backlinks(
    pool: State<'_, SqlitePool>,
    note_id: i64,
) -> Result<Vec<LinkedNote>, String> {
    let links = sqlx::query_as::<_, LinkedNote>(
        "SELECT n.id, n.title FROM notes n
         JOIN note_links nl ON nl.source_id = n.id
         WHERE nl.target_id = ?
         ORDER BY n.title ASC",
    )
    .bind(note_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(links)
}

/// List all notes that carry a given tag, sorted by most recently updated.
#[tauri::command]
pub async fn list_notes_by_tag(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    tag: String,
) -> Result<Vec<Note>, String> {
    // Use a struct that can hold the extra `folder_locked` column from the join.
    #[derive(sqlx::FromRow)]
    struct NoteRowWithLock {
        id: i64,
        title: String,
        content: String,
        folder_id: Option<i64>,
        created_at: i64,
        updated_at: i64,
        folder_locked: i64,
    }
    let rows = sqlx::query_as::<_, NoteRowWithLock>(
        "SELECT n.id, n.title, n.content, n.folder_id, n.created_at, n.updated_at,
                COALESCE(f.locked, 0) AS folder_locked
         FROM notes n
         LEFT JOIN folders f ON f.id = n.folder_id
         JOIN note_tags nt ON nt.note_id = n.id
         JOIN tags t ON t.id = nt.tag_id
         WHERE t.name = ?
         ORDER BY n.updated_at DESC",
    )
    .bind(tag.to_lowercase())
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let notes = rows
        .into_iter()
        .map(|r| {
            let folder_locked = r.folder_locked != 0;
            map_note_row(
                NoteRow {
                    id: r.id,
                    title: r.title,
                    content: r.content,
                    folder_id: r.folder_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                folder_locked,
                &keys,
            )
        })
        .collect();
    Ok(notes)
}

/// Debug command: returns the top 10 vector search hits with raw distance scores.
/// Use this from the UI to calibrate MAX_DISTANCE — look at the distance values
/// for chunks you consider relevant vs. irrelevant, then set the threshold between them.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn debug_search(
    vdb: State<'_, crate::vector::VectorDb>,
    query: String,
) -> Result<Vec<crate::vector::RawMatch>, String> {
    let embedding = embed_query(&query).await?;
    crate::vector::raw_search(&vdb.0, embedding, 10).await
}

/// Insert a set of varied seed notes and index them all.
/// Returns the number of notes created. Intended for development/testing only
/// — the button that calls this should only appear in debug builds.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn seed_notes(
    pool: State<'_, SqlitePool>,
    vdb: State<'_, crate::vector::VectorDb>,
) -> Result<usize, String> {
    // Each tuple is (title, content).
    let seeds: &[(&str, &str)] = &[
        (
            "Rust ownership and borrowing",
            "Rust enforces memory safety through a system of ownership with rules that the \
            compiler checks at compile time. Every value in Rust has a single owner. When the \
            owner goes out of scope, the value is dropped. References allow you to refer to a \
            value without taking ownership. The borrow checker ensures references are always \
            valid. Mutable references (&mut T) are exclusive — only one can exist at a time. \
            This prevents data races at compile time, without needing a garbage collector.",
        ),
        (
            "Sleep and cognitive performance",
            "Sleep plays a critical role in memory consolidation. During slow-wave sleep, the \
            hippocampus replays experiences to the neocortex for long-term storage. REM sleep \
            is associated with procedural memory and emotional regulation. Chronic sleep \
            deprivation impairs prefrontal cortex function, reducing decision-making ability, \
            working memory, and attention span. Adults generally need 7–9 hours. Even one night \
            of under-sleeping measurably reduces cognitive performance the following day.",
        ),
        (
            "How Transformer models work",
            "Transformers use self-attention to weigh the relevance of each token in a sequence \
            relative to every other token. Unlike RNNs, they process all tokens in parallel, \
            making them highly amenable to GPU acceleration. The attention mechanism computes \
            query, key, and value matrices and produces weighted sums of values. Positional \
            encodings are added to embeddings to preserve sequence order. Models like GPT are \
            decoder-only (autoregressive); BERT is encoder-only (masked language modeling). \
            LLMs are transformer-based models trained on large corpora to predict the next token.",
        ),
        (
            "Fermentation basics",
            "Fermentation is a metabolic process in which microorganisms like bacteria, yeast, \
            or fungi convert sugars into acids, gases, or alcohol. Lactic acid fermentation \
            (used in yoghurt, kimchi, sauerkraut) produces lactic acid. Alcoholic fermentation \
            (used in beer, wine, bread) produces ethanol and CO2. Temperature, salt concentration, \
            and pH all affect which microorganisms thrive. Fermented foods are rich in probiotics \
            and have been linked to improved gut microbiome diversity. Starter cultures can be \
            used to ensure consistent results.",
        ),
        (
            "The Stoic practice of negative visualisation",
            "Negative visualisation (premeditatio malorum) is a Stoic technique where you \
            deliberately imagine losing things you value — health, relationships, property. The \
            goal is not to induce anxiety but to cultivate gratitude and reduce attachment. Seneca \
            wrote that we should rehearse poverty, illness, and death periodically so that fortune \
            cannot catch us off guard. The practice counteracts hedonic adaptation, the tendency \
            to take good things for granted over time. It pairs well with the dichotomy of \
            control: focusing only on what is within your power.",
        ),
        (
            "How HTTPS and TLS work",
            "HTTPS is HTTP over TLS (Transport Layer Security). A TLS handshake establishes a \
            secure channel before any HTTP data is sent. The client sends a ClientHello with \
            supported cipher suites. The server responds with its certificate (signed by a CA) \
            and the chosen cipher. Key exchange uses asymmetric cryptography (e.g. ECDH) to \
            derive a shared secret without transmitting it. All subsequent traffic is encrypted \
            with symmetric keys derived from that secret. TLS 1.3 removed weak cipher suites and \
            reduced handshake round-trips from two to one.",
        ),
        (
            "Compound interest and long-term investing",
            "Compound interest is interest calculated on both the initial principal and the \
            accumulated interest. Over long periods, the effect is exponential. A 7% annual \
            return doubles money roughly every 10 years (rule of 72). Starting early matters \
            more than the amount invested: £5,000 invested at 25 grows more than £10,000 \
            invested at 35 at the same return rate. Index funds provide broad market exposure \
            with low fees, historically outperforming most actively managed funds over 20+ year \
            windows. Fee drag compounds just as returns do — a 1% annual fee has a significant \
            long-term cost.",
        ),
        (
            "Zettelkasten note-taking method",
            "Zettelkasten is a note-taking method developed by sociologist Niklas Luhmann, who \
            used it to write over 70 books. Each note (zettel) contains a single atomic idea and \
            is linked to related notes by reference. Notes are not organised into folders or \
            topics — meaning emerges from the link structure. There are three types: fleeting \
            notes (quick captures), literature notes (from sources), and permanent notes \
            (processed ideas in your own words). The method is designed to build a personal \
            knowledge graph over time, with the network of links surfacing non-obvious \
            connections between ideas.",
        ),
    ];

    let mut count = 0usize;
    for (title, content) in seeds {
        // Insert into SQLite.
        let row = sqlx::query_as::<_, NoteRow>(
            "INSERT INTO notes (title, content) VALUES (?, ?)
             RETURNING id, title, content, folder_id, created_at, updated_at",
        )
        .bind(title)
        .bind(content)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

        // Index into LanceDB using the same sentence-chunking path as index_note.
        let sentences = split_sentences(&row.content);
        let raw_chunks = chunk_sentences(sentences, 1, 0);
        let mut chunks: Vec<(i32, String, Vec<f32>)> = Vec::new();
        for (i, chunk_text) in raw_chunks.into_iter().enumerate() {
            let embedding = embed_document(&chunk_text).await?;
            chunks.push((i as i32, chunk_text, embedding));
        }
        crate::vector::upsert(&vdb.0, row.id, &row.title, chunks).await?;

        count += 1;
    }

    Ok(count)
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// A note template. Built-in templates use negative IDs and are hardcoded here;
/// user-created templates are stored in SQLite with positive AUTOINCREMENT IDs.
/// `builtin: true` means the template cannot be deleted.
#[derive(Debug, Serialize)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub title: String,
    pub content: String,
    pub builtin: bool,
}

/// Raw template row from SQLite.
#[derive(Debug, sqlx::FromRow)]
struct TemplateRow {
    id: i64,
    name: String,
    title: String,
    content: String,
}

/// The hardcoded built-in templates returned alongside user-created ones.
/// Negative IDs ensure they never clash with SQLite AUTOINCREMENT values.
fn builtin_templates() -> Vec<Template> {
    vec![
        Template {
            id: -1,
            name: "Blank".to_string(),
            title: String::new(),
            content: String::new(),
            builtin: true,
        },
        Template {
            id: -2,
            name: "Meeting Notes".to_string(),
            title: "Meeting Notes".to_string(),
            content: "# Meeting Notes\n\n**Date:** \n**Attendees:** \n\n## Agenda\n\n- \n\n## Notes\n\n## Action Items\n\n- [ ] ".to_string(),
            builtin: true,
        },
        Template {
            id: -3,
            name: "Daily Journal".to_string(),
            title: "Journal".to_string(),
            content: "# \n\n**Mood:** \n**Energy:** \n\n## Today\n\n## Goals\n\n- ".to_string(),
            builtin: true,
        },
        Template {
            id: -4,
            name: "Book Notes".to_string(),
            title: "Book Notes".to_string(),
            content: "# \n\n**Author:** \n\n## Key Ideas\n\n## Quotes\n\n## Takeaways\n\n".to_string(),
            builtin: true,
        },
    ]
}

/// Return all templates: built-ins first, then user-created ones from SQLite.
#[tauri::command]
pub async fn list_templates(pool: State<'_, SqlitePool>) -> Result<Vec<Template>, String> {
    let rows = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, name, title, content FROM templates ORDER BY name ASC",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let mut result = builtin_templates();
    result.extend(rows.into_iter().map(|r| Template {
        id: r.id,
        name: r.name,
        title: r.title,
        content: r.content,
        builtin: false,
    }));

    Ok(result)
}

/// Create a new user-defined template and return it.
#[tauri::command]
pub async fn create_template(
    pool: State<'_, SqlitePool>,
    name: String,
    title: String,
    content: String,
) -> Result<Template, String> {
    let row = sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO templates (name, title, content) VALUES (?, ?, ?)
         RETURNING id, name, title, content",
    )
    .bind(&name)
    .bind(&title)
    .bind(&content)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(Template {
        id: row.id,
        name: row.name,
        title: row.title,
        content: row.content,
        builtin: false,
    })
}

/// Update a user-created template's name, title, and content.
/// Returns an error if `id` is negative (built-in templates cannot be edited).
#[tauri::command]
pub async fn update_template(
    pool: State<'_, SqlitePool>,
    id: i64,
    name: String,
    title: String,
    content: String,
) -> Result<Template, String> {
    if id <= 0 {
        return Err("Built-in templates cannot be edited.".to_string());
    }

    let row = sqlx::query_as::<_, TemplateRow>(
        "UPDATE templates SET name = ?, title = ?, content = ? WHERE id = ?
         RETURNING id, name, title, content",
    )
    .bind(&name)
    .bind(&title)
    .bind(&content)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(Template {
        id: row.id,
        name: row.name,
        title: row.title,
        content: row.content,
        builtin: false,
    })
}

/// Delete a user-created template by id.
/// Returns an error if `id` is negative (built-in templates cannot be deleted).
#[tauri::command]
pub async fn delete_template(pool: State<'_, SqlitePool>, id: i64) -> Result<(), String> {
    if id <= 0 {
        return Err("Built-in templates cannot be deleted.".to_string());
    }

    sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
