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
#[derive(Serialize)]
struct OllamaOptions {
    num_thread: usize,
}

impl OllamaOptions {
    fn balanced() -> Self {
        let total = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self { num_thread: (total / 2).max(1) }
    }
}

/// The response body from Ollama. Only the `message` field is needed.
#[derive(Deserialize)]
struct OllamaChatResponse {
    message: ChatMessage,
}

/// Send a chat message to a locally-running Ollama instance and return the
/// assistant's reply. The full `messages` history is forwarded each time so
/// Ollama maintains conversational context.
/// `keep_in_memory`: when true, keep_alive is set to -1 so the model is
/// never unloaded; when false the Ollama default (300s) is used.
#[tauri::command]
pub async fn chat(
    model: String,
    messages: Vec<ChatMessage>,
    keep_in_memory: bool,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    let body = OllamaChatRequest {
        model,
        messages,
        stream: false,
        keep_alive: if keep_in_memory { -1 } else { 300 },
        options: OllamaOptions::balanced(),
    };

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Could not reach Ollama — is it running? ({e})"))?;

    let text = response
        .text()
        .await
        .map_err(|e| format!("Could not read Ollama response: {e}"))?;

    let parsed: OllamaChatResponse = serde_json::from_str(&text)
        .map_err(|e| format!("Unexpected response from Ollama: {e}\nResponse: {text}"))?;

    Ok(parsed.message.content)
}
