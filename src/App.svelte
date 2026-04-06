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
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { marked } from 'marked';
  import Calendar from './lib/Calendar.svelte';
  import Chat from './lib/Chat.svelte';
  import Graph from './lib/Graph.svelte';
  import TabBar from './lib/TabBar.svelte';
  import LockScreen from './lib/LockScreen.svelte';
  import PasswordModal from './lib/PasswordModal.svelte';
  import TemplateModal from './lib/TemplateModal.svelte';
  import NoteProperties from './lib/NoteProperties.svelte';
  import DatabaseView from './lib/DatabaseView.svelte';
  import Settings from './lib/Settings.svelte';
  import Search from './lib/Search.svelte';
  import ConfirmModal from './lib/ConfirmModal.svelte';
  import QuickSwitcher from './lib/QuickSwitcher.svelte';

  const appWindow = getCurrentWindow();

  // ── State ──────────────────────────────────────────────────────────────────

  let folders = $state([]);
  let notes = $state([]);
  let selectedFolderId = $state(null); // null = "All notes"
  let activeNote = $state(null);       // the note currently open in the editor

  // ── Tab state ──────────────────────────────────────────────────────────────
  // Each tab: { id: string, type: 'note'|'graph'|'chat', noteId: number|null, label: string, customLabel: string|null }
  let tabs = $state([]);
  let activeTabId = $state(null);
  let activeTab = $derived(tabs.find(t => t.id === activeTabId) ?? null);
  function makeTabId() { return Math.random().toString(36).slice(2, 9); }

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
  let newNoteTitleInputEl = $state(null); // bound to the new-note input for Ctrl+N focus

  // Error display
  let errorMsg = $state('');

  // Chat panel
  let chatOpen = $state(false);

  // When set, Chat.svelte will inject this text as a blockquote into its input.
  // Uses a seq counter so the same text can be injected multiple times.
  let chatInsert = $state(null); // { text: string, seq: number } | null

  // Reference to the note editor textarea — used to read the current selection.
  let editorTextareaEl = $state(null);

  // Calendar overlay
  // (removed — calendar is now a tab type)

  // Search panel
  let searchOpen = $state(false);

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

  // Settings overlay
  let settingsOpen = $state(false);
  let keepModelInMemory = $state(localStorage.getItem('keepModelInMemory') === 'true');
  let accent = $state(localStorage.getItem('accent') ?? 'red');
  let theme  = $state(localStorage.getItem('theme')  ?? 'system');
  let dailyNoteFormat = $state(localStorage.getItem('dailyNoteFormat') ?? 'DD-MM-YYYY');

  $effect(() => {
    localStorage.setItem('keepModelInMemory', String(keepModelInMemory));
  });

  $effect(() => {
    localStorage.setItem('dailyNoteFormat', dailyNoteFormat);
  });

  $effect(() => {
    localStorage.setItem('grimoire_tabs', JSON.stringify({
      tabs: tabs.map(t => ({ id: t.id, type: t.type, noteId: t.noteId, label: t.label, customLabel: t.customLabel })),
      activeTabId,
    }));
  });

  $effect(() => {
    localStorage.setItem('accent', accent);
    localStorage.setItem('theme',  theme);

    const root = document.documentElement;
    // Accent — remove attribute when red so :root values remain the source of truth.
    if (accent === 'red') {
      root.removeAttribute('data-accent');
    } else {
      root.setAttribute('data-accent', accent);
    }
    // Theme — remove attribute when system so the OS media query fires normally.
    if (theme === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', theme);
    }
  });

  // Drag-and-drop
  let dragOverFolderId = $state(null); // folder ID currently being hovered during a note drag
  let isDragging = $state(false);      // true while a note drag is in progress

  // Quick Switcher
  let quickSwitcherOpen = $state(false);

  // Database / table view
  let tableViewOpen = $state(false);
  let dbKey = $state(0); // bumped to force DatabaseView remount after template sync
  let folderHasProperties = $state(false); // true when the selected folder has any property defs
  let noteProperties = $state([]); // properties for the active note (for RAG suffix)
  let propertiesReady = $state(true); // false while NoteProperties is fetching, to prevent layout shift
  // Tags shown in the sidebar: if searching, filter by prefix match;
  // otherwise show the top TAG_LIMIT by note count.
  let visibleTags = $derived(
    tagSearch.trim()
      ? allTags.filter(t => t.name.includes(tagSearch.trim().toLowerCase().replace(/^#/, '')))
      : allTags.slice(0, TAG_LIMIT)
  );

  // Note sort order and sorted list.
  let noteSort = $state('modified');
  let sortedNotes = $derived.by(() => {
    const arr = [...notes];
    if (noteSort === 'name') arr.sort((a, b) => a.title.localeCompare(b.title));
    else if (noteSort === 'created') arr.sort((a, b) => b.created_at - a.created_at);
    else arr.sort((a, b) => b.updated_at - a.updated_at);
    return arr;
  });

  // Word count and estimated reading time for the active note.
  let wordCount = $derived(
    editorContent ? editorContent.trim().split(/\s+/).filter(Boolean).length : 0
  );
  let readingTime = $derived(Math.max(1, Math.round(wordCount / 200)));

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

  // Pending delete confirmations — null when no dialog is open.
  // noteDeletePending: id of note awaiting confirmation
  // folderDeletePending: { id, name } of folder awaiting confirmation
  let noteDeletePending = $state(null);
  let folderDeletePending = $state(null);

  // Set of folder IDs that have been unlocked in the current session.
  // Used to show the "remove password" button for password-protected folders.
  let unlockedFolderIds = $state(new Set());

  // Compute grid column widths reactively from all panel states.
  // $derived re-evaluates automatically whenever any of its dependencies change.
  // Spellbook mode uses narrower left panels and a wider chat for better book proportions.
  let gridCols = $derived.by(() => {
    const sb = theme === 'spellbook';
    return [
      foldersOpen ? (sb ? '170px' : '200px') : '28px',
      notesOpen   ? (sb ? '200px' : '240px') : '28px',
      '1fr',
      ...(chatOpen ? [sb ? '440px' : '360px'] : []),
    ].join(' ');
  });

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
      await restoreTabs();
      if (tabs.length === 0) newTab();
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
    const folder = folders.find(f => f.id === id);
    folderDeletePending = { id, name: folder?.name ?? 'this folder' };
  }

  async function confirmDeleteFolder() {
    const id = folderDeletePending.id;
    folderDeletePending = null;
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
    tableViewOpen = false;
    await loadNotes();
    // Check if this folder has property definitions (for the table view toggle).
    if (id && id !== 'all') {
      invoke('get_property_defs', { folderId: id })
        .then(defs => { folderHasProperties = defs.length > 0; })
        .catch(() => { folderHasProperties = false; });
    } else {
      folderHasProperties = false;
    }
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
      // Apply template property defs to the folder AND seed initial note_properties
      // rows for this note — BEFORE openNote so NoteProperties loads correctly on first mount.
      const fid = selectedFolderId === 'all' ? null : (selectedFolderId ?? null);
      if (fid && selectedTemplateId > 0) {
        try {
          const defs = await invoke('apply_template_to_note', { noteId: note.id, folderId: fid, templateId: selectedTemplateId });
          folderHasProperties = defs.length > 0;
        } catch { /* non-fatal */ }
      }
      navigateToNote(note);
      // Apply template content after navigateToNote so it overrides the empty content.
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
    searchOpen = false;
    activeNote = note;
    editorTitle = note.title;
    editorContent = note.content;
    isDirty = false;
    // If the note is in a folder, hold the textarea until the properties panel
    // has loaded — prevents the content jumping down as properties appear.
    propertiesReady = !note.folder_id;
    noteTags = [];
    noteLinks = [];
    noteBacklinks = [];
    invoke('get_note_tags', { noteId: note.id }).then(t => (noteTags = t)).catch(() => {});
    invoke('get_note_links', { noteId: note.id }).then(l => (noteLinks = l)).catch(() => {});
    invoke('get_backlinks', { noteId: note.id }).then(b => (noteBacklinks = b)).catch(() => {});
  }

  // ── Tab helpers ────────────────────────────────────────────────────────────

  // Toggles edit/read mode for the active tab.
  function toggleReadMode() {
    if (!activeTabId) return;
    tabs = tabs.map(t => t.id === activeTabId ? { ...t, readMode: !t.readMode } : t);
  }

  // Navigates the active tab to a note (updating it in place).
  // If no tab exists yet, creates the first one implicitly.
  // This is called for all normal note navigation (sidebar click, search, links).
  function navigateToNote(note) {
    if (isDirty) saveNote();
    if (!activeTabId || tabs.length === 0) {
      // No tabs at all — create the first one.
      const id = makeTabId();
      tabs = [{ id, type: 'note', noteId: note.id, label: note.title, customLabel: null, readMode: false }];
      activeTabId = id;
    } else {
      // Update the current tab to point to this note.
      tabs = tabs.map(t => t.id === activeTabId
        ? { ...t, type: 'note', noteId: note.id, label: note.title }
        : t
      );
    }
    openNote(note);
  }

  // Creates a blank tab and activates it (Ctrl+T / + button).
  function newTab() {
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'note', noteId: null, label: 'New Tab', customLabel: null, readMode: false }];
    activeTabId = id;
    activeNote = null;
    editorTitle = '';
    editorContent = '';
    isDirty = false;
    noteTags = [];
    noteLinks = [];
    noteBacklinks = [];
  }

  // Activates an existing tab by ID. Auto-saves the current dirty note first.
  async function activateTab(id) {
    if (activeTabId === id) return;
    if (isDirty) await saveNote();
    activeTabId = id;
    const tab = tabs.find(t => t.id === id);
    if (!tab) return;
    if (tab.type === 'note' && tab.noteId != null) {
      try {
        const note = await invoke('get_note', { id: tab.noteId });
        openNote(note);
      } catch (e) {
        showError(e);
        closeTab(id);
      }
    } else {
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      noteTags = [];
      noteLinks = [];
      noteBacklinks = [];
    }
  }

  // Closes a tab. Auto-saves if dirty, then activates the nearest remaining tab.
  async function closeTab(id) {
    const idx = tabs.findIndex(t => t.id === id);
    if (idx === -1) return;
    if (id === activeTabId && isDirty) await saveNote();
    const newTabs = tabs.filter(t => t.id !== id);
    tabs = newTabs;
    if (activeTabId === id) {
      if (newTabs.length === 0) {
        // Always keep at least one tab — open a fresh New Tab.
        newTab();
      } else {
        const next = newTabs[idx] ?? newTabs[idx - 1];
        activeTabId = next.id;
        if (next.type === 'note' && next.noteId != null) {
          try {
            const note = await invoke('get_note', { id: next.noteId });
            openNote(note);
          } catch {
            activeNote = null;
            editorTitle = '';
            editorContent = '';
          }
        } else {
          activeNote = null;
          editorTitle = '';
          editorContent = '';
          isDirty = false;
          noteTags = [];
          noteLinks = [];
          noteBacklinks = [];
        }
      }
    }
  }

  // Opens (or activates) a graph tab in the editor column.
  function openGraphTab() {
    if (isDirty) saveNote();
    const existing = tabs.find(t => t.type === 'graph');
    if (existing) {
      activeTabId = existing.id;
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      return;
    }
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'graph', noteId: null, label: 'Graph', customLabel: null }];
    activeTabId = id;
    activeNote = null;
    editorTitle = '';
    editorContent = '';
    isDirty = false;
  }

  // Opens (or activates) a calendar tab in the editor column.
  function openCalendarTab() {
    if (isDirty) saveNote();
    const existing = tabs.find(t => t.type === 'calendar');
    if (existing) {
      activeTabId = existing.id;
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      return;
    }
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'calendar', noteId: null, label: 'Calendar', customLabel: null }];
    activeTabId = id;
    activeNote = null;
    editorTitle = '';
    editorContent = '';
    isDirty = false;
  }

  // Opens (or activates) a chat tab in the editor column.
  function openChatTab() {
    if (isDirty) saveNote();
    const existing = tabs.find(t => t.type === 'chat');
    if (existing) {
      activeTabId = existing.id;
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      return;
    }
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'chat', noteId: null, label: 'Chat', customLabel: null }];
    activeTabId = id;
    activeNote = null;
    editorTitle = '';
    editorContent = '';
    isDirty = false;
  }

  // Sets a custom display label for a tab (double-click rename in TabBar).
  function renameTab(id, label) {
    tabs = tabs.map(t => t.id === id ? { ...t, customLabel: label || null } : t);
  }

  // Restores tabs from localStorage after the vault is unlocked.
  async function restoreTabs() {
    try {
      const saved = localStorage.getItem('grimoire_tabs');
      if (!saved) return;
      const { tabs: savedTabs, activeTabId: savedActiveId } = JSON.parse(saved);
      if (!Array.isArray(savedTabs) || savedTabs.length === 0) return;
      const restored = [];
      for (const t of savedTabs) {
        if (t.type === 'note' && t.noteId != null) {
          try {
            const note = await invoke('get_note', { id: t.noteId });
            restored.push({ ...t, label: note.title });
          } catch { /* Note was deleted — drop it */ }
        } else if (t.type === 'graph') {
          restored.push(t);
        }
        // Chat tabs are not restored (no persistent content).
      }
      if (restored.length === 0) return;
      tabs = restored;
      const target = restored.find(t => t.id === savedActiveId) ?? restored[restored.length - 1];
      activeTabId = target.id;
      if (target.type === 'note' && target.noteId != null) {
        const note = await invoke('get_note', { id: target.noteId });
        openNote(note);
      }
    } catch { /* Malformed localStorage data — start fresh */ }
  }

  // ── Template actions ───────────────────────────────────────────────────────

  async function saveTemplate(name, title, content, properties) {
    // Throws on failure so TemplateModal can display the error.
    await invoke('create_template', { name, title, content, properties });
    await loadTemplates();
    templateModalOpen = false;
  }

  async function updateTemplate(name, title, content, properties) {
    // Throws on failure so TemplateModal can display the error.
    const savedId = editingTemplate.id;
    await invoke('update_template', { id: savedId, name, title, content, properties });
    await loadTemplates();
    editingTemplate = null;
    // Auto-sync: push the updated specs to all notes/folders tracked to this template.
    try {
      await invoke('sync_template_to_notes', { templateId: savedId });
      // Bump the key so DatabaseView remounts and picks up the new column.
      dbKey += 1;
    } catch { /* non-fatal — sync errors should not block the save */ }
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
    const note = notes.find(n => n.id === id) ?? activeNote;
    noteDeletePending = { id, title: note?.title ?? 'this note' };
  }

  async function confirmDeleteNote() {
    const id = noteDeletePending.id;
    noteDeletePending = null;
    try {
      await invoke('delete_note', { id });
      const tab = tabs.find(t => t.type === 'note' && t.noteId === id);
      if (tab) await closeTab(tab.id);
      else if (activeNote?.id === id) {
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

  function onNoteDragStart(e, note) {
    e.dataTransfer.setData('text/plain', String(note.id));
    e.dataTransfer.effectAllowed = 'move';
    isDragging = true;
  }

  function onNoteDragEnd() {
    isDragging = false;
    dragOverFolderId = null;
  }

  function onFolderDragOver(e, folderId) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    dragOverFolderId = folderId;
  }

  async function onFolderDrop(e, folderId) {
    e.preventDefault();
    dragOverFolderId = null;
    const noteId = Number(e.dataTransfer.getData('text/plain'));
    if (!noteId) return;
    await moveNote(noteId, folderId);
  }

  // Save on Ctrl+S; lock vault on Ctrl+Shift+L; send selection to chat on Ctrl+Shift+Enter
  function handleKeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'p' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      quickSwitcherOpen = true;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'f' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      searchOpen = true;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'n' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      notesOpen = true;
      // Svelte may not have rendered the panel yet; wait a frame before focusing.
      requestAnimationFrame(() => newNoteTitleInputEl?.focus());
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 't' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      newTab();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'w' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      if (activeTabId) closeTab(activeTabId);
    }
    if (e.key === 'Delete' && !e.ctrlKey && !e.metaKey && !e.altKey) {
      const tag = /** @type {HTMLElement} */ (document.activeElement)?.tagName ?? '';
      const isEditing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
        || /** @type {HTMLElement} */ (document.activeElement)?.isContentEditable;
      if (!isEditing && activeNote && !activeNote.locked) {
        e.preventDefault();
        deleteNote(activeNote.id);
      }
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      saveNote();
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'L') {
      e.preventDefault();
      if (vaultHasPassword && !vaultLocked) lockVault();
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'Enter') {
      e.preventDefault();
      let text = '';
      if (editorTextareaEl) {
        const { selectionStart, selectionEnd, value } = editorTextareaEl;
        text = value.slice(selectionStart, selectionEnd).trim();
      }
      // Fall back to note title if nothing is selected
      if (!text && activeNote) text = activeNote.title;
      if (!text) return;
      chatOpen = true;
      chatInsert = { text, seq: (chatInsert?.seq ?? 0) + 1 };
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
      const msg = await invoke('reindex_all');
      showError(`✓ ${msg}.`);
    } catch (e) {
      showError(e);
    } finally {
      isReindexing = false;
    }
  }

  async function openNoteById(id) {
    try {
      const note = await invoke('get_note', { id });
      navigateToNote(note);
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
    await restoreTabs();
    if (tabs.length === 0) newTab();
    // Re-index notes now that the vault is open.
    invoke('reindex_all').catch(() => {});
  }

  async function lockVault() {
    if (!vaultHasPassword) return;
    try {
      await invoke('lock_vault');
      vaultLocked = true;
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      tabs = [];
      activeTabId = null;
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

{#if noteDeletePending}
  <ConfirmModal
    title="Delete note"
    message={'Are you sure you want to delete \u201c' + noteDeletePending.title + '\u201d?'}
    confirmLabel="Delete"
    onConfirm={confirmDeleteNote}
    onCancel={() => (noteDeletePending = null)}
  />
{/if}

{#if folderDeletePending}
  <ConfirmModal
    title="Delete folder"
    message={'Are you sure you want to delete \u201c' + folderDeletePending.name + '\u201d? Notes inside will become unfiled.'}
    confirmLabel="Delete"
    onConfirm={confirmDeleteFolder}
    onCancel={() => (folderDeletePending = null)}
  />
{/if}

<!-- ── Custom title bar ──────────────────────────────────────────── -->
<div class="titlebar">
  <div class="titlebar-left">
    <button class="titlebar-btn" onclick={() => (foldersOpen = !foldersOpen)} title="Toggle folders">
      <svg width="15" height="15" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <rect x="1" y="1" width="13" height="13" rx="1"/>
        <line x1="5" y1="1" x2="5" y2="14"/>
      </svg>
    </button>
    <button class="titlebar-btn" onclick={() => (notesOpen = !notesOpen)} title="Toggle notes list">
      <svg width="15" height="15" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <line x1="4" y1="4" x2="11" y2="4"/>
        <line x1="4" y1="7.5" x2="11" y2="7.5"/>
        <line x1="4" y1="11" x2="9" y2="11"/>
      </svg>
    </button>
    <button class="titlebar-btn" onclick={() => (searchOpen = !searchOpen)} title="Search (Ctrl+F)">
      <svg width="15" height="15" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <circle cx="6.5" cy="6.5" r="4.5"/>
        <line x1="10" y1="10" x2="13.5" y2="13.5"/>
      </svg>
    </button>
  </div>

  <!-- Tab strip — replaces the old "Grimoire" drag region -->
  <TabBar
    {tabs}
    {activeTabId}
    onActivate={activateTab}
    onClose={closeTab}
    onRename={renameTab}
    onNew={newTab}
  />

  <div class="titlebar-right">
    <button
      class="titlebar-btn"
      class:titlebar-btn-active={chatOpen}
      onclick={() => (chatOpen = !chatOpen)}
      title="Toggle chat"
    >
      <svg width="15" height="15" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 2h11a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H5l-3 3V3a1 1 0 0 1 1-1z"/>
      </svg>
    </button>
  </div>

  <div class="titlebar-winctl">
    <button class="winctl-btn" onclick={() => appWindow.minimize()} title="Minimise" aria-label="Minimise">
      <svg width="11" height="11" viewBox="0 0 11 11" fill="currentColor"><rect x="0" y="5" width="11" height="1"/></svg>
    </button>
    <button class="winctl-btn" onclick={() => appWindow.toggleMaximize()} title="Maximise" aria-label="Maximise">
      <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" stroke-width="1"><rect x="0.5" y="0.5" width="10" height="10"/></svg>
    </button>
    <button class="winctl-btn close" onclick={() => appWindow.close()} title="Close" aria-label="Close">
      <svg width="11" height="11" viewBox="0 0 11 11" stroke="currentColor" stroke-width="1.2" stroke-linecap="round">
        <line x1="1" y1="1" x2="10" y2="10"/><line x1="10" y1="1" x2="1" y2="10"/>
      </svg>
    </button>
  </div>
</div>

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
          <li
            class:active={selectedFolderId === folder.id}
            class:locked-row={folder.locked}
            class:drag-over={dragOverFolderId === folder.id}
            class:drag-active={isDragging && !folder.locked}
            ondragover={(e) => !folder.locked && onFolderDragOver(e, folder.id)}
            ondragleave={(e) => { if (dragOverFolderId === folder.id && !e.currentTarget.contains(/** @type {Node} */ (e.relatedTarget))) dragOverFolderId = null; }}
            ondrop={(e) => !folder.locked && onFolderDrop(e, folder.id)}
          >
            {#if folder.locked}
              <button class="row-btn folder-name" onclick={() => requestFolderUnlock(folder)}>
                <span class="lock-icon">🔒</span>{folder.name === '<locked>' ? '(locked folder)' : folder.name}
              </button>
            {:else}
              {@const folderCount = notes.filter(n => n.folder_id === folder.id).length}
              <button class="row-btn folder-name" onclick={() => selectFolder(folder.id)}>{folder.name}</button>
              {#if folderCount > 0}<span class="folder-count">{folderCount}</span>{/if}
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
        <select class="sort-select" bind:value={noteSort} title="Sort notes">
          <option value="modified">Modified</option>
          <option value="created">Created</option>
          <option value="name">Name</option>
        </select>
        <button class="collapse-btn" onclick={() => (notesOpen = false)} title="Collapse">‹</button>
      </div>

      <ul>
        {#each sortedNotes as note (note.id)}
          <li
            class:active={activeNote?.id === note.id}
            class:locked-row={note.locked}
          >
            {#if note.locked}
              <span class="row-btn note-title note-locked"><span class="lock-icon">🔒</span>(locked)</span>
            {:else}
              <span
                class="drag-handle"
                draggable="true"
                ondragstart={(e) => onNoteDragStart(e, note)}
                ondragend={onNoteDragEnd}
                title="Drag to move"
                aria-hidden="true"
              >⠇</span>
              <button class="row-btn note-title" onclick={() => navigateToNote(note)}>{note.title}</button>
              <button class="icon-btn danger" onclick={() => deleteNote(note.id)} title="Delete note">✕</button>
            {/if}
          </li>
        {:else}
          <li class="empty">No notes here</li>
        {/each}
      </ul>

      <div class="new-item-row">
        <input
          bind:this={newNoteTitleInputEl}
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
    <!-- Search is always mounted so query/results survive panel close. Hidden via CSS when inactive. -->
    <div style="display: {searchOpen ? 'contents' : 'none'};">
      <Search
        {folders}
        open={searchOpen}
        onSelectNote={(id) => {
          searchOpen = false;
          openNoteById(id);
        }}
      />
    </div>
    {#if !searchOpen}
      {#if activeTab?.type === 'graph'}
        <div class="tab-fullview">
          <button class="tab-fullview-close" onclick={() => closeTab(activeTabId)} title="Close graph">✕ Close</button>
          <Graph
            activeNoteId={activeNote?.id ?? null}
            {theme}
            onSelectNote={(id) => openNoteById(id)}
          />
        </div>
      {:else if activeTab?.type === 'calendar'}
        <div class="tab-fullview">
          <button class="tab-fullview-close" onclick={() => closeTab(activeTabId)} title="Close calendar">✕ Close</button>
          <Calendar
            onSelectNote={(note) => navigateToNote(note)}
            onRefresh={() => { loadFolders(); loadNotes(); }}
            onSelectFolder={(id) => selectFolder(id)}
            dateFormat={dailyNoteFormat}
          />
        </div>
      {:else if activeTab?.type === 'chat'}
        <Chat activeNote={null} pendingInsert={null} keepInMemory={keepModelInMemory} onClose={() => closeTab(activeTabId)} />
      {:else if activeNote}
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
          {#if folderHasProperties}
            <button class="graph-toggle" onclick={() => { if (isDirty) saveNote(); activeNote = null; tableViewOpen = true; }}>
              ← Table
            </button>
          {/if}
          <button class="graph-toggle" onclick={openGraphTab}>Graph</button>
          <button class="graph-toggle" onclick={openCalendarTab}>Calendar</button>
          <button class="graph-toggle" onclick={toggleReadMode}>{activeTab?.readMode ? 'Edit' : 'Read'}</button>
          <span class="word-count">{wordCount} word{wordCount === 1 ? '' : 's'} · {readingTime} min</span>
        </div>
      </div>
      {#if noteTags.length > 0}
        <div class="note-tags-strip">
          {#each noteTags as tag}
            <button class="tag-pill" onclick={() => filterByTag(tag)}>#{tag}</button>
          {/each}
        </div>
      {/if}
      {#if activeNote.folder_id}
        {#key activeNote.id}
          <NoteProperties
            noteId={activeNote.id}
            folderId={activeNote.folder_id}
            onPropertiesLoad={(p) => { noteProperties = p; folderHasProperties = p.length > 0 || folderHasProperties; propertiesReady = true; }}
          />
        {/key}
      {/if}
      {#if propertiesReady}
        {#if activeTab?.readMode}
          <div class="content-area read-mode-content">{@html marked.parse(editorContent || '')}</div>
        {:else}
          <textarea
            class="content-area"
            bind:this={editorTextareaEl}
            bind:value={editorContent}
            oninput={markDirty}
            onkeydown={handleEditorKeydown}
            placeholder="Write your note…"
          ></textarea>
        {/if}
      {/if}
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
          {#if folderHasProperties && selectedFolderId && selectedFolderId !== 'all'}
            <button class="graph-toggle" onclick={() => (tableViewOpen = !tableViewOpen)}>
              {tableViewOpen ? '✕ Table' : 'Table'}
            </button>
          {/if}
          <button class="graph-toggle" onclick={openGraphTab}>Graph</button>
          <button class="graph-toggle" onclick={openCalendarTab}>Calendar</button>
        </div>
      </div>
      {#if tableViewOpen && selectedFolderId && selectedFolderId !== 'all'}        {#key dbKey}
        <DatabaseView
          folderId={selectedFolderId}
          onOpenNote={(id) => openNoteById(id)}
        />
        {/key}
      {:else}
        <div class="empty-editor">Select or create a note</div>
      {/if}
    {/if}
    {/if}
  </main>

  {#if chatOpen && activeTab?.type !== 'chat'}
    <Chat {activeNote} pendingInsert={chatInsert} keepInMemory={keepModelInMemory} onClose={() => (chatOpen = false)} />
  {/if}
</div>

{#if quickSwitcherOpen}
  <QuickSwitcher
    onSelect={(note) => navigateToNote(note)}
    onClose={() => (quickSwitcherOpen = false)}
  />
{/if}

{#if settingsOpen}
  <Settings
    onClose={() => (settingsOpen = false)}
    vaultHasPassword={vaultHasPassword}
    onSetVaultPassword={() => (vaultPwModal = 'set')}
    onChangeVaultPassword={() => (vaultPwModal = 'change')}
    onRemoveVaultPassword={() => (vaultPwModal = 'remove')}
    onLockVault={lockVault}
    keepInMemory={keepModelInMemory}
    onKeepInMemoryChange={(v) => (keepModelInMemory = v)}
    {accent}
    onAccentChange={(v) => (accent = v)}
    {theme}
    onThemeChange={(v) => (theme = v)}
    dateFormat={dailyNoteFormat}
    onDateFormatChange={(v) => (dailyNoteFormat = v)}
  />
{/if}

<!-- Gear button — always visible in the bottom-left corner -->
<div class="bottom-left-btns">
  {#if vaultHasPassword}
    <button class="lock-quick-btn" onclick={lockVault} title="Lock vault (Ctrl+Shift+L)">🔒</button>
  {/if}
  <button class="gear-btn" onclick={() => (settingsOpen = true)} title="Settings">⚙</button>
</div>

{/if} <!-- end of vault-unlocked block -->
