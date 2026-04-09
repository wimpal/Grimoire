<!-- Copyright (C) 2026 Wim Palland

This file is part of Grimoire.

Grimoire is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

Grimoire is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with Grimoire. If not, see <https://www.gnu.org/licenses/>. -->

<script>
  import { invoke } from '@tauri-apps/api/core';

  // ── Props ────────────────────────────────────────────────────────────────
  /** @type {{ folders: Array<{id: number, name: string}>, open: boolean, onSelectNote: (id: number) => void }} */
  let { folders = [], open = false, onSelectNote } = $props();

  // ── State ────────────────────────────────────────────────────────────────
  let query = $state('');
  let results = $state([]);
  let loading = $state(false);         // FTS in-flight
  let semanticLoading = $state(false); // semantic refinement in-flight
  let errorMsg = $state('');

  // Reciprocal Rank Fusion constant
  const RRF_K = 60;
  function rrfScore(rank) { return 1 / (RRF_K + rank + 1); }

  // Input element ref for auto-focus
  let inputEl = $state(null);

  // ── Derived ──────────────────────────────────────────────────────────────
  /** Map folder id → name for quick lookup when rendering results. */
  let folderMap = $derived(
    Object.fromEntries(folders.map(f => [f.id, f.name]))
  );

  // ── Search logic ─────────────────────────────────────────────────────────

  let debounceTimer = null;

  function onInput() {
    clearTimeout(debounceTimer);
    errorMsg = '';
    if (!query.trim()) {
      results = [];
      loading = false;
      return;
    }
    loading = true;
    debounceTimer = setTimeout(runSearch, 200);
  }

  async function runSearch() {
    const current = query.trim();
    if (!current) return;

    // ── Phase 1: instant FTS ─────────────────────────────────────────────
    loading = true;
    semanticLoading = false;
    results = [];
    try {
      const ftsRes = await invoke('fts_search', { query: current, limit: 12 });
      if (query.trim() !== current) return; // stale
      results = ftsRes.map((r, i) => ({
        note_id: r.note_id,
        title: r.title,
        folder_id: r.folder_id,
        snippet: r.snippet,
        excerpt: null,
        matched_by: 'fts',
        score: rrfScore(i),
      }));
      loading = false;
    } catch (e) {
      if (query.trim() === current) { errorMsg = String(e); loading = false; }
      return;
    }

    // ── Phase 2: semantic refinement (background) ────────────────────────
    semanticLoading = true;
    try {
      const semanticRes = await invoke('search_notes', { query: current });
      if (query.trim() !== current) return; // stale

      // JS-side RRF merge: promote notes that match both sources.
      const merged = new Map();
      for (const r of results) merged.set(r.note_id, { ...r });
      for (const [rank, m] of semanticRes.entries()) {
        const score = rrfScore(rank);
        if (merged.has(m.note_id)) {
          const entry = merged.get(m.note_id);
          entry.score += score;
          entry.matched_by = 'both';
          if (!entry.excerpt && m.excerpts.length > 0) entry.excerpt = m.excerpts[0];
        } else {
          merged.set(m.note_id, {
            note_id: m.note_id,
            title: m.title,
            folder_id: null,
            snippet: null,
            excerpt: m.excerpts[0] ?? null,
            matched_by: 'semantic',
            score,
          });
        }
      }
      results = [...merged.values()]
        .sort((a, b) => b.score - a.score)
        .slice(0, 12);
      semanticLoading = false;
    } catch (_e) {
      // Semantic failed silently — keep FTS results intact.
      if (query.trim() === current) semanticLoading = false;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') {
      query = '';
      results = [];
    }
  }

  // Focus the input whenever the panel becomes visible.
  $effect(() => {
    if (open && inputEl) inputEl.focus();
  });
</script>

