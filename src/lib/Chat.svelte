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
  import { listen } from '@tauri-apps/api/event';
  import { untrack, tick } from 'svelte';
  import { FEATURE_GUIDE } from './utils/featureGuide.js';

  // ── Props ──────────────────────────────────────────────────────────────────

  // The note currently open in the editor, passed down from App.svelte.
  // Shape: { id, title, content, ... } | null
  // pendingInsert: { text: string, seq: number } — injected quote from the editor keybind.
  // keepInMemory: when true, keep_alive: -1 is sent so Ollama never unloads the model.
  // llmEnabled: false disables the chat UI and shows a hardware warning banner.
  let { activeNote = null, pendingInsert = null, keepInMemory = false, llmEnabled = true, onClose = null, onContextMenu = null, onInsertIntoNote = null, activeView = null, activeViewFolderId = null, activeViewLabel = '', activeViewFilters = {} } = $props();

  // ── State ──────────────────────────────────────────────────────────────────

  // ── Chat input placeholder ──────────────────────────────────────────────────

  const PLACEHOLDERS = [
    'Consult the grimoire…',
    'How to cast a fireball?',
    'What are the ingredients for a healing potion?',
    'Translate this ancient rune…',
    'Ask the oracle…',
    'Summon an answer from the void…',
    'Which spell works best against undead?',
    'Where did I write about the lost city?',
    'What does my future hold? (Ask about your notes)',
    'Speak, mortal…',
  ];

  const inputPlaceholder = PLACEHOLDERS[Math.floor(Math.random() * PLACEHOLDERS.length)];

  // Each message: { role: 'user' | 'assistant', content: string }
  let messages = $state([]);
  let input = $state('');
  let model = $state('llama3.2');
  let isLoading = $state(false);
  let error = $state('');
  let useNotes = $state(true);
  let useViewContext = $state(true);
  let useFeatureGuide = $state(true);

  $effect(() => {
    localStorage.setItem('grimoire:chat:useViewContext', JSON.stringify(useViewContext));
  });
  $effect(() => {
    const saved = localStorage.getItem('grimoire:chat:useViewContext');
    if (saved !== null) useViewContext = JSON.parse(saved);
  });
  $effect(() => {
    localStorage.setItem('grimoire:chat:useFeatureGuide', JSON.stringify(useFeatureGuide));
  });
  $effect(() => {
    const saved = localStorage.getItem('grimoire:chat:useFeatureGuide');
    if (saved !== null) useFeatureGuide = JSON.parse(saved);
  });

  // Load chat model from SQLite on mount.
  $effect(() => {
    invoke('get_setting', { key: 'chat_model' })
      .then(val => { if (val !== '') model = val; })
      .catch(() => {});
  });

  // Persist chat model to SQLite when changed.
  $effect(() => {
    invoke('set_setting', { key: 'chat_model', value: model }).catch(() => {});
  });

  // Reference to the chat textarea so we can focus it after injection.
  let inputEl = $state(null);

  // Watch for new injections from the editor keybind.
  $effect(() => {
    if (!pendingInsert) return;
    // Format the selection as a blockquote.
    const quoted = pendingInsert.text
      .split('\n')
      .map(line => `> ${line}`)
      .join('\n');
    // Read input without tracking it as a dependency — avoids an infinite loop
    // where writing input would re-trigger this effect.
    const current = untrack(() => input);
    input = current ? `${current}\n\n${quoted}\n` : `${quoted}\n`;
    inputEl?.focus();
  });

  // Titles of notes injected as context for the most recent message.
  // Empty when notes search is off, failed, or returned nothing.
  let sourcesUsed = $state([]);
  let notesError = $state('');

  // Reference to the scrollable messages container so we can auto-scroll.
  let messagesEl = $state(null);

  // Scroll to the bottom whenever the messages array changes.
  // tick() waits for Svelte to finish updating the DOM before measuring scrollHeight,
  // otherwise we'd scroll to the pre-update height and land one message short.
  $effect(() => {
    if (messages.length && messagesEl) {
      tick().then(() => {
        messagesEl.scrollTop = messagesEl.scrollHeight;
      });
    }
  });

  // ── Actions ────────────────────────────────────────────────────────────────

  // ── Filter helper (mirrored from DatabaseView) ──────────────────────────────

  function applyFilters(rows, filters, defs) {
    const active = Object.entries(filters).filter(([, f]) => f && !isFilterEmpty(f));
    if (active.length === 0) return rows;
    return rows.filter(note => {
      return active.every(([defId, f]) => {
        const def = defs.find(d => d.id === Number(defId));
        if (!def) return true;
        const prop = note.properties.find(p => p.def_id === Number(defId));
        const val = prop?.value ?? null;
        if (def.type === 'text') {
          if (f.op === 'is empty')     return val === null || val === '';
          if (f.op === 'is not empty') return val !== null && val !== '';
          if (val === null) return false;
          if (f.op === 'contains') return val.toLowerCase().includes(f.value.toLowerCase());
          if (f.op === 'equals')   return val.toLowerCase() === f.value.toLowerCase();
        }
        if (def.type === 'number') {
          if (val === null || val === '') return false;
          const n = parseFloat(val);
          const v = parseFloat(f.value);
          if (isNaN(n) || isNaN(v)) return false;
          if (f.op === '=')  return n === v;
          if (f.op === '≠')  return n !== v;
          if (f.op === '>')  return n > v;
          if (f.op === '<')  return n < v;
          if (f.op === '≥')  return n >= v;
          if (f.op === '≤')  return n <= v;
        }
        if (def.type === 'date') {
          if (val === null || val === '') return false;
          if (f.op === 'on')     return val === f.value;
          if (f.op === 'before') return val < f.value;
          if (f.op === 'after')  return val > f.value;
          if (f.op === 'between') {
            const [from, to] = Array.isArray(f.value) ? f.value : ['', ''];
            return val >= from && val <= to;
          }
        }
        if (def.type === 'boolean') {
          const b = val === 'true';
          if (f.op === 'is true')  return b === true;
          if (f.op === 'is false') return b === false;
        }
        if (def.type === 'select') {
          const selected = Array.isArray(f.value) ? f.value : [];
          if (selected.length === 0) return true;
          if (f.op === 'any of') return selected.includes(val ?? '');
          if (f.op === 'none of') return !selected.includes(val ?? '');
        }
        return true;
      });
    });
  }

  function isFilterEmpty(f) {
    if (!f || !f.op) return true;
    const { op, value } = f;
    if (op === 'is empty' || op === 'is not empty' || op === 'is true' || op === 'is false') return false;
    if (Array.isArray(value)) return value.every(v => v === '');
    return value === '';
  }

  // Builds the system prompt, pushes a placeholder assistant message, and
  // streams the response. `history` must be the full message list ending with
  // a user message — it is never mutated.
  async function streamResponse(history, params = {}) {
    const {
      temperature = 0.8,
      top_p = 0.9,
      top_k = 40,
      repeat_penalty = 1.1,
      num_ctx = 8192,
      verbosity = 'concise',
    } = params;
    let payload = history;
    const systemParts = [];

    // ── 1. Active note ───────────────────────────────────────────────────────
    if (activeNote) {
      systemParts.push(
        `## Note the user currently has open\n### ${activeNote.title}\n${activeNote.content}`
      );
    }

    // ── 1b. Board/table context ──────────────────────────────────────────────
    if (useViewContext && activeView && activeViewFolderId != null) {
      try {
        const [defs, notes] = await Promise.all([
          invoke('get_property_defs', { folderId: activeViewFolderId }),
          invoke('list_notes_with_properties', { folderId: activeViewFolderId }),
        ]);

        const contextLines = [`## ${activeViewLabel}`];

        // Property schema
        contextLines.push('Property definitions:');
        for (const def of defs) {
          let desc = `${def.name} (${def.type})`;
          if (def.type === 'select' && def.options) {
            try {
              const opts = JSON.parse(def.options).join(', ');
              desc += ` — options: ${opts}`;
            } catch {}
          }
          contextLines.push(`- ${desc}`);
        }

        // Apply table filters if any
        let visibleNotes = notes;
        if (activeView === 'database' && Object.keys(activeViewFilters).length > 0) {
          visibleNotes = applyFilters(notes, activeViewFilters, defs);
        }

        const MAX_NOTES = 50;
        let total = 0;

        if (activeView === 'kanban') {
          const selectDef = defs.find(d => d.type === 'select');
          if (selectDef) {
            let options = [];
            try { options = JSON.parse(selectDef.options ?? '[]'); } catch { options = []; }
            const groups = {};
            for (const opt of options) groups[opt] = [];
            groups['__unset__'] = [];

            for (const note of visibleNotes) {
              const prop = note.properties.find(p => p.def_id === selectDef.id);
              const val = prop?.value?.trim() || '';
              const key = val === '' ? '__unset__' : val;
              if (!groups[key]) groups[key] = [];
              groups[key].push(note);
            }

            for (const [colKey, colNotes] of Object.entries(groups)) {
              if (total >= MAX_NOTES) break;
              const label = colKey === '__unset__' ? 'Unset' : colKey;
              const remaining = MAX_NOTES - total;
              const shown = colNotes.slice(0, remaining);
              contextLines.push(`\n### ${label} (${colNotes.length})`);
              for (const note of shown) {
                const props = note.properties
                  .filter(p => p.value?.trim())
                  .map(p => `${p.name}: ${p.value}`)
                  .join(', ');
                contextLines.push(`- ${note.title}${props ? ` [${props}]` : ''}`);
              }
              total += shown.length;
            }
            const hidden = notes.length - total;
            if (hidden > 0) contextLines.push(`\n... and ${hidden} more cards not shown`);
          }
        } else if (activeView === 'database') {
          for (const note of visibleNotes) {
            if (total >= MAX_NOTES) {
              contextLines.push(`\n... and ${visibleNotes.length - total} more rows not shown`);
              break;
            }
            const props = note.properties
              .filter(p => p.value?.trim())
              .map(p => `${p.name}: ${p.value}`)
              .join(', ');
            contextLines.push(`- **${note.title}**${props ? ` — ${props}` : ''}`);
            total++;
          }
        }

        systemParts.push(contextLines.join('\n'));
      } catch (e) {
        // Silently fail — don't interrupt chat if context fetch fails
      }
    }

    // ── 1c. Feature guide ───────────────────────────────────────────────────
    if (useFeatureGuide) {
      systemParts.push(FEATURE_GUIDE);
    }

    // ── 1d. Verbosity instruction ────────────────────────────────────────────
    // Appended after the INSTRUCTIONS block (below) so it takes highest priority.

    // ── 2. RAG context ───────────────────────────────────────────────────────
    // Always search even when a note is open — the question may be relevant
    // to other notes beyond the one currently being edited.
    if (useNotes) {
      // Use the current message plus the previous user turn for the RAG query.
      // Strip meta-question framing ("what have I written about X?", "tell me about X")
      // so the semantic search matches topic content rather than the question phrasing.
      const recentUserMessages = history
        .filter(m => m.role === 'user')
        .slice(-2)
        .map(m => m.content)
        .join(' ');
      const ragQuery = recentUserMessages
        .replace(/what (have i|did i|do i have) (written?|noted?|said?) (about|on|regarding)\s*/gi, '')
        .replace(/tell me (about|what i (wrote|know|noted) about)\s*/gi, '')
        .replace(/what (are|is) my (notes?|thoughts?) (on|about)\s*/gi, '')
        .replace(/show me (my notes? on|what i wrote about)\s*/gi, '')
        .trim() || recentUserMessages;
      let matches = [];
      try {
        matches = await invoke('search_notes', { query: ragQuery });
      } catch (e) {
        notesError = `Note search failed: ${e}`;
      }
      if (matches.length > 0) {
        const byTitle = {};
        for (const m of matches) {
          if (!byTitle[m.title]) byTitle[m.title] = [];
          byTitle[m.title].push(...m.excerpts);
        }
        sourcesUsed = Object.keys(byTitle);
        const context = Object.entries(byTitle)
          .map(([title, excerpts]) => `NOTE: "${title}"\nCONTENT:\n${excerpts.join('\n')}\nEND NOTE`)
          .join('\n\n');
        systemParts.push(`SEARCH RESULTS:\n${context}`);
      }
    }

    // ── Assemble system message ──────────────────────────────────────────────
    if (systemParts.length > 0) {
      const preamble =
        `You are a personal knowledge assistant embedded in Grimoire, a local note-taking app.\n` +
        `The user's notes are stored in the app. When the user asks "what have I written about X" or ` +
        `"tell me about X", they are asking you to retrieve and summarise information from their own notes.\n` +
        `You also have access to a feature guide that documents Grimoire's keyboard shortcuts and features.\n` +
        `You do NOT have general world knowledge to draw on — every answer must come only from the notes or feature guide provided below. Never guess or use outside knowledge.`;

      let content;
      if (verbosity === 'caveman') {
        content =
          `${preamble}\n\n` +
          systemParts.join('\n\n') +
          `\n\nINSTRUCTIONS:\n` +
          `- Use ONLY the notes above.\n` +
          `- Read the note CONTENT and compress the key facts into telegraphic bullet points — no full sentences, no filler words, no articles.\n` +
          `- Example good answer: "• metabolic process • microbes convert sugars → acids/gas/alcohol • lactic: yoghurt, kimchi • alcoholic: beer, wine → ethanol+CO2"\n` +
          `- Never just list the note title. Always give the actual content.\n` +
          `- If notes contain nothing relevant: "not in notes".\n` +
          `- No speculation. No "According to". Ignore [[ ]] and **.`;
      } else {
        let styleInstruction = '';
        if (verbosity === 'thorough') {
          styleInstruction = '\n\nSTYLE: Provide thorough, detailed answers with full context. Do not skip nuance.';
        }
        content =
          `${preamble}\n\n` +
          `Below are excerpts from the user's notes that are relevant to their question.\n` +
          `Each result is a note with its title and content. Read the content of each note carefully.\n\n` +
          systemParts.join('\n\n') +
          `\n\nINSTRUCTIONS:\n` +
          `1. Answer the user's question using ONLY the information from the notes above.\n` +
          `2. If a note contains information that answers the question, use that information in your answer.\n` +
          `3. If the notes do NOT contain information to answer the question, say so directly. Do not speculate or make indirect connections.\n` +
          `4. Quote or paraphrase from the note content when possible.\n` +
          `5. Ignore any formatting like brackets [[ ]] or asterisks **. Focus on the text.` +
          styleInstruction;
      }
      payload = [{ role: 'system', content }, ...history];
    }

    // Push a placeholder assistant message filled in token by token.
    messages = [...history, { role: 'assistant', content: '' }];

    const unlisten = await listen('chat:token', (event) => {
      messages = messages.map((m, i) =>
        i === messages.length - 1
          ? { ...m, content: m.content + event.payload }
          : m
      );
    });

    try {
      await invoke('chat', { model, messages: payload, keepInMemory, temperature, topP: top_p, topK: top_k, repeatPenalty: repeat_penalty, numCtx: num_ctx });
    } finally {
      unlisten();
    }
  }

  async function send() {
    const text = input.trim();
    if (!text || isLoading) return;

    // Append the user message before the await so the UI updates immediately.
    const updated = [...messages, { role: 'user', content: text }];
    messages = updated;
    input = '';
    isLoading = true;
    error = '';
    sourcesUsed = [];
    notesError = '';

    // Load inference params from settings so changes take effect immediately.
    let params = {};
    try {
      const [temperature, top_p, top_k, repeat_penalty, num_ctx, verbosity] = await Promise.all([
        invoke('get_setting', { key: 'chat_temperature' }),
        invoke('get_setting', { key: 'chat_top_p' }),
        invoke('get_setting', { key: 'chat_top_k' }),
        invoke('get_setting', { key: 'chat_repeat_penalty' }),
        invoke('get_setting', { key: 'chat_num_ctx' }),
        invoke('get_setting', { key: 'chat_verbosity' }),
      ]);
      params = {
        temperature: temperature !== '' ? parseFloat(temperature) : 0.8,
        top_p:       top_p !== ''       ? parseFloat(top_p)       : 0.9,
        top_k:       top_k !== ''       ? parseInt(top_k, 10)     : 40,
        repeat_penalty: repeat_penalty !== '' ? parseFloat(repeat_penalty) : 1.1,
        num_ctx:     num_ctx !== ''     ? parseInt(num_ctx, 10)   : 8192,
        verbosity:   verbosity !== ''   ? verbosity               : 'concise',
      };
    } catch {
      params = {};
    }

    try {
      await streamResponse(updated, params);
    } catch (e) {
      // Remove the empty placeholder if the request failed before any tokens arrived.
      if (messages.length > 0 && messages[messages.length - 1].role === 'assistant'
          && messages[messages.length - 1].content === '') {
        messages = messages.slice(0, -1);
      }
      error = String(e);
    } finally {
      isLoading = false;
    }
  }

  function deleteMessage(index) {
    if (isLoading) return;
    messages = messages.filter((_, i) => i !== index);
  }

  async function regenerate() {
    if (isLoading) return;
    // Strip the last assistant message so we end on a user message.
    const history = messages[messages.length - 1]?.role === 'assistant'
      ? messages.slice(0, -1)
      : messages;
    if (history.length === 0 || history[history.length - 1]?.role !== 'user') return;

    isLoading = true;
    error = '';
    sourcesUsed = [];
    notesError = '';

    // Load inference params from settings (same as send()).
    let params = {};
    try {
      const [temperature, top_p, top_k, repeat_penalty, num_ctx, verbosity] = await Promise.all([
        invoke('get_setting', { key: 'chat_temperature' }),
        invoke('get_setting', { key: 'chat_top_p' }),
        invoke('get_setting', { key: 'chat_top_k' }),
        invoke('get_setting', { key: 'chat_repeat_penalty' }),
        invoke('get_setting', { key: 'chat_num_ctx' }),
        invoke('get_setting', { key: 'chat_verbosity' }),
      ]);
      params = {
        temperature: temperature !== '' ? parseFloat(temperature) : 0.8,
        top_p:       top_p !== ''       ? parseFloat(top_p)       : 0.9,
        top_k:       top_k !== ''       ? parseInt(top_k, 10)     : 40,
        repeat_penalty: repeat_penalty !== '' ? parseFloat(repeat_penalty) : 1.1,
        num_ctx:     num_ctx !== ''     ? parseInt(num_ctx, 10)   : 8192,
        verbosity:   verbosity !== ''   ? verbosity               : 'concise',
      };
    } catch {
      params = {};
    }

    try {
      await streamResponse(history, params);
    } catch (e) {
      if (messages.length > 0 && messages[messages.length - 1].role === 'assistant'
          && messages[messages.length - 1].content === '') {
        messages = messages.slice(0, -1);
      }
      error = String(e);
    } finally {
      isLoading = false;
    }
  }

  // ── Context menu ────────────────────────────────────────────────────────────

  function handleMessageContextMenu(e, msg, i) {
    e.preventDefault();
    const isLastAssistant = i === messages.length - 1 && msg.role === 'assistant';

    const items = [
      {
        label: 'Copy',
        action: () => navigator.clipboard.writeText(msg.content),
      },
      {
        label: 'Copy as quote',
        action: () => navigator.clipboard.writeText(`"${msg.content}"`),
      },
      ...(onInsertIntoNote ? [
        { divider: true },
        {
          label: 'Insert into note',
          action: () => onInsertIntoNote(msg.content),
        },
      ] : []),
      { divider: true },
      ...(isLastAssistant ? [{
        label: 'Regenerate',
        disabled: isLoading,
        action: regenerate,
      }] : []),
      {
        label: 'Delete',
        danger: true,
        disabled: isLoading,
        action: () => deleteMessage(i),
      },
    ];

    onContextMenu?.(e.clientX, e.clientY, items);
  }

  // ── Debug search ───────────────────────────────────────────────────────────

  let debugQuery = $state('');
  let debugResults = $state([]);
  let debugOpen = $state(false);

  async function runDebugSearch() {
    const q = debugQuery.trim();
    if (!q) return;
    try {
      debugResults = await invoke('debug_search', { query: q });
      debugOpen = true;
    } catch (e) {
      debugResults = [{ title: 'Error', excerpt: String(e), distance: -1 }];
      debugOpen = true;
    }
  }

  function handleKeydown(e) {
    // Enter sends; Shift+Enter inserts a newline.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<aside class="chat-panel">
  {#if !llmEnabled}
    <div class="chat-hw-banner">
      <strong>LLM features unavailable.</strong>
      Your hardware doesn’t meet the minimum requirements for chat. You can override this in
      <strong>Settings → Hardware</strong>.
    </div>
  {/if}

  <div class="chat-header">
    <span class="chat-title">Chat</span>
    <input
      class="model-input"
      bind:value={model}
      placeholder="model"
      title="Ollama model name (e.g. llama3.2)"
    />
    <label class="notes-toggle" title="Search your notes and inject the most relevant ones as context before each message (requires nomic-embed-text)">
      <input type="checkbox" bind:checked={useNotes} />
      Use notes
    </label>
    <label class="notes-toggle view-context-toggle" title="Include the current board or table view state as context for the LLM" class:disabled={!activeView}>
      <input type="checkbox" bind:checked={useViewContext} disabled={!activeView} />
      Use view
    </label>
    <label class="notes-toggle" title="Include a feature guide describing Grimoire keyboard shortcuts, view types, and commands">
      <input type="checkbox" bind:checked={useFeatureGuide} />
      Use guide
    </label>
    {#if onClose}
      <button class="chat-close-btn" onclick={onClose} aria-label="Close chat">✕</button>
    {/if}
  </div>

  <div class="chat-messages" role="log" aria-live="polite" aria-atomic="false" bind:this={messagesEl}>
    {#each messages as msg, i (i)}
      {#if msg.role !== 'assistant' || msg.content !== ''}
        <div class="chat-message {msg.role}" role="listitem" oncontextmenu={(e) => handleMessageContextMenu(e, msg, i)}>
          <p>{msg.content}</p>
        </div>
      {/if}
    {:else}
      <p class="chat-empty">Consult the grimoire.</p>
    {/each}

    {#if isLoading && (messages.length === 0 || messages[messages.length - 1]?.role !== 'assistant' || messages[messages.length - 1]?.content === '')}
      <div class="chat-message assistant loading">
        <p>Thinking…</p>
      </div>
    {/if}
  </div>

  {#if notesError}
    <p class="chat-error">{notesError}</p>
  {/if}

  {#if error}
    <p class="chat-error">{error}</p>
  {/if}

  {#if sourcesUsed.length > 0 && !isLoading}
    <details class="chat-sources">
      <summary class="chat-sources-summary">Sources ({sourcesUsed.length})</summary>
      <div class="chat-sources-pills">
        {#each sourcesUsed as title}
          <span class="chat-source-pill">{title}</span>
        {/each}
      </div>
    </details>
  {/if}

  {#if import.meta.env.DEV}
  <details class="debug-search" bind:open={debugOpen}>
    <summary>Debug: raw scores</summary>
    <div class="debug-input-row">
      <input bind:value={debugQuery} placeholder="query…" onkeydown={e => e.key === 'Enter' && runDebugSearch()} />
      <button onclick={runDebugSearch}>Search</button>
    </div>
    {#if debugResults.length > 0}
      <table class="debug-table">
        <thead><tr><th>dist</th><th>title</th><th>excerpt</th></tr></thead>
        <tbody>
          {#each debugResults as r}
            <tr class:debug-pass={r.distance <= 1.1} class:debug-fail={r.distance > 1.1}>
              <td>{r.distance.toFixed(3)}</td>
              <td>{r.title}</td>
              <td>{r.excerpt}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </details>
  {/if}

  <div class="chat-input-row">
    <textarea
      bind:this={inputEl}
      bind:value={input}
      onkeydown={handleKeydown}
      placeholder={inputPlaceholder}
      aria-label="Message"
      rows="6"
      disabled={isLoading || !llmEnabled}
    ></textarea>
    <button onclick={send} disabled={isLoading || !input.trim() || !llmEnabled} aria-busy={isLoading}>Send</button>
  </div>
</aside>
