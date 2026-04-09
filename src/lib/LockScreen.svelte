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

  // Props
  let { onUnlocked } = $props();

  let password = $state('');
  let error = $state('');
  let loading = $state(false);

  async function submit() {
    if (!password) return;
    loading = true;
    error = '';
    try {
      const ok = await invoke('unlock_vault', { password });
      if (ok) {
        onUnlocked();
      } else {
        error = 'Incorrect password.';
        password = '';
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Enter') submit();
  }

  function focus(el) {
    el.focus();
  }
</script>

<div class="lock-screen">
  <div class="lock-box">
    <h1 class="lock-title">Grimoire</h1>
    <p id="lock-subtitle" class="lock-subtitle">This vault is locked.</p>

    <div class="lock-field">
      <input
        type="password"
        bind:value={password}
        onkeydown={handleKeydown}
        placeholder="Enter password…"
        aria-label="Password"
        aria-describedby="lock-subtitle"
        disabled={loading}
        use:focus
      />
    </div>

    {#if error}
      <p class="lock-error">{error}</p>
    {/if}

    <button onclick={submit} disabled={loading || !password} class="lock-btn">
      {loading ? 'Unlocking…' : 'Unlock'}
    </button>
  </div>
</div>

<style>
  .lock-screen {
    position: fixed;
    inset: 0;
    background: var(--bg);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .lock-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    width: 280px;
  }

  .lock-title {
    font: 600 22px/1 var(--sans);
    color: var(--text-h);
    letter-spacing: 0.05em;
    margin: 0 0 4px;
  }

  .lock-subtitle {
    font-size: 13px;
    color: var(--text);
    margin: 0;
  }

  .lock-field {
    width: 100%;
    margin-top: 8px;
  }

  .lock-field input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border);
    background: var(--bg2);
    color: var(--text-h);
    font: 14px var(--sans);
    border-radius: 4px;
    outline: none;
  }

  .lock-field input:focus {
    border-color: var(--accent);
  }

  .lock-error {
    font-size: 12px;
    color: var(--danger);
    margin: 0;
  }

  .lock-btn {
    width: 100%;
    padding: 8px;
    background: var(--accent-bg);
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 4px;
    font: 13px var(--sans);
    cursor: pointer;
  }

  .lock-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .lock-btn:not(:disabled):hover {
    background: var(--accent);
    color: #fff;
  }
</style>
