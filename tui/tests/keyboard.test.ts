// Keyboard interaction tests: search mode, help overlay, navigation.

import path from "node:path";
import { test, expect } from "@microsoft/tui-test";

const repoRoot = path.resolve(process.cwd(), "..");
const wrapper = path.join(repoRoot, "bin", "skillview");
const fixtureRoot = path.join(repoRoot, "skills");

test.use({
  program: {
    file: wrapper,
    args: ["--tui", "--no-usage", "--root", fixtureRoot],
  },
  env: {
    SKILLVIEW_NO_CACHE: "1",
    PATH: process.env.PATH,
    HOME: process.env.HOME,
  },
  rows: 30,
  columns: 120,
});

test("/ enters search mode with prompt", async ({ terminal }) => {
  // Wait for the inventory to render before sending input.
  await expect(terminal.getByText("skillview-cli")).toBeVisible();
  terminal.write("/");
  // Search mode replaces the footer with a prompt that ends with
  // "esc to clear" — unique enough to confirm the mode change.
  await expect(terminal.getByText(/esc to clear/g)).toBeVisible();
});

test("? opens help overlay", async ({ terminal }) => {
  await expect(terminal.getByText("skillview-cli")).toBeVisible();
  terminal.write("?");
  // The help overlay's title is "skillview · help" — the " · help"
  // substring doesn't appear anywhere in the browse view, so it's a
  // reliable marker that the overlay swapped in.
  await expect(terminal.getByText(/· help/g)).toBeVisible();
});
