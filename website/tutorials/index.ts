import type { Page } from "../types";

export const tutorials: Page = {
  path: "tutorials/index.html",
  title: "Tutorials",
  description: "Build complete working Synaps workflows, verify each result, and understand what to clean up when you are done.",
  kind: "tutorial",
  toc: [
    { label: "Recommended order", id: "order" },
    { label: "Every tutorial", id: "all" },
  ],
  body: `
    <h2 id="order">Recommended order</h2>
    <ol class="steps">
      <li><h3><a href="connect/">Install and connect your first tools</a></h3><p>Start from the release archive, install the CLI, connect Codex or Claude Code, and verify all three MCP tools.</p></li>
      <li><h3><a href="continuity/">Carry one decision between tools</a></h3><p>Store a confirmed convention in one tool, end the session, recover it in another, and inspect the exact record.</p></li>
      <li><h3><a href="secrets/">Use a scoped secret in either shell mode</a></h3><p>Create a Keychain-backed value, compare one-command and ambient loading without printing the value, and exercise invalidation.</p></li>
      <li><h3><a href="curate/">Curate and optimize memory</a></h3><p>Add, find, correct, delete, and budget durable memory using both human-readable and JSON CLI output.</p></li>
      <li><h3><a href="recovery/">Export and restore safely</a></h3><p>Create a validated snapshot, make a reversible change, acquire the exclusive lifecycle lock, restore, and verify recovery.</p></li>
    </ol>

    <h2 id="all">What every tutorial includes</h2>
    <p>Each tutorial names prerequisites, builds one complete outcome, includes observable verification, calls out the security boundary, and provides cleanup or the next safe step. Commands assume the installed <code>synaps</code> CLI is on <code>PATH</code>.</p>
    <p>If you need exact command syntax outside a guided workflow, use the <a href="../docs/cli/">CLI reference</a>. If a step fails, stop at that point and use <a href="../docs/troubleshoot/">Troubleshooting</a> rather than skipping a trust or integrity check.</p>
  `,
};
