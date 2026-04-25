<!-- Copyright (C) 2026 Wim Palland — see App.svelte for license header. -->
<script>
  import { marked } from 'marked';
  import NoteProperties from './NoteProperties.svelte';
  import DiffView from './DiffView.svelte';
  import ImprovePopover from './ImprovePopover.svelte';

  let {
    activeNote = null,
    editorTitle = $bindable(''),
    editorContent = $bindable(''),
    isDirty = false,
    indexState = 'idle',
    folders = [],
    tabs = [],
    activeTabId = null,
    noteTags = [],
    noteLinks = [],
    noteBacklinks = [],
    unlinkedMentions = [],
    folderHasProperties = false,
    propertiesReady = true,

    // Output bindings
    editorTextareaEl = $bindable(null),

    // Improve state
    improveState = { status: 'idle' },
    onStartImprove,
    onImproveSend,
    onImproveCancel,
    onImproveAcceptAll,
    onImproveRejectAll,
    onImproveAcceptHunk,
    onImproveRejectHunk,

    // Refine hunk state
    refineState = { status: 'idle' },
    onRefineHunk,
    onRefineSend,
    onRefineCancel,

    // Events
    onMarkDirty,
    onSave,
    onCloseNote,
    onToggleReadMode,
    onMoveNote,
    onRevealFolder,
    onOpenKanbanTab,
    onOpenNoteById,
    onFilterByTag,
    onConvertMention,
    onPropertiesLoad,
    onHandleEditorKeydown,
    onOpenTableView,
  } = $props();

  const activeTab = $derived(tabs.find(t => t.id === activeTabId) ?? null);
  const wordCount = $derived(
    editorContent ? editorContent.trim().split(/\s+/).filter(Boolean).length : 0
  );
  const readingTime = $derived(Math.max(1, Math.round(wordCount / 200)));
</script>

