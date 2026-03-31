use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A note row as returned from the database.
/// `sqlx::FromRow` lets sqlx map a query result row directly into this struct.
/// `Serialize` lets Tauri convert it to JSON before sending it to the frontend.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub folder_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A folder row as returned from the database.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub created_at: i64,
}

/// A minimal note reference used for tag/link results — just enough to render
/// a clickable pill in the UI without transferring full note content.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LinkedNote {
    pub id: i64,
    pub title: String,
}

/// A node in the graph: a note with its id and title.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GraphNode {
    pub id: i64,
    pub title: String,
}

/// A directed edge between two notes.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GraphEdge {
    pub source: i64,
    pub target: i64,
}

// ---------------------------------------------------------------------------
// Note commands
// ---------------------------------------------------------------------------

/// Create a new note and return the full row.
#[tauri::command]
pub async fn create_note(
    pool: State<'_, SqlitePool>,
    title: String,
    folder_id: Option<i64>,
) -> Result<Note, String> {
    let row = sqlx::query_as::<_, Note>(
        "INSERT INTO notes (title, folder_id) VALUES (?, ?)
         RETURNING id, title, content, folder_id, created_at, updated_at",
    )
    .bind(&title)
    .bind(folder_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

/// Fetch a single note by id.
#[tauri::command]
pub async fn get_note(pool: State<'_, SqlitePool>, id: i64) -> Result<Note, String> {
    let row = sqlx::query_as::<_, Note>(
        "SELECT id, title, content, folder_id, created_at, updated_at
         FROM notes WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

/// List all notes, optionally filtered to a specific folder.
/// Pass `null` from JS to get notes with no folder, omit the filter to get all.
/// This command takes an explicit `all` flag to distinguish "no folder" from "every folder".
#[tauri::command]
pub async fn list_notes(
    pool: State<'_, SqlitePool>,
    folder_id: Option<i64>,
    all: Option<bool>,
) -> Result<Vec<Note>, String> {
    let rows = if all.unwrap_or(false) {
        // Return every note regardless of folder.
        sqlx::query_as::<_, Note>(
            "SELECT id, title, content, folder_id, created_at, updated_at
             FROM notes ORDER BY updated_at DESC",
        )
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?
    } else {
        // Return notes in a specific folder (or unfiled notes when folder_id is null).
        sqlx::query_as::<_, Note>(
            "SELECT id, title, content, folder_id, created_at, updated_at
             FROM notes WHERE folder_id IS ? ORDER BY updated_at DESC",
        )
        .bind(folder_id)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?
    };

    Ok(rows)
}

/// Update a note's title and content. Bumps updated_at to the current time.
#[tauri::command]
pub async fn update_note(
    pool: State<'_, SqlitePool>,
    id: i64,
    title: String,
    content: String,
) -> Result<Note, String> {
    let row = sqlx::query_as::<_, Note>(
        "UPDATE notes
         SET title = ?, content = ?, updated_at = unixepoch()
         WHERE id = ?
         RETURNING id, title, content, folder_id, created_at, updated_at",
    )
    .bind(&title)
    .bind(&content)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

/// Move a note to a different folder (or to no folder when folder_id is null).
#[tauri::command]
pub async fn move_note(
    pool: State<'_, SqlitePool>,
    id: i64,
    folder_id: Option<i64>,
) -> Result<Note, String> {
    let row = sqlx::query_as::<_, Note>(
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

    Ok(row)
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
    name: String,
    parent_id: Option<i64>,
) -> Result<Folder, String> {
    let row = sqlx::query_as::<_, Folder>(
        "INSERT INTO folders (name, parent_id) VALUES (?, ?)
         RETURNING id, name, parent_id, created_at",
    )
    .bind(&name)
    .bind(parent_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

/// List all folders. The frontend is responsible for building the tree from parent_id.
#[tauri::command]
pub async fn list_folders(pool: State<'_, SqlitePool>) -> Result<Vec<Folder>, String> {
    let rows = sqlx::query_as::<_, Folder>(
        "SELECT id, name, parent_id, created_at FROM folders ORDER BY name ASC",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows)
}

/// Rename a folder.
#[tauri::command]
pub async fn rename_folder(
    pool: State<'_, SqlitePool>,
    id: i64,
    name: String,
) -> Result<Folder, String> {
    let row = sqlx::query_as::<_, Folder>(
        "UPDATE folders SET name = ? WHERE id = ?
         RETURNING id, name, parent_id, created_at",
    )
    .bind(&name)
    .bind(id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
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
const EMBED_MODEL: &str = "nomic-embed-text";

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
        let embedding = crate::vector::embed(&chunk_text, EMBED_MODEL).await?;
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
#[tauri::command]
pub async fn search_notes(
    vdb: State<'_, crate::vector::VectorDb>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::vector::NoteMatch>, String> {
    let embedding = crate::vector::embed(&query, EMBED_MODEL).await?;
    crate::vector::search(&vdb.0, embedding, limit.unwrap_or(10)).await
}

/// Re-index every note currently in SQLite into LanceDB from scratch.
/// Useful after schema migrations, accidental index loss, or any time SQLite
/// and LanceDB have drifted out of sync. Existing index entries are replaced.
/// Returns the number of notes indexed.
#[tauri::command]
pub async fn reindex_all(
    pool: State<'_, SqlitePool>,
    vdb: State<'_, crate::vector::VectorDb>,
) -> Result<usize, String> {
    let notes = sqlx::query_as::<_, Note>(
        "SELECT id, title, content, folder_id, created_at, updated_at FROM notes",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    for note in &notes {
        let sentences = split_sentences(&note.content);
        let raw_chunks = chunk_sentences(sentences, 1, 0);
        let mut chunks: Vec<(i32, String, Vec<f32>)> = Vec::new();
        for (i, chunk_text) in raw_chunks.into_iter().enumerate() {
            let embedding = crate::vector::embed(&chunk_text, EMBED_MODEL).await?;
            chunks.push((i as i32, chunk_text, embedding));
        }
        crate::vector::upsert(&vdb.0, note.id, &note.title, chunks).await?;
    }

    Ok(notes.len())
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
    tag: String,
) -> Result<Vec<Note>, String> {
    let notes = sqlx::query_as::<_, Note>(
        "SELECT n.id, n.title, n.content, n.folder_id, n.created_at, n.updated_at
         FROM notes n
         JOIN note_tags nt ON nt.note_id = n.id
         JOIN tags t ON t.id = nt.tag_id
         WHERE t.name = ?
         ORDER BY n.updated_at DESC",
    )
    .bind(tag.to_lowercase())
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
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
    let embedding = crate::vector::embed(&query, EMBED_MODEL).await?;
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
        let row = sqlx::query_as::<_, Note>(
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
            let embedding = crate::vector::embed(&chunk_text, EMBED_MODEL).await?;
            chunks.push((i as i32, chunk_text, embedding));
        }
        crate::vector::upsert(&vdb.0, row.id, &row.title, chunks).await?;

        count += 1;
    }

    Ok(count)
}
