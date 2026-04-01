<script>
  /**
   * TemplateModal — modal for creating or editing a user-defined template.
   *
   * Props:
   *   onSave(name, title, content)  — called when the form is submitted
   *   onCancel                      — called when the modal is dismissed
   *   template                      — optional; when provided, the modal opens in edit mode
   *                                   pre-filled with the template's current values
   */

  let { onSave, onCancel, template = null } = $props();

  let name    = $state(template?.name    ?? '');
  let title   = $state(template?.title   ?? '');
  let content = $state(template?.content ?? '');
  let loading = $state(false);
  let error   = $state('');

  const isEditing = template !== null;

  $effect(() => {
    document.getElementById('tmpl-modal-name')?.focus();
  });

  async function submit() {
    if (!name.trim()) return;
    loading = true;
    error = '';
    try {
      await onSave(name.trim(), title.trim(), content);
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
        rows="8"
        disabled={loading}
      ></textarea>
    </label>

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
    width: 400px;
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
</style>
