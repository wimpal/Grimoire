<script>
  import { invoke } from '@tauri-apps/api/core';

  // ── State ──────────────────────────────────────────────────────────────────

  // Each message: { role: 'user' | 'assistant', content: string }
  let messages = $state([]);
  let input = $state('');
  let model = $state('llama3.2');
  let isLoading = $state(false);
  let error = $state('');
  let useNotes = $state(true);

  // Titles of notes injected as context for the most recent message.
  // Empty when notes search is off, failed, or returned nothing.
  let sourcesUsed = $state([]);
  let notesError = $state('');

  // Reference to the scrollable messages container so we can auto-scroll.
  let messagesEl = $state(null);

  // Scroll to the bottom whenever the messages array changes.
  $effect(() => {
    // Read messages.length to create a dependency on the array.
    if (messages.length && messagesEl) {
      messagesEl.scrollTop = messagesEl.scrollHeight;
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
      // When useNotes is on, search the vector index and inject relevant notes
      // as a system message. The system message is never added to the visible
      // messages array — it's injected per-send so it doesn't grow the history.
      let payload = updated;
      if (useNotes) {
        let matches = [];
        try {
          matches = await invoke('search_notes', { query: text });
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
            byTitle[m.title].push(m.excerpt);
          }
          sourcesUsed = Object.keys(byTitle);
          const context = Object.entries(byTitle)
            .map(([title, excerpts]) => `### ${title}\n${excerpts.join('\n')}`)
            .join('\n\n---\n\n');
          const systemMsg = {
            role: 'system',
            content:
              `You are a personal knowledge assistant. The user's own notes are provided below.\n\n` +
              `${context}\n\n` +
              `Instructions:\n` +
              `- Base your answer on what the user has written in their notes.\n` +
              `- If a note contains the user's opinion or statement about something, report it directly using their words.\n` +
              `- Do not substitute general knowledge for note content. If the notes address the question, the notes come first.\n` +
              `- If the notes do not contain relevant information, say so clearly instead of answering from general knowledge.`,
          };
          payload = [systemMsg, ...updated];
        }
      }

      // Pass the full conversation history so Ollama keeps context.
      const reply = await invoke('chat', { model, messages: payload });
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

  {#if sourcesUsed.length > 0}
    <div class="chat-sources">
      <span class="chat-sources-label">Sources:</span>
      {#each sourcesUsed as title}
        <span class="chat-source-pill">{title}</span>
      {/each}
    </div>
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
      bind:value={input}
      onkeydown={handleKeydown}
      placeholder="Message… (Enter to send, Shift+Enter for newline)"
      rows="3"
      disabled={isLoading}
    ></textarea>
    <button onclick={send} disabled={isLoading || !input.trim()}>Send</button>
  </div>
</aside>
