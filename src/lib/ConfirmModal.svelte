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
  /**
   * ConfirmModal — a centered confirmation dialog replacing browser confirm().
   *
   * Props:
   *   title        — heading text (e.g. "Delete note")
   *   message      — body text (e.g. "Are you sure you want to delete…")
   *   confirmLabel — label for the confirm button (default: "Delete")
   *   onConfirm    — called when the user confirms
   *   onCancel     — called when the user cancels or presses Escape
   */
  let {
    title = 'Are you sure?',
    message = '',
    confirmLabel = 'Delete',
    onConfirm,
    onCancel,
  } = $props();

  function handleKeydown(e) {
    if (e.key === 'Escape') onCancel();
    if (e.key === 'Enter') onConfirm();
  }

  // Focus the cancel button by default so Enter doesn't immediately confirm.
  let cancelBtn = $state(null);
  $effect(() => {
    if (cancelBtn) cancelBtn.focus();
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onCancel} onkeydown={handleKeydown} role="dialog" aria-modal="true" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <h2 class="modal-title">{title}</h2>
      <button class="close-btn" onclick={onCancel} aria-label="Close">✕</button>
    </div>

    {#if message}
      <p class="modal-message">{message}</p>
    {/if}

    <div class="modal-actions">
      <button class="btn-confirm" onclick={onConfirm}>{confirmLabel}</button>
      <button class="btn-cancel" bind:this={cancelBtn} onclick={onCancel}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 500;
  }

  .modal {
    background: var(--bg2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 20px 24px;
    width: 340px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal-title {
    font: 600 14px/1 var(--sans);
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-h);
    margin: 0;
  }

  .close-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text);
    opacity: 0.45;
    font-size: 12px;
    padding: 2px 4px;
    line-height: 1;
  }

  .close-btn:hover {
    opacity: 1;
  }

  .modal-message {
    font-size: 13px;
    color: var(--text);
    margin: 0;
    line-height: 1.55;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 2px;
  }

  .btn-confirm {
    padding: 6px 16px;
    background: var(--danger);
    color: #fff;
    border: none;
    border-radius: 4px;
    font: 13px var(--sans);
    cursor: pointer;
    transition: opacity 0.1s;
  }

  .btn-confirm:hover {
    opacity: 0.85;
  }

  .btn-cancel {
    padding: 6px 14px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 4px;
    font: 13px var(--sans);
    cursor: pointer;
    transition: background 0.1s;
  }

  .btn-cancel:hover {
    background: var(--bg3);
  }
</style>
