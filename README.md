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