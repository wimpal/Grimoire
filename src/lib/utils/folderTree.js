// Copyright (C) 2026 Wim Palland — see App.svelte for license header.

// Pure utility functions for folder tree structure — no reactivity, no side effects.

/**
 * Builds a nested tree from a flat array of folders.
 * @param {any[]} flatFolders
 * @param {number|null} parentId
 * @returns {{ folder: any, children: any[] }[]}
 */
export function buildFolderTree(flatFolders, parentId = null) {
  return flatFolders
    .filter(f => (f.parent_id ?? null) === parentId)
    .map(f => ({ folder: f, children: buildFolderTree(flatFolders, f.id) }));
}

/**
 * Returns true if `targetId` is a descendant-or-self of `ancestorId`.
 * Used to prevent dragging a folder into one of its own descendants.
 * @param {any[]} folders
 * @param {number} targetId
 * @param {number} ancestorId
 * @returns {boolean}
 */
export function isFolderDescendantOrSelf(folders, targetId, ancestorId) {
  if (targetId === ancestorId) return true;
  const node = folders.find(f => f.id === targetId);
  if (!node || node.parent_id == null) return false;
  return isFolderDescendantOrSelf(folders, node.parent_id, ancestorId);
}
