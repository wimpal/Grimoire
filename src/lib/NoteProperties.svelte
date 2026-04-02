<script>
  /**
   * NoteProperties — collapsible properties panel shown between the note title
   * and the content editor. Displays all property definitions for the note's
   * folder and lets the user view/edit values inline.
   *
   * Props:
   *   noteId           — the active note's id
   *   folderId         — the note's folder_id (properties are folder-scoped)
   *   onPropertiesLoad — optional callback; receives the loaded properties array
   */
  import { invoke } from '@tauri-apps/api/core';

  let { noteId, folderId, onPropertiesLoad = () => {} } = $props();

  let defs = $state([]);
  let propValues = $state([]);
  let open = $state(true);
  let loading = $state(false);

  // "Add property" inline form state
  let adding = $state(false);
  let newName = $state('');
  let newType = $state('text');
  let newOptions = $state(''); // comma-separated, for 'select' type

  // Load defs + values on mount. {#key noteId} in the parent ensures this
  // component is always remounted fresh when the note changes, so there is
  // never a stale render frame showing the previous note's properties.
  $effect(() => {
    if (noteId && folderId) {
      loadProperties(noteId, folderId);
    }
  });

  async function loadProperties(nid, fid) {
    loading = true;
    try {
      const [d, p] = await Promise.all([
        invoke('get_property_defs', { folderId: fid }),
        invoke('get_note_properties', { noteId: nid }),
      ]);
      defs = d;
      propValues = p;
      onPropertiesLoad(p);
    } catch {
      // Non-fatal — just show nothing
    } finally {
      loading = false;
    }
  }

  async function setValue(defId, value) {
    try {
      await invoke('set_note_property', { noteId, defId, value });
      // Update local state immediately
      propValues = propValues.map(p => p.def_id === defId ? { ...p, value } : p);
      onPropertiesLoad(propValues);
    } catch {
      // Silently fail — value will reload next time
    }
  }

  async function addProperty() {
    const name = newName.trim();
    if (!name) return;
    try {
      const options = newType === 'select' && newOptions.trim()
        ? JSON.stringify(newOptions.split(',').map(s => s.trim()).filter(Boolean))
        : null;
      // create_property_def returns the new PropertyDef (including its id).
      const def = await invoke('create_property_def', {
        folderId,
        name,
        type: newType,
        options,
      });
      // Seed an empty note_properties row so THIS note immediately shows the
      // new property. Other notes in the folder are not affected.
      await invoke('set_note_property', { noteId, defId: def.id, value: '' });
      newName = '';
      newType = 'text';
      newOptions = '';
      adding = false;
      await loadProperties(noteId, folderId);
    } catch (e) {
      console.error('Failed to create property:', e);
    }
  }

  async function deleteDef(defId) {
    try {
      await invoke('delete_property_def', { id: defId });
      await loadProperties(noteId, folderId);
    } catch (e) {
      console.error('Failed to delete property:', e);
    }
  }

  function handleBooleanChange(defId, checked) {
    setValue(defId, checked ? 'true' : 'false');
  }
</script>

{#if folderId && (defs.length > 0 || !loading)}
<div class="note-properties">
  <button class="props-toggle" onclick={() => (open = !open)}>
    <span class="props-toggle-icon">{open ? '˅' : '›'}</span>
    <span class="props-toggle-label">Properties</span>
    {#if !open && defs.length > 0}
      <span class="props-count">{defs.length}</span>
    {/if}
  </button>

  {#if open}
    <div class="props-grid">
      {#each propValues as prop (prop.def_id)}
        <div class="prop-row">
          <span class="prop-name">{prop.name}</span>
          <div class="prop-value">
            {#if prop.type === 'text'}
              <input
                type="text"
                value={prop.value}
                onblur={(e) => setValue(prop.def_id, e.currentTarget.value)}
                onkeydown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
                class="prop-input"
                placeholder="—"
              />
            {:else if prop.type === 'number'}
              <input
                type="number"
                value={prop.value}
                onblur={(e) => setValue(prop.def_id, e.currentTarget.value)}
                onkeydown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
                class="prop-input"
                placeholder="—"
              />
            {:else if prop.type === 'date'}
              <input
                type="date"
                value={prop.value}
                onchange={(e) => setValue(prop.def_id, e.currentTarget.value)}
                class="prop-input"
              />
            {:else if prop.type === 'boolean'}
              <label class="prop-checkbox">
                <input
                  type="checkbox"
                  checked={prop.value === 'true'}
                  onchange={(e) => handleBooleanChange(prop.def_id, e.currentTarget.checked)}
                />
              </label>
            {:else if prop.type === 'select'}
              <select
                class="prop-input"
                value={prop.value}
                onchange={(e) => setValue(prop.def_id, e.currentTarget.value)}
              >
                <option value="">—</option>
                {#each JSON.parse(prop.options || '[]') as opt}
                  <option value={opt}>{opt}</option>
                {/each}
              </select>
            {/if}
          </div>
          <button class="prop-delete icon-btn danger" onclick={() => deleteDef(prop.def_id)} title="Remove property">✕</button>
        </div>
      {/each}
    </div>

    {#if adding}
      <div class="prop-add-form">
        <input
          class="prop-input"
          bind:value={newName}
          placeholder="Property name"
          onkeydown={(e) => { if (e.key === 'Enter') addProperty(); if (e.key === 'Escape') adding = false; }}
        />
        <select class="prop-input" bind:value={newType}>
          <option value="text">Text</option>
          <option value="number">Number</option>
          <option value="date">Date</option>
          <option value="boolean">Checkbox</option>
          <option value="select">Select</option>
        </select>
        {#if newType === 'select'}
          <input
            class="prop-input"
            bind:value={newOptions}
            placeholder="Options (comma-separated)"
          />
        {/if}
        <button class="prop-add-btn" onclick={addProperty}>Add</button>
        <button class="prop-cancel-btn" onclick={() => (adding = false)}>Cancel</button>
      </div>
    {:else}
      <button class="prop-add-trigger" onclick={() => (adding = true)}>+ Add property</button>
    {/if}
  {/if}
</div>
{/if}
