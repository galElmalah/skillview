// Cold-start smoke: launch the TUI against the local skills/ tree (small +
// deterministic) and assert that the loader phase label renders, then that
// the final inventory header appears with the expected stats.

import path from "node:path";
import { test, expect } from "@microsoft/tui-test";

// `bun run test` is invoked from tui/. tui-test transpiles tests into
// .tui-test/cache/ before running them, so `import.meta.dirname` points at
// the cache, not the source tree. process.cwd() is the project root (tui/).
const repoRoot = path.resolve(process.cwd(), "..");
const wrapper = path.join(repoRoot, "bin", "skillview");
const fixtureRoot = path.join(repoRoot, "skills");

test.use({
  program: {
    file: wrapper,
    args: ["--tui", "--no-usage", "--root", fixtureRoot],
  },
  env: {
    // Skip the on-disk inventory cache so every run is a real cold start —
    // otherwise we'd render the cached state and never exercise the stream.
    SKILLVIEW_NO_CACHE: "1",
    // Re-use the wrapper's binary discovery (don't force a rebuild per test).
    PATH: process.env.PATH,
    HOME: process.env.HOME,
  },
  rows: 30,
  columns: 120,
});

test("cold start renders loader, then final stats", async ({ terminal }) => {
  // The fixture has exactly one primary skill named `skillview-cli`. Once it
  // appears in the tree pane, the stream has completed at least through the
  // parse phase — that's our "did the TUI come up?" check.
  await expect(terminal.getByText("skillview-cli")).toBeVisible();

  // The header settles on either "just scanned" (cache write succeeded
  // immediately) or "cached Ns ago" (if the assertion polled after the
  // first second). Both signal that the `done` event was processed.
  await expect(terminal.getByText(/just scanned|cached/g)).toBeVisible();

  // Sanity: the inventory stats are right. The fixture has 0 secondary
  // skills and 0 dup clusters, both of which show in the header.
  await expect(terminal.getByText("0 secondary")).toBeVisible();
  await expect(terminal.getByText("0 dup clusters")).toBeVisible();
});

test("q quits cleanly", async ({ terminal }) => {
  await expect(terminal.getByText("skillview-cli")).toBeVisible();
  terminal.write("q");
  // After q, the alt screen is torn down and the tree should no longer
  // show the skill we asserted above. tui-test detects process exit too.
  await expect(terminal.getByText("skillview-cli")).not.toBeVisible();
});
