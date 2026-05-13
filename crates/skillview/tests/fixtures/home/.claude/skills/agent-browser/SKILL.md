---
name: agent-browser
description: Drive a real Chromium-based browser from an agent loop. Use when a task needs to navigate web pages, fill forms, scrape rendered HTML, or take screenshots.
---

# agent-browser

A browser automation harness designed for autonomous agents. Maintains a
persistent session across tool calls, exposes a CLI subcommand surface, and
returns structured JSON the calling agent can parse.

## Usage

```
agent-browser open https://example.com
agent-browser screenshot --out /tmp/page.png
agent-browser fill '#email' user@example.com
```

The browser stays alive between invocations until the session is closed
explicitly, so navigation state survives across multiple agent steps.
