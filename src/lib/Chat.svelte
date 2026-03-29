<script>
  import { invoke } from '@tauri-apps/api/core';

  // ── State ──────────────────────────────────────────────────────────────────

  // Each message: { role: 'user' | 'assistant', content: string }
  let messages = $state([]);
  let input = $state('');
  let model = $state('llama3.2');
  let isLoading = $state(false);
  let error = $state('');
  let useNotes = $state(false);

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

    try {
      // When useNotes is on, search the vector index and inject relevant notes
      // as a system message. The system message is never added to the visible
      // messages array — it's injected per-send so it doesn't grow the history.
      let payload = updated;
      if (useNotes) {
        const matches = await invoke('search_notes', { query: text, limit: 3 }).catch(() => []);
        if (matches.length > 0) {
          const context = matches
            .map(m => `### ${m.title}\n${m.excerpt}`)
            .join('\n\n---\n\n');
          const systemMsg = {
            role: 'system',
            content: `Relevant notes from the user's knowledge base:\n\n${context}\n\nUse these notes to inform your answers where relevant.`,
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

  {#if error}
    <p class="chat-error">{error}</p>
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
