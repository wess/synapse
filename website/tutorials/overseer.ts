import { code, note } from "../markup";
import type { Page } from "../types";

export const overseer: Page = {
  path: "tutorials/overseer/index.html",
  title: "Hand a job to one agent and watch it grow a team",
  description:
    "Start a single overseer, describe an outcome instead of picking a roster, answer it when it asks, and see why nothing sits between you and the workers it starts.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Start one agent", id: "start" },
    { label: "Describe an outcome", id: "describe" },
    { label: "Answer it", id: "answer" },
    { label: "Reach past it", id: "past" },
    { label: "Do it from the app", id: "app" },
    { label: "Bound the fan-out", id: "bound" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will start one agent, give it a goal rather than a task list, and let it decide whether the work needs a team at all. Along the way you will answer a question it cannot answer itself, and message a worker it started without going through it — which is the difference between this and every other way of running agents.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and on <code>PATH</code>, with the mesh on: <code>synapse settings mesh on</code>.</li>
      <li>Claude Code installed and signed in. The agents run under whatever account you already have — Synapse holds no model credential and makes no model call.</li>
      <li>A checkout you can afford to have edited. Agents here touch real files.</li>
      <li>About thirty minutes.</li>
    </ul>
    <p>If the mesh is new to you, read <a href="../mesh/">Run a team of agents and drive it yourself</a> first. This tutorial is the same machinery with one agent instead of four.</p>

    <ol class="steps">
      <li>
        <h2 id="start">Start one agent, not a roster</h2>
        <p>The other teams ask you to choose a shape before you understand the job. <code>overseer</code> is one member:</p>
        ${code("shell", `cd ~/code/your-project
synapse relay team show overseer
synapse mux --team overseer`)}
        <p>Two things happened. An agent started in the background under the <code>overseer</code> role, and <em>you</em> joined the mesh under your login name. Check both:</p>
        ${code("shell", `/agents`)}
        <p>You should see two rows: yourself, marked as a person, and <code>overseer</code>. The mark matters — every agent on the mesh is told to ask a human row questions and never delegate work to one.</p>
      </li>

      <li>
        <h2 id="describe">Describe an outcome</h2>
        <p>Say what you want to be true, not which steps to run:</p>
        ${code("text", `@overseer the release notes for the next version are missing. Work out what changed since the last tag and write them.`)}
        <p>Watch what it does before it does anything:</p>
        ${code("shell", `/agents`)}
        <p>Its note is the interesting part. An overseer's brief tells it to weigh doing the work itself against splitting it, because every worker it starts is a separate session on the same account you are paying for. A small job stays one agent. A job with two genuinely parallel halves gets a second.</p>
        ${note("Spawning is not free, and it knows that", "The role says so in as many words: spawn for work two agents can do at the same time, not to look busy, and stop one whose piece is finished rather than leaving it idling.")}
      </li>

      <li>
        <h2 id="answer">Answer it when it asks</h2>
        <p>Sooner or later it will reach something only you can settle — which version number, whether a change is worth mentioning, whether to touch a file it is unsure about. It will send you the question and report itself blocked:</p>
        ${code("shell", `/agents`)}
        <p>A row reading <code>blocked</code> with a note is an agent waiting on you. Reply to it like anything else:</p>
        ${code("text", `@overseer call it 2.4.0, and leave the vendored files out entirely`)}
        <p>This is the whole reason a person belongs on the roster. A headless worker runs with its permission prompts bypassed, so without somebody to address, its only option at a fork is to guess.</p>
      </li>

      <li>
        <h2 id="past">Reach past it</h2>
        <p>If the overseer started workers, they are on the roster too — and you can address them directly:</p>
        ${code("shell", `/agents
/log notes`)}
        ${code("text", `@notes skip the dependency bumps, they are noise`)}
        <p>Nothing was relayed. You did not interrupt the overseer's context to correct one of its workers, and it did not have to spend a turn passing the message along.</p>
        ${note("Why that matters more than it sounds", "A lead that relays for you pays its own context on every message and becomes a bottleneck for a job it is not doing. The overseer is addressable, never interposed — which is the same reason <code>synapse mux</code> exists at all.")}
      </li>

      <li>
        <h2 id="app">Do the same from the app</h2>
        <p>Open Synapse and click <strong>Console</strong>. It is the same seat: a transcript of everything said, what the mesh is doing, the roster, and a box to type in. Opening it registers you under the same login name, so an agent looking for somebody to ask finds one row rather than two.</p>
        <p>Click an agent in the roster to aim a bare line at it, or use <code>@name</code> exactly as in the terminal. The grammar is one piece of code shared by both surfaces.</p>
      </li>

      <li>
        <h2 id="bound">Bound the fan-out</h2>
        <p>How many agents a machine can usefully carry is a fact about the machine:</p>
        ${code("shell", `synapse settings workers 4
synapse settings show | grep workers`)}
        <p>A supervisor already running picks this up on its next spawn. The number is clamped to a ceiling in code as well as refused on the way in, so a mistyped digit cannot buy an unbounded fleet — the setting is a preference, and the ceiling is the promise.</p>
      </li>

      <li>
        <h2 id="cleanup">Clean up</h2>
        ${code("shell", `/workers
/kill notes
/quit`)}
        <p>Leaving takes the workers it started with it, and drops your roster row so no agent goes on addressing questions to somebody who is not there.</p>
      </li>
    </ol>

    <h2>What to read next</h2>
    <p><a href="../mesh/">Run a team of agents and drive it yourself</a> is the same mesh with a roster you choose. <a href="../learn/">Let your agents write skills</a> turns a procedure one of these sessions worked out into something the next one starts with.</p>
  `,
};
