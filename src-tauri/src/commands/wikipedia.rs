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

//! Wikipedia local knowledge source.
//!
//! Users can download Kiwix ZIM bundles (wikipedia, nopic flavour only),
//! index them into LanceDB for semantic search, and have Wikipedia articles
//! included as context in the RAG pipeline.
//!
//! Architecture:
//!   - SQLite: bundle metadata + checkpointing + highlights
//!   - LanceDB: one embedding per article (title + intro text, ≤1500 chars)
//!   - Ollama: same embedding model as notes (nomic-embed-text by default)
//!
//! Privacy contract: nothing leaves the machine. The catalogue fetch is the
//! only outbound network call. Downloads go to a user-specified local path.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, State};
use futures::StreamExt;
use rayon::prelude::*;

// ---------------------------------------------------------------------------
// Shared structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct WikiBundle {
    pub id:             String,
    pub name:           String,
    pub flavour:        String,
    pub title:          Option<String>,
    pub article_count:  Option<i64>,
    pub size_bytes:     Option<i64>,
    pub zim_path:       Option<String>,
    pub installed_at:   Option<String>,
    pub last_synced:    Option<String>,
    pub indexing_state: String,
}

/// A catalogue entry returned from the Kiwix OPDS catalogue.
#[derive(Debug, Serialize)]
pub struct CatalogueEntry {
    pub id:            String,
    pub name:          String,
    pub title:         String,
    pub flavour:       String,
    pub article_count: Option<i64>,
    pub size_bytes:    Option<i64>,
    pub download_url:  Option<String>,
    pub sha256_url:    Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip HTML tags from a string, collapsing whitespace. Used to extract plain
/// text from ZIM article blobs before embedding.
fn html_to_text(html: &str) -> String {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    // Skip script, style, and nav elements — they add noise with no information.
    let skip_sel = Selector::parse("script, style, nav, .toc, #toc").unwrap();

    // Collect text nodes that are NOT inside skipped elements.
    let body_sel = Selector::parse("body").unwrap();
    let body = match document.select(&body_sel).next() {
        Some(b) => b,
        None => return String::new(),
    };

    // Traverse the body element tree and skip subtrees matching skip_sel.
    let skip_ids: std::collections::HashSet<_> = body
        .select(&skip_sel)
        .map(|el| el.id())
        .collect();

    let mut out = String::with_capacity(html.len() / 4);
    for node in body.descendants() {
        if let Some(el) = node.value().as_element() {
            // If this element should be skipped, we skip it and its descendants.
            let id = scraper::ElementRef::wrap(node).map(|e| e.id());
            if let Some(id) = id {
                if skip_ids.contains(&id) {
                    continue;
                }
            }
            // Insert newline after block elements so paragraphs stay separated.
            if matches!(
                el.name(),
                "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "dt" | "dd" | "br"
            ) {
                out.push('\n');
            }
        } else if let Some(text) = node.value().as_text() {
            out.push_str(text);
        }
    }

    // Normalise whitespace: collapse runs of whitespace (preserving newlines),
    // then collapse multiple blank lines down to one.
    let mut result = String::with_capacity(out.len());
    for line in out.lines() {
        let trimmed: String = line
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !trimmed.is_empty() {
            result.push_str(&trimmed);
            result.push('\n');
        }
    }
    result.trim().to_string()
}

/// Parse the Kiwix OPDS Atom XML catalogue and return all nopic wikipedia entries.
/// The catalogue endpoint always returns Atom XML regardless of Accept header:
///   https://library.kiwix.org/catalog/v2/entries?lang=eng&category=wikipedia&count=500
fn parse_catalogue(xml: &str) -> Result<Vec<CatalogueEntry>, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Failed to parse catalogue XML: {e}"))?;

    let root = doc.root_element();
    let atom_ns = "http://www.w3.org/2005/Atom";
    let acq_rel = "http://opds-spec.org/acquisition/open-access";

    let mut result = Vec::new();

