// Copyright (C) 2026 Wim Palland
//
// This file is part of Grimoire.
//
// Grimoire is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Grimoire is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Grimoire. If not, see <https://www.gnu.org/licenses/>.

use serde::{Deserialize, Serialize};
use futures::StreamExt;

// ---------------------------------------------------------------------------
// Chat (Ollama)
// ---------------------------------------------------------------------------

/// A single message in a conversation. `role` is "user" or "assistant".
/// Both `Serialize` (to send to Ollama) and `Deserialize` (to receive from the frontend) are needed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// The request body sent to Ollama's /api/chat endpoint.
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    /// Seconds to keep the model loaded after the request finishes.
    /// -1 = keep forever (keep-in-memory setting), 300 = default 5-minute timeout.
    keep_alive: i64,
    options: OllamaOptions,
}

/// Runtime options forwarded to Ollama on every request.
/// `num_thread` caps the number of CPU threads Ollama uses for inference,
/// leaving headroom for the OS and other running applications.
/// The remaining fields are user-configurable inference parameters.
#[derive(Serialize)]
struct OllamaOptions {
    num_thread: usize,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    repeat_penalty: f32,
    num_ctx: i32,
}

impl OllamaOptions {
    fn new(temperature: f32, top_p: f32, top_k: i32, repeat_penalty: f32, num_ctx: i32) -> Self {
        let total = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            num_thread: (total / 2).max(1),
            temperature,
            top_p,
            top_k,
            repeat_penalty,
            num_ctx,
        }
    }
}

/// One line in Ollama's NDJSON streaming response.
/// `done` is true on the final (empty) message that signals end of stream.
#[derive(Deserialize)]
struct OllamaStreamChunk {
    message: ChatMessage,
    done: bool,
}

/// Send a chat message to a locally-running Ollama instance.
/// Tokens are emitted incrementally via the `chat:token` Tauri event as they
/// arrive from Ollama. The command resolves once the stream is complete.
/// `keep_in_memory`: when true, keep_alive is set to -1 so the model is
/// never unloaded; when false the Ollama default (300s) is used.
#[tauri::command(rename_all = "camelCase")]
pub async fn chat(
    app: tauri::AppHandle,
    model: String,
    messages: Vec<ChatMessage>,
    keep_in_memory: bool,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    repeat_penalty: f32,
    num_ctx: i32,
) -> Result<(), String> {
    use tauri::Emitter;

    let client = reqwest::Client::new();

    let body = OllamaChatRequest {
        model,
        messages,
        stream: true,
        keep_alive: if keep_in_memory { -1 } else { 300 },
        options: OllamaOptions::new(temperature, top_p, top_k, repeat_penalty, num_ctx),
    };

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Could not reach Ollama — is it running? ({e})"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {status}: {body}"));
    }

    // Ollama streams NDJSON: one JSON object per line, terminated by a final
    // object with `"done": true`. We buffer bytes into lines and parse each one.
    let mut stream = response.bytes_stream();
    let mut line_buf = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("Stream read error: {e}"))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("UTF-8 error: {e}"))?;

        for ch in text.chars() {
            if ch == '\n' {
                let line = line_buf.trim().to_string();
                line_buf.clear();
                if line.is_empty() { continue; }

                let parsed: OllamaStreamChunk = serde_json::from_str(&line)
                    .map_err(|e| format!("Unexpected Ollama chunk: {e}\nLine: {line}"))?;

                if !parsed.done && !parsed.message.content.is_empty() {
                    app.emit("chat:token", &parsed.message.content)
                        .map_err(|e| format!("Event emit error: {e}"))?;
                }

                if parsed.done {
                    return Ok(());
                }
            } else {
                line_buf.push(ch);
            }
        }
    }

    Ok(())
}
