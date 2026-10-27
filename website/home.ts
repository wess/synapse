import type { Page } from "./types";
import { downloadurl, releaseurl, repositoryurl } from "./deploy";

const body = `
<main id="content">
  <section class="hero">
    <div class="herocopy">
      <h1>synapse<span class="cursor" aria-hidden="true"></span></h1>
      <h2>Your tools change.<br>Your memory stays.</h2>
      <p>Decisions, corrections, and skills that survive the next session. One local memory for every connected coding tool.</p>
      <div class="heroactions">
        <a class="button" href="${downloadurl}">Download for macOS</a>
        <a class="button secondary" href="docs/install/">Install on your machine</a>
      </div>
      <p class="heronote">macOS &amp; Windows desktop. Linux CLI &amp; MCP.<br>Signed in-app updates. No account. Your machine.</p>
    </div>
    <div class="console" aria-label="Illustrative memory handoff between two sessions">
      <div class="consoletitle"><span>project / workshop</span><span>local memory</span></div>
      <div class="buffer">
        <p class="bufferlabel">session one / save a decision</p>
        <p><span class="nick">you</span> Use Bun in this repository.<br><span class="indent">Remember that for this project.</span></p>
        <p><span class="nick">synapse</span> Saved to project memory.</p>
        <div class="savedmemory"><span>memory #24</span><strong>Use Bun in this repository.</strong><span>project scope · stored locally</span></div>
        <p class="bufferlabel">session two / another connected tool</p>
        <p><span class="nick">you</span> How do I install dependencies?</p>
        <p><span class="nick">synapse</span> Recalled your project convention.</p>
        <pre><code>$ bun install</code></pre>
      </div>
      <div class="consolestatus"><span>remember → recall → continue</span><span>illustrative example</span></div>
    </div>
  </section>
  <div class="systemline"><span>LOCAL-FIRST MEMORY</span><span>SQLite · MCP · shared skills · scoped credentials</span><a href="${repositoryurl}">Open source / MIT</a></div>
  <section class="workflows" aria-labelledby="workflowtitle">
    <div class="sectionintro"><h2 id="workflowtitle">Pick up where<br>you left off.</h2><p>A new session shouldn’t mean explaining the project again. Keep what you settled, inspect what gets recalled, and change it when the decision changes.</p></div>
    <div class="workflowrows">
      <article><h3>Remember the decision.</h3><p>Keep confirmed conventions, corrections, and project context. Project memory follows the repository across clones and worktrees.</p><a href="docs/memory/">Memory &amp; recall</a></article>
      <article><h3>Connect your tools.</h3><p>Use the built-in connectors or add a TOML descriptor for another harness. Ainz has a reciprocal bridge: connect it here or there and both products agree.</p><a href="docs/harnesses/">Supported &amp; custom tools</a> · <a href="https://wess.io/ainz/">Meet Ainz</a></article>
      <article><h3>Keep one skill library.</h3><p>Save a reusable procedure once. Review proposed skills, approve revisions, and update managed copies without overwriting your edits.</p><a href="docs/skills/">Skills &amp; learning</a></article>
      <article><h3>Work together.</h3><p>Turn on the optional mesh for direct messages, roles, teams, and bounded background workers. Join the conversation from your terminal.</p><a href="docs/mesh/">The agent mesh</a></article>
    </div>
  </section>
  <section class="boundary" aria-labelledby="boundarytitle">
    <div><h2 id="boundarytitle">Local is the default.</h2><p>Memory lives in a local SQLite database. Secret values live in a separate encrypted vault or macOS Keychain. You choose which environment variables a project can receive.</p><a href="docs/security/">Read the security model</a></div>
    <dl><div><dt>Memory</dt><dd>Searchable, inspectable, exportable.</dd></div><div><dt>Credentials</dt><dd>Approved scopes. Values stay out of memory.</dd></div><div><dt>Changes</dt><dd>Supersede a fact. Restore its history.</dd></div><div><dt>Connection</dt><dd>Useful offline. No hosted service required.</dd></div></dl>
  </section>
  <section class="guidegate" aria-labelledby="guidetitle">
    <div class="sectionintro"><h2 id="guidetitle">Start with<br>something small.</h2><p>Connect a tool, save one convention, then recall it from the next session. The guides cover the rest.</p></div>
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
  description: "Local memory, shared skills, scoped credentials, and an optional agent mesh. Carry decisions between coding tools without an account or hosted service.",
  kind: "home",
  body,
};
