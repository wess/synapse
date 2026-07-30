import { code, note } from "../markup";
import type { Page } from "../types";

export const mesh: Page = {
  path: "docs/mesh/index.html",
  title: "Agent mesh",
  description:
    "Let connected coding tools message each other, split up a job, and park for free between tasks, using the same local database as your memory.",
  kind: "docs",
  toc: [
    { label: "What the mesh is", id: "what" },
    { label: "Turn it on", id: "enable" },
    { label: "How agents join", id: "join" },
    { label: "Roles", id: "roles" },
    { label: "Teams", id: "teams" },
    { label: "Background workers", id: "workers" },
    { label: "Watching the mesh", id: "watching" },
    { label: "Limits", id: "limits" },
  ],
  body: `
    <h2 id="what">What the mesh is</h2>
    <p>Every tool you connect to Synapse already runs its own copy of the same local server. The mesh gives those sessions a way to reach each other: a supervisor breaks a job into tasks, hands them out by name or by channel, and collects the results, while every idle agent waits at almost no cost.</p>
    <p>There is no daemon, no port, and no token. The message bus is the same local database that holds your memory, so a message between two agents never leaves your Mac and is inspectable with the same tools as everything else Synapse stores.</p>
    ${note("The mesh is off until you turn it on. Its tools are loaded into every connected session, and that costs context in each one, so Synapse does not add them to a setup that will not use them.")}

    <h2 id="enable">Turn it on</h2>
    <p>Use <strong>Settings → Agent mesh</strong> in the app, or the command line:</p>
    ${code("shell", `synapse settings mesh on`)}
    <p>A tool that is already running keeps the tool list it started with. Restart it, or open a new session, to pick the change up. The same is true when you turn the mesh back off.</p>

    <h2 id="join">How agents join</h2>
    <p>Switching the mesh on makes the tools available; it does not put anyone on the mesh. A session joins when it calls <code>register</code> with a name of its own, which happens when you ask it to work with other agents or when Synapse launched it with a role.</p>
    <table>
      <thead><tr><th>Tool</th><th>What it does</th></tr></thead>
      <tbody>
        <tr><td><code>register</code></td><td>Join under a unique name. Called once, before anything else.</td></tr>
        <tr><td><code>send</code>, <code>post</code>, <code>broadcast</code></td><td>Message one agent, a channel, or everyone.</td></tr>
        <tr><td><code>join</code>, <code>leave</code></td><td>Subscribe to and unsubscribe from a channel.</td></tr>
        <tr><td><code>wait</code></td><td>Block until work arrives. This is how an agent stays reachable while doing nothing.</td></tr>
        <tr><td><code>inbox</code></td><td>Take whatever is waiting right now, without blocking.</td></tr>
        <tr><td><code>reportstatus</code>, <code>waitstatus</code></td><td>Report working, blocked, or done, and block until a teammate reaches one of those.</td></tr>
        <tr><td><code>agents</code>, <code>channels</code>, <code>whoami</code></td><td>See who is here, what channels exist, and your own place in it.</td></tr>
        <tr><td><code>spawn</code>, <code>workers</code>, <code>stopworker</code></td><td>Grow, inspect, and shut down a team of background workers.</td></tr>
      </tbody>
    </table>
    <p>A parked <code>wait</code> returns an empty list after a few idle minutes and the agent simply calls it again, so an idle teammate costs one tool call every few minutes rather than a loop that spins.</p>

    <h2 id="roles">Roles</h2>
    <p>A role is the durable identity an agent launches with: a brief describing what it owns and how it coordinates, plus optional defaults. It is separate from a task, which is the one-off assignment. Synapse ships with <code>supervisor</code>, <code>worker</code>, <code>frontend</code>, <code>backend</code>, <code>reviewer</code>, <code>devops</code>, and <code>qa</code>.</p>
    ${code("shell", `synapse relay role list
synapse relay role show frontend
synapse relay role create reviewer          # writes into this project
synapse relay role create reviewer --user   # writes into your own layer`)}
    <p>Roles resolve from the project first, then your own layer, then the built-ins. A role saved into a project lives in <code>.synapse/roles/</code> and travels with the checkout. Editing a built-in copies it down into a layer you own, so the shipped templates stay intact.</p>
    ${code("toml", `channels = ["frontend"]
tool = "claude"
# model = "claude-opus-5"
# driver = true                            # stay interactive instead of parking
# tools = ["Read", "Edit", "Bash(git:*)"]  # pre-granted tool rules
description = """
You own the frontend. Follow the existing component conventions and report
blockers to the supervisor.
"""`)}

    <h2 id="teams">Teams</h2>
    <p>A team is a named roster. Opening one launches every member at once: the first is the lead and runs in your terminal so you have someone to steer, and the rest run in the background.</p>
    ${code("shell", `synapse relay team list
synapse relay team open web`)}
    <p>Closing the lead stops its team. The command that opened the team is the process supervising it, so nothing is left running behind you.</p>
    ${code("toml", `[[member]]
name = "lead"
role = "supervisor"

[[member]]
name = "backend"
role = "backend"`)}

    <h2 id="workers">Background workers</h2>
    <p>A supervisor can also grow its own team as it works, by calling <code>spawn</code>. A worker started that way runs headless, registers itself, and parks on <code>wait</code> until it is given something to do. Because nobody is watching a headless session, it runs with its own tool's permission prompts bypassed.</p>
    <p>Workers belong to the session that started them, for as long as that session lives. A worker that exits is restarted with a backoff that grows, and one that never manages a healthy run is retired rather than restarted forever. At most eight run at once.</p>
    ${code("shell", `synapse relay launch backend --role backend --task "build the login API"
synapse relay ps
synapse relay kill backend`)}
    ${note("A launched agent is given the project folder it should work in, and reaches Synapse through the same connection it already has. Nothing generates a credential, and no secret value is ever passed to a launched agent.")}

    <h2 id="watching">Watching the mesh</h2>
    <p>The <strong>Mesh</strong> page in the app shows who has joined, what each one last reported, which workers are running, and the recent traffic between them. The same is available from the terminal:</p>
    ${code("shell", `synapse relay status
synapse relay agents
synapse relay channels
synapse relay feed --follow`)}
    <p>Every command takes <code>--json</code> for scripting.</p>

    <h2 id="limits">Limits</h2>
    <ul>
      <li>A message from another agent is information, not instruction. Connected tools are told to treat mesh traffic as untrusted input that never overrides you, your shared guidance, or repository rules.</li>
      <li>Broadcast and channel history is not replayed. A new agent sees only what is sent after it joins, so start your workers before you brief them. A direct message is held for an agent that has not registered yet.</li>
      <li>Delivery is at least once. A reply lost in flight is delivered again rather than dropped; a duplicate is possible on that rare path.</li>
      <li>An agent that stops answering leaves the roster within about a minute and a half, and a session that closes takes its name with it.</li>
      <li>Agents work on your real machine with your real files. A background worker runs with its permission prompts bypassed, because there is no terminal in which to answer one.</li>
    </ul>
  `,
};
