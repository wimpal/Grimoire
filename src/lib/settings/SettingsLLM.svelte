<script>
  import { invoke } from '@tauri-apps/api/core';

  let {
    keepInMemory = false, onKeepInMemoryChange = () => {},
    chatModel = $bindable('llama3.2'),
    embeddingModel = $bindable('nomic-embed-text'),
    chatTemperature = $bindable(0.8),
    chatTopP = $bindable(0.9),
    chatTopK = $bindable(40),
    chatRepeatPenalty = $bindable(1.1),
    chatNumCtx = $bindable(8192),
    verbosity = $bindable('concise'),
  } = $props();

  function persist(key) {
    return (val) => {
      invoke('set_setting', { key, value: String(val) }).catch(() => {});
    };
  }

  $effect(() => { persist('chat_model')(chatModel); });
  $effect(() => { persist('embedding_model')(embeddingModel); });
  $effect(() => { persist('chat_temperature')(chatTemperature); });
  $effect(() => { persist('chat_top_p')(chatTopP); });
  $effect(() => { persist('chat_top_k')(chatTopK); });
  $effect(() => { persist('chat_repeat_penalty')(chatRepeatPenalty); });
  $effect(() => { persist('chat_num_ctx')(chatNumCtx); });
  $effect(() => { persist('chat_verbosity')(verbosity); });
</script>

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

<h4 class="settings-subsection">Inference parameters</h4>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Temperature</span>
    <span class="setting-desc">Controls randomness. Higher values produce more creative, less predictable responses.</span>
  </div>
  <input type="number" class="setting-num" bind:value={chatTemperature} min="0" max="2" step="0.05" />
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Verbosity</span>
    <span class="setting-desc">Controls how detailed the model's responses are.</span>
  </div>
  <select bind:value={verbosity}>
    <option value="concise">Concise (default)</option>
    <option value="thorough">Thorough</option>
    <option value="caveman">Caveman</option>
  </select>
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Top P</span>
    <span class="setting-desc">Nucleus sampling threshold. Lower values make output more focused.</span>
  </div>
  <input type="number" class="setting-num" bind:value={chatTopP} min="0" max="1" step="0.05" />
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Top K</span>
    <span class="setting-desc">Limits the next token selection to the K most likely candidates. 0 disables it.</span>
  </div>
  <input type="number" class="setting-num" bind:value={chatTopK} min="0" max="200" step="1" />
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Repeat penalty</span>
    <span class="setting-desc">Penalises tokens that have already appeared. Higher values reduce repetition.</span>
  </div>
  <input type="number" class="setting-num" bind:value={chatRepeatPenalty} min="0.5" max="2" step="0.05" />
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Context window</span>
    <span class="setting-desc">Maximum tokens the model can see at once. Higher values use more RAM.</span>
  </div>
  <input type="number" class="setting-num" bind:value={chatNumCtx} min="512" max="131072" step="512" />
</div>
