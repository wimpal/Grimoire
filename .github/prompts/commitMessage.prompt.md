---
description: Generate a short, concise one-line commit message for staged changes
---

You are an expert at writing clean Git commit messages.

Analyze the staged changes provided in `#git:staged` context and generate a single-line commit message.

## Rules
- One line only, no bullet points, no body
- Use imperative mood (e.g. "Add", "Fix", "Update", not "Added" or "Adds")
- Be specific but concise — under 72 characters
- Follow conventional commits format if applicable: `type: description`
  (types: feat, fix, docs, refactor, chore, test, style)

Output ONLY the commit message, nothing else.