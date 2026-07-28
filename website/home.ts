import type { Page } from "./types";
import { repositoryurl } from "./deploy";

const contract = `<!--
THESIS: A confirmed decision survives a cinematic match cut between developer tools; this refuses the generic neon node graph and the centered SaaS hero.
OWN-WORLD: Committed cobalt film stock, cool leader white, dark ink transcripts, carmine cue marks, ruled continuity logs, condensed display titles, and measured timecode captions.
STORY: Understand the local continuity layer, watch one memory cross tools, trust the safety boundaries, then download the macOS beta or enter the complete guide.
FIRST VIEWPORT: Copy occupies the left third; two offset terminal transcripts fill the right; an off-white memory strip crosses both at center scale; download sits directly below the offer.
FORM: Continuity-room match cut, third grounded direction, staged as a wordless four-scene sequence; seed ec0103e2.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, and DESIGN.md
-->`;

const body = `
<main id="content">
  <section class="hero" data-take="Scene 01">
    <div class="filmgrain" aria-hidden="true"></div>
    <div class="heroindex" aria-hidden="true">
      <span>Roll S-001</span>
      <span data-scene>Scene 01</span>
      <span>Local only</span>
    </div>
    <div class="herogrid">
      <div class="herocopy">
        <h1>One memory. Every tool.</h1>
        <p>Synaps keeps durable project context on your Mac, then makes it available to every connected coding tool—without an account or a cloud memory service.</p>
        <div class="heroactions">
          <a class="button" href="${repositoryurl}/releases/latest">Download macOS beta <span aria-hidden="true">↓</span></a>
          <a class="button secondary" href="docs/">Read the docs</a>
        </div>
        <span class="heronote">Apple silicon · macOS 13+ · notarized beta</span>
      </div>
      <div class="matchcut" aria-label="A confirmed project convention moves from Codex to Claude Code through Synaps">
        <div class="terminal codex">
          <div class="terminalbar"><span>Illustrative · Codex</span><span>10:42:11</span></div>
          <pre><span class="prompt">›</span> We settled the module structure.

remember({
  content: "Prefer small, focused Rust modules.",
  source: "synaps"
})

<span class="result">✓ stored locally as memory #24</span></pre>
        </div>
        <div class="terminal claude">
          <div class="terminalbar"><span>Illustrative · Claude Code</span><span>10:47:03</span></div>
          <pre><span class="prompt">›</span> recall({ query: "module structure" })

<span class="result">Memory #24 · synaps</span>
Prefer small, focused Rust modules.

<span class="result">Context recovered before work begins.</span></pre>
        </div>
        <div class="thread" aria-hidden="true"></div>
        <div class="memorystrip">
          <div class="stripcell"><span>Memory</span><strong>#24</strong></div>
          <div class="stripcell"><span>Confirmed decision</span><strong>Prefer small, focused Rust modules.</strong></div>
          <div class="stripcell striparrow" aria-hidden="true">→</div>
          <div class="stripcell"><span>Carried to</span><strong>Connected tools</strong></div>
          <div class="stripcell"><span>State</span><strong>Local</strong></div>
        </div>
      </div>
    </div>
  </section>

  <section class="proofbar" aria-label="Core properties">
    <div><strong>Local SQLite</strong><span>Memory stays on this Mac.</span></div>
    <div><strong>macOS Keychain</strong><span>Secret values stay out of the database.</span></div>
    <div><strong>Open MCP</strong><span>Codex and Claude Code connect over stdio.</span></div>
  </section>

  <section class="sequence" data-take="Scene 02">
    <div class="sectionhead">
      <h2>Continuity without ceremony.</h2>
      <p>Connect once. Synaps becomes the quiet handoff layer between sessions and tools, while every stored memory remains visible and editable.</p>
    </div>
    <div class="takes">
      <article class="take">
        <span>01 · Connect</span>
        <h3>Add the local MCP server.</h3>
        <p>The desktop app detects Codex and Claude Code, registers <code>synaps mcp</code> at user scope, and adds a managed instruction block without replacing your own content.</p>
      </article>
      <article class="take">
        <span>02 · Remember</span>
        <h3>Keep only what lasts.</h3>
        <p>Decisions, corrections, conventions, and preferences become durable memory. Source labels keep each entry understandable later.</p>
      </article>
      <article class="take">
        <span>03 · Recall</span>
        <h3>Start with the missing context.</h3>
        <p>Connected tools search the same full-text store before making a decision. Full, Balanced, and Lean response budgets control how much context returns.</p>
      </article>
      <article class="take">
        <span>04 · Inspect</span>
        <h3>You hold the final cut.</h3>
        <p>Search, read, edit, delete, or explicitly wipe memory from the app or CLI. Recall optimization never changes the original stored entry.</p>
      </article>
    </div>
  </section>

  <section class="vaultscene" data-take="Scene 03">
    <div class="vaultcopy">
      <h2>One scope. Two boundaries.</h2>
      <p>Synaps stores values in macOS Keychain. YAML contains references, SQLite contains labels, and MCP returns names and trust state—not values.</p>
      <p>Give one child the resolved environment, or enable automatic directory loading from Settings. Any scope edit blocks both modes until you approve it again.</p>
      <a class="button" href="docs/vault/">Understand vault scopes</a>
    </div>
    <div class="scopeframe" aria-label="Example approved Synaps project scope">
      <pre><span class="yamlkey">version:</span> <span class="yamlvalue">1</span>
<span class="yamlkey">scope:</span> <span class="yamlvalue">project</span>
<span class="yamlkey">env:</span>
  DATABASE_URL: work.database
  SENTRY_AUTH_TOKEN: work.sentry
<span class="yamlkey">deny:</span>
  - <span class="yamldeny">PRODUCTION_TOKEN</span>

$ eval "$(synaps hook zsh)"
$ synaps allow
Allowed /path/to/project/.synaps.yaml

$ cargo test

# Or keep the boundary to one child:
$ synaps run -- cargo test</pre>
    </div>
  </section>

  <section class="controlscene" data-take="Scene 04">
    <div class="memoryledger" aria-label="Example memory history">
      <div class="memoryrow"><span>#24</span><div><strong>Prefer small, focused Rust modules.</strong><small>synaps · convention</small></div><time>10:42</time></div>
      <div class="memoryrow"><span>#23</span><div><strong>Use Bun for JavaScript tasks in this repository.</strong><small>synaps · tooling</small></div><time>09:18</time></div>
      <div class="memoryrow"><span>#22</span><div><strong>The beta target is Apple-silicon macOS 13+.</strong><small>synaps · release</small></div><time>Yesterday</time></div>
    </div>
    <div class="controlcopy">
      <h2>Inspectable by design.</h2>
      <p>A local memory layer should never become a black box. Synaps exposes the history, its source, and its exact stored text. Data checks run before use; numbered migrations create recovery backups; exports are consistent SQLite snapshots.</p>
      <a class="button secondary" href="docs/data/">Read the data-safety guide</a>
    </div>
  </section>

  <section class="guidegate">
    <div>
      <h2>From first launch to full control.</h2>
      <p>The guide documents every command, MCP tool, file, trust boundary, recovery path, and supported workflow. The tutorials build complete working setups—not isolated snippets.</p>
    </div>
    <div class="guidelist">
      <a href="docs/install/"><span><strong>Install and connect</strong><span>Move the app, install the CLI, connect tools.</span></span><b aria-hidden="true">→</b></a>
      <a href="docs/memory/"><span><strong>Memory and recall</strong><span>Storage, search, editing, and response budgets.</span></span><b aria-hidden="true">→</b></a>
      <a href="docs/vault/"><span><strong>Vaults and scopes</strong><span>Keychain values, two shell modes, trust, and precedence.</span></span><b aria-hidden="true">→</b></a>
      <a href="docs/cli/"><span><strong>Complete CLI reference</strong><span>Every command, option, input, and safety guard.</span></span><b aria-hidden="true">→</b></a>
      <a href="tutorials/continuity/"><span><strong>Share a project decision</strong><span>Remember in one tool and recover it in another.</span></span><b aria-hidden="true">→</b></a>
      <a href="tutorials/secrets/"><span><strong>Scope a project secret</strong><span>Compare one-command and automatic directory loading.</span></span><b aria-hidden="true">→</b></a>
    </div>
  </section>

  <section class="closing">
    <div>
      <h2>Keep the thread.</h2>
      <p>Install the notarized macOS beta, connect your tools, and let confirmed context survive the next handoff.</p>
      <div class="heroactions">
        <a class="button" href="${repositoryurl}/releases/latest">Download for macOS</a>
        <a class="button secondary" href="tutorials/connect/">Follow the setup tutorial</a>
      </div>
    </div>
  </section>
</main>`;

export const home: Page = {
  path: "index.html",
  title: "Synaps",
  description: "Local memory and scoped credentials shared across your developer tools.",
  kind: "home",
  body,
  contract,
};
