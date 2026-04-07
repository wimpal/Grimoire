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
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  let { onClose, vaultHasPassword = false, onSetVaultPassword, onChangeVaultPassword, onRemoveVaultPassword, onLockVault, keepInMemory = false, onKeepInMemoryChange, accent = 'red', onAccentChange, theme = 'system', onThemeChange, dateFormat = 'DD-MM-YYYY', onDateFormatChange, devNativeContextMenu = false, onDevNativeContextMenuChange } = $props();

  const isDev = import.meta.env.DEV;

  let activeSection = $state('llm');

  const sections = $derived([
    { id: 'llm',        label: 'LLM' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'security',   label: 'Security' },
    { id: 'privacy',    label: 'Privacy' },
    { id: 'data',       label: 'Data' },
    { id: 'keybinds',   label: 'Keybinds' },
    ...(isDev ? [{ id: 'developer', label: 'Developer' }] : []),
  ]);

  // ── Placeholder state (no backend yet) ────────────────────────────────────

  let chatModel      = $state('llama3.2');
  let embeddingModel = $state('nomic-embed-text');
  let logFileAccess  = $state(true);

  // ── Export state ──────────────────────────────────────────────────────────

  let exportStatus = $state(''); // '' | 'running' | 'done:N' | 'error:msg'

  async function runExport() {
    exportStatus = 'running';
    try {
      const dir = await openDialog({ directory: true, multiple: false, title: 'Export notes to…' });
      if (!dir) { exportStatus = ''; return; }
      const count = await invoke('export_notes', { destDir: dir });
      exportStatus = `done:${count}`;
    } catch (e) {
      exportStatus = `error:${e}`;
    }
  }
</script>

