<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Chat from './lib/Chat.svelte';
  import Graph from './lib/Graph.svelte';

  // ── State ──────────────────────────────────────────────────────────────────

  let folders = $state([]);
  let notes = $state([]);
  let selectedFolderId = $state(null); // null = "All notes"
  let activeNote = $state(null);       // the note currently open in the editor

  // Editor fields (kept in sync with activeNote)
  let editorTitle = $state('');
  let editorContent = $state('');
  let isDirty = $state(false);

  // Tracks the state of the background LanceDB indexing after each save.
  // 'idle'     — no index operation in progress
  // 'indexing' — embed calls are running (can be slow with many sentences)
  // 'error'    — indexing failed; note is saved in SQLite but not searchable
  let indexState = $state('idle');

  // Inline-creation inputs
  let newFolderName = $state('');
  let newNoteTitle = $state('');

  // Error display
  let errorMsg = $state('');

  // Chat panel
  let chatOpen = $state(false);

  // Graph overlay
  let graphOpen = $state(false);

  // Seed state
  let isSeeding = $state(false);
  let isReindexing = $state(false);

  // Tags and links for the active note
  let noteTags = $state([]);
  let noteLinks = $state([]);
  let noteBacklinks = $state([]);
  let tagFilter = $state(null); // when set, the note list shows only notes with this tag

  // All tags (for the sidebar browser)
  let allTags = $state([]);
  let tagSearch = $state('');
  const TAG_LIMIT = 3;  let tagsOpen = $state(true);
  // Tags shown in the sidebar: if searching, filter by prefix match;
  // otherwise show the top TAG_LIMIT by note count.
  let visibleTags = $derived(
    tagSearch.trim()
      ? allTags.filter(t => t.name.includes(tagSearch.trim().toLowerCase().replace(/^#/, '')))
      : allTags.slice(0, TAG_LIMIT)
  );

  // Sidebar collapse state
  let foldersOpen = $state(true);
  let notesOpen = $state(true);

  // Compute grid column widths reactively from all panel states.
  // $derived re-evaluates automatically whenever any of its dependencies change.
  let gridCols = $derived(
    [
      foldersOpen ? '200px' : '28px',
      notesOpen ? '240px' : '28px',
      '1fr',
      ...(chatOpen ? ['360px'] : []),
    ].join(' ')
  );

  // ── Helpers ────────────────────────────────────────────────────────────────

  function showError(e) {
    errorMsg = String(e);
    setTimeout(() => (errorMsg = ''), 4000);
  }

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadFolders() {
    try {
      folders = await invoke('list_folders');
    } catch (e) {
      showError(e);
    }
  }

  async function loadAllTags() {
    try {
      allTags = await invoke('list_all_tags');
    } catch (e) {
      // Non-fatal — sidebar just shows no tags
    }
  }

  async function loadNotes() {
    try {
      if (tagFilter) {
        notes = await invoke('list_notes_by_tag', { tag: tagFilter });
      } else if (selectedFolderId === 'all') {
        notes = await invoke('list_notes', { all: true });
      } else {
        // null means "unfiled", a number means a specific folder
        notes = await invoke('list_notes', { folderId: selectedFolderId ?? null, all: false });
      }
    } catch (e) {
      showError(e);
    }
  }

  onMount(async () => {
    await loadFolders();
    await loadNotes();
    loadAllTags();
  });

  // ── Folder actions ─────────────────────────────────────────────────────────

  async function createFolder() {
    const name = newFolderName.trim();
    if (!name) return;
    try {
      await invoke('create_folder', { name, parentId: null });
      newFolderName = '';
      await loadFolders();
    } catch (e) {
      showError(e);
    }
  }

  async function deleteFolder(id) {
    if (!confirm('Delete this folder? Notes inside will become unfiled.')) return;
    try {
      await invoke('delete_folder', { id });
      if (selectedFolderId === id) selectedFolderId = null;
      await loadFolders();
      await loadNotes();
    } catch (e) {
      showError(e);
    }
  }

  async function selectFolder(id) {
    selectedFolderId = id;
    tagFilter = null;
    activeNote = null;
    await loadNotes();
  }

  // ── Note actions ───────────────────────────────────────────────────────────

  async function createNote() {
    const title = newNoteTitle.trim() || 'Untitled';
    try {
      const note = await invoke('create_note', {
        title,
        folderId: selectedFolderId === 'all' ? null : (selectedFolderId ?? null),
      });
      newNoteTitle = '';
      await loadNotes();
      openNote(note);
      // Index in the background — don't block the UI, but surface failures.
      indexState = 'indexing';
      invoke('index_note', { noteId: note.id, title: note.title, content: '' })
        .then(() => { indexState = 'idle'; })
        .catch(() => { indexState = 'error'; });
    } catch (e) {
      showError(e);
    }
  }

  function openNote(note) {
    activeNote = note;
    editorTitle = note.title;
    editorContent = note.content;
    isDirty = false;
    noteTags = [];
    noteLinks = [];
    noteBacklinks = [];
    invoke('get_note_tags', { noteId: note.id }).then(t => (noteTags = t)).catch(() => {});
    invoke('get_note_links', { noteId: note.id }).then(l => (noteLinks = l)).catch(() => {});
    invoke('get_backlinks', { noteId: note.id }).then(b => (noteBacklinks = b)).catch(() => {});
  }

  function markDirty() {
    isDirty = true;
  }

  async function saveNote() {
    if (!activeNote) return;
    try {
      const updated = await invoke('update_note', {
        id: activeNote.id,
        title: editorTitle,
        content: editorContent,
      });
      activeNote = updated;
      isDirty = false;
      await loadNotes(); // refresh the list so the title updates
      // Index in the background — don't block the UI, but surface failures.
      indexState = 'indexing';
      invoke('index_note', { noteId: updated.id, title: editorTitle, content: editorContent })
        .then(() => { indexState = 'idle'; })
        .catch(() => { indexState = 'error'; });
      // Sync tags and wiki-links, then refresh the displayed relations.
      invoke('sync_note_relations', { noteId: updated.id, content: editorContent })
        .then(() => Promise.all([
          invoke('get_note_tags', { noteId: updated.id }),
          invoke('get_note_links', { noteId: updated.id }),
          invoke('get_backlinks', { noteId: updated.id }),
          invoke('list_all_tags'),
        ]))
        .then(([tags, links, backlinks, updatedAllTags]) => {
          noteTags = tags;
          noteLinks = links;
          noteBacklinks = backlinks;
          allTags = updatedAllTags;
        })
        .catch(() => {});
    } catch (e) {
      showError(e);
    }
  }

  async function deleteNote(id) {
    if (!confirm('Delete this note?')) return;
    try {
      await invoke('delete_note', { id });
      if (activeNote?.id === id) {
        activeNote = null;
        editorTitle = '';
        editorContent = '';
        noteTags = [];
        noteLinks = [];
        noteBacklinks = [];
      }
      await loadNotes();
      invoke('remove_note_index', { noteId: id }).catch(() => {});
    } catch (e) {
      showError(e);
    }
  }

  async function moveNote(noteId, targetFolderId) {
    try {
      await invoke('move_note', { id: noteId, folderId: targetFolderId });
      await loadNotes();
    } catch (e) {
      showError(e);
    }
  }

  // Save on Ctrl+S
  function handleKeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      saveNote();
    }
  }

  async function seedNotes() {
    isSeeding = true;
    try {
      const n = await invoke('seed_notes');
      await loadNotes();
      showError(`✓ Seeded ${n} notes and indexed them.`);
    } catch (e) {
      showError(e);
    } finally {
      isSeeding = false;
    }
  }

  async function reindexAll() {
    isReindexing = true;
    try {
      const n = await invoke('reindex_all');
      showError(`✓ Re-indexed ${n} notes.`);
    } catch (e) {
      showError(e);
    } finally {
      isReindexing = false;
    }
  }

  async function openNoteById(id) {
    try {
      const note = await invoke('get_note', { id });
      openNote(note);
    } catch (e) {
      showError(e);
    }
  }

  async function filterByTag(tag) {
    tagFilter = tag;
    selectedFolderId = null;
    await loadNotes();
  }

  async function clearTagFilter() {
    tagFilter = null;
    await loadNotes();
  }

  // Auto-pair brackets in the editor.
  // `[`  → inserts `[]` and places cursor between them.
  // `[[` → when the previous char is already `[`, replaces it and inserts `[[]]`
  //         with cursor between the pairs.
  function handleEditorKeydown(e) {
    if (e.key !== '[') return;
    const el = /** @type {HTMLTextAreaElement} */ (e.currentTarget);
    const { selectionStart: start, selectionEnd: end, value } = el;
    const prevChar = value[start - 1];

    e.preventDefault();

    if (prevChar === '[') {
      // Replace the already-inserted `[` + new `[` with `[[]]`.
      // If the char after the cursor is `]` (the auto-paired one from the first `[`),
      // consume it too so we don't end up with [[]]]
      const before = value.slice(0, start - 1);
      const after  = value.slice(end + (value[end] === ']' ? 1 : 0));
      const cursor = before.length + 2; // inside [[ | ]]
      editorContent = before + '[[]]' + after;
      markDirty();
      // Svelte's bind:value updates the DOM asynchronously; schedule cursor move.
      requestAnimationFrame(() => {
        el.selectionStart = cursor;
        el.selectionEnd   = cursor;
      });
    } else {
      // Plain `[` → `[]`.
      const before = value.slice(0, start);
      const after  = value.slice(end);
      const cursor = before.length + 1; // inside [ | ]
      editorContent = before + '[]' + after;
      markDirty();
      requestAnimationFrame(() => {
        el.selectionStart = cursor;
        el.selectionEnd   = cursor;
      });
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if errorMsg}
  <div class="error-banner">{errorMsg}</div>
{/if}

<div class="layout" style:grid-template-columns={gridCols}>
  <!-- ── Sidebar: Folders ──────────────────────────────────────────── -->
  <aside class="sidebar" class:collapsed={!foldersOpen}>
    {#if foldersOpen}
      <div class="panel-header">
        <h2>Folders</h2>
        <button class="collapse-btn" onclick={() => (foldersOpen = false)} title="Collapse">‹</button>
      </div>

      <ul class="folder-list">
        <li class:active={selectedFolderId === 'all'}>
          <button class="row-btn" onclick={() => selectFolder('all')}>All Notes</button>
        </li>
        <li class:active={selectedFolderId === null}>
          <button class="row-btn" onclick={() => selectFolder(null)}>Unfiled</button>
        </li>
        {#each folders as folder (folder.id)}
          <li class:active={selectedFolderId === folder.id}>
            <button class="row-btn folder-name" onclick={() => selectFolder(folder.id)}>{folder.name}</button>
            <button class="icon-btn danger" onclick={() => deleteFolder(folder.id)} title="Delete folder">✕</button>
          </li>
        {/each}
      </ul>

      <div class="new-item-row">
        <input
          bind:value={newFolderName}
          placeholder="New folder…"
          onkeydown={(e) => e.key === 'Enter' && createFolder()}
        />
        <button onclick={createFolder}>+</button>
      </div>

      {#if allTags.length > 0}
        <div class="sidebar-section-label tags-header">
          <span>Tags</span>
          <button class="collapse-btn" onclick={() => (tagsOpen = !tagsOpen)} title={tagsOpen ? 'Collapse' : 'Expand'}>
            {tagsOpen ? '˅' : '›'}
          </button>
        </div>
        {#if tagsOpen}
          <div class="tag-search-row">
            <input
              class="tag-search-input"
              bind:value={tagSearch}
              placeholder="Search tags…"
            />
            {#if tagSearch}
              <button class="clear-filter-btn" onclick={() => (tagSearch = '')} title="Clear">✕</button>
            {/if}
          </div>
          <ul class="tag-list">
            {#each visibleTags as tag}
              <li class:active={tagFilter === tag.name}>
                <button class="row-btn" onclick={() => filterByTag(tag.name)}>#{tag.name}</button>
                <span class="tag-count">{tag.count}</span>
              </li>
            {:else}
              <li class="empty">No matches</li>
            {/each}
          </ul>
          {#if !tagSearch && allTags.length > TAG_LIMIT}
            <p class="tag-overflow">{allTags.length - TAG_LIMIT} more — search to find them</p>
          {/if}
        {/if}
      {/if}
    {:else}
      <button class="collapsed-strip" onclick={() => (foldersOpen = true)} title="Expand folders">
        <span>Folders</span>
      </button>
    {/if}
  </aside>

  <!-- ── Note list ─────────────────────────────────────────────────── -->
  <div class="note-list" class:collapsed={!notesOpen}>
    {#if notesOpen}
      <div class="panel-header">
        <h2>
          {#if tagFilter}#{tagFilter}
          {:else if selectedFolderId === 'all'}All Notes
          {:else if selectedFolderId === null}Unfiled
          {:else}{folders.find(f => f.id === selectedFolderId)?.name ?? ''}
          {/if}
        </h2>
        {#if tagFilter}
          <button class="clear-filter-btn" onclick={clearTagFilter} title="Clear tag filter">✕</button>
        {/if}
        <button class="collapse-btn" onclick={() => (notesOpen = false)} title="Collapse">‹</button>
      </div>

      <ul>
        {#each notes as note (note.id)}
          <li class:active={activeNote?.id === note.id}>
            <button class="row-btn note-title" onclick={() => openNote(note)}>{note.title}</button>
            <button class="icon-btn danger" onclick={() => deleteNote(note.id)} title="Delete note">✕</button>
          </li>
        {:else}
          <li class="empty">No notes here</li>
        {/each}
      </ul>

      <div class="new-item-row">
        <input
          bind:value={newNoteTitle}
          placeholder="New note…"
          onkeydown={(e) => e.key === 'Enter' && createNote()}
        />
        <button onclick={createNote}>+</button>
      </div>

      {#if import.meta.env.DEV && notes.length === 0}
        <button class="seed-btn" onclick={seedNotes} disabled={isSeeding}>
          {isSeeding ? 'Seeding…' : 'Seed test notes'}
        </button>
      {/if}
      <button class="seed-btn" onclick={reindexAll} disabled={isReindexing}>
        {isReindexing ? 'Indexing…' : 'Re-index all'}
      </button>
    {:else}
      <button class="collapsed-strip" onclick={() => (notesOpen = true)} title="Expand notes">
        <span>Notes</span>
      </button>
    {/if}
  </div>

  <!-- ── Editor ────────────────────────────────────────────────────── -->
  <main class="editor">
    {#if activeNote}
      <div class="editor-toolbar">
        <input
          class="title-input"
          bind:value={editorTitle}
          oninput={markDirty}
          placeholder="Note title"
        />
        <div class="toolbar-actions">
          <label>
            Move to:
            <select
              onchange={(e) => { const v = /** @type {HTMLSelectElement} */ (e.target).value; moveNote(activeNote.id, v === 'null' ? null : Number(v)); }}
            >
              <option value="null">Unfiled</option>
              {#each folders as f (f.id)}
                <option value={f.id} selected={activeNote.folder_id === f.id}>{f.name}</option>
              {/each}
            </select>
          </label>
          <button onclick={saveNote} disabled={!isDirty} class:index-error={!isDirty && indexState === 'error'}>
            {isDirty ? 'Save (Ctrl+S)' : indexState === 'indexing' ? 'Indexing…' : indexState === 'error' ? '⚠ Index failed' : 'Saved'}
          </button>
          <button class="graph-toggle" onclick={() => (graphOpen = !graphOpen)}>
            {graphOpen ? '✕ Graph' : 'Graph'}
          </button>
          <button class="chat-toggle" onclick={() => (chatOpen = !chatOpen)}>
            {chatOpen ? '✕ Chat' : 'Chat'}
          </button>
        </div>
      </div>
      {#if noteTags.length > 0}
        <div class="note-tags-strip">
          {#each noteTags as tag}
            <button class="tag-pill" onclick={() => filterByTag(tag)}>#{tag}</button>
          {/each}
        </div>
      {/if}
      <textarea
        class="content-area"
        bind:value={editorContent}
        oninput={markDirty}
        onkeydown={handleEditorKeydown}
        placeholder="Write your note…"
      ></textarea>
      {#if noteLinks.length > 0 || noteBacklinks.length > 0}
        <div class="note-footer">
          {#if noteLinks.length > 0}
            <div class="note-footer-section">
              <span class="note-footer-label">Links</span>
              {#each noteLinks as link}
                <button class="link-pill" onclick={() => openNoteById(link.id)}>{link.title}</button>
              {/each}
            </div>
          {/if}
          {#if noteBacklinks.length > 0}
            <div class="note-footer-section">
              <span class="note-footer-label">Backlinks</span>
              {#each noteBacklinks as link}
                <button class="link-pill" onclick={() => openNoteById(link.id)}>{link.title}</button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="editor-toolbar">
        <div class="toolbar-actions">
          <button class="graph-toggle" onclick={() => (graphOpen = !graphOpen)}>
            {graphOpen ? '✕ Graph' : 'Graph'}
          </button>
          <button class="chat-toggle" onclick={() => (chatOpen = !chatOpen)}>
            {chatOpen ? '✕ Chat' : 'Chat'}
          </button>
        </div>
      </div>
      <div class="empty-editor">Select or create a note</div>
    {/if}
  </main>

  {#if chatOpen}
    <Chat />
  {/if}
</div>

{#if graphOpen}
  <div class="graph-overlay">
    <button class="graph-close" onclick={() => (graphOpen = false)}>✕ Close</button>
    <Graph
      activeNoteId={activeNote?.id ?? null}
      onSelectNote={(id) => {
        invoke('get_note', { id })
          .then(note => { openNote(note); graphOpen = false; })
          .catch(e => showError(e));
      }}
    />
  </div>
{/if}

