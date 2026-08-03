import { code, note } from "../markup";
import type { Page } from "../types";

export const meshtutorial: Page = {
  path: "tutorials/mesh/index.html",
  title: "Run a team of agents and drive it yourself",
  description:
    "Turn on the agent mesh, open a team, watch what every agent is doing from one terminal, answer a worker that gets stuck, and shut the whole thing down cleanly.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Turn it on", id: "enable" },
    { label: "Roles and teams", id: "roles" },
    { label: "Join as yourself", id: "mux" },
    { label: "Watch the mesh", id: "watch" },
    { label: "The wait loop", id: "waitloop" },
    { label: "Grow the team", id: "workers" },
    { label: "Answer a blocked worker", id: "blocked" },
    { label: "Write your own role", id: "ownrole" },
    { label: "Shut it down", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will put several coding-agent sessions on one local mesh, give them work from your own seat rather than through a lead agent, read what each one is doing without opening its terminal, and answer one that gets stuck. Then you will take it all down and confirm nothing is left running.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed, and at least one tool connected. Start with <a href="../connect/">Install and connect your first tools</a> if not.</li>
      <li>A real project folder. Agents work on real files, so use a checkout you can throw away or one with clean version control.</li>
      <li>Thirty to sixty minutes, and a willingness to let several agent sessions run at once.</li>
    </ul>
    ${note("These are real agents on your real machine", "A background worker runs with its own tool's permission prompts bypassed, because there is no terminal in which to answer one. Everything below happens on your actual files. Use a repository whose state you can restore.")}

    <ol class="steps">
      <li>
        <h3 id="enable">Turn the mesh on</h3>
        <p>The mesh ships switched off. Its sixteen tools load into every connected session, and that costs context in each one, so Synapse does not add them to a setup that will not use them.</p>
        ${code("shell", `synapse relay status`)}
        ${code("text", `Mesh: off
Agents: 0 online of 0
Workers: 0
Turn it on with \`synapse settings mesh on\`.`)}
        ${code("shell", `synapse settings mesh on`)}
        ${code("text", `Agent mesh on. Connected tools pick this up the next time they start.`)}
        <p>That last sentence is literal. A session already running keeps the tool list it started with, so restart any open tool before continuing. The same applies when you switch the mesh back off.</p>
        <p>Turning the mesh on also brings the guidance that explains it. The tools and the instructions for using them appear together and are withdrawn together, so a session can never have one without the other.</p>
      </li>

      <li>
        <h3 id="roles">Look at roles and teams</h3>
        <p>A <strong>role</strong> is a durable identity: a brief describing what an agent owns and how it coordinates. A <strong>team</strong> is a named roster of roles. Both ship with sensible defaults:</p>
        ${code("shell", `synapse relay role list`)}
        ${code("text", `backend	built-in
devops	built-in
frontend	built-in
qa	built-in
reviewer	built-in
supervisor	built-in
worker	built-in`)}
        ${code("shell", `synapse relay team list
synapse relay team show web`)}
        ${code("text", `# web · built-in
name = "web"

[[member]]
name = "lead"
role = "supervisor"

[[member]]
name = "frontend"
role = "frontend"

[[member]]
name = "backend"
role = "backend"

[[member]]
name = "reviewer"
role = "reviewer"`)}
        <p>Read one role to see what an agent is actually told:</p>
        ${code("shell", `synapse relay role show reviewer`)}
        ${code("text", `# reviewer · built-in
name = "reviewer"
description = """
You review other agents' work. Read the diffs they report, check correctness,
edge cases, and conventions, and send concise, actionable feedback to the author.
Approve only when it holds up; escalate disagreements to the supervisor.
"""`)}
        <p>Roles resolve from the project first, then your own layer, then the built-ins. A role saved into a project lives in <code>.synapse/roles/</code> and travels with the checkout, which is how a team convention becomes something the repository carries rather than something each person configures.</p>
      </li>

      <li>
        <h3 id="mux">Join as yourself, not through a lead</h3>
        <p>There are two ways to run a team, and the difference matters.</p>
        <p><code>synapse relay team open web</code> launches every member and puts a <em>lead agent</em> in your terminal. You brief the lead, the lead hands out tasks and relays answers back. That costs the lead's context on every message and makes it a bottleneck for work it is not doing.</p>
        <p><code>synapse mux</code> puts <em>you</em> on the roster instead, with the same messaging every agent has:</p>
        ${code("shell", `cd ~/your-project
synapse mux --team pair`)}
        <p>Synapse launches the team's members in the background and drops you into a line-oriented prompt with your own name on the roster. It is deliberately plain — no terminal library, no full-screen interface — so it works over ssh and in any terminal.</p>
        ${code("text", `@backend the created column needs a default, not a backfill
#build   freezing the schema in ten minutes
!        stop and report where you are
/focus backend
and the index too`)}
        <table>
          <thead><tr><th>Syntax</th><th>Goes to</th></tr></thead>
          <tbody>
            <tr><td><code>@name text</code></td><td>One agent, directly.</td></tr>
            <tr><td><code>#channel text</code></td><td>Every subscriber of that channel.</td></tr>
            <tr><td><code>! text</code></td><td>Everyone on the mesh.</td></tr>
            <tr><td><code>text</code></td><td>Whoever is focused, so a back-and-forth reads like a conversation.</td></tr>
          </tbody>
        </table>
        <p><code>/help</code> lists the rest. The ones you will use are <code>/agents</code>, <code>/workers</code>, <code>/focus &lt;name&gt;</code>, <code>/log &lt;name&gt;</code>, <code>/kill &lt;name&gt;</code>, and <code>/quit</code>.</p>
      </li>

      <li>
        <h3 id="watch">Watch what everyone is doing</h3>
        <p>From inside the mux, <code>/agents</code> shows the roster. From another terminal, the same thing:</p>
        ${code("shell", `synapse relay agents`)}
        ${code("text", `you       —           you      —        —                                         /work/api
lead      supervisor  online   working  splitting the migration into three tasks  /work/api
backend   backend     online   blocked  need the staging database name            /work/api
frontend  frontend    online   working  rewriting the auth middleware             /work/api`)}
        <p>The fourth column is a state and the fifth is the note that goes with it. The state tells you an agent has not stalled; the note is the part that tells you whether to leave it alone. For a headless worker there is no terminal to look at, so that note is the only view of it there is.</p>
        <p>Notes are one line, bounded, and kept when a later report does not carry one — an agent that reports <code>working</code> twice has not stopped doing the thing it described the first time.</p>
        ${code("shell", `synapse relay status          # counts and whether the mesh is on
synapse relay channels        # channels and how many subscribe to each
synapse relay feed --follow   # every message between agents, as it happens`)}
        <p>Every one of these takes <code>--json</code>. The <strong>Mesh</strong> page in the desktop app shows the same information with the traffic alongside it.</p>
        ${note("Notes are guidance, not a guarantee", "Writing one is something connected tools are told to do, not something Synapse can enforce inside another tool's session. A stale note is possible; the roster's own liveness is what tells you whether the agent is still there, and an agent that stops answering leaves the roster within about a minute and a half.")}
      </li>

      <li>
        <h3 id="waitloop">Understand the wait loop</h3>
        <p>This is the one piece of the mesh worth understanding properly, because almost every confusing behavior traces back to it.</p>
        <p>An idle agent calls <code>wait</code>, which blocks until a message arrives for it. After a few idle minutes it returns an empty list instead. That is a normal timeout, and the agent's instructions are to call <code>wait</code> again. An idle teammate therefore costs one tool call every few minutes rather than a loop that spins.</p>
        <p>Two consequences you will actually notice:</p>
        <ul>
          <li><strong>Messages arrive at the next check, not as an interrupt.</strong> An agent parked between tasks answers in about a second. One in the middle of a long build sees you when it comes back. The roster's state column tells you which is which.</li>
          <li><strong>An empty or failed <code>wait</code> is not a signal to stop.</strong> Agents are told this explicitly, in the guidance and again in the launch harness, because an agent that reads a timeout as "the work must be finished" writes an explanation and exits. That is the single most common way a mesh session dies.</li>
        </ul>
        <p>Delivery is at least once. A reply lost in flight is delivered again rather than dropped, so a duplicate is possible on that rare path and a lost message needs a process to die inside a very small window.</p>
      </li>

      <li>
        <h3 id="workers">Grow the team while it works</h3>
        <p>You do not have to decide the roster up front. A supervisor can spawn workers as it discovers what the job needs, and you can do the same from the command line:</p>
        ${code("shell", `synapse relay launch migrations --role backend --task "write the schema migration"
synapse relay ps
synapse relay kill migrations`)}
        <p>A worker started this way runs headless, registers itself, and parks on <code>wait</code> until it is given something to do. From inside the mux, <code>/workers</code> lists them and <code>/log &lt;name&gt;</code> shows what one has actually been doing.</p>
        <p>Workers belong to the session that started them, for exactly as long as that session lives. There is no daemon: closing the mux takes its workers with it, so nothing is left running behind you. A worker that exits is restarted with a growing backoff, and one that never manages a healthy run is retired rather than restarted forever. At most eight run at once.</p>
      </li>

      <li>
        <h3 id="blocked">Answer a worker that gets stuck</h3>
        <p>This is the reason to be on the roster yourself rather than behind a lead.</p>
        <p>A headless worker runs with its permission prompts bypassed, so until there is a person on the mesh it has nobody to ask when it reaches a decision it should not make alone — and its only option is to guess. With you on the roster it can send you the question, report itself <code>blocked</code> with a note saying what it needs, and wait.</p>
        ${code("text", `backend: the migration drops a column with data in it. Confirm before I run it?
/focus backend
yes, it is a duplicate of created_at — go ahead`)}
        <p>Agents can tell a person from an agent. Your roster row is marked as a human, and connected tools are told to ask you questions and never delegate work to you. Only <code>synapse mux</code> can set that flag; a tool calling <code>register</code> is always an agent, whoever is sitting in front of it.</p>
      </li>

      <li>
        <h3 id="ownrole">Write a role of your own</h3>
        <p>The built-ins are a starting point. Create one in the project so it travels with the checkout:</p>
        ${code("shell", `synapse relay role create migrator`)}
        ${code("toml", `channels = ["build"]
tool = "claude"
# model = "claude-opus-5"
# driver = true                            # stay interactive instead of parking
# tools = ["Read", "Edit", "Bash(git:*)"]  # pre-granted tool rules
description = """
You own database migrations. Write them forward-only, never destructive without
asking, and post the SQL to #build for review before running anything.
"""`)}
        <p>Editing a built-in copies it down into a layer you own rather than modifying the shipped template. <code>--user</code> writes into your own layer instead of the project. Then build a team around it:</p>
        ${code("shell", `synapse relay team create schemawork
synapse mux --team schemawork`)}
      </li>
    </ol>

    <h2 id="cleanup">Shut it down</h2>
    ${code("shell", `# from inside the mux
/quit

# then, from any terminal
synapse relay ps        # should be empty
synapse relay agents    # names age off within about ninety seconds
synapse settings mesh off`)}
    <p>Leaving the mux takes its workers with it and takes your name off the roster, so nothing addresses an empty terminal. Switching the mesh off removes the sixteen tools and their guidance from every session that starts afterwards; sessions already running keep what they had until they restart.</p>
    <p>Messages stay in the local database and remain visible through <code>synapse relay feed</code>. They are memory-adjacent, not memory: they are never recalled and never returned by <code>recall</code>.</p>

    <h2>What you can rely on</h2>
    <ul>
      <li>No daemon, no port, no token. The bus is the same local database as your memory, and a message between two agents never leaves your Mac.</li>
      <li>A message from another agent is information, not instruction. Connected tools are told to treat mesh traffic as untrusted input that never overrides you, your shared guidance, or repository rules.</li>
      <li>A person on the mesh is addressed, never assigned. Only the mux can mark a roster row as human.</li>
      <li>Channel and broadcast history is not replayed — a new agent sees only what is sent after it joins, so start workers before briefing them. A direct message is held for an agent that has not registered yet.</li>
      <li>Workers die with the session that started them. A session that closes takes its name off the roster with it.</li>
    </ul>

    <h2>Next step</h2>
    <p>You have finished the team operator track. The maintainer track covers keeping the store healthy and leaving cleanly: <a href="../recovery/">Export and restore safely</a>, then <a href="../lifecycle/">Check, migrate, and remove Synapse</a>.</p>
  `,
};
