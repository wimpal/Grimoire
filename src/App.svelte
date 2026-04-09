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
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { onMount, tick } from 'svelte';
  import { marked } from 'marked';
  import ActivityBar from './lib/ActivityBar.svelte';
  import Calendar from './lib/Calendar.svelte';
  import Chat from './lib/Chat.svelte';
  import Graph from './lib/Graph.svelte';
  import Kanban from './lib/Kanban.svelte';
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
  import ContextMenu from './lib/ContextMenu.svelte';

  const appWindow = getCurrentWindow();

  // ── State ──────────────────────────────────────────────────────────────────

  let folders = $state([]);
  let notes = $state([]);
  let bookmarks = $state([]); // BookmarkEntry[]
  const bookmarkedNoteIds = $derived(new Set(bookmarks.map(b => b.note_id)));
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

  // Inline-creation inputs — kept for Ctrl+N compatibility; see startNoteInline()
  let newNoteTitleInputEl = $state(null); // kept for potential focus fallback

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
  let bookmarksOpen = $state(true);

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

  // Svelte's <svelte:window ondragover> is unreliable in Tauri/WebView2 — use raw DOM API instead.
  // Without this, the browser shows a "not allowed" cursor everywhere outside explicit drop targets.
  $effect(() => {
    const allow = (e) => e.preventDefault();
    document.addEventListener('dragover', allow);
    return () => document.removeEventListener('dragover', allow);
  });

  // ── Context menu ───────────────────────────────────────────────────────────

  let ctxMenu = $state(null); // { x, y, items } — null when closed

  // In dev builds, a toggle in Settings lets you revert to the native WebView2
  // context menu (useful for Inspect Element).
  let devNativeContextMenu = $state(
    import.meta.env.DEV ? (localStorage.getItem('devNativeContextMenu') === 'true') : false
  );

  $effect(() => {
    if (import.meta.env.DEV) {
      localStorage.setItem('devNativeContextMenu', String(devNativeContextMenu));
    }

    // If the user switched to native, make sure any open custom menu is closed.
    if (devNativeContextMenu) ctxMenu = null;

    const handler = (e) => {
      if (devNativeContextMenu) return; // let native menu show
      e.preventDefault();
      const items = buildCtxItems(e);
      if (items.length === 0) return;
      const x = Math.min(e.clientX, window.innerWidth  - 174);
      const y = Math.min(e.clientY, window.innerHeight - items.length * 28 - 16);
      ctxMenu = { x, y, items };
    };

    document.addEventListener('contextmenu', handler);
    return () => document.removeEventListener('contextmenu', handler);
  });

  function buildCtxItems(e) {
    const tabEl      = e.target.closest('[data-tab-id]');
    const noteLiEl   = e.target.closest('[data-note-id]');
    const folderLiEl = e.target.closest('[data-folder-id]');
    const createNoteBtn = e.target.closest('[data-action="create-note-btn"]');
    const isEditor   = !!e.target.closest('.content-area');

    let items = /** @type {any[]} */ ([]);

    if (createNoteBtn) {
      // Right-click on the "new note" header button shows template choices.
      items = templates.map(t => ({
        label: t.name,
        action: () => startNoteInline(t.id),
      }));
    } else if (tabEl) {
      const tabId = tabEl.dataset.tabId;
      items = [
        { label: 'Close',        action: () => closeTab(tabId) },
        { label: 'Close Others', action: () => closeOtherTabs(tabId) },
        { label: 'Rename',       action: () => startTabRenameExternal(tabId) },
      ];
    } else if (noteLiEl && !noteLiEl.classList.contains('locked-row')) {
      const noteId = Number(noteLiEl.dataset.noteId);
      const note = notes.find(n => n.id === noteId);
      items = [
        { label: 'Open in New Tab', action: () => note && openNoteInNewTab(note) },
        { label: 'Duplicate',       action: async () => {
            await invoke('duplicate_note', { id: noteId });
            await loadNotes();
          }
        },
        { divider: true },
        bookmarkedNoteIds.has(noteId)
          ? { label: 'Remove from Bookmarks', action: () => removeBookmark(noteId) }
          : { label: 'Add to Bookmarks',      action: () => addBookmark(noteId) },
        { divider: true },
        { label: 'Delete', action: () => deleteNote(noteId), danger: true },
      ];
    } else if (folderLiEl) {
      const raw = folderLiEl.dataset.folderId;
      if (raw && raw !== 'all' && raw !== 'unfiled') {
        const folderId = Number(raw);
        const folder = folders.find(f => f.id === folderId);
        if (folder && !folder.locked) {
          items = [
            { label: 'Open as Table',  action: async () => { await selectFolder(folderId); tableViewOpen = true; } },
            { label: 'Open as Kanban', action: () => openKanbanTab(folderId, folder.name) },
            { divider: true },
            unlockedFolderIds.has(folderId)
              ? { label: 'Remove password', action: () => (folderPwModal = { mode: 'remove', folderId }) }
              : { label: 'Set password',    action: () => (folderPwModal = { mode: 'set',    folderId }) },
            { label: 'Delete', action: () => deleteFolder(folderId), danger: true },
          ];
        }
      }
    } else if (isEditor) {
      // Capture selection NOW, at contextmenu time, before the textarea loses focus.
      const el = editorTextareaEl;
      const start = el?.selectionStart ?? 0;
      const end   = el?.selectionEnd   ?? 0;
      const val   = el?.value ?? '';
      const selText = val.slice(start, end);
      const hasSel  = selText.length > 0;

      /** @type {any[]} */
      const formatSubmenu = hasSel ? [
        { label: 'Bold',          action: () => applyInlineFormat(start, end, val, '**', '**') },
        { label: 'Italic',        action: () => applyInlineFormat(start, end, val, '*',  '*')  },
        { label: 'Strikethrough', action: () => applyInlineFormat(start, end, val, '~~', '~~') },
        { label: 'Inline Code',   action: () => applyInlineFormat(start, end, val, '`',  '`')  },
        { divider: true },
        { label: 'Heading 1',     action: () => applyLinePrefix(start, end, val, '# ')   },
        { label: 'Heading 2',     action: () => applyLinePrefix(start, end, val, '## ')  },
        { label: 'Heading 3',     action: () => applyLinePrefix(start, end, val, '### ') },
        { divider: true },
        { label: 'Code Block',    action: () => applyInlineFormat(start, end, val, '```\n', '\n```') },
      ] : [];

      items = [
        ...(hasSel ? [{ label: 'Format', submenu: formatSubmenu }, { divider: true }] : []),
        {
          label: 'Cut',
          disabled: !hasSel,
          action: () => {
            navigator.clipboard.writeText(selText);
            editorContent = val.slice(0, start) + val.slice(end);
            markDirty();
          },
        },
        {
          label: 'Copy',
          disabled: !hasSel,
          action: () => navigator.clipboard.writeText(selText),
        },
        {
          label: 'Paste',
          action: async () => {
            const text = await navigator.clipboard.readText();
            editorContent = val.slice(0, start) + text + val.slice(end);
            markDirty();
          },
        },
        ...(hasSel ? [{ divider: true }, { label: 'Send to Chat', action: () => sendSelectionToChat() }] : []),
      ];
    }

    return items;
  }

  // Wraps the captured selection with a prefix and suffix (e.g. "**" for bold).
  // Trailing whitespace is trimmed from the selection before wrapping — double-clicking
  // a word in browsers selects the trailing space, which would break markdown syntax.
  function applyInlineFormat(start, end, val, prefix, suffix) {
    const sel     = val.slice(start, end);
    const trimmed = sel.trimEnd();
    editorContent = val.slice(0, start) + prefix + trimmed + suffix + val.slice(end);
    markDirty();
  }

  // Prepends a Markdown heading prefix to the selected text, ensuring it sits
  // on its own line. If the selection is mid-line, newlines are inserted around
  // it so only the selected content becomes the heading — not the rest of the line.
  // Strips any existing heading prefix first so toggling works correctly.
  function applyLinePrefix(start, end, val, prefix) {
    const sel        = val.slice(start, end).trimEnd();
    const needBefore = start > 0 && val[start - 1] !== '\n';
    const needAfter  = (start + sel.length) < val.length && val[start + sel.length] !== '\n';
    const prefixed   = sel.split('\n').map(line => prefix + line.replace(/^#{1,6}\s*/, '')).join('\n');
    editorContent    = val.slice(0, start)
      + (needBefore ? '\n' : '')
      + prefixed
      + (needAfter ? '\n' : '')
      + val.slice(end);
    markDirty();
  }

  async function closeOtherTabs(keepId) {
    if (isDirty) await saveNote();
    tabs = tabs.filter(t => t.id === keepId);
    if (activeTabId !== keepId) {
      activeTabId = keepId;
      const tab = tabs[0];
      if (tab?.type === 'note' && tab.noteId != null) {
        try {
          const note = await invoke('get_note', { id: tab.noteId });
          openNote(note);
        } catch { activeNote = null; editorContent = ''; editorTitle = ''; }
      } else {
        activeNote = null; editorContent = ''; editorTitle = ''; isDirty = false;
      }
    }
  }

  // Triggers inline rename in TabBar for a tab by id.
  // We signal TabBar via a reactive prop; it watches with $effect and calls startRename().
  let externalRenameTabId = $state(null);
  function startTabRenameExternal(id) {
    externalRenameTabId = id;
    // Reset after a tick so the same tab can be renamed twice consecutively.
    setTimeout(() => { externalRenameTabId = null; }, 50);
  }

  function sendSelectionToChat() {
    let text = '';
    if (editorTextareaEl) {
      const { selectionStart, selectionEnd, value } = editorTextareaEl;
      text = value.slice(selectionStart, selectionEnd).trim();
    }
    if (!text && activeNote) text = activeNote.title;
    if (!text) return;
    chatOpen = true;
    chatInsert = { text, seq: (chatInsert?.seq ?? 0) + 1 };
  }

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

  // Panel widths — persisted to localStorage, controlled by drag handles
  const COLLAPSE_THRESHOLD = 80;
  let foldersWidth = $state(Number(localStorage.getItem('grimoire:foldersWidth')) || 200);
  let notesWidth   = $state(Number(localStorage.getItem('grimoire:notesWidth'))   || 240);
  let chatWidth    = $state(Number(localStorage.getItem('grimoire:chatWidth'))    || 360);

  function savePanelWidth(panel, width) {
    localStorage.setItem(`grimoire:${panel}Width`, String(width));
  }

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
  // Divider columns (5px) sit between each panel pair; they collapse to 0px when the
  // adjacent panel is closed so the collapsed strip has no visible gap beside it.
  let gridCols = $derived.by(() => {
    return [
      foldersOpen ? `${foldersWidth}px` : '28px',
      foldersOpen ? '5px' : '0px',
      notesOpen   ? `${notesWidth}px`   : '28px',
      notesOpen   ? '5px' : '0px',
      '1fr',
      ...(chatOpen ? ['5px', `${chatWidth}px`] : []),
    ].join(' ');
  });

  // ── Panel drag-to-resize ───────────────────────────────────────────────────

  // Holds the in-progress drag: which panel, where the mouse started, and the
  // width it had at that moment. Null when no drag is active.
  let activeDrag = $state(null); // { panel: string, startX: number, startWidth: number }

  function startDrag(panel, e) {
    e.preventDefault();
    const startWidth = panel === 'folders' ? foldersWidth
                     : panel === 'notes'   ? notesWidth
                     : chatWidth;
    activeDrag = { panel, startX: e.clientX, startWidth };
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }

  function onDragMove(e) {
    if (!activeDrag) return;
    const delta = e.clientX - activeDrag.startX;
    // Chat opens on the right, so its divider is on its left edge — dragging
    // right makes it smaller, dragging left makes it larger (inverted delta).
    const newWidth = activeDrag.panel === 'chat'
      ? activeDrag.startWidth - delta
      : activeDrag.startWidth + delta;

    if (newWidth < COLLAPSE_THRESHOLD) {
      // Snap collapsed — save the pre-drag width so reopening restores it.
      savePanelWidth(activeDrag.panel, activeDrag.startWidth);
      if (activeDrag.panel === 'folders') foldersOpen = false;
      else if (activeDrag.panel === 'notes') notesOpen = false;
      else chatOpen = false;
      activeDrag = null;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      return;
    }

    const clamped = Math.max(COLLAPSE_THRESHOLD, newWidth);
    if (activeDrag.panel === 'folders') foldersWidth = clamped;
    else if (activeDrag.panel === 'notes') notesWidth = clamped;
    else chatWidth = clamped;
  }

  function onDragEnd() {
    if (!activeDrag) return;
    const width = activeDrag.panel === 'folders' ? foldersWidth
                : activeDrag.panel === 'notes'   ? notesWidth
                : chatWidth;
    savePanelWidth(activeDrag.panel, width);
    activeDrag = null;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }

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

  async function loadBookmarks() {
    try {
      bookmarks = await invoke('list_bookmarks');
    } catch (e) {
      // Non-fatal — bookmarks section just stays empty
    }
  }

  async function addBookmark(noteId) {
    await invoke('add_bookmark', { noteId });
    await loadBookmarks();
  }

  async function removeBookmark(noteId) {
    await invoke('remove_bookmark', { noteId });
    await loadBookmarks();
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
      loadBookmarks();
      await restoreTabs();
      if (tabs.length === 0) newTab();
    }
  });

  // ── Folder actions ─────────────────────────────────────────────────────────

  // Build a tree structure from the flat folders list by parent_id.
  // Returns: { folder, children }[]
  function buildFolderTree(flatFolders, parentId = null) {
    return flatFolders
      .filter(f => (f.parent_id ?? null) === parentId)
      .map(f => ({ folder: f, children: buildFolderTree(flatFolders, f.id) }));
  }

  let folderTree = $derived(buildFolderTree(folders));

  // Per-folder expand state — true (default) means expanded.
  let folderExpanded = $state({}); // { [folderId]: boolean }

  function toggleFolder(id) {
    folderExpanded = { ...folderExpanded, [id]: !(folderExpanded[id] ?? true) };
  }
  function expandAll() {
    const expanded = {};
    for (const f of folders) expanded[f.id] = true;
    folderExpanded = expanded;
  }
  function collapseAll() {
    const collapsed = {};
    for (const f of folders) collapsed[f.id] = false;
    folderExpanded = collapsed;
  }

  // Inline rename — set on a newly-created item awaiting its first name.
  let inlineRenaming = $state(null); // { id, type: 'folder'|'note', value }

  // Svelte action: focus+select a newly mounted input element.
  function autofocus(node) {
    requestAnimationFrame(() => { node.focus(); node.select(); });
  }

  async function startFolderInline() {
    try {
      const parentId = typeof selectedFolderId === 'number' ? selectedFolderId : null;
      const folder = await invoke('create_folder', { name: 'Untitled', parentId });
      await loadFolders();
      // Expand the parent so the new folder is immediately visible.
      if (parentId) folderExpanded = { ...folderExpanded, [parentId]: true };
      inlineRenaming = { id: folder.id, type: 'folder', value: 'Untitled' };
    } catch (e) {
      showError(e);
    }
  }

  async function confirmInlineRename() {
    if (!inlineRenaming) return;
    const { id, type, value } = inlineRenaming;
    const name = value.trim() || 'Untitled';
    inlineRenaming = null;
    try {
      if (type === 'folder') {
        await invoke('rename_folder', { id, name });
        await loadFolders();
      } else {
        await invoke('rename_note', { id, name });
        await loadNotes();
        if (activeNote?.id === id) {
          editorTitle = name;
          activeNote = { ...activeNote, title: name };
          tabs = tabs.map(t => t.noteId === id ? { ...t, label: name } : t);
        }
      }
    } catch (e) {
      showError(e);
    }
  }

  // ── Folder drag-to-reparent ────────────────────────────────────────────────

  let draggingFolderId = $state(null);

  function onFolderRowDragStart(e, folderId) {
    e.stopPropagation();
    e.dataTransfer.setData('folder-id', String(folderId));
    e.dataTransfer.effectAllowed = 'move';
    draggingFolderId = folderId;
  }

  function onFolderRowDragEnd() {
    draggingFolderId = null;
    dragOverFolderId = null;
  }

  // Check if `targetId` is a descendant-or-self of the folder being dragged.
  function isFolderDescendantOrSelf(targetId, ancestorId) {
    if (targetId === ancestorId) return true;
    const node = folders.find(f => f.id === targetId);
    if (!node || node.parent_id == null) return false;
    return isFolderDescendantOrSelf(node.parent_id, ancestorId);
  }

  function onFolderDropZoneDragOver(e, targetFolderId) {
    e.stopPropagation();
    if (e.dataTransfer.types.includes('folder-id')) {
      if (draggingFolderId && isFolderDescendantOrSelf(targetFolderId, draggingFolderId)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
    } else {
      onFolderDragOver(e, targetFolderId);
    }
    dragOverFolderId = targetFolderId;
  }

  async function onFolderDropZoneDrop(e, targetFolderId) {
    e.stopPropagation();
    e.preventDefault();
    dragOverFolderId = null;
    if (e.dataTransfer.types.includes('folder-id')) {
      const movingId = Number(e.dataTransfer.getData('folder-id'));
      if (!movingId || isFolderDescendantOrSelf(targetFolderId, movingId)) return;
      try {
        await invoke('move_folder', { id: movingId, newParentId: targetFolderId });
        // Expand the target folder so the moved folder is visible.
        folderExpanded = { ...folderExpanded, [targetFolderId]: true };
        await loadFolders();
      } catch (err) { showError(err); }
    } else {
      onFolderDrop(e, targetFolderId);
    }
  }

  // ── Reveal in folder panel ─────────────────────────────────────────────────

  async function revealFolder(folderId) {
    if (!foldersOpen) foldersOpen = true;
    // Walk ancestors upward and expand each one.
    const newExpanded = { ...folderExpanded };
    let current = folders.find(f => f.id === folderId);
    while (current?.parent_id) {
      newExpanded[current.parent_id] = true;
      current = folders.find(f => f.id === current.parent_id);
    }
    folderExpanded = newExpanded;
    selectFolder(folderId);
    await tick();
    document.querySelector(`[data-folder-id="${folderId}"]`)
      ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
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

  async function startNoteInline(templateId = -1) {
    notesOpen = true;
    const folderId = selectedFolderId === 'all' ? null : (selectedFolderId ?? null);
    try {
      const note = await invoke('create_note', { title: 'Untitled', folderId });
      await loadNotes();
      // Apply template property defs and seed note_properties rows before navigation.
      if (folderId && templateId > 0) {
        try {
          const defs = await invoke('apply_template_to_note', { noteId: note.id, folderId, templateId });
          folderHasProperties = defs.length > 0;
        } catch { /* non-fatal */ }
      }
      navigateToNote(note);
      // Apply template content.
      const template = templates.find(t => t.id === templateId);
      const templateContent = template?.content ?? '';
      if (templateContent) {
        editorContent = templateContent;
        isDirty = true;
        invoke('update_note', { id: note.id, title: 'Untitled', content: templateContent }).catch(() => {});
      }
      // Index in the background.
      indexState = 'indexing';
      invoke('index_note', { noteId: note.id, title: 'Untitled', content: templateContent })
        .then(() => { indexState = 'idle'; })
        .catch(() => { indexState = 'error'; });
      inlineRenaming = { id: note.id, type: 'note', value: 'Untitled' };
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
      const currentTab = tabs.find(t => t.id === activeTabId);
      if (currentTab?.type !== 'note') {
        // Non-note tab (kanban, graph, etc.) — keep it, open note in a new tab.
        const id = makeTabId();
        tabs = [...tabs, { id, type: 'note', noteId: note.id, label: note.title, customLabel: null, readMode: false }];
        activeTabId = id;
      } else {
        // Reuse the current note tab.
        tabs = tabs.map(t => t.id === activeTabId
          ? { ...t, type: 'note', noteId: note.id, label: note.title }
          : t
        );
      }
    }
    openNote(note);
  }

  // Opens a note in a brand-new tab without touching the current tab.
  function openNoteInNewTab(note) {
    if (isDirty) saveNote();
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'note', noteId: note.id, label: note.title, customLabel: null, readMode: false }];
    activeTabId = id;
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

  function openKanbanTab(folderId, folderName) {
    if (isDirty) saveNote();
    selectedFolderId = folderId;
    tagFilter = null;
    tableViewOpen = false;
    loadNotes();
    invoke('get_property_defs', { folderId }).then(d => { folderHasProperties = d.length > 0; }).catch(() => {});
    const existing = tabs.find(t => t.type === 'kanban' && t.folderId === folderId);
    if (existing) {
      activeTabId = existing.id;
      activeNote = null;
      editorTitle = '';
      editorContent = '';
      isDirty = false;
      return;
    }
    const id = makeTabId();
    tabs = [...tabs, { id, type: 'kanban', noteId: null, label: `Kanban — ${folderName}`, customLabel: null, folderId }];
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

  // Creates a new daily note for today ("DD-MM-YYYY") via the activity bar button.
  // Always inserts a fresh note — uses "(2)", "(3)" suffixes when today already has one.
  async function createDailyNote() {
    try {
      const note = await invoke('create_daily_note', { dateFormat: dailyNoteFormat });
      await loadNotes();
      openNoteInNewTab(note);
    } catch (e) {
      showError(e);
    }
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
      loadBookmarks();
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
      startNoteInline();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 't' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      newTab();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Tab' && tabs.length > 1) {
      e.preventDefault();
      const idx = tabs.findIndex(t => t.id === activeTabId);
      const next = e.shiftKey
        ? tabs[(idx - 1 + tabs.length) % tabs.length]
        : tabs[(idx + 1) % tabs.length];
      activateTab(next.id);
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
      sendSelectionToChat();
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
<svelte:document onmousemove={onDragMove} onmouseup={onDragEnd} />

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

<!-- ── Activity bar ──────────────────────────────────────────────── -->
<ActivityBar
  searchActive={searchOpen}
  showLock={vaultHasPassword}
  onSearch={() => (searchOpen = !searchOpen)}
  onGraph={openGraphTab}
  onCalendar={openCalendarTab}
  onDailyNote={createDailyNote}
  onQuickSwitcher={() => (quickSwitcherOpen = true)}
  onLock={lockVault}
  onSettings={() => (settingsOpen = true)}
  onHelp={() => openUrl('https://grimoire.app')}
  onForum={() => openUrl('https://grimoire.app/forum')}
/>

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
  </div>

  <!-- Tab strip — replaces the old "Grimoire" drag region -->
  <TabBar
    {tabs}
    {activeTabId}
    onActivate={activateTab}
    onClose={closeTab}
    onRename={renameTab}
    onNew={newTab}
    renameRequestId={externalRenameTabId}
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
        <span class="panel-header-actions">
          <button class="icon-btn" onclick={expandAll} title="Expand all">
            <svg width="14" height="14" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="3,2.5 7.5,7 12,2.5"/>
              <polyline points="3,7.5 7.5,12 12,7.5"/>
            </svg>
          </button>
          <button class="icon-btn" onclick={collapseAll} title="Collapse all">
            <svg width="14" height="14" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="3,7 7.5,2.5 12,7"/>
              <polyline points="3,12 7.5,7.5 12,12"/>
            </svg>
          </button>
          <button class="icon-btn" data-action="create-note-btn" onclick={() => startNoteInline()} title="New note (right-click to pick template)">
            <svg width="14" height="14" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M11.5 1.5 L13.5 3.5 L5 12 L2 12.5 L2.5 9.5 Z"/>
              <line x1="9.5" y1="3.5" x2="11.5" y2="5.5"/>
            </svg>
          </button>
          <button class="icon-btn" onclick={startFolderInline} title="New folder">
            <svg width="14" height="14" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M1 4A1 1 0 0 1 2 3H5L6.5 5H12A1 1 0 0 1 13 6V11A1 1 0 0 1 12 12H2A1 1 0 0 1 1 11V4Z"/>
              <line x1="9.5" y1="7.5" x2="9.5" y2="10.5"/>
              <line x1="8" y1="9" x2="11" y2="9"/>
            </svg>
          </button>
        </span>
        <button class="collapse-btn" onclick={() => (foldersOpen = false)} title="Collapse">‹</button>
      </div>

      {#if bookmarks.length > 0}
        <section class="bookmarks-section">
          <div class="sidebar-section-label">
            <span>Bookmarks</span>
            <button class="collapse-btn" onclick={() => (bookmarksOpen = !bookmarksOpen)} title={bookmarksOpen ? 'Collapse' : 'Expand'}>
              {bookmarksOpen ? '˅' : '›'}
            </button>
          </div>
          {#if bookmarksOpen}
            <ul class="bookmark-list">
              {#each bookmarks as bm (bm.note_id)}
                <li class="bookmark-row" data-note-id={bm.note_id}>
                  <button class="bookmark-name" onclick={() => openNoteById(bm.note_id)} title={bm.title}>
                    {bm.title}
                  </button>
                  <button class="bookmark-remove icon-btn" onclick={() => removeBookmark(bm.note_id)} title="Remove bookmark">✕</button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}

      <ul class="folder-list">
        <li class:active={selectedFolderId === 'all'} data-folder-id="all">
          <div class="folder-row">
            <span class="folder-expand-spacer"></span>
            <button class="row-btn" onclick={() => selectFolder('all')}>All Notes</button>
          </div>
        </li>
        <li
          class:active={selectedFolderId === null}
          class:drag-over={dragOverFolderId === 'unfiled'}
          class:drag-active={isDragging || !!draggingFolderId}
          data-folder-id="unfiled"
          ondragover={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; dragOverFolderId = 'unfiled'; }}
          ondragleave={(e) => { if (dragOverFolderId === 'unfiled' && !e.currentTarget.contains(/** @type {Node} */ (e.relatedTarget))) dragOverFolderId = null; }}
          ondrop={(e) => {
            e.preventDefault(); dragOverFolderId = null;
            if (e.dataTransfer.types.includes('folder-id')) {
              const fid = Number(e.dataTransfer.getData('folder-id'));
              if (fid) invoke('move_folder', { id: fid, newParentId: null }).then(() => loadFolders()).catch(showError);
            } else {
              const noteId = Number(e.dataTransfer.getData('text/plain'));
              if (noteId) moveNote(noteId, null);
            }
          }}
        >
          <div class="folder-row">
            <span class="folder-expand-spacer"></span>
            <button class="row-btn" onclick={() => selectFolder(null)}>Unfiled</button>
          </div>
        </li>

        {#snippet renderFolder(node)}
          {@const folder = node.folder}
          {@const isExpanded = folderExpanded[folder.id] ?? true}
          <li
            class:active={selectedFolderId === folder.id}
            class:locked-row={folder.locked}
            class:drag-over={dragOverFolderId === folder.id}
            class:drag-active={(isDragging || !!draggingFolderId) && !folder.locked}
            data-folder-id={folder.id}
            draggable={!folder.locked}
            ondragstart={(e) => !folder.locked && onFolderRowDragStart(e, folder.id)}
            ondragend={onFolderRowDragEnd}
            ondragover={(e) => !folder.locked && onFolderDropZoneDragOver(e, folder.id)}
            ondragleave={(e) => { if (dragOverFolderId === folder.id && !e.currentTarget.contains(/** @type {Node} */ (e.relatedTarget))) dragOverFolderId = null; }}
            ondrop={(e) => !folder.locked && onFolderDropZoneDrop(e, folder.id)}
          >
            <div class="folder-row">
              {#if node.children.length > 0}
                <button class="folder-expand-btn" onclick={() => toggleFolder(folder.id)} title={isExpanded ? 'Collapse' : 'Expand'}>
                  {isExpanded ? '▾' : '▸'}
                </button>
              {:else}
                <span class="folder-expand-spacer"></span>
              {/if}

              {#if folder.locked}
                <button class="row-btn folder-name" onclick={() => requestFolderUnlock(folder)}>
                  <span class="lock-icon">🔒</span>{folder.name === '<locked>' ? '(locked folder)' : folder.name}
                </button>
              {:else if inlineRenaming?.id === folder.id && inlineRenaming?.type === 'folder'}
                <input
                  class="inline-rename"
                  use:autofocus
                  bind:value={inlineRenaming.value}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); confirmInlineRename(); } }}
                  onblur={confirmInlineRename}
                />
              {:else}
                {@const folderCount = notes.filter(n => n.folder_id === folder.id).length}
                <button class="row-btn folder-name" onclick={() => selectFolder(folder.id)}>{folder.name}</button>
                {#if folderCount > 0}<span class="folder-count">{folderCount}</span>{/if}
                {#if unlockedFolderIds.has(folder.id)}
                  <button class="icon-btn" title="Remove folder password"
                    onclick={() => (folderPwModal = { mode: 'remove', folderId: folder.id })}>
                    <svg width="13" height="13" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                      <rect x="2" y="7" width="11" height="7" rx="1"/>
                      <path d="M5 7V4.5a2.5 2.5 0 0 1 5 0"/>
                    </svg>
                  </button>
                {:else}
                  <button class="icon-btn" title="Set folder password"
                    onclick={() => (folderPwModal = { mode: 'set', folderId: folder.id })}>
                    <svg width="13" height="13" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                      <rect x="2" y="7" width="11" height="7" rx="1"/>
                      <path d="M5 7V4.5a2.5 2.5 0 0 1 5 0V7"/>
                    </svg>
                  </button>
                {/if}
              {/if}
              <button class="icon-btn danger" onclick={() => deleteFolder(folder.id)} title="Delete folder">✕</button>
            </div>

            {#if isExpanded && node.children.length > 0}
              <ul class="folder-children">
                {#each node.children as child}
                  {@render renderFolder(child)}
                {/each}
              </ul>
            {/if}
          </li>
        {/snippet}

        {#each folderTree as node}
          {@render renderFolder(node)}
        {/each}
      </ul>

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
  <button class="panel-divider" aria-label="Resize folders panel" class:dragging={activeDrag?.panel === 'folders'} onmousedown={(e) => startDrag('folders', e)}></button>

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
        {#if !tagFilter && selectedFolderId && selectedFolderId !== 'all'}
          <button
            class="panel-view-btn"
            class:active={tableViewOpen}
            title="Table view"
            onclick={() => {
              if (isDirty) saveNote();
              const kanban = tabs.find(t => t.type === 'kanban' && t.folderId === selectedFolderId);
              if (kanban) tabs = tabs.filter(t => t.id !== kanban.id);
              if (activeTabId === kanban?.id) { activeNote = null; editorTitle = ''; editorContent = ''; isDirty = false; }
              tableViewOpen = !tableViewOpen;
            }}
          >Table</button>
          <button
            class="panel-view-btn"
            title="Kanban view"
            onclick={() => openKanbanTab(selectedFolderId, folders.find(f => f.id === selectedFolderId)?.name ?? '')}
          >Board</button>
        {/if}
        <button class="collapse-btn" onclick={() => (notesOpen = false)} title="Collapse">‹</button>
      </div>

      <ul>
        {#each sortedNotes as note (note.id)}
          <li
            class:active={activeNote?.id === note.id}
            class:locked-row={note.locked}
            data-note-id={note.id}
            draggable={!note.locked}
            ondragstart={(e) => !note.locked && onNoteDragStart(e, note)}
            ondragend={onNoteDragEnd}
          >
            {#if note.locked}
              <span class="row-btn note-title note-locked"><span class="lock-icon">🔒</span>(locked)</span>
            {:else if inlineRenaming?.id === note.id && inlineRenaming?.type === 'note'}
              <input
                class="inline-rename"
                use:autofocus
                bind:value={inlineRenaming.value}
                onkeydown={(e) => { if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); confirmInlineRename(); } }}
                onblur={confirmInlineRename}
              />
            {:else}
              <span
                class="drag-handle"
                title="Drag to move"
                aria-hidden="true"
              >⠇</span>
              <button class="row-btn note-title" onclick={(e) => e.ctrlKey ? openNoteInNewTab(note) : navigateToNote(note)}>{note.title}</button>
              <button class="icon-btn danger" onclick={() => deleteNote(note.id)} title="Delete note">✕</button>
            {/if}
          </li>
        {:else}
          <li class="empty">No notes here</li>
        {/each}
      </ul>

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
  <button class="panel-divider" aria-label="Resize notes panel" class:dragging={activeDrag?.panel === 'notes'} onmousedown={(e) => startDrag('notes', e)}></button>

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
      {#if tableViewOpen && selectedFolderId && selectedFolderId !== 'all'}
        <div class="tab-fullview">
          <button class="tab-fullview-close" onclick={() => (tableViewOpen = false)} title="Close table">✕ Close</button>
          {#key dbKey}
          <DatabaseView
            folderId={selectedFolderId}
            onOpenNote={(id) => { tableViewOpen = false; openNoteById(id); }}
          />
          {/key}
        </div>
      {:else if activeTab?.type === 'graph'}
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
      {:else if activeTab?.type === 'kanban'}
        <div class="tab-fullview">
          <button class="tab-fullview-close" onclick={() => closeTab(activeTabId)} title="Close kanban">✕ Close</button>
          <Kanban folderId={activeTab.folderId} onOpenNote={(id) => openNoteById(id)} />
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
            <button class="graph-toggle" onclick={() => {
              if (isDirty) saveNote();
              const kanban = tabs.find(t => t.type === 'kanban' && t.folderId === activeNote.folder_id);
              if (kanban) tabs = tabs.filter(t => t.id !== kanban.id);
              activeNote = null; tableViewOpen = true;
            }}>← Table</button>
          {/if}
          {#if activeNote.folder_id && tabs.some(t => t.type === 'kanban' && t.folderId === activeNote.folder_id)}
            <button class="graph-toggle" onclick={() => openKanbanTab(activeNote.folder_id, folders.find(f => f.id === activeNote.folder_id)?.name ?? '')}>
              ← Board
            </button>
          {/if}
          {#if activeNote.folder_id}
            <button class="graph-toggle" onclick={() => revealFolder(activeNote.folder_id)} title="Reveal in folder panel">Reveal</button>
          {/if}
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
      <div class="empty-editor">Select or create a note</div>
    {/if}
    {/if}
  </main>

  {#if chatOpen && activeTab?.type !== 'chat'}
    <button class="panel-divider" aria-label="Resize chat panel" class:dragging={activeDrag?.panel === 'chat'} onmousedown={(e) => startDrag('chat', e)}></button>
    <Chat {activeNote} pendingInsert={chatInsert} keepInMemory={keepModelInMemory} onClose={() => (chatOpen = false)} />
  {/if}
</div>

{#if quickSwitcherOpen}
  <QuickSwitcher
    onSelect={(note) => navigateToNote(note)}
    onSelectNewTab={(note) => openNoteInNewTab(note)}
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
    {devNativeContextMenu}
    onDevNativeContextMenuChange={(v) => (devNativeContextMenu = v)}
  />
{/if}

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={ctxMenu.items}
    onClose={() => (ctxMenu = null)}
  />
{/if}

{/if} <!-- end of vault-unlocked block -->
