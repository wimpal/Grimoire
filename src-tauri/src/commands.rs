use serde::Serialize;
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
