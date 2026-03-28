# Grimoire

A local-first note-taking app with built-in LLM assistance. Everything runs on your machine — no internet required, no data leaves your device.

## Stack

| Layer | Technology |
|---|---|
| UI | Svelte + Vite |
| Desktop shell | Tauri |
| Backend | Rust |
| Database | SQLite (sqlx) |
| Vector search | LanceDB |
| LLM runtime | Ollama |

## Development

```bash
npm install
npm run tauri dev
```

## Principles

Privacy comes first. Nothing leaves the machine and all file access is logged. Performance comes second — keep it lightweight and only load the LLM when it is needed.

**Why is HMR not preserving my local component state?**

HMR state preservation comes with a number of gotchas! It has been disabled by default in both `svelte-hmr` and `@sveltejs/vite-plugin-svelte` due to its often surprising behavior. You can read the details [here](https://github.com/sveltejs/svelte-hmr/tree/master/packages/svelte-hmr#preservation-of-local-state).

If you have state that's important to retain within a component, consider creating an external store which would not be replaced by HMR.

```js
// store.js
// An extremely simple external store
import { writable } from 'svelte/store'
export default writable(0)
```