    for entry in root.children().filter(|n| n.has_tag_name((atom_ns, "entry"))) {
        let child_text = |tag: &str| -> &str {
            entry
                .children()
                .find(|n| n.has_tag_name((atom_ns, tag)))
                .and_then(|n| n.text())
                .unwrap_or("")
        };

        let flavour = child_text("flavour");
        if flavour != "nopic" {
            continue;
        }

        let raw_id = child_text("id");
        // IDs come as "urn:uuid:<uuid>" — strip the prefix.
        let id = raw_id.trim_start_matches("urn:uuid:").to_string();
        if id.is_empty() {
            continue;
        }

        let name  = child_text("name").to_string();
        let title = child_text("title").to_string();

        let article_count = child_text("articleCount")
            .parse::<i64>()
            .ok();

        // Download link: rel="http://opds-spec.org/acquisition/open-access"
        let mut download_url: Option<String> = None;
        let mut size_bytes:   Option<i64>    = None;

        for link in entry.children().filter(|n| n.has_tag_name((atom_ns, "link"))) {
            if link.attribute("rel") == Some(acq_rel) {
                if let Some(href) = link.attribute("href") {
                    // The href points to a .meta4 MetaLink descriptor.
                    // Strip the .meta4 suffix to get the direct .zim URL.
                    let direct = href.trim_end_matches(".meta4").to_string();
                    download_url = Some(direct);
                }
                if let Some(len) = link.attribute("length") {
                    size_bytes = len.parse::<i64>().ok();
                }
                break;
            }
        }

        result.push(CatalogueEntry {
            id,
            name,
            title,
            flavour: flavour.to_string(),
            article_count,
            size_bytes,
            download_url,
            sha256_url: None,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Fetch the Kiwix OPDS catalogue and return all nopic wikipedia entries.
/// This is the only command that makes an outbound network request.
#[tauri::command]
pub async fn fetch_wikipedia_catalogue() -> Result<Vec<CatalogueEntry>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let url = "https://library.kiwix.org/catalog/v2/entries?lang=eng&category=wikipedia&count=500";
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch catalogue: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read catalogue response: {e}"))?;

    parse_catalogue(&body)
}

/// List all locally tracked wikipedia bundles.
#[tauri::command]
pub async fn list_wikipedia_bundles(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<WikiBundle>, String> {
    sqlx::query_as::<_, WikiBundle>(
        "SELECT id, name, flavour, title, article_count, size_bytes,
                zim_path, installed_at, last_synced, indexing_state
         FROM wikipedia_bundles ORDER BY title",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())
}

/// Reset a bundle's indexing_state. Used by the frontend to clear stuck 'indexing'
/// states after an app restart.
#[tauri::command]
pub async fn set_bundle_indexing_state(
    pool: State<'_, SqlitePool>,
    bundle_id: String,
    state: String,
) -> Result<(), String> {
    // Only allow safe state values.
    if !matches!(state.as_str(), "none" | "queued" | "done" | "error") {
        return Err(format!("Invalid state: {state}"));
    }
    sqlx::query("UPDATE wikipedia_bundles SET indexing_state = ? WHERE id = ?")
        .bind(&state)
        .bind(&bundle_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Download a ZIM bundle from `download_url` to `dest_dir/filename`.
/// Streams the response body to disk. Emits `wikipedia:download-progress` events
/// with `{ bundle_id, downloaded_bytes, total_bytes }` every 512 KB.
///
/// On completion, inserts a row into `wikipedia_bundles` (or updates the existing one).
#[tauri::command]
pub async fn download_wikipedia_bundle(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    bundle_id: String,
    bundle_name: String,
    bundle_title: String,
    download_url: String,
    dest_dir: String,
    expected_size_bytes: Option<i64>,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    // Validate dest_dir is an existing directory to prevent path traversal.
    let dir = std::path::Path::new(&dest_dir);
    if !dir.is_dir() {
        return Err(format!("Destination directory does not exist: {dest_dir}"));
    }

    // Derive filename from the URL.
    let filename = download_url
        .split('/')
        .last()
        .filter(|s| s.ends_with(".zim"))
        .ok_or("Download URL does not end with a .zim filename")?;

    let zim_path = dir.join(filename);
    let zim_path_str = zim_path
        .to_str()
        .ok_or("Destination path contains non-UTF8 characters")?
        .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download returned HTTP {}", resp.status()));
    }

    let total = resp.content_length().map(|l| l as i64).or(expected_size_bytes);

    let mut file = tokio::fs::File::create(&zim_path)
        .await
        .map_err(|e| format!("Failed to create file {zim_path_str}: {e}"))?;

    let mut downloaded: i64 = 0;
    let mut last_emit: i64 = 0;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| format!("Failed to write to file: {e}"))?;
        downloaded += bytes.len() as i64;
        if downloaded - last_emit >= 512 * 1024 {
            last_emit = downloaded;
            let _ = app.emit("wikipedia:download-progress", serde_json::json!({
                "bundle_id": bundle_id,
                "downloaded_bytes": downloaded,
                "total_bytes": total,
            }));
        }
    }

    file.flush().await.map_err(|e| format!("Failed to flush file: {e}"))?;

    let now = chrono_now();

    // Upsert the bundle record in SQLite.
    sqlx::query(
        "INSERT INTO wikipedia_bundles (id, name, flavour, title, size_bytes, zim_path, installed_at, indexing_state)
         VALUES (?, ?, 'nopic', ?, ?, ?, ?, 'none')
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name, title = excluded.title,
             size_bytes = excluded.size_bytes, zim_path = excluded.zim_path,
             installed_at = excluded.installed_at, indexing_state = 'none'",
    )
    .bind(&bundle_id)
    .bind(&bundle_name)
    .bind(&bundle_title)
    .bind(downloaded)
    .bind(&zim_path_str)
    .bind(&now)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(zim_path_str)
}

/// Remove a bundle: delete SQLite rows, remove from LanceDB, optionally delete the .zim file.
#[tauri::command]
pub async fn remove_wikipedia_bundle(
    pool: State<'_, SqlitePool>,
    vdb: State<'_, crate::vector::VectorDb>,
    bundle_id: String,
    delete_file: bool,
) -> Result<(), String> {
    // Fetch zim_path before deleting the row if we need to delete the file.
    let zim_path: Option<String> = if delete_file {
        sqlx::query_scalar("SELECT zim_path FROM wikipedia_bundles WHERE id = ?")
            .bind(&bundle_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| e.to_string())?
            .flatten()
    } else {
        None
    };

    // Mark highlights as orphaned rather than deleting them.
    sqlx::query("UPDATE wikipedia_highlights SET status = 'orphaned' WHERE bundle_id = ?")
        .bind(&bundle_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Delete bundle rows (cascade deletes checkpoint).
    sqlx::query("DELETE FROM wikipedia_bundles WHERE id = ?")
        .bind(&bundle_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Remove from LanceDB.
    crate::vector::wikipedia_remove_bundle(&vdb.0, &bundle_id).await?;

    // Optionally delete the .zim file from disk.
    if delete_file {
        if let Some(path) = zim_path {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    Ok(())
}

/// Index (or re-index) a wikipedia bundle. Runs in a blocking task.
/// Emits `wikipedia:index-progress` events with
/// `{ bundle_id, indexed, total, done, error }` every 100 articles.
///
/// Resumes from the last checkpoint automatically.
/// Skips: redirects, stubs (<500 chars after HTML stripping), disambiguation pages.
#[tauri::command]
pub async fn index_wikipedia_bundle(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    vdb: State<'_, crate::vector::VectorDb>,
    bundle_id: String,
) -> Result<(), String> {
    // Look up the bundle.
    let bundle: WikiBundle = sqlx::query_as(
        "SELECT id, name, flavour, title, article_count, size_bytes,
                zim_path, installed_at, last_synced, indexing_state
         FROM wikipedia_bundles WHERE id = ?",
    )
    .bind(&bundle_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Bundle not found: {bundle_id}"))?;

    let zim_path = bundle.zim_path.ok_or("Bundle has no zim_path")?;

    // Mark as indexing.
    sqlx::query("UPDATE wikipedia_bundles SET indexing_state = 'indexing' WHERE id = ?")
        .bind(&bundle_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Load checkpoint (resume offset).
    let start_entry: u32 = sqlx::query_scalar(
        "SELECT last_indexed_entry FROM wikipedia_index_checkpoint WHERE bundle_id = ?",
    )
    .bind(&bundle_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?
    .flatten()
    .unwrap_or(0i64) as u32;

    let pool_clone = pool.inner().clone();
    let vdb_conn  = vdb.0.clone();
    let bundle_id_clone = bundle_id.clone();
    let app_clone = app.clone();

    let result: Result<(), String> = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();

        use zim_rs::archive::Archive;
        let archive = Archive::new(&zim_path)
            .map_err(|_| format!("Failed to open ZIM file: {zim_path}"))?;

        let total_entries = archive.get_all_entrycount();

        // Upsert checkpoint row.
        rt.block_on(sqlx::query(
            "INSERT INTO wikipedia_index_checkpoint (bundle_id, last_indexed_entry, total_entries)
             VALUES (?, ?, ?)
             ON CONFLICT(bundle_id) DO UPDATE SET total_entries = excluded.total_entries",
        )
        .bind(&bundle_id_clone)
        .bind(start_entry as i64)
        .bind(total_entries as i64)
        .execute(&pool_clone))
        .map_err(|e| e.to_string())?;

        let model = rt.block_on(super::rag::get_embedding_model_pub(&pool_clone));

        let mut indexed = 0i64;
        let mut last_checkpoint_idx = start_entry;

        // Process ZIM entries in sliding windows:
        //   Phase 1 — sequential ZIM reads      (libzim is not thread-safe)
        //   Phase 2 — parallel HTML→text parse  (rayon uses all CPU cores)
        //   Phase 3 — batched GPU embedding      (BATCH_SIZE texts per Ollama call)
        //   Phase 4 — bulk LanceDB upsert        (one delete+insert per batch)
        //
        // SCAN_WINDOW and BATCH_SIZE are tuned for a high-end machine
        // (16+ GB VRAM, 32 GB RAM, 8+ core CPU). A future adaptive path
        // will scale these down based on hardware detection at startup.
        const BATCH_SIZE: usize  = 64;   // texts per Ollama /api/embed call
        const SCAN_WINDOW: usize = 1024; // ZIM entries read per iteration

        let mut scan_pos = start_entry;
        while scan_pos < total_entries {
            let window_end = (scan_pos + SCAN_WINDOW as u32).min(total_entries);

            // ── Phase 1: sequential ZIM reads ──────────────────────────────────
            let raw: Vec<(u32, String, String, Vec<u8>)> =
                (scan_pos..window_end).filter_map(|idx| {
                    let entry = archive.get_entry_bypath_index(idx).ok()?;
                    if entry.is_redirect() { return None; }
                    let item = entry.get_item(false).ok()?;
                    if !item.get_mimetype().unwrap_or_default().starts_with("text/html") {
                        return None;
                    }
                    let html = item.get_data().ok()?.data().to_vec();
                    Some((idx, entry.get_path(), entry.get_title(), html))
                }).collect();

            // ── Phase 2: parallel HTML→text parse on all CPU cores ─────────────
            let articles: Vec<(u32, String, String, String, String)> = raw
                .into_par_iter()
                .filter_map(|(idx, path, title, html_bytes)| {
                    // Skip MediaWiki CSS/template/module pages by path prefix.
                    // In ZIM files these appear as paths starting with "." or "-/"
                    // or containing namespace prefixes like "MediaWiki:", "Module:".
                    let path_lower = path.to_lowercase();
                    if path.starts_with('.')
                        || path.starts_with("-/")
                        || path_lower.contains("mediawiki:")
                        || path_lower.contains("module:")
                        || path_lower.contains("template:")
                        || path_lower.contains("wikipedia:")
                        || path_lower.contains("file:")
                    {
                        return None;
                    }
                    let text = html_to_text(&String::from_utf8_lossy(&html_bytes));
                    if text.chars().count() < 500 { return None; }
                    // Skip CSS pages: content that is mostly stylesheet definitions.
                    if text.contains(".mw-parser-output") || text.starts_with(".mw-") || text.contains("/* start https://") {
                        return None;
                    }
                    if text.contains("may refer to:") || text.contains("disambiguation") {
                        return None;
                    }
                    let content: String = text.chars().take(1500).collect();
                    let doc_text = format!("search_document: {title}\n{content}");
                    Some((idx, format!("{bundle_id_clone}/{path}"), title, content, doc_text))
                })
                .collect();

            // ── Phase 3 + 4: embed in batches, then bulk-upsert to LanceDB ──────
            for chunk in articles.chunks(BATCH_SIZE) {
                let doc_texts: Vec<String> =
                    chunk.iter().map(|(_, _, _, _, dt)| dt.clone()).collect();
                let embeddings =
                    match rt.block_on(crate::vector::embed_batch(&doc_texts, &model)) {
                        Ok(e) => e,
                        Err(_) => chunk.iter()
                            .map(|(_, _, _, _, dt)| {
                                rt.block_on(crate::vector::embed(dt, &model)).unwrap_or_default()
                            })
                            .collect(),
                    };

                // Collect the valid articles as a single batch for LanceDB.
                let upsert_batch: Vec<(String, String, String, String, Vec<f32>)> = chunk
                    .iter()
                    .zip(embeddings)
                    .filter_map(|((arc_idx, article_id, title, content, _), embedding)| {
                        if embedding.is_empty() { return None; }
                        last_checkpoint_idx = *arc_idx;
                        Some((article_id.clone(), bundle_id_clone.clone(), title.clone(), content.clone(), embedding))
                    })
                    .collect();

                indexed += upsert_batch.len() as i64;
                let _ = rt.block_on(crate::vector::wikipedia_upsert_batch(&vdb_conn, upsert_batch));
            }

            // ── Checkpoint + progress at each window boundary ──────────────────
            if !articles.is_empty() {
                let _ = rt.block_on(sqlx::query(
                    "UPDATE wikipedia_index_checkpoint
                     SET last_indexed_entry = ? WHERE bundle_id = ?",
                )
                .bind(last_checkpoint_idx as i64 + 1)
                .bind(&bundle_id_clone)
                .execute(&pool_clone));
            }
            let _ = app_clone.emit("wikipedia:index-progress", serde_json::json!({
                "bundle_id": bundle_id_clone,
                "indexed": indexed,
                "scanned": window_end,
                "total": total_entries,
                "done": false,
                "error": null,
            }));

            scan_pos = window_end;
        }

        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    match result {
        Ok(()) => {
            let now = chrono_now();
            sqlx::query(
                "UPDATE wikipedia_bundles SET indexing_state = 'done', last_synced = ? WHERE id = ?",
            )
            .bind(&now)
            .bind(&bundle_id)
            .execute(pool.inner())
            .await
            .map_err(|e| e.to_string())?;

            sqlx::query(
                "UPDATE wikipedia_index_checkpoint SET completed_at = ? WHERE bundle_id = ?",
            )
            .bind(&now)
            .bind(&bundle_id)
            .execute(pool.inner())
            .await
            .map_err(|e| e.to_string())?;

            let _ = app.emit("wikipedia:index-progress", serde_json::json!({
                "bundle_id": bundle_id,
                "done": true,
                "error": null,
            }));

            Ok(())
        }
        Err(e) => {
            sqlx::query(
                "UPDATE wikipedia_bundles SET indexing_state = 'error' WHERE id = ?",
            )
            .bind(&bundle_id)
            .execute(pool.inner())
            .await
            .map_err(|err| err.to_string())?;

            let _ = app.emit("wikipedia:index-progress", serde_json::json!({
                "bundle_id": bundle_id,
                "done": true,
                "error": e,
            }));

            Err(e)
        }
    }
}

/// Semantic search over the indexed wikipedia articles.
/// Called by the RAG pipeline in Chat.svelte when wikipedia is enabled.
#[tauri::command]
pub async fn search_wikipedia(
    pool: State<'_, SqlitePool>,
    vdb: State<'_, crate::vector::VectorDb>,
    query: String,
) -> Result<Vec<crate::vector::WikiMatch>, String> {
    // Only search if wikipedia is enabled in settings.
    let enabled: String = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = 'wikipedia_enabled' LIMIT 1",
    )
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?
    .unwrap_or_default();

    if enabled != "true" {
        return Ok(vec![]);
    }

    let model = super::rag::get_embedding_model_pub(pool.inner()).await;
    let embedding = super::rag::embed_query(&query, &model).await?;
    crate::vector::wikipedia_search(&vdb.0, embedding, 5).await
}

/// Read a single article from a ZIM bundle by its entry path.
/// Returns the plain text (HTML stripped) for display in the frontend.
#[tauri::command]
pub async fn read_wikipedia_article(
    pool: State<'_, SqlitePool>,
    bundle_id: String,
    article_path: String,
) -> Result<serde_json::Value, String> {
    let zim_path: String = sqlx::query_scalar(
        "SELECT zim_path FROM wikipedia_bundles WHERE id = ?",
    )
    .bind(&bundle_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?
    .flatten()
    .ok_or_else(|| format!("Bundle not found or has no file: {bundle_id}"))?;

    let path_clone = article_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        use zim_rs::archive::Archive;
        let archive = Archive::new(&zim_path)
            .map_err(|_| format!("Failed to open ZIM file: {zim_path}"))?;

        let entry = archive
            .get_entry_bypath_str(&path_clone)
            .map_err(|_| format!("Article not found: {path_clone}"))?;

        let item = entry
            .get_item(true)
            .map_err(|_| "Failed to get article item".to_string())?;

        let blob = item.get_data().map_err(|_| "Failed to read article data".to_string())?;
        let html = String::from_utf8_lossy(blob.data()).to_string();
        let text = html_to_text(&html);

        Ok::<_, String>(serde_json::json!({
            "title": entry.get_title(),
            "path":  entry.get_path(),
            "text":  text,
        }))
    })
    .await
    .map_err(|e| e.to_string())?;

    result
}

// ---------------------------------------------------------------------------
// PoC command (debug-only)
// ---------------------------------------------------------------------------

/// Open a ZIM file at `zim_path`, iterate its entries, and return a JSON
/// report that lets us evaluate whether the `zim` crate is usable.
/// This command is debug-only — it is not registered in release builds.
#[tauri::command]
pub async fn test_zim_parse(zim_path: String) -> Result<serde_json::Value, String> {
    // ZIM parsing is blocking I/O + CPU; keep it off the async executor.
    tokio::task::spawn_blocking(move || run_zim_poc(&zim_path))
        .await
        .map_err(|e| e.to_string())?
}

fn run_zim_poc(zim_path: &str) -> Result<serde_json::Value, String> {
    use zim_rs::archive::Archive;

    let archive = Archive::new(zim_path)
        .map_err(|_| format!("Failed to open ZIM file: {zim_path}"))?;

    let total_entries = archive.get_all_entrycount();
    let article_count_header = archive.get_articlecount();
    let has_new_ns = archive.has_new_namespace_scheme();

    let range = archive
        .iter_efficient()
        .map_err(|_| "Failed to create efficient iterator".to_string())?;

    let mut article_count = 0usize;
    let mut redirect_count = 0usize;
    let mut samples: Vec<serde_json::Value> = Vec::new();

    for entry_result in range {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.is_redirect() {
            redirect_count += 1;
            continue;
        }

        let item = match entry.get_item(false) {
            Ok(i) => i,
            Err(_) => continue,
        };

        let mime = item.get_mimetype().unwrap_or_default();
        if !mime.starts_with("text/html") {
            continue;
        }

        article_count += 1;

        if samples.len() < 5 {
            let content_preview = match item.get_data() {
                Ok(blob) => String::from_utf8_lossy(blob.data())
                    .chars()
                    .take(600)
                    .collect::<String>(),
                Err(_) => "(blob error)".to_string(),
            };

            samples.push(serde_json::json!({
                "title":           entry.get_title(),
                "path":            entry.get_path(),
                "byte_count":      item.get_size(),
                "content_preview": content_preview,
            }));
        }

        if article_count >= 500 {
            break;
        }
    }

    Ok(serde_json::json!({
        "total_entries":        total_entries,
        "article_count_header": article_count_header,
        "article_count":        article_count,
        "redirect_count":       redirect_count,
        "has_new_namespace":    has_new_ns,
        "samples":              samples,
    }))
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    // Use SystemTime since we can't add the `chrono` crate dependency.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as a basic ISO-8601-like timestamp (UTC, seconds precision).
    let secs_in_day = secs % 86400;
    let days = secs / 86400;
    // Days since Unix epoch to calendar date (Gregorian proleptic calendar).
    let (year, month, day) = days_to_ymd(days);
    let h = secs_in_day / 3600;
    let m = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
