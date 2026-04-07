<!-- Copyright (C) 2026 Wim Palland
This file is part of Grimoire — licensed under GPL-3.0 or later. -->

<script>
  const { tabs, activeTabId, onActivate, onClose, onRename, onNew, renameRequestId = null } = $props();

  // Local state for the inline rename input.
  let editingTabId = $state(null);
  let renameValue  = $state('');

  $effect(() => {
    if (renameRequestId != null) startRename(renameRequestId);
  });

  function startRename(id) {
    const tab = tabs.find(t => t.id === id);
    if (!tab) return;
    editingTabId = id;
    renameValue  = tab.customLabel ?? tab.label;
  }

  function commitRename() {
    if (!editingTabId) return;
    onRename(editingTabId, renameValue.trim() || null);
    editingTabId = null;
  }

  function handleRenameKeydown(e) {
    if (e.key === 'Enter')  { e.preventDefault(); commitRename(); }
    if (e.key === 'Escape') { editingTabId = null; }
  }
</script>

<!--
  The container itself carries data-tauri-drag-region so any gap between
  or after the tabs still drags the window.  Individual tab buttons are NOT
  drag regions — click events propagate normally.
-->
<div class="tabbar" data-tauri-drag-region>
  {#each tabs as tab (tab.id)}
    <div
      class="tab"
      class:active={tab.id === activeTabId}
      role="tab"
      aria-selected={tab.id === activeTabId}
      tabindex="-1"
      data-tab-id={tab.id}
      onmousedown={(e) => { if (e.button === 1) { e.preventDefault(); onClose(tab.id); } }}
    >
      <button
        class="tab-label"
        onclick={() => onActivate(tab.id)}
        ondblclick={() => startRename(tab.id)}
        title={tab.customLabel ?? tab.label}
      >
        {#if editingTabId === tab.id}
          <input
            class="tab-rename"
            bind:value={renameValue}
            onblur={commitRename}
            onkeydown={handleRenameKeydown}
            onclick={(e) => e.stopPropagation()}
          />
        {:else}
          {tab.customLabel ?? tab.label}
        {/if}
      </button>
      <button
        class="tab-close"
        onclick={(e) => { e.stopPropagation(); onClose(tab.id); }}
        title="Close tab"
        aria-label="Close tab"
      >✕</button>
    </div>
  {/each}

  <!-- + button sits immediately after the last tab -->
  <button class="tab-new" onclick={onNew} title="New tab (Ctrl+T)" aria-label="New tab">+</button>

  <!-- Remaining space is drag region -->
  <div class="tabbar-fill" data-tauri-drag-region></div>
</div>
