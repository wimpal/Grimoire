<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Chat from './lib/Chat.svelte';

  // ── State ──────────────────────────────────────────────────────────────────

  let folders = $state([]);
  let notes = $state([]);
  let selectedFolderId = $state(null); // null = "All notes"
  let activeNote = $state(null);       // the note currently open in the editor

  // Editor fields (kept in sync with activeNote)
  let editorTitle = $state('');
  let editorContent = $state('');
  let isDirty = $state(false);

  // Inline-creation inputs
  let newFolderName = $state('');
  let newNoteTitle = $state('');

  // Error display
  let errorMsg = $state('');

  // Chat panel
  let chatOpen = $state(false);

  // Seed state
  let isSeeding = $state(false);

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

  async function loadNotes() {
    try {
      if (selectedFolderId === 'all') {
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
      // Index in the background — don't block the UI.
      invoke('index_note', { noteId: note.id, title: note.title, content: '' }).catch(() => {});
    } catch (e) {
      showError(e);
    }
  }

  function openNote(note) {
    activeNote = note;
    editorTitle = note.title;
    editorContent = note.content;
    isDirty = false;
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
      invoke('index_note', { noteId: updated.id, title: editorTitle, content: editorContent }).catch(() => {});
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
          {#if selectedFolderId === 'all'}All Notes
          {:else if selectedFolderId === null}Unfiled
          {:else}{folders.find(f => f.id === selectedFolderId)?.name ?? ''}
          {/if}
        </h2>
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

      {#if notes.length === 0}
        <button class="seed-btn" onclick={seedNotes} disabled={isSeeding}>
          {isSeeding ? 'Seeding…' : 'Seed test notes'}
        </button>
      {/if}
    {:else}
      <button class="collapsed-strip" onclick={() => (notesOpen = true)} title="Expand notes">
        <span>Notes</span>
      </button>
    {/if}
  </div>

  <!-- ── Editor ────────────────────────────────────────────────────── -->
  <main class="editor">
    <button class="chat-toggle" onclick={() => (chatOpen = !chatOpen)}>
      {chatOpen ? '✕ Chat' : 'Chat'}
    </button>
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
          <button onclick={saveNote} disabled={!isDirty}>
            {isDirty ? 'Save (Ctrl+S)' : 'Saved'}
          </button>
        </div>
      </div>
      <textarea
        class="content-area"
        bind:value={editorContent}
        oninput={markDirty}
        placeholder="Write your note…"
      ></textarea>
    {:else}
      <div class="empty-editor">Select or create a note</div>
    {/if}
  </main>

  {#if chatOpen}
    <Chat />
  {/if}
</div>

