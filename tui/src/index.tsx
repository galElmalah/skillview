import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { App } from "./app";
import { readCache, resolveBinary } from "./bridge";

async function main() {
  const binary = resolveBinary();

  // Read cache (fast — file read + JSON parse, no scan) BEFORE rendering so
  // the UI mounts with data on cache hit. Cold start mounts immediately with
  // a loader and triggers the scan in an effect.
  const cached = await readCache(binary);

  // Renderer cleanup is critical: without `exitOnCtrlC` + `clearOnShutdown`,
  // Ctrl-C and SIGTERM bypass the alt-screen / raw-mode teardown, and the
  // terminal keeps printing escape sequences after we leave.
  const renderer = await createCliRenderer({
    exitOnCtrlC: true,
    clearOnShutdown: true,
    exitSignals: ["SIGINT", "SIGTERM", "SIGHUP"],
  });
  createRoot(renderer).render(<App binary={binary} initial={cached} />);
}

main().catch((err) => {
  process.stderr.write(`skillview: fatal: ${err?.stack || err}\n`);
  process.exit(1);
});