<div class="search-panel">
  <div class="search-bar">
    <svg class="search-icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
      <circle cx="8.5" cy="8.5" r="5.5"/>
      <line x1="12.5" y1="12.5" x2="17" y2="17"/>
    </svg>
    <input
      bind:this={inputEl}
      bind:value={query}
      oninput={onInput}
      onkeydown={handleKeydown}
      class="search-input"
      type="search"
      placeholder="Search notes…"
      aria-label="Search notes"
      autocomplete="off"
      spellcheck="false"
    />
    {#if query}
      <button class="clear-btn" onclick={() => { query = ''; results = []; }} aria-label="Clear search">✕</button>
    {/if}
  </div>

  <div class="search-body" role="region" aria-label="Search results" aria-live="polite" aria-atomic="true" aria-busy={loading}>
    {#if loading}
      <span class="status-msg">Searching…</span>
    {:else if errorMsg}
      <span class="status-msg error">{errorMsg}</span>
    {:else if query.trim() && results.length === 0}
      <span class="status-msg">No results for <em>{query}</em></span>
    {:else if !query.trim()}
      <span class="status-msg hint">Type to search your notes</span>
    {:else}
      {#if semanticLoading}
        <span class="status-msg refining">Refining with semantic search…</span>
      {/if}
      <ul class="result-list">
        {#each results as result (result.note_id)}
          <li>
            <button class="result-row" onclick={() => onSelectNote(result.note_id)}>
              <div class="result-header">
                <span class="result-title">{result.title}</span>
                <span class="match-badge" class:badge-both={result.matched_by === 'both'}>
                  {result.matched_by === 'both' ? 'FTS + Semantic' : result.matched_by === 'fts' ? 'FTS' : 'Semantic'}
                </span>
              </div>
              {#if result.folder_id && folderMap[result.folder_id]}
                <span class="result-folder">{folderMap[result.folder_id]}</span>
              {/if}
              {#if result.snippet}
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                <!-- snippet comes from SQLite's FTS5 snippet() — the only HTML it can
                     contain is <b>…</b> tags wrapping matched terms. No user content
                     can inject arbitrary HTML here because FTS5 escapes all other text. -->
                <p class="result-excerpt">{@html result.snippet}</p>
              {:else if result.excerpt}
                <p class="result-excerpt">{result.excerpt}</p>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .search-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
  }

  /* ── Search bar ── */

  .search-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--bg2);
  }

  .search-icon {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    color: var(--text);
    opacity: 0.6;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font: inherit;
    font-size: 14px;
    color: var(--text-h);
    /* Remove browser default search input decorations */
    -webkit-appearance: none;
    appearance: none;
  }

  .search-input::placeholder {
    color: var(--text);
    opacity: 0.5;
  }

  /* Hide native clear button from browser — we render our own */
  .search-input::-webkit-search-cancel-button {
    display: none;
  }

  .clear-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text);
    opacity: 0.5;
    font-size: 12px;
    padding: 2px 4px;
    line-height: 1;
  }

  .clear-btn:hover {
    opacity: 1;
  }

  /* ── Body / status ── */

  .search-body {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0;
  }

  .status-msg {
    display: block;
    padding: 20px 16px;
    color: var(--text);
    opacity: 0.6;
    font-size: 13px;
  }

  .status-msg.hint {
    opacity: 0.45;
  }

  .status-msg.refining {
    padding: 6px 16px;
    font-size: 11px;
    opacity: 0.5;
  }

  .status-msg.error {
    opacity: 1;
    color: var(--danger);
  }

  /* ── Result list ── */

  .result-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .result-row {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    color: inherit;
    font: inherit;
    transition: background 0.1s;
  }

  .result-row:hover {
    background: var(--bg2);
  }

  .result-header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 2px;
  }

  .result-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--text-h);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Match badge ── */

  .match-badge {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text);
    opacity: 0.5;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: 3px;
    line-height: 1.4;
  }

  .match-badge.badge-both {
    color: var(--accent);
    opacity: 1;
    border-color: var(--accent);
  }

  /* ── Folder breadcrumb ── */

  .result-folder {
    display: block;
    font-size: 11px;
    color: var(--text);
    opacity: 0.6;
    margin-bottom: 3px;
  }

  /* ── Excerpt ── */

  .result-excerpt {
    margin: 0;
    font-size: 12px;
    color: var(--text);
    opacity: 0.75;
    line-height: 1.45;
    /* Cut to two lines */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  /* FTS5 snippet bold terms */
  .result-excerpt :global(b) {
    font-weight: 700;
    color: var(--text-h);
    opacity: 1;
  }
</style>
