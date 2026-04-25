<script>
  let {
    hunks = [],
    instruction = '',
    onAcceptAll,
    onRejectAll,
    onAcceptHunk,
    onRejectHunk,
    onRefineHunk,
    acceptedIndices = [],
    rejectedIndices = [],
  } = $props();

  const changeHunks = $derived(
    hunks.map((h, i) => ({ ...h, idx: i })).filter(h => h.type !== 'unchanged')
  );

  const activeHunks = $derived(
    changeHunks.filter(h => !acceptedIndices.includes(h.idx) && !rejectedIndices.includes(h.idx))
  );
</script>

<div class="diff-view">
  <div class="diff-header">
    <span class="diff-header-title">Suggested improvements</span>
    {#if instruction}
      <span class="diff-header-instruction" title={instruction}>"{instruction}"</span>
    {/if}
    <div class="diff-actions">
      <button class="accept-all" onclick={onAcceptAll}>Accept All</button>
      <button class="reject-all" onclick={onRejectAll}>Reject All</button>
    </div>
  </div>

  {#if activeHunks.length === 0}
    <div class="diff-empty">
      <span class="diff-empty-msg">No changes suggested</span>
      <span class="diff-empty-sub">The LLM returned text identical to the original.</span>
    </div>
  {:else}
    <div class="diff-body">
      {#each hunks as hunk, i}
        <div class="diff-hunk {hunk.type}" class:unchanged={hunk.type === 'unchanged'} class:accepted={acceptedIndices.includes(i)} class:rejected={rejectedIndices.includes(i)}>
          {#if hunk.type !== 'unchanged'}
            <div class="diff-hunk-header">
              <span class="diff-hunk-header-label">
                {hunk.type === 'add' ? '+ Added' : '- Removed'}
              </span>
              <span>({hunk.lines.length} line{hunk.lines.length === 1 ? '' : 's'})</span>
              <div class="diff-hunk-actions">
                {#if acceptedIndices.includes(i)}
                  <span class="accepted-label">Accepted</span>
                {:else if rejectedIndices.includes(i)}
                  <span class="rejected-label">Rejected</span>
                {:else}
                  <button class="accept" onclick={() => onAcceptHunk?.(i)}>Accept</button>
                  <button class="reject" onclick={() => onRejectHunk?.(i)}>Reject</button>
                  <button class="refine" onclick={(e) => {
                    const rect = e.currentTarget.getBoundingClientRect();
                    onRefineHunk?.(i, rect.left, rect.bottom);
                  }}>Refine</button>
                {/if}
              </div>
            </div>
          {/if}
          <div class="diff-hunk-lines">
            {#each hunk.lines as line}
              <div class="diff-line {hunk.type}">{line || '\u00A0'}</div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  @import './styles/diff-view.css';
</style>
