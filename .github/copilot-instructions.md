# Copilot Instructions — Grimoire

## Behaviour
- Be honest and direct. Call out mistakes clearly.
- If asked for something but there is a significantly better option, say so before doing it.
- When fixing something, actually fix it. Do not claim something is fixed if it is not.
- Communicate clearly and concisely.

## Teaching & Explanation
- The developer is new to this tech stack (Rust, Tauri, SQLite, LanceDB, Ollama, Svelte). Assume unfamiliarity with concepts unless demonstrated otherwise.
- Before making any non-trivial change, briefly explain:
  1. What you are about to do
  2. Why it is necessary
  3. Any important concepts to understand
- After writing code, point out anything confusing or that will need to be understood to work with it later.
- When introducing a new crate, tool, or pattern for the first time, give a one-sentence explanation of what it is.

## Code Changes
- Read existing files before editing them.
- Make only the changes that are needed. Do not refactor or improve unrelated code.
- If a change touches something risky, explain the risk before proceeding.
- Build one piece of functionality at a time. Do not move on until the current piece is working and confirmed.

## Tracking
- When a new concept is introduced, explain it and add it to `.vscode/Guidelines/Concepts.md`.
- When a roadmap item is completed, mark it as done in `.vscode/Guidelines/ROADMAP.md`.

## Project Context
- Project: **Grimoire** — a local-first note-taking app with built-in LLM assistance.
- Tech stack: Tauri (frontend), Rust (backend), Svelte (UI), SQLite (notes DB), LanceDB (vector/RAG), Ollama (LLM runtime).
- **Privacy is the top priority.** Nothing leaves the machine. All file access is logged.
- **Performance is second.** Keep everything lightweight. LLM only active when needed.
- Refer to `.vscode/Guidelines/` for architecture, roadmap, and behaviour guidelines before suggesting approaches.
