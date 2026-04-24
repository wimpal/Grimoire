import { invoke } from '@tauri-apps/api/core';
import { tick } from 'svelte';

export function createFolderService({ onError, onBeforeFolderChange }) {
  let folders = $state([]);
  let selectedFolderId = $state(null);
  let unlockedFolderIds = $state(new Set());
  let folderExpanded = $state({});
  let inlineRenaming = $state(null);
  let folderDeletePending = $state(null);
  let folderPwModal = $state(null);
  let folderUnlockTarget = $state(null);

  let folderHasProperties = $state(false);

  async function loadFolders() {
    try {
      folders = await invoke('list_folders');
    } catch (e) {
      onError?.(e);
    }
  }

  async function startFolderInline() {
    try {
      const parentId = typeof selectedFolderId === 'number' ? selectedFolderId : null;
      const folder = await invoke('create_folder', { name: 'Untitled', parentId });
      await loadFolders();
      if (parentId) folderExpanded = { ...folderExpanded, [parentId]: true };
      inlineRenaming = { id: folder.id, type: 'folder', value: 'Untitled' };
    } catch (e) {
      onError?.(e);
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
      }
      // Note renames handled by noteService
      return { id, type, name };
    } catch (e) {
      onError?.(e);
      return null;
    }
  }

  async function revealFolder(folderId, openFolders) {
    if (!openFolders) loadFolders();
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

  function deleteFolder(id) {
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
      return 'refresh_notes';
    } catch (e) {
      onError?.(e);
      return null;
    }
  }

  async function selectFolder(id, saveFn) {
    if (saveFn) await saveFn();
    selectedFolderId = id;
    return id;
  }

  function requestFolderUnlock(folder) {
    folderUnlockTarget = folder;
  }

  async function handleFolderUnlockSafe(password) {
    if (!folderUnlockTarget) return false;
    const targetId = folderUnlockTarget.id;
    const ok = await invoke('unlock_folder', { folderId: targetId, password });
    if (ok) {
      folderUnlockTarget = null;
      unlockedFolderIds = new Set([...unlockedFolderIds, targetId]);
      await loadFolders();
    }
    return ok;
  }

  async function handleFolderPwSubmit(password) {
    if (!folderPwModal) return true;
    if (folderPwModal.mode === 'set') {
      await invoke('set_folder_password', { folderId: folderPwModal.folderId, password });
      const next = new Set(unlockedFolderIds);
      next.delete(folderPwModal.folderId);
      unlockedFolderIds = next;
    } else if (folderPwModal.mode === 'remove') {
      await invoke('remove_folder_password', { folderId: folderPwModal.folderId, password });
      const next = new Set(unlockedFolderIds);
      next.delete(folderPwModal.folderId);
    }
    folderPwModal = null;
    await loadFolders();
    return true;
  }

  async function loadFolderPropertyDefs(folderId) {
    if (!folderId || folderId === 'all') {
      folderHasProperties = false;
      return;
    }
    try {
      const defs = await invoke('get_property_defs', { folderId });
      folderHasProperties = defs.length > 0;
      return defs;
    } catch {
      folderHasProperties = false;
      return [];
    }
  }

  function openFolderPwModal(fid, mode) {
    folderPwModal = { mode, folderId: fid };
  }

  return {
    // State
    get folders() { return folders; },
    set folders(v) { folders = v; },
    get selectedFolderId() { return selectedFolderId; },
    set selectedFolderId(v) { selectedFolderId = v; },
    get unlockedFolderIds() { return unlockedFolderIds; },
    set unlockedFolderIds(v) { unlockedFolderIds = v; },
    get folderExpanded() { return folderExpanded; },
    set folderExpanded(v) { folderExpanded = v; },
    get inlineRenaming() { return inlineRenaming; },
    set inlineRenaming(v) { inlineRenaming = v; },
    get folderDeletePending() { return folderDeletePending; },
    set folderDeletePending(v) { folderDeletePending = v; },
    get folderPwModal() { return folderPwModal; },
    set folderPwModal(v) { folderPwModal = v; },
    get folderUnlockTarget() { return folderUnlockTarget; },
    set folderUnlockTarget(v) { folderUnlockTarget = v; },
    get folderHasProperties() { return folderHasProperties; },
    set folderHasProperties(v) { folderHasProperties = v; },
    // Functions
    loadFolders,
    startFolderInline,
    confirmInlineRename,
    revealFolder,
    deleteFolder,
    confirmDeleteFolder,
    selectFolder,
    requestFolderUnlock,
    handleFolderUnlockSafe,
    handleFolderPwSubmit,
    loadFolderPropertyDefs,
    openFolderPwModal,
  };
}
