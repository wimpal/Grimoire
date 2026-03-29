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

/// Embed a note and store it in the vector index.
/// Called fire-and-forget from the frontend after every save/create.
#[tauri::command]
pub async fn index_note(
    vdb: State<'_, crate::vector::VectorDb>,
    note_id: i64,
    title: String,
    content: String,
) -> Result<(), String> {
    // Embed title + content together so search matches on either.
    let text = format!("{title}\n\n{content}");
    let embedding = crate::vector::embed(&text, EMBED_MODEL).await?;
    crate::vector::upsert(&vdb.0, note_id, &title, &content, embedding).await
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
    crate::vector::search(&vdb.0, embedding, limit.unwrap_or(3)).await
}

/// Insert a set of varied seed notes and index them all.
/// Returns the number of notes created. Intended for development/testing only
/// — the button that calls this should only appear in debug builds.
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

        // Index into LanceDB (embed title + content together).
        let text = format!("{}\n\n{}", row.title, row.content);
        let embedding = crate::vector::embed(&text, EMBED_MODEL).await?;
        crate::vector::upsert(&vdb.0, row.id, &row.title, &row.content, embedding).await?;

        count += 1;
    }

    Ok(count)
}
