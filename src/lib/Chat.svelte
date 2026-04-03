<script>
  import { invoke } from '@tauri-apps/api/core';
  import { untrack, tick } from 'svelte';

  // ── Props ──────────────────────────────────────────────────────────────────

  // The note currently open in the editor, passed down from App.svelte.
  // Shape: { id, title, content, ... } | null
  // pendingInsert: { text: string, seq: number } — injected quote from the editor keybind.
  // keepInMemory: when true, keep_alive: -1 is sent so Ollama never unloads the model.
  let { activeNote = null, pendingInsert = null, keepInMemory = false } = $props();

  // ── State ──────────────────────────────────────────────────────────────────

  // Each message: { role: 'user' | 'assistant', content: string }
  let messages = $state([]);
  let input = $state('');
  let model = $state('llama3.2');
  let isLoading = $state(false);
  let error = $state('');
  let useNotes = $state(true);

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

    try {
      // Build a system message from two optional sources:
      //   1. The note the user currently has open (always injected if present).
      //   2. RAG search results from the vector index (injected when useNotes is on).
      // The system message is never added to the visible messages array — it's
      // injected per-send so it doesn't grow the visible history.
      let payload = updated;
      const systemParts = [];

      // ── 1. Active note ─────────────────────────────────────────────────────
      if (activeNote) {
        systemParts.push(
          `## Note the user currently has open\n### ${activeNote.title}\n${activeNote.content}`
        );
      }

      // ── 2. RAG context ─────────────────────────────────────────────────────
      // Always search even when a note is open — the question may be relevant
      // to other notes beyond the one currently being edited.
      if (useNotes) {
        // Use the current message plus the previous user turn for the RAG query.
        // This gives short follow-ups like "and what about cows?" enough context
        // to find the right note, without diluting the signal when the user
        // switches topics (e.g. "and ducks?" after discussing horses and cows).
        const recentUserMessages = updated
          .filter(m => m.role === 'user')
          .slice(-2)
          .map(m => m.content)
          .join(' ');
        let matches = [];
        try {
          matches = await invoke('search_notes', { query: recentUserMessages });
        } catch (e) {
          // Surface the error so the user knows RAG isn't working rather than
          // silently falling back to the model's generic training data.
          notesError = `Note search failed: ${e}`;
        }
        if (matches.length > 0) {
          // Group chunks by note title. A single note may contribute multiple
          // matching chunks (e.g. two sentences from different parts of the note)
          // — all of them are passed so the model can see every relevant excerpt.
          const byTitle = {};
          for (const m of matches) {
            if (!byTitle[m.title]) byTitle[m.title] = [];
            byTitle[m.title].push(...m.excerpts);
          }
          sourcesUsed = Object.keys(byTitle);
          const context = Object.entries(byTitle)
            .map(([title, excerpts]) => `### ${title}\n${excerpts.join('\n')}`)
            .join('\n\n---\n\n');
          systemParts.push(`## Related notes\n${context}`);
        }
      }

      // ── Assemble system message ────────────────────────────────────────────
      if (systemParts.length > 0) {
        const systemMsg = {
          role: 'system',
          content:
            `You are a personal knowledge assistant for the user.\n\n` +
            systemParts.join('\n\n---\n\n') +
            `\n\nIMPORTANT RULES:\n` +
            `1. Read every note section above carefully before answering.\n` +
            `2. If ANY note contains information that answers the question — even a single sentence — you MUST report it. Do NOT say the user hasn't mentioned something if it appears above.\n` +
            `3. Quote or paraphrase the user's own words when reporting what they've written.\n` +
            `4. Only say the notes don't contain something if you have checked every section and found nothing relevant.`,
        };
        payload = [systemMsg, ...updated];
      }

      // Pass the full conversation history so Ollama keeps context.
      const reply = await invoke('chat', { model, messages: payload, keepInMemory });
      messages = [...messages, { role: 'assistant', content: reply }];
    } catch (e) {
      error = String(e);
    } finally {
      isLoading = false;
    }
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
  </div>

  <div class="chat-messages" bind:this={messagesEl}>
    {#each messages as msg, i (i)}
      <div class="chat-message {msg.role}">
        <p>{msg.content}</p>
      </div>
    {:else}
      <p class="chat-empty">Ask anything. Runs locally via Ollama.</p>
    {/each}

    {#if isLoading}
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
      placeholder="Message… (Enter to send, Shift+Enter for newline)"
      rows="3"
      disabled={isLoading}
    ></textarea>
    <button onclick={send} disabled={isLoading || !input.trim()}>Send</button>
  </div>
</aside>
