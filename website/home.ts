import type { Page } from "./types";
import { downloadurl, releaseurl, repositoryurl } from "./deploy";

const body = `
<main id="content">
  <section class="workstation" aria-labelledby="synapsetitle">
    <div class="workstationhead">
      <div class="brandstripes" aria-hidden="true"><i></i><i></i><i></i></div>
      <p>Local continuity system</p>
      <span>ready</span>
    </div>
    <div class="memoryscreen">
      <div class="screenintro">
        <h1 id="synapsetitle">synapse</h1>
        <h2>Your tools change.<br>Your memory stays.</h2>
        <p>Decisions, corrections, and skills survive the next session. One local memory for every connected coding tool.</p>
      </div>
      <div class="handoff" aria-label="Illustrative memory handoff between two sessions">
        <div class="handoffhead"><span>project: backend</span><span>memory channel 24</span></div>
        <div class="session">
          <span>Codex / session one</span>
          <p><b>you</b> Use Bun in this repository. Remember that.</p>
          <p><b>synapse</b> Saved to project memory.</p>
        </div>
        <div class="memorytape"><span>stored locally</span><strong>Use Bun in this repository.</strong><span>project scope</span></div>
        <div class="session">
          <span>Claude / next session</span>
          <p><b>you</b> How do I install dependencies?</p>
          <p><b>synapse</b> Recalled the project convention.</p>
          <code>$ bun install</code>
        </div>
        <div class="handoffstatus"><span>remember</span><i aria-hidden="true"></i><span>recall</span><i aria-hidden="true"></i><span>continue</span></div>
      </div>
    </div>
    <div class="workstationcontrols">
      <div class="controlcopy"><strong>Runs on your machine.</strong><span>No account. No hosted memory. No secret values in the store.</span></div>
      <div class="heroactions"><a class="button" href="${downloadurl}">Download for macOS</a><a class="button secondary" href="docs/install/">Install another way</a></div>
      <p>macOS &amp; Windows desktop<br>Linux CLI &amp; MCP</p>
    </div>
  </section>
  <div class="systemline"><span>Synapse local backend</span><span>memory · jobs · logs · skills · scoped credentials</span><a href="${repositoryurl}">Source / MIT</a></div>
  <section class="workflows" aria-labelledby="workflowtitle">
    <div class="sectionintro"><h2 id="workflowtitle">Pick up where you left off.</h2><p>A new session shouldn’t mean explaining the project again. Keep what you settled, inspect what gets recalled, and change it when the decision changes.</p></div>
    <div class="workflowrows">
      <article><h3>Remember the decision.</h3><p>Keep confirmed conventions, corrections, and project context. Project memory follows the repository across clones and worktrees.</p><a href="docs/memory/">Memory &amp; recall</a></article>
      <article><h3>Connect your tools.</h3><p>Use the built-in connectors or add a TOML descriptor for another harness. Ainz has a reciprocal bridge: connect it here or there and both products agree.</p><a href="docs/harnesses/">Supported &amp; custom tools</a> · <a href="https://wess.io/ainz/">Meet Ainz</a></article>
      <article><h3>Keep one skill library.</h3><p>Save a reusable procedure once. Review proposed skills, approve revisions, and update managed copies without overwriting your edits.</p><a href="docs/skills/">Skills &amp; learning</a></article>
      <article><h3>Work together.</h3><p>Turn on the optional mesh for direct messages, roles, teams, and bounded background workers. Join the conversation from your terminal.</p><a href="docs/mesh/">The agent mesh</a></article>
      <article><h3>See how the work happened.</h3><p>Follow runs, changed files, reviews, checkpoints, isolated workspaces, runners, previews, and live events from one local view.</p><a href="docs/app/#logs">Activity and runtime status</a></article>
    </div>
  </section>
  <section class="boundary" aria-labelledby="boundarytitle">
    <div class="boundarycopy"><h2 id="boundarytitle">Local is the default.</h2><p>Memory lives in a local SQLite database. Secret values live in a separate encrypted vault or macOS Keychain. You choose which environment variables a project can receive.</p><a href="docs/security/">Read the security model</a></div>
    <dl><div><dt>Memory</dt><dd>Searchable, inspectable, exportable.</dd></div><div><dt>Credentials</dt><dd>Approved scopes. Values stay out of memory.</dd></div><div><dt>Changes</dt><dd>Supersede a fact. Restore its history.</dd></div><div><dt>Connection</dt><dd>Useful offline. No hosted service required.</dd></div></dl>
  </section>
  <section class="guidegate" aria-labelledby="guidetitle">
    <div class="sectionintro"><h2 id="guidetitle">Start with something small.</h2><p>Connect a tool, save one convention, then recall it from the next session. The guides cover the rest.</p></div>
    <div class="guidelist">
      <a href="docs/install/"><strong>Install &amp; connect</strong><span>macOS, Windows, or Linux</span></a>
      <a href="tutorials/continuity/"><strong>Try a handoff</strong><span>One decision, two sessions</span></a>
      <a href="tutorials/secrets/"><strong>Scope a credential</strong><span>Give one project what it needs</span></a>
      <a href="docs/"><strong>Read the manual</strong><span>Commands, tools, and guarantees</span></a>
    </div>
  </section>
  <section class="closing"><h2>A little less starting over.</h2><div class="heroactions"><a class="button" href="${downloadurl}">Download for macOS</a><a href="${releaseurl}">All releases &amp; Windows installer</a></div></section>
</main>`;

export const home: Page = {
  path: "index.html",
  title: "Synapse",
  description: "A local backend for agent harnesses: durable memory, jobs, logs, artifacts, shared skills, scoped credentials, and an optional mesh.",
  kind: "home",
  body,
};
