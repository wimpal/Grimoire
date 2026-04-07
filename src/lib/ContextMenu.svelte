<!-- Copyright (C) 2026 Wim Palland
This file is part of Grimoire — licensed under GPL-3.0 or later. -->

<script>
  let { x, y, items, onClose } = $props();

  // Clamp to viewport so the main menu never clips off-screen.
  const clampedX = $derived(Math.min(x, window.innerWidth  - 174));
  const clampedY = $derived(Math.min(y, window.innerHeight - items.length * 28 - 16));

  // Whether submenus open to the right (enough room) or flip left.
  const subRight = $derived(clampedX + 340 < window.innerWidth);

  function handleKeydown(e) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Transparent backdrop — catches any click outside the menu -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="ctx-backdrop" onmousedown={onClose}></div>

<ul
  class="ctx-menu"
  style="left: {clampedX}px; top: {clampedY}px"
  role="menu"
>
  {#each items as item}
    {#if item.divider}
      <li class="ctx-divider" role="separator"></li>
    {:else if item.submenu}
      <li class="ctx-item-wrap" role="none">
        <button class="ctx-item ctx-item-has-sub" role="menuitem">
          {item.label}
          <span class="ctx-arrow">›</span>
        </button>
        <ul class="ctx-submenu" class:sub-left={!subRight} role="menu">
          {#each item.submenu as sub}
            {#if sub.divider}
              <li class="ctx-divider" role="separator"></li>
            {:else}
              <li role="none">
                <button
                  class="ctx-item"
                  role="menuitem"
                  onmousedown={(e) => { e.stopPropagation(); sub.action(); onClose(); }}
                >{sub.label}</button>
              </li>
            {/if}
          {/each}
        </ul>
      </li>
    {:else}
      <li role="none">
        <button
          class="ctx-item"
          class:danger={item.danger}
          class:disabled={item.disabled}
          role="menuitem"
          disabled={item.disabled}
          onmousedown={(e) => {
            e.stopPropagation();
            if (!item.disabled) { item.action(); onClose(); }
          }}
        >{item.label}</button>
      </li>
    {/if}
  {/each}
</ul>

<style>
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }

  .ctx-menu {
    position: fixed;
    z-index: 200;
    background: var(--bg2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 0;
    min-width: 160px;
    list-style: none;
    margin: 0;
  }

  .ctx-divider {
    height: 1px;
    background: var(--border);
    margin: 3px 0;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 5px 14px;
    text-align: left;
    background: none;
    border: none;
    font: 13px var(--sans);
    color: var(--text);
    cursor: default;
    white-space: nowrap;
  }

  .ctx-item:not(.disabled):hover {
    background: var(--bg3);
    color: var(--text-h);
  }

  .ctx-item.danger:not(.disabled):hover {
    color: var(--danger);
  }

  .ctx-item.disabled {
    color: var(--text-dim);
    cursor: not-allowed;
  }

  /* ── Submenu ─────────────────────────────────────────────────────── */

  .ctx-item-wrap {
    position: relative;
  }

  .ctx-item-has-sub {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
  }

  .ctx-arrow {
    opacity: 0.45;
    font-size: 14px;
    line-height: 1;
    flex-shrink: 0;
  }

  .ctx-submenu {
    display: none;
    position: absolute;
    left: calc(100% + 2px);
    top: -3px;
    background: var(--bg2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 0;
    min-width: 155px;
    list-style: none;
    margin: 0;
    z-index: 201;
  }

  .ctx-submenu.sub-left {
    left: auto;
    right: calc(100% + 2px);
  }

  .ctx-item-wrap:hover > .ctx-submenu {
    display: block;
  }
</style>
