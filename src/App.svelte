<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Chat from './lib/Chat.svelte';
  import Graph from './lib/Graph.svelte';
  import LockScreen from './lib/LockScreen.svelte';
  import PasswordModal from './lib/PasswordModal.svelte';
  import TemplateModal from './lib/TemplateModal.svelte';

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
  const TAG_LIMIT = 3;  let tagsOpen = $state(false);

  // Templates
  let templates = $state([]);
  let selectedTemplateId = $state(-1); // -1 = Blank (built-in default)
  let templatesOpen = $state(false);
  let templateModalOpen = $state(false);
  let editingTemplate = $state(null); // set to a template object to open the edit modal
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

  // ── Password / lock state ──────────────────────────────────────────────────

  // true while we're waiting for the vault-lock check on startup
  let lockCheckDone = $state(false);
  // true when the vault has a password and it hasn't been entered this session
  let vaultLocked = $state(false);
  // true when a vault password exists (independent of locked/unlocked state)
  let vaultHasPassword = $state(false);

  // Modal state for folder unlock
  let folderUnlockTarget = $state(null); // { id, name } of folder waiting for password

  // Modal state for vault password management
  // mode: 'set' | 'change' | 'remove' | null
  let vaultPwModal = $state(null);

  // Modal state for folder password management
  // mode: 'set' | 'remove' | null, folderId
  let folderPwModal = $state(null);

  // Set of folder IDs that have been unlocked in the current session.
  // Used to show the "remove password" button for password-protected folders.
  let unlockedFolderIds = $state(new Set());

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

  async function loadTemplates() {
    try {
      templates = await invoke('list_templates');
    } catch (e) {
      // Non-fatal — picker just shows nothing
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
    // Check vault lock state before loading any content.
    try {
      const [locked, hasPw] = await Promise.all([
        invoke('is_vault_locked'),
        invoke('vault_has_password'),
      ]);
      vaultLocked = locked;
      vaultHasPassword = hasPw;
    } catch (e) {
      // If the check itself fails, treat as unlocked (e.g. DB not yet migrated — safe to show).
    }
    lockCheckDone = true;

    if (!vaultLocked) {
      await loadFolders();
      await loadNotes();
      loadAllTags();
      loadTemplates();
    }
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
    // Auto-save unsaved changes before leaving the current note.
    if (isDirty) await saveNote();
    selectedFolderId = id;
    tagFilter = null;
    activeNote = null;
    await loadNotes();
  }

  // ── Note actions ───────────────────────────────────────────────────────────

  async function createNote() {
    const template = templates.find(t => t.id === selectedTemplateId);
    const title = newNoteTitle.trim() || template?.title || 'Untitled';
    try {
      const note = await invoke('create_note', {
        title,
        folderId: selectedFolderId === 'all' ? null : (selectedFolderId ?? null),
      });
      newNoteTitle = '';
      await loadNotes();
      openNote(note);
      // Apply template content after openNote so it overrides the empty content.
      const templateContent = template?.content ?? '';
      if (templateContent) {
        editorContent = templateContent;
        isDirty = true;
        // Persist immediately so a force-quit doesn't lose the template content.
        invoke('update_note', { id: note.id, title, content: templateContent }).catch(() => {});
      }
      // Index in the background — don't block the UI, but surface failures.
      indexState = 'indexing';
      invoke('index_note', { noteId: note.id, title: note.title, content: templateContent })
        .then(() => { indexState = 'idle'; })
        .catch(() => { indexState = 'error'; });
    } catch (e) {
      showError(e);
    }
  }

  function openNote(note) {
    // Auto-save unsaved changes before switching to a different note.
    if (isDirty && activeNote && activeNote.id !== note.id) saveNote();
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

  // ── Template actions ───────────────────────────────────────────────────────

  async function saveTemplate(name, title, content) {
    // Throws on failure so TemplateModal can display the error.
    await invoke('create_template', { name, title, content });
    await loadTemplates();
    templateModalOpen = false;
  }

  async function updateTemplate(name, title, content) {
    // Throws on failure so TemplateModal can display the error.
    await invoke('update_template', { id: editingTemplate.id, name, title, content });
    await loadTemplates();
    editingTemplate = null;
  }

  async function deleteTemplate(id) {
    try {
      await invoke('delete_template', { id });
      await loadTemplates();
      // Reset picker to Blank if the deleted template was selected.
      if (selectedTemplateId === id) selectedTemplateId = -1;
    } catch (e) {
      showError(e);
    }
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

  // ── Lock / unlock functions ────────────────────────────────────────────────

  async function onVaultUnlocked() {
    vaultLocked = false;
    await loadFolders();
    await loadNotes();
    loadAllTags();
    // Re-index notes now that the vault is open.
    invoke('reindex_all').catch(() => {});
  }

  async function lockVault() {
    if (!vaultHasPassword) return;
    try {
      await invoke('lock_vault');
      vaultLocked = true;
      activeNote = null;
      notes = [];
      folders = [];
      allTags = [];
    } catch (e) {
      showError(e);
    }
  }

  // Called when vault password modal submits.
  async function handleVaultPwSubmit(password) {
    if (vaultPwModal === 'set' || vaultPwModal === 'change') {
      await invoke('set_vault_password', { password });
      vaultHasPassword = true;
      vaultPwModal = null;
      // Re-index now that the vault key is in memory and LanceDB was just purged.
      invoke('reindex_all').catch(() => {});
    } else if (vaultPwModal === 'remove') {
      await invoke('remove_vault_password', { password });
      vaultHasPassword = false;
      vaultPwModal = null;
    }
    // Return true to dismiss (onSubmit expects true = success).
    return true;
  }

  // Called when a locked folder is clicked — show the unlock modal.
  function requestFolderUnlock(folder) {
    folderUnlockTarget = folder;
  }

  async function handleFolderUnlock(password) {
    const ok = await invoke('unlock_folder', { folderId: folderUnlockTarget.id, password });
    if (ok) {
      folderUnlockTarget = null;
      await loadFolders();
      await loadNotes();
      // Index this folder's notes in the background.
      const folderNotes = await invoke('list_notes', { folderId: folderUnlockTarget?.id ?? null });
      for (const n of folderNotes) {
        invoke('index_note', { noteId: n.id, title: n.title, content: n.content }).catch(() => {});
      }
    }
    return ok;
  }

  // Safe version — folderUnlockTarget is cleared before re-indexing above, so re-read notes.
  async function handleFolderUnlockSafe(password) {
    if (!folderUnlockTarget) return false;
    const targetId = folderUnlockTarget.id;
    const ok = await invoke('unlock_folder', { folderId: targetId, password });
    if (ok) {
      folderUnlockTarget = null;
      unlockedFolderIds = new Set([...unlockedFolderIds, targetId]);
      await loadFolders();
      await loadNotes();
      // Re-index notes in this folder.
      invoke('list_notes', { folderId: targetId })
        .then(ns => {
          for (const n of ns) {
            invoke('index_note', { noteId: n.id, title: n.title, content: n.content }).catch(() => {});
          }
        })
        .catch(() => {});
    }
    return ok;
  }

  async function handleFolderPwSubmit(password) {
    if (!folderPwModal) return true;
    if (folderPwModal.mode === 'set') {
      await invoke('set_folder_password', { folderId: folderPwModal.folderId, password });
      // Clear the active note if it belongs to the folder just locked.
      if (activeNote?.folder_id === folderPwModal.folderId) {
        activeNote = null;
        editorTitle = '';
        editorContent = '';
      }
      // Folder is now locked — remove from unlocked session set.
      const next = new Set(unlockedFolderIds);
      next.delete(folderPwModal.folderId);
      unlockedFolderIds = next;
    } else if (folderPwModal.mode === 'remove') {
      await invoke('remove_folder_password', { folderId: folderPwModal.folderId, password });
      // Password gone — remove from unlocked session set.
      const next = new Set(unlockedFolderIds);
      next.delete(folderPwModal.folderId);
      unlockedFolderIds = next;
      // Re-index after removing folder password.
      invoke('list_notes', { folderId: folderPwModal.folderId })
        .then(ns => {
          for (const n of ns) {
            invoke('index_note', { noteId: n.id, title: n.title, content: n.content }).catch(() => {});
          }
        })
        .catch(() => {});
    }
    folderPwModal = null;
    await loadFolders();
    await loadNotes();
    return true;
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

{#if !lockCheckDone}
  <!-- Blank while we check vault lock state to avoid a flash of content -->
{:else if vaultLocked}
  <LockScreen onUnlocked={onVaultUnlocked} />
{:else}

{#if errorMsg}
  <div class="error-banner">{errorMsg}</div>
{/if}

<!-- Password modals (rendered above everything) -->
{#if folderUnlockTarget}
  <PasswordModal
    title="Locked folder"
    confirmLabel="Unlock"
    onSubmit={handleFolderUnlockSafe}
    onCancel={() => (folderUnlockTarget = null)}
  />
{/if}

{#if vaultPwModal === 'set'}
  <PasswordModal
    title="Set vault password"
    confirmLabel="Set password"
    warning="If you forget this password, your notes cannot be recovered. There is no reset option."
    requireAck={true}
    onSubmit={handleVaultPwSubmit}
    onCancel={() => (vaultPwModal = null)}
  />
{:else if vaultPwModal === 'change'}
  <PasswordModal
    title="Change vault password"
    confirmLabel="Set new password"
    warning="If you forget this password, your notes cannot be recovered. There is no reset option."
    requireAck={true}
    onSubmit={handleVaultPwSubmit}
    onCancel={() => (vaultPwModal = null)}
  />
{:else if vaultPwModal === 'remove'}
  <PasswordModal
    title="Remove vault password"
    confirmLabel="Remove password"
    onSubmit={handleVaultPwSubmit}
    onCancel={() => (vaultPwModal = null)}
  />
{/if}

{#if folderPwModal?.mode === 'set'}
  <PasswordModal
    title="Set folder password"
    confirmLabel="Set password"
    warning="If you forget this password, notes in this folder cannot be recovered."
    requireAck={true}
    onSubmit={handleFolderPwSubmit}
    onCancel={() => (folderPwModal = null)}
  />
{:else if folderPwModal?.mode === 'remove'}
  <PasswordModal
    title="Remove folder password"
    confirmLabel="Remove password"
    onSubmit={handleFolderPwSubmit}
    onCancel={() => (folderPwModal = null)}
  />
{/if}

{#if templateModalOpen}
  <TemplateModal onSave={saveTemplate} onCancel={() => (templateModalOpen = false)} />
{:else if editingTemplate}
  <TemplateModal template={editingTemplate} onSave={updateTemplate} onCancel={() => (editingTemplate = null)} />
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
          <li class:active={selectedFolderId === folder.id} class:locked-row={folder.locked}>
            {#if folder.locked}
              <button class="row-btn folder-name" onclick={() => requestFolderUnlock(folder)}>
                <span class="lock-icon">🔒</span>{folder.name === '<locked>' ? '(locked folder)' : folder.name}
              </button>
            {:else}
              <button class="row-btn folder-name" onclick={() => selectFolder(folder.id)}>{folder.name}</button>
              {#if unlockedFolderIds.has(folder.id)}
                <button class="icon-btn" title="Remove folder password"
                  onclick={() => (folderPwModal = { mode: 'remove', folderId: folder.id })}>🔓</button>
              {:else}
                <button class="icon-btn" title="Set folder password"
                  onclick={() => (folderPwModal = { mode: 'set', folderId: folder.id })}>🔑</button>
              {/if}
            {/if}
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

      <!-- Vault password controls -->
      <div class="vault-lock-section">
        {#if !vaultHasPassword}
          <button class="vault-btn" onclick={() => (vaultPwModal = 'set')} title="Password-protect the entire vault">
            Set vault password
          </button>
        {:else}
          <button class="vault-btn" onclick={() => (vaultPwModal = 'change')} title="Change vault password">
            Change vault password
          </button>
          <button class="vault-btn" onclick={() => (vaultPwModal = 'remove')} title="Remove vault password">
            Remove vault password
          </button>
          <button class="vault-btn danger-text" onclick={lockVault} title="Lock the vault now">
            Lock vault
          </button>
        {/if}
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

      <!-- Templates section -->
      <div class="sidebar-section-label">
        <span>Templates</span>
        <button class="collapse-btn" onclick={() => (templatesOpen = !templatesOpen)} title={templatesOpen ? 'Collapse' : 'Expand'}>
          {templatesOpen ? '˅' : '›'}
        </button>
      </div>
      {#if templatesOpen}
        <ul class="template-list">
          {#each templates as t (t.id)}
            <li>
              <span class="template-name">{t.name}</span>
              {#if !t.builtin}
                <button class="icon-btn" onclick={() => (editingTemplate = t)} title="Edit template">✎</button>
                <button class="icon-btn danger" onclick={() => deleteTemplate(t.id)} title="Delete template">✕</button>
              {/if}
            </li>
          {/each}
        </ul>
        <button class="vault-btn" onclick={() => (templateModalOpen = true)}>+ New template</button>
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
          <li class:active={activeNote?.id === note.id} class:locked-row={note.locked}>
            {#if note.locked}
              <span class="row-btn note-title note-locked"><span class="lock-icon">🔒</span>(locked)</span>
            {:else}
              <button class="row-btn note-title" onclick={() => openNote(note)}>{note.title}</button>
              <button class="icon-btn danger" onclick={() => deleteNote(note.id)} title="Delete note">✕</button>
            {/if}
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
        {#if templates.length > 1}
          <select
            class="template-select"
            bind:value={selectedTemplateId}
            title="Template for new note"
          >
            {#each templates as t (t.id)}
              <option value={t.id}>{t.name}</option>
            {/each}
          </select>
        {/if}
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

{/if} <!-- end of vault-unlocked block -->
