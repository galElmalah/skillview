// tui-test config for skillview's OpenTUI frontend.
//
// Tests spawn `./bin/skillview --tui` (the dev wrapper, which (re)builds the
// Rust binary on demand) and assert against the rendered screen via a PTY.
// We point `--root` at the local `./skills` tree so each test scan is small
// and predictable — running against $HOME would be slow and flaky.

import { defineConfig } from "@microsoft/tui-test";

export default defineConfig({
  // Each test gets a fresh PTY; cap at 60s so a broken loop doesn't hang CI.
  timeout: 60_000,
  // 5s for individual `expect(...).toBeVisible()` polls. Cold-start has to
  // (re)build Rust the first time, so the first test pays for `cargo build`
  // before this kicks in via the wrapper's own waiting.
  expect: { timeout: 8_000 },
  retries: 0,
  reporter: "list",
});
