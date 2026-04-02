<script>
  /**
   * TemplateModal — modal for creating or editing a user-defined template.
   *
   * Props:
   *   onSave(name, title, content, properties)  — called when the form is submitted
   *   onCancel                                  — called when the modal is dismissed
   *   template                                  — optional; edit mode pre-fills all fields
   */
  import { untrack } from 'svelte';

  let { onSave, onCancel, template = null } = $props();

  // untrack: this modal mounts fresh each open, so capturing the initial prop value is intentional.
  let name    = $state(untrack(() => template?.name    ?? ''));
  let title   = $state(untrack(() => template?.title   ?? ''));
  let content = $state(untrack(() => template?.content ?? ''));
  // Each property spec is { name, type, options } — options is a comma-separated string in the UI,
  // stored as a JSON array string when sent to Rust.
  let templateProps = $state(
    untrack(() =>
      (template?.properties ?? []).map(p => ({
        name: p.name,
        type: p.type,
        options: p.options
          ? JSON.parse(p.options).join(', ')
          : '',
      }))
    )
  );
  let loading = $state(false);
  let error   = $state('');

  const isEditing = $derived(template !== null);

  $effect(() => {
    document.getElementById('tmpl-modal-name')?.focus();
  });

  function addProp() {
    templateProps = [...templateProps, { name: '', type: 'text', options: '' }];
  }

  function removeProp(i) {
    templateProps = templateProps.filter((_, idx) => idx !== i);
  }

  function updateProp(i, field, value) {
    templateProps = templateProps.map((p, idx) =>
      idx === i ? { ...p, [field]: value } : p
    );
  }

  async function submit() {
    if (!name.trim()) return;
    loading = true;
    error = '';
    try {
      // Convert UI comma-separated options back to JSON array strings for Rust.
      const properties = templateProps
        .filter(p => p.name.trim())
        .map(p => ({
          name: p.name.trim(),
          type: p.type,
          options: p.type === 'select' && p.options.trim()
            ? JSON.stringify(p.options.split(',').map(s => s.trim()).filter(Boolean))
            : null,
        }));
      await onSave(name.trim(), title.trim(), content, properties);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') onCancel();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={onCancel} onkeydown={handleKeydown} role="dialog" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <h2 class="modal-title">{isEditing ? 'Edit Template' : 'New Template'}</h2>

    <label class="field">
      <span>Template name</span>
      <input
        id="tmpl-modal-name"
        type="text"
        bind:value={name}
        placeholder="e.g. Project Kickoff"
        disabled={loading}
      />
    </label>

    <label class="field">
      <span>Default note title <span class="optional">(optional)</span></span>
      <input
        type="text"
        bind:value={title}
        placeholder="Leave blank to use template name"
        disabled={loading}
      />
    </label>

    <label class="field">
      <span>Content <span class="optional">(optional)</span></span>
      <textarea
        bind:value={content}
        placeholder="Template body…"
        rows="6"
        disabled={loading}
      ></textarea>
    </label>

    <!-- Property definitions -->
    <div class="props-section">
      <span class="props-section-label">Properties</span>
      {#if templateProps.length > 0}
        <div class="props-list">
          {#each templateProps as prop, i}
            <div class="prop-spec-row">
              <input
                type="text"
                class="prop-spec-input"
                value={prop.name}
                oninput={(e) => updateProp(i, 'name', e.currentTarget.value)}
                placeholder="Name"
                disabled={loading}
              />
              <select
                class="prop-spec-input prop-spec-type"
                value={prop.type}
                onchange={(e) => updateProp(i, 'type', e.currentTarget.value)}
                disabled={loading}
              >
                <option value="text">Text</option>
                <option value="number">Number</option>
                <option value="date">Date</option>
                <option value="boolean">Checkbox</option>
                <option value="select">Select</option>
              </select>
              {#if prop.type === 'select'}
                <input
                  type="text"
                  class="prop-spec-input prop-spec-options"
                  value={prop.options}
                  oninput={(e) => updateProp(i, 'options', e.currentTarget.value)}
                  placeholder="Options (comma-separated)"
                  disabled={loading}
                />
              {/if}
              <button
                class="prop-spec-delete"
                onclick={() => removeProp(i)}
                disabled={loading}
                title="Remove"
              >✕</button>
            </div>
          {/each}
        </div>
      {/if}
      <button class="prop-spec-add" onclick={addProp} disabled={loading}>+ Add property</button>
    </div>

    {#if error}
      <p class="modal-error">{error}</p>
    {/if}

    <div class="modal-actions">
      <button class="modal-cancel" onclick={onCancel} disabled={loading}>Cancel</button>
      <button
        class="modal-confirm"
        onclick={submit}
        disabled={loading || !name.trim()}
      >
        {loading ? 'Saving…' : (isEditing ? 'Save Changes' : 'Save Template')}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 500;
  }

  .modal {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 480px;
    max-height: 90vh;
    overflow-y: auto;
  }

  .modal-title {
    font: 600 14px/1 var(--sans);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-h);
    margin: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field span {
    font: 12px var(--sans);
    color: var(--text);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .optional {
    text-transform: none;
    letter-spacing: 0;
    opacity: 0.6;
  }

  input[type="text"],
  textarea {
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--border);
    background: var(--bg2);
    color: var(--text-h);
    font: 13px var(--sans);
    border-radius: 4px;
    outline: none;
    box-sizing: border-box;
  }

  input[type="text"]:focus,
  textarea:focus {
    border-color: var(--accent);
  }

  textarea {
    resize: vertical;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.5;
  }

  .modal-error {
    font-size: 12px;
    color: var(--danger);
    margin: 0;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .modal-cancel {
    padding: 6px 14px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    font: 13px var(--sans);
    border-radius: 4px;
    cursor: pointer;
  }

  .modal-cancel:not(:disabled):hover {
    background: var(--bg3);
  }

  .modal-confirm {
    padding: 6px 14px;
    border: 1px solid var(--accent);
    background: var(--accent-bg);
    color: var(--accent);
    font: 13px var(--sans);
    border-radius: 4px;
    cursor: pointer;
  }

  .modal-confirm:not(:disabled):hover {
    background: var(--accent);
    color: #fff;
  }

  .modal-confirm:disabled,
  .modal-cancel:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* ── Property spec editor ─────────────────────────────────────────── */

  .props-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .props-section-label {
    font: 12px var(--sans);
    color: var(--text);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .props-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .prop-spec-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .prop-spec-input {
    padding: 5px 8px;
    border: 1px solid var(--border);
    background: var(--bg2);
    color: var(--text-h);
    font: 12px var(--sans);
    border-radius: 4px;
    outline: none;
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
  }

  .prop-spec-input:focus {
    border-color: var(--accent);
  }

  .prop-spec-type {
    flex: 0 0 auto;
    width: 100px;
  }

  .prop-spec-options {
    flex: 1.5;
  }

  .prop-spec-delete {
    background: none;
    border: none;
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
    padding: 2px 5px;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .prop-spec-delete:hover {
    color: var(--danger);
    background: rgba(229, 62, 62, 0.1);
  }

  .prop-spec-add {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 2px 0;
    font: 12px var(--sans);
    color: var(--text);
    cursor: pointer;
  }

  .prop-spec-add:hover {
    color: var(--accent);
  }
</style>
