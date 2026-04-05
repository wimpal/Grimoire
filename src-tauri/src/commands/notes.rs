// Copyright (C) 2026 Wim Palland
//
// This file is part of Grimoire.
//
// Grimoire is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Grimoire is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Grimoire. If not, see <https://www.gnu.org/licenses/>.

use sqlx::SqlitePool;
use tauri::State;
use crate::KeyStore;
use super::{NoteRow, Note, FolderRow, Folder, resolve_key, folder_is_locked, map_note_row, map_folder_row};

// ---------------------------------------------------------------------------
// Note commands
// ---------------------------------------------------------------------------

/// Create a new note and return the full row.
#[tauri::command]
pub async fn create_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    title: String,
    folder_id: Option<i64>,
) -> Result<Note, String> {
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

    let note = map_note_row(row, false, &keys);
    if !note.locked {
        super::search::fts_upsert(pool.inner(), note.id, &note.title, &note.content).await;
    }
    Ok(note)
}

/// Fetch a single note by id.
#[tauri::command]
pub async fn get_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    id: i64,
) -> Result<Note, String> {
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

    let note = map_note_row(row, folder_locked, &keys);
    if !note.locked {
        super::search::fts_upsert(pool.inner(), note.id, &note.title, &note.content).await;
    }
    Ok(note)
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

    super::search::fts_delete(pool.inner(), id).await;
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
