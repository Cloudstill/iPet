// Derive each tool package's `openai-function.json` from its `tool.json`.
//
// `tool.json` is the single source of truth for a tool's metadata + parameter
// schema. The OpenAI-compatible function-calling envelope (`{type:"function",
// function:{name,description,parameters}}`) is fully determined by it — the
// host app builds the same shape at runtime in `tool_dispatcher::tool_definition`.
// This script regenerates the distributable `openai-function.json` so the two
// never drift.
//
// Run: npm run sync:tools

import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const PACKAGES_DIR = new URL("../tool-packages/", import.meta.url);

async function main() {
  const entries = await readdir(PACKAGES_DIR, { withFileTypes: true });
  const dirs = entries.filter((e) => e.isDirectory()).map((e) => e.name);

  let written = 0;
  for (const dir of dirs) {
    const pkgDir = new URL(join(dir, "/"), PACKAGES_DIR);
    let toolJson;
    try {
      toolJson = JSON.parse(
        await readFile(new URL("tool.json", pkgDir), "utf8"),
      );
    } catch {
      continue; // not a tool package
    }

    // Only builtin tools ship a function envelope today — HTTP tools get their
    // definition built live from the imported config, so there's nothing to
    // pre-bake.
    if (toolJson.kind !== "builtin") continue;

    const envelope = {
      type: "function",
      function: {
        name: toolJson.name,
        description: toolJson.description,
        parameters: toolJson.parameters,
      },
    };

    const out = new URL("openai-function.json", pkgDir);
    await writeFile(out, JSON.stringify(envelope, null, 2) + "\n", "utf8");
    console.log(`synced ${dir}/openai-function.json`);
    written += 1;
  }
  console.log(`done: ${written} function envelope(s) synced.`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