<div class="settings-overlay">
  <div class="settings-header">
    <span class="settings-title">Settings</span>
    <button class="settings-close" onclick={onClose}>✕ Close</button>
  </div>

  <div class="settings-body">
    <!-- ── Left nav ──────────────────────────────────────────────────── -->
    <nav class="settings-nav">
      {#each sections as s}
        <button
          class="settings-nav-item"
          class:active={activeSection === s.id}
          onclick={() => (activeSection = s.id)}
        >
          {s.label}
        </button>
      {/each}
    </nav>

    <!-- ── Content pane ─────────────────────────────────────────────── -->
    <div class="settings-content">

      {#if activeSection === 'llm'}
        <h3>LLM</h3>
        <p class="settings-notice">
          Model changes take effect on the next chat. Models are installed and managed through Ollama.
        </p>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Chat model</span>
            <span class="setting-desc">The model used for chat responses and note improvements.</span>
          </div>
          <select bind:value={chatModel}>
            <option value="llama3.2">llama3.2 · general (default)</option>
            <option value="phi3">phi3 · lightweight</option>
            <option value="gemma2:2b">gemma2:2b · lightweight</option>
            <option value="mistral">mistral · general</option>
            <option value="codellama">codellama · programming</option>
            <option value="llama3:70b">llama3:70b · high quality (GPU)</option>
          </select>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Keep model in memory</span>
            <span class="setting-desc">
              Keeps the chat model loaded at all times. Eliminates cold-start delay but
              holds ~4–8 GB of RAM continuously.
            </span>
          </div>
          <label class="toggle">
            <input type="checkbox" checked={keepInMemory} onchange={(e) => onKeepInMemoryChange(e.currentTarget.checked)} />
            <span class="toggle-label">{keepInMemory ? 'On' : 'Off'}</span>
          </label>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Embedding model</span>
            <span class="setting-desc">
              Used to index notes for semantic search. Changing this invalidates the
              current index — a full re-index will be required.
            </span>
          </div>
          <select bind:value={embeddingModel}>
            <option value="nomic-embed-text">nomic-embed-text (default, ~270 MB)</option>
            <option value="mxbai-embed-large">mxbai-embed-large · higher quality</option>
          </select>
        </div>

      {:else if activeSection === 'appearance'}
        <h3>Appearance</h3>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Theme</span>
            <span class="setting-desc">Controls light/dark mode. "System" follows your OS preference.</span>
          </div>
          <select value={theme} onchange={(e) => onThemeChange(e.currentTarget.value)}>
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
            <option value="spellbook">Spellbook ✦</option>
          </select>
        </div>

        <div class="setting-row" class:faded={theme === 'spellbook'}>
          <div class="setting-label">
            <span class="setting-name">Accent colour</span>
            <span class="setting-desc">{theme === 'spellbook' ? 'Not available in Spellbook theme — accent is fixed gold.' : 'Changes the highlight colour used across the app.'}</span>
          </div>
          <div class="accent-swatches">
            <button
              class="accent-swatch"
              class:active={accent === 'red'}
              style="--swatch-color: #9b2020"
              title="Crimson (default)"
              disabled={theme === 'spellbook'}
              onclick={() => onAccentChange('red')}
            ></button>
            <button
              class="accent-swatch"
              class:active={accent === 'cyan'}
              style="--swatch-color: #0c6e7e"
              title="Cyan"
              disabled={theme === 'spellbook'}
              onclick={() => onAccentChange('cyan')}
            ></button>
            <button
              class="accent-swatch"
              class:active={accent === 'green'}
              style="--swatch-color: #256b3a"
              title="Forest green"
              disabled={theme === 'spellbook'}
              onclick={() => onAccentChange('green')}
            ></button>
          </div>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Date format</span>
            <span class="setting-desc">
              Controls how dates are displayed in the calendar. Notes are always stored with
              ISO 8601 titles (YYYY-MM-DD) — changing this only affects display.
            </span>
          </div>
          <select value={dateFormat} onchange={(e) => onDateFormatChange(e.currentTarget.value)}>
            <option value="DD-MM-YYYY">DD-MM-YYYY</option>
            <option value="YYYY-MM-DD">YYYY-MM-DD</option>
            <option value="MM-DD-YYYY">MM-DD-YYYY</option>
          </select>
        </div>

      {:else if activeSection === 'security'}
        <h3>Security</h3>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Vault password</span>
            <span class="setting-desc">
              Password-protects the entire vault. Contents are encrypted at rest using a key
              derived from the password via Argon2id. The password is never stored — if you
              forget it, your notes cannot be recovered.
            </span>
          </div>
          <div class="vault-actions">
            {#if !vaultHasPassword}
              <button class="settings-action-btn" onclick={onSetVaultPassword}>Set password</button>
            {:else}
              <button class="settings-action-btn" onclick={onChangeVaultPassword}>Change password</button>
              <button class="settings-action-btn" onclick={onRemoveVaultPassword}>Remove password</button>
              <button class="settings-action-btn danger" onclick={onLockVault}>Lock vault now</button>
            {/if}
          </div>
        </div>

      {:else if activeSection === 'data'}
        <h3>Data</h3>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Export notes to Markdown</span>
            <span class="setting-desc">
              Saves all unlocked notes as <code>.md</code> files in a folder you choose.
              Folder structure is preserved as subdirectories. Locked notes are skipped.
            </span>
          </div>
          <div class="setting-actions">
            <button class="settings-action-btn" onclick={runExport} disabled={exportStatus === 'running'}>
              {exportStatus === 'running' ? 'Exporting…' : 'Export'}
            </button>
            {#if exportStatus.startsWith('done:')}
              <span class="export-ok">✓ {exportStatus.slice(5)} notes exported</span>
            {:else if exportStatus.startsWith('error:')}
              <span class="export-err">{exportStatus.slice(6)}</span>
            {/if}
          </div>
        </div>

      {:else if activeSection === 'privacy'}
        <h3>Privacy</h3>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Log file access</span>
            <span class="setting-desc">
              Records every file the app reads to a local audit log stored on this machine.
            </span>
          </div>
          <label class="toggle">
            <input type="checkbox" bind:checked={logFileAccess} />
            <span class="toggle-label">{logFileAccess ? 'On' : 'Off'}</span>
          </label>
        </div>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Local only</span>
            <span class="setting-desc">No data ever leaves this machine. Cannot be disabled.</span>
          </div>
          <label class="toggle toggle-locked">
            <input type="checkbox" checked disabled />
            <span class="toggle-label">Always on</span>
          </label>
        </div>

      {:else if activeSection === 'keybinds'}
        <h3>Keybinds</h3>
        <p class="settings-notice">Custom keybind editing is not yet available.</p>

        <table class="keybinds-table">
          <thead>
            <tr>
              <th>Action</th>
              <th>Shortcut</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Save note</td>
              <td><kbd>Ctrl+S</kbd></td>
            </tr>
            <tr>
              <td>Lock vault</td>
              <td><kbd>Ctrl+Shift+L</kbd></td>
            </tr>
            <tr>
              <td>Send selection to chat</td>
              <td><kbd>Ctrl+Shift+Enter</kbd></td>
            </tr>
            <tr>
              <td>Cycle tab forward</td>
              <td><kbd>Ctrl+Tab</kbd></td>
            </tr>
            <tr>
              <td>Cycle tab backward</td>
              <td><kbd>Ctrl+Shift+Tab</kbd></td>
            </tr>
            <tr>
              <td>Open note in new tab (click)</td>
              <td><kbd>Ctrl+Click</kbd></td>
            </tr>
            <tr>
              <td>Toggle chat panel</td>
              <td>—</td>
            </tr>
            <tr>
              <td>Toggle graph view</td>
              <td>—</td>
            </tr>
          </tbody>
        </table>

      {:else if activeSection === 'developer'}
        <h3>Developer</h3>
        <p class="settings-notice">These settings are only visible in dev builds.</p>

        <div class="setting-row">
          <div class="setting-label">
            <span class="setting-name">Use native context menu</span>
            <span class="setting-desc">
              Disables the custom context menu and restores the native WebView2 menu,
              which includes Inspect Element. Useful for debugging layout and styles.
            </span>
          </div>
          <label class="toggle">
            <input
              type="checkbox"
              checked={devNativeContextMenu}
              onchange={(e) => onDevNativeContextMenuChange(e.currentTarget.checked)}
            />
            <span class="toggle-label">{devNativeContextMenu ? 'On' : 'Off'}</span>
          </label>
        </div>

      {/if}

    </div>
  </div>
</div>

<style>
  .settings-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--bg);
    display: flex;
    flex-direction: column;
  }

  /* ── Header ─────────────────────────────────────────────────────── */

  .settings-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .settings-title {
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text);
  }

  .settings-close {
    padding: 5px 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg2);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
  }

  .settings-close:hover {
    background: var(--bg3);
    color: var(--text-h);
  }

  /* ── Body (nav + content) ────────────────────────────────────────── */

  .settings-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* ── Left nav ────────────────────────────────────────────────────── */

  .settings-nav {
    width: 180px;
    flex-shrink: 0;
    background: var(--bg2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 16px 8px;
    gap: 2px;
    overflow-y: auto;
  }

  .settings-nav-item {
    background: none;
    border: none;
    border-radius: 5px;
    padding: 7px 10px;
    text-align: left;
    font: 13px var(--sans);
    color: var(--text-h);
    cursor: pointer;
  }

  .settings-nav-item:hover {
    background: var(--bg3);
  }

  .settings-nav-item.active {
    background: var(--accent-bg);
    color: var(--accent);
    font-weight: 600;
  }

  /* ── Content pane ────────────────────────────────────────────────── */

  .settings-content {
    flex: 1;
    overflow-y: auto;
    padding: 28px 40px;
    max-width: 680px;
  }

  .settings-content h3 {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-h);
    margin: 0 0 20px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }

  .settings-notice {
    font-size: 12px;
    color: var(--text);
    margin: 0 0 20px;
    font-style: italic;
  }

  /* ── Individual setting row ──────────────────────────────────────── */

  .setting-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 0;
    border-bottom: 1px solid var(--border);
  }

  .setting-row.faded {
    opacity: 0.55;
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  .setting-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-h);
  }

  .setting-desc {
    font-size: 12px;
    color: var(--text);
    line-height: 1.5;
  }

  .setting-row select {
    flex-shrink: 0;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text-h);
    font: 13px var(--sans);
    cursor: pointer;
    min-width: 200px;
  }

  .setting-row select:focus {
    outline: none;
    border-color: var(--accent);
  }

  /* ── Accent swatches ─────────────────────────────────────────────── */

  .accent-swatches {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }

  .accent-swatch {
    width: 26px;
    height: 26px;
    border-radius: 5px;
    background: var(--swatch-color);
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
    transition: border-color 0.1s;
  }

  .accent-swatch:hover {
    border-color: var(--text);
  }

  .accent-swatch:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .accent-swatch:disabled:hover {
    border-color: transparent;
  }

  .accent-swatch.active {
    border-color: var(--text-h);
    outline: 2px solid var(--swatch-color);
    outline-offset: 2px;
  }

  /* ── Toggle ──────────────────────────────────────────────────────── */

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    flex-shrink: 0;
    user-select: none;
  }

  .toggle input[type="checkbox"] {
    accent-color: var(--accent);
    width: 14px;
    height: 14px;
    cursor: pointer;
  }

  .toggle-label {
    font-size: 13px;
    color: var(--text-h);
    min-width: 50px;
  }

  .toggle-locked {
    opacity: 0.5;
    cursor: default;
  }

  .toggle-locked input {
    cursor: default;
  }

  /* ── Keybinds table ──────────────────────────────────────────────── */

  .keybinds-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .keybinds-table th {
    text-align: left;
    padding: 6px 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text);
    border-bottom: 1px solid var(--border);
  }

  .keybinds-table td {
    padding: 10px 12px;
    color: var(--text-h);
    border-bottom: 1px solid var(--border);
  }

  .keybinds-table tr:last-child td {
    border-bottom: none;
  }

  /* ── Vault action buttons ─────────────────────────────────────────── */

  .vault-actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }

  .settings-action-btn {
    padding: 5px 14px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg2);
    color: var(--text-h);
    font: 13px var(--sans);
    cursor: pointer;
    text-align: center;
    white-space: nowrap;
  }

  .settings-action-btn:hover {
    background: var(--bg3);
  }

  .settings-action-btn.danger:hover {
    color: var(--danger);
    border-color: var(--danger);
    background: rgba(192, 57, 43, 0.08);
  }

  .setting-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .export-ok {
    font-size: 12px;
    color: var(--text-m);
  }

  .export-err {
    font-size: 12px;
    color: var(--danger);
  }

  kbd {
    display: inline-block;
    padding: 2px 7px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg2);
    font: 12px var(--mono);
    color: var(--text-h);
  }
</style>