<div class="editor-toolbar">
  <input
    class="title-input"
    bind:value={editorTitle}
    oninput={onMarkDirty}
    placeholder="Note title"
    aria-label="Note title"
  />
  <div class="toolbar-actions">
    <label>
      Move to:
      <select
        onchange={(e) => {
          const v = /** @type {HTMLSelectElement} */ (e.target).value;
          onMoveNote?.(activeNote.id, v === 'null' ? null : Number(v));
        }}
      >
        <option value="null">Unfiled</option>
        {#each folders as f (f.id)}
          <option value={f.id} selected={activeNote.folder_id === f.id}>{f.name}</option>
        {/each}
      </select>
    </label>
    <button
      onclick={onSave}
      disabled={!isDirty}
      class:index-error={!isDirty && indexState === 'error'}
      aria-live="polite"
      aria-atomic="true"
    >
      {isDirty ? 'Save (Ctrl+S)' : indexState === 'indexing' ? 'Indexing…' : indexState === 'error' ? '⚠ Index failed' : 'Saved'}
    </button>
    {#if folderHasProperties}
      <button class="graph-toggle" aria-label="Switch to table view" onclick={onOpenTableView}>← Table</button>
    {/if}
    {#if activeNote.folder_id && tabs.some(t => t.type === 'kanban' && t.folderId === activeNote.folder_id)}
      <button class="graph-toggle" aria-label="Switch to board view" onclick={() => onOpenKanbanTab?.(activeNote.folder_id, folders.find(f => f.id === activeNote.folder_id)?.name ?? '')}>
        ← Board
      </button>
    {/if}
    {#if activeNote.folder_id}
      <button class="graph-toggle" onclick={() => onRevealFolder?.(activeNote.folder_id)} title="Reveal in folder panel" aria-label="Reveal in folder panel">Reveal</button>
    {/if}
    <button class="graph-toggle" aria-label="Suggest improvements" title="Suggest improvements" onclick={onStartImprove} disabled={improveState.status !== 'idle' || !editorContent}>
      Improve
    </button>
    <button class="graph-toggle" aria-label={activeTab?.readMode ? 'Switch to edit mode' : 'Switch to read mode'} onclick={onToggleReadMode}>
      {activeTab?.readMode ? 'Edit' : 'Read'}
    </button>
    <button class="close-note-btn" aria-label="Close note" title="Close note" onclick={onCloseNote}>✕</button>
    <span class="word-count">{wordCount} word{wordCount === 1 ? '' : 's'} · {readingTime} min</span>
  </div>
</div>

{#if noteTags.length > 0}
  <div class="note-tags-strip">
    {#each noteTags as tag}
      <button class="tag-pill" onclick={() => onFilterByTag?.(tag)}>#{tag}</button>
    {/each}
  </div>
{/if}

{#if activeNote.folder_id}
  {#key activeNote.id}
    <NoteProperties
      noteId={activeNote.id}
      folderId={activeNote.folder_id}
      onPropertiesLoad={onPropertiesLoad}
    />
  {/key}
{/if}

{#if propertiesReady}
  {#if improveState.status === 'diff'}
    <DiffView
      hunks={improveState.hunks}
      instruction={improveState.instruction}
      onAcceptAll={onImproveAcceptAll}
      onRejectAll={onImproveRejectAll}
      onAcceptHunk={onImproveAcceptHunk}
      onRejectHunk={onImproveRejectHunk}
      {onRefineHunk}
      rejectedIndices={improveState.rejectedIndices}
      acceptedIndices={improveState.acceptedIndices}
    />
  {:else if improveState.status === 'streaming'}
    <div class="content-area" style="overflow-y: auto; white-space: pre-wrap; font-family: var(--mono); padding: 24px;">
      {improveState.improvedText || 'Thinking\u2026'}
    </div>
  {:else if activeTab?.readMode}
    <div class="content-area read-mode-content">{@html marked.parse(editorContent || '')}</div>
  {:else}
    <textarea
      class="content-area"
      bind:this={editorTextareaEl}
      bind:value={editorContent}
      oninput={onMarkDirty}
      onkeydown={onHandleEditorKeydown}
      placeholder="Write your note…"
    ></textarea>
  {/if}
{/if}

{#if improveState.status === 'prompt'}
  <ImprovePopover
    x={200}
    y={100}
    onSend={onImproveSend}
    onCancel={onImproveCancel}
  />
{/if}

{#if refineState.status === 'prompt'}
  <ImprovePopover
    x={refineState.x}
    y={refineState.y}
    label="How should this section be refined?"
    onSend={onRefineSend}
    onCancel={onRefineCancel}
  />
{/if}

{#if noteLinks.length > 0 || noteBacklinks.length > 0 || unlinkedMentions.length > 0}
  <div class="note-footer">
    {#if noteLinks.length > 0}
      <div class="note-footer-section">
        <span class="note-footer-label">Links</span>
        {#each noteLinks as link}
          <button class="link-pill" onclick={() => onOpenNoteById?.(link.id)}>{link.title}</button>
        {/each}
      </div>
    {/if}
    {#if noteBacklinks.length > 0}
      <div class="note-footer-section">
        <span class="note-footer-label">Backlinks</span>
        {#each noteBacklinks as link}
          <button class="link-pill" onclick={() => onOpenNoteById?.(link.id)}>{link.title}</button>
        {/each}
      </div>
    {/if}
    {#if unlinkedMentions.length > 0}
      <div class="note-footer-section">
        <span class="note-footer-label">Unlinked mentions</span>
        {#each unlinkedMentions as mention}
          <span class="link-pill-group">
            <button class="link-pill" onclick={() => onOpenNoteById?.(mention.id)}>{mention.title}</button>
            <button class="link-pill-action" onclick={() => onConvertMention?.(mention)} title="Convert to wiki-link">→ link</button>
          </span>
        {/each}
      </div>
    {/if}
  </div>
{/if}
