<script>
  /**
   * DatabaseView — spreadsheet-style table view for a folder's notes + properties.
   * Shown when the user toggles "Table" on a folder that has property definitions.
   *
   * Props:
   *   folderId   — the folder to display
   *   onOpenNote — callback(noteId) to open a note in the editor
   */
  import { invoke } from '@tauri-apps/api/core';

  let { folderId, onOpenNote = () => {} } = $props();

  let defs = $state([]);
  let rows = $state([]);
  let loading = $state(false);

  $effect(() => {
    if (folderId) {
      loadData(folderId);
    } else {
      defs = [];
      rows = [];
    }
  });

  async function loadData(fid) {
    loading = true;
    try {
      const [d, r] = await Promise.all([
        invoke('get_property_defs', { folderId: fid }),
        invoke('list_notes_with_properties', { folderId: fid }),
      ]);
      defs = d;
      rows = r;
    } catch {
      defs = [];
      rows = [];
    } finally {
      loading = false;
    }
  }

  // Returns the value string if the note owns this property, or null if it doesn't.
  // null  = no note_properties row (LEFT JOIN miss) — cell is not editable
  // ''    = row exists but value is empty — cell is editable
  function getPropValue(note, defId) {
    const p = note.properties.find(pr => pr.def_id === defId);
    if (!p) return null;          // def not in this note at all
    return p.value ?? null;       // null from Rust means unowned
  }

  function getPropType(defId) {
    const d = defs.find(df => df.id === defId);
    return d?.type ?? 'text';
  }

  async function updateValue(noteId, defId, value) {
    try {
      await invoke('set_note_property', { noteId, defId, value });
      // Update local state
      rows = rows.map(r => {
        if (r.id !== noteId) return r;
        return {
          ...r,
          properties: r.properties.map(p =>
            p.def_id === defId ? { ...p, value } : p
          ),
        };
      });
    } catch {
      // Non-fatal
    }
  }

  function handleCellBlur(noteId, defId, e) {
    updateValue(noteId, defId, e.currentTarget.value);
  }

  function handleCellKeydown(e) {
    if (e.key === 'Enter') e.currentTarget.blur();
  }

  function handleBoolChange(noteId, defId, e) {
    updateValue(noteId, defId, e.currentTarget.checked ? 'true' : 'false');
  }

  function handleSelectChange(noteId, defId, e) {
    updateValue(noteId, defId, e.currentTarget.value);
  }
</script>

<div class="db-view">
  {#if loading}
    <p class="db-loading">Loading…</p>
  {:else if rows.length === 0}
    <p class="db-empty">No notes in this folder.</p>
  {:else}
    <div class="db-table-wrap">
      <table class="db-table">
        <thead>
          <tr>
            <th class="db-th-title">Title</th>
            {#each defs as def (def.id)}
              <th>{def.name}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as note (note.id)}
            <tr>
              <td class="db-td-title">
                <button class="db-note-link" onclick={() => onOpenNote(note.id)}>
                  {note.title}
                </button>
              </td>
              {#each defs as def (def.id)}
                {@const val = getPropValue(note, def.id)}
                <td class="db-td-value">
                  {#if val === null}
                    <span class="db-cell-empty">—</span>
                  {:else if def.type === 'boolean'}
                    <input
                      type="checkbox"
                      checked={val === 'true'}
                      onchange={(e) => handleBoolChange(note.id, def.id, e)}
                    />
                  {:else if def.type === 'select'}
                    <select
                      class="db-cell-input"
                      value={val}
                      onchange={(e) => handleSelectChange(note.id, def.id, e)}
                    >
                      <option value="">—</option>
                      {#each JSON.parse(def.options || '[]') as opt}
                        <option value={opt}>{opt}</option>
                      {/each}
                    </select>
                  {:else if def.type === 'date'}
                    <input
                      type="date"
                      class="db-cell-input"
                      value={val}
                      onchange={(e) => handleCellBlur(note.id, def.id, e)}
                    />
                  {:else if def.type === 'number'}
                    <input
                      type="number"
                      class="db-cell-input"
                      value={val}
                      onblur={(e) => handleCellBlur(note.id, def.id, e)}
                      onkeydown={handleCellKeydown}
                    />
                  {:else}
                    <input
                      type="text"
                      class="db-cell-input"
                      value={val}
                      onblur={(e) => handleCellBlur(note.id, def.id, e)}
                      onkeydown={handleCellKeydown}
                      placeholder="—"
                    />
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
