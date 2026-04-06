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

use std::collections::HashMap;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;
use crate::KeyStore;
use super::{NoteRow, Note, map_note_row, resolve_key};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Per-day activity counts returned to the frontend for the heatmap.
///
/// `created`  = number of notes whose `created_at` falls on this day.
/// `modified` = number of notes whose `updated_at` falls on this day AND on a
///              different calendar day than their `created_at` (so a note is not
///              double-counted if it was created and saved on the same day).
#[derive(Debug, Serialize)]
pub struct ActivityDay {
    pub date: String, // YYYY-MM-DD (UTC)
    pub created: i64,
    pub modified: i64,
}

/// Internal SQL aggregate row — not exposed to the frontend.
#[derive(sqlx::FromRow)]
struct DateCount {
    date: String,
    cnt: i64,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Return per-day activity counts for the past 365 days.
///
/// Two queries are run: one counting created notes per day, one counting notes
/// that were modified on a different day than they were created. The results
/// are merged in Rust and returned sorted oldest-first.
///
/// Timestamps are stored as UTC Unix epoch integers, so all date grouping uses
/// SQLite's `date(ts, 'unixepoch')` which also returns UTC dates.
#[tauri::command]
pub async fn get_activity_heatmap(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<ActivityDay>, String> {
    let created: Vec<DateCount> = sqlx::query_as(
        "SELECT date(created_at, 'unixepoch') AS date, COUNT(*) AS cnt
         FROM notes
         WHERE created_at >= unixepoch('now', '-365 days')
         GROUP BY date
         ORDER BY date",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let modified: Vec<DateCount> = sqlx::query_as(
        "SELECT date(updated_at, 'unixepoch') AS date, COUNT(*) AS cnt
         FROM notes
         WHERE updated_at >= unixepoch('now', '-365 days')
           AND date(updated_at, 'unixepoch') != date(created_at, 'unixepoch')
         GROUP BY date
         ORDER BY date",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let mut map: HashMap<String, ActivityDay> = HashMap::new();

    for row in created {
        let entry = map.entry(row.date.clone()).or_insert(ActivityDay {
            date: row.date,
            created: 0,
            modified: 0,
        });
        entry.created += row.cnt;
    }

    for row in modified {
        let entry = map.entry(row.date.clone()).or_insert(ActivityDay {
            date: row.date,
            created: 0,
            modified: 0,
        });
        entry.modified += row.cnt;
    }

    let mut days: Vec<ActivityDay> = map.into_values().collect();
    days.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(days)
}

/// Return all notes that were created or last modified on the given calendar day.
///
/// `date_str` must be a UTC date in `YYYY-MM-DD` format.
/// Locked-folder notes are returned as locked stubs (no title/content), matching
/// the behaviour of `list_notes`.
#[tauri::command]
pub async fn get_notes_for_day(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    date_str: String,
) -> Result<Vec<Note>, String> {
    let rows: Vec<NoteRow> = sqlx::query_as(
        "SELECT id, title, content, folder_id, created_at, updated_at
         FROM notes
         WHERE date(created_at, 'unixepoch') = ?1
            OR date(updated_at,  'unixepoch') = ?1",
    )
    .bind(&date_str)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let folder_lock_states: HashMap<i64, bool> = {
        let locked_rows: Vec<(i64, i64)> = sqlx::query_as("SELECT id, locked FROM folders")
            .fetch_all(pool.inner())
            .await
            .unwrap_or_default();
        locked_rows.into_iter().map(|(id, lk)| (id, lk != 0)).collect()
    };

    let notes = rows
        .into_iter()
        .map(|row| {
            let fl = row
                .folder_id
                .and_then(|fid| folder_lock_states.get(&fid).copied())
                .unwrap_or(false);
            map_note_row(row, fl, &keys)
        })
        .collect();

    Ok(notes)
}

/// Find the daily note for `date_str` inside the "Daily Notes" folder, creating
/// both the folder and the note if they do not yet exist.
///
/// The stored note title is always `date_str` (ISO 8601: `YYYY-MM-DD`), encrypted
/// with the vault key when one is active. The "Daily Notes" folder never carries a
/// per-folder password, so `folder_locked = false` is safe to pass to `map_note_row`.
///
/// Because AES-GCM is non-deterministic we cannot query by encrypted title directly.
/// Instead, all notes in the folder are fetched and decrypted in Rust to find the
/// match — at most ~365 notes for a year of daily use, so this is acceptable.
#[tauri::command]
pub async fn get_or_create_daily_note(
    pool: State<'_, SqlitePool>,
    keys: State<'_, KeyStore>,
    date_str: String,
) -> Result<Note, String> {
    let vault_key = resolve_key(None, &keys);

    // ── Step 1: find or create the "Daily Notes" folder ──────────────────────

    let folder_rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM folders WHERE parent_id IS NULL")
            .fetch_all(pool.inner())
            .await
            .map_err(|e| e.to_string())?;

    let daily_folder_id = folder_rows.iter().find(|(_, enc_name)| {
        let name = if let Some(key) = vault_key {
            crate::crypto::decrypt(&key, enc_name)
                .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
                .unwrap_or_else(|_| enc_name.clone())
        } else {
            enc_name.clone()
        };
        name == "Daily Notes"
    }).map(|(id, _)| *id);

    let folder_id: i64 = if let Some(id) = daily_folder_id {
        id
    } else {
        let stored_name = if let Some(key) = vault_key {
            crate::crypto::encrypt(&key, b"Daily Notes")
        } else {
            "Daily Notes".to_string()
        };
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO folders (name, parent_id) VALUES (?, NULL) RETURNING id",
        )
        .bind(&stored_name)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
        id
    };

    // ── Step 2: find or create the note for this date ─────────────────────────

    let note_rows: Vec<NoteRow> = sqlx::query_as(
        "SELECT id, title, content, folder_id, created_at, updated_at
         FROM notes WHERE folder_id = ?",
    )
    .bind(folder_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let existing = note_rows.into_iter().find(|row| {
        let title = if let Some(key) = vault_key {
            crate::crypto::decrypt(&key, &row.title)
                .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
                .unwrap_or_else(|_| row.title.clone())
        } else {
            row.title.clone()
        };
        title == date_str
    });

    if let Some(row) = existing {
        return Ok(map_note_row(row, false, &keys));
    }

    let stored_title = if let Some(key) = vault_key {
        crate::crypto::encrypt(&key, date_str.as_bytes())
    } else {
        date_str.clone()
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
    super::search::fts_upsert(pool.inner(), note.id, &note.title, &note.content).await;
    Ok(note)
}
