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

## License

Grimoire is free software released under the [GNU General Public License v3.0](LICENSE). You are free to use, modify, and distribute it under the terms of that license.

For commercial use cases that cannot comply with the GPL (e.g. embedding Grimoire in a proprietary product), a separate commercial license is available on request.