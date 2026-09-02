import type { Page } from "../types";

export const tutorials: Page = {
  path: "tutorials/index.html",
  title: "Tutorials",
  description:
    "Eleven guided walkthroughs arranged by how far into Synapse you are, from a first connection to running agent teams and removing everything cleanly.",
  kind: "tutorial",
  toc: [
    { label: "Newcomer", id: "newcomer" },
    { label: "Daily driver", id: "daily" },
    { label: "Team operator", id: "operator" },
    { label: "Maintainer", id: "maintainer" },
    { label: "What every tutorial includes", id: "all" },
  ],
  body: `
    <p>These are arranged by how far into Synapse you are, not by feature. Each level assumes the one before it, and each tutorial ends by pointing at the next. If you are new, start at the top and stop whenever Synapse is doing what you need — most people never leave the second level, and that is a complete way to use it.</p>

    <h2 id="newcomer">Level 1 · Newcomer</h2>
    <p>You have downloaded Synapse and want one tool remembering things. Roughly an hour for both.</p>
    <ol class="steps">
      <li><h3><a href="connect/">Install and connect your first tools</a></h3><p>Start from the release archive, install the CLI, connect Codex, Claude Code, or pi, and verify all three MCP tools from the app, the terminal, and the tool itself.</p></li>
      <li><h3><a href="continuity/">Carry one decision between tools</a></h3><p>Store a confirmed convention in one tool, end the session, recover it in another, and inspect the exact record that made it across.</p></li>
    </ol>

    <h2 id="daily">Level 2 · Daily driver</h2>
    <p>Synapse is connected and you use it every day. This level is about controlling what it remembers, what it can hand to a command, and what your tools know how to do.</p>
    <ol class="steps">
      <li><h3><a href="curate/">Curate and optimize memory</a></h3><p>Add, find, correct, delete, and budget durable memory using both human-readable and JSON output. The one to read if recall is returning too much or the wrong thing.</p></li>
      <li><h3><a href="secrets/">Use a scoped secret in either shell mode</a></h3><p>Create a vaulted value, compare one-command and ambient loading without printing the value, and watch trust invalidate when the file changes.</p></li>
      <li><h3><a href="skills/">Keep one skill library across every tool</a></h3><p>Write an Agent Skill once, install it into every connected tool, and see how Synapse tells a library that moved on from a copy somebody edited by hand.</p></li>
      <li><h3><a href="learn/">Let your agents write skills</a></h3><p>Let a session write down a procedure it worked out, prove for yourself that it reaches no tool until you approve it, then correct it and take the correction back.</p></li>
    </ol>

    <h2 id="operator">Level 3 · Team operator</h2>
    <p>You want several agents working at once on one job. This is the deepest level, and the one where agents touch real files on your machine — use a checkout you can restore.</p>
    <ol class="steps">
      <li><h3><a href="launch/">Start a tool with everything in place</a></h3><p>Open a tool with memory, scoped credentials, and the project root already wired, without writing anything into that tool's own configuration.</p></li>
      <li><h3><a href="mesh/">Run a team of agents and drive it yourself</a></h3><p>Turn on the mesh, open a team, read what every agent is doing from one terminal, answer a worker that gets stuck, and shut it all down cleanly.</p></li>
      <li><h3><a href="overseer/">Hand a job to one agent and watch it grow a team</a></h3><p>Describe an outcome instead of picking a roster, answer the question only you can answer, and reach a worker directly without going through the agent that started it.</p></li>
    </ol>

    <h2 id="maintainer">Level 4 · Maintainer</h2>
    <p>You are responsible for the store staying sound, or you are handing the machine on. Read these before you need them.</p>
    <ol class="steps">
      <li><h3><a href="recovery/">Export and restore safely</a></h3><p>Create a validated snapshot, make a reversible change, acquire the exclusive lifecycle lock, restore, and verify recovery.</p></li>
      <li><h3><a href="lifecycle/">Check, migrate, and remove Synapse</a></h3><p>Read a full health report, understand what a schema migration does, disconnect one tool, and remove everything while keeping your memory and your own files.</p></li>
    </ol>

    <h2 id="all">What every tutorial includes</h2>
    <p>Each one names its prerequisites, builds one complete outcome, includes verification you can observe rather than assume, calls out the security boundary it crosses, and ends with cleanup or the next safe step. Commands assume the installed <code>synapse</code> CLI is on <code>PATH</code>.</p>
    <p>If you need exact command syntax outside a guided workflow, use the <a href="../docs/cli/">CLI reference</a>. If a step fails, stop there and use <a href="../docs/troubleshoot/">Troubleshooting</a> rather than skipping a trust or integrity check — those checks are the reason the destructive steps later are safe.</p>
  `,
};
