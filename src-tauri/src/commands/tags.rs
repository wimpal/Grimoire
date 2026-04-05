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

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;
use crate::KeyStore;
use super::{Note, NoteRow, LinkedNote, GraphNode, GraphEdge, map_note_row};

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

/// Return all tags with a count of how many notes use each one, sorted by
/// name. Used by the sidebar tag browser.
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
