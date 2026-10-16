import { code, note } from "../markup";
import type { Page } from "../types";

export const servers: Page = {
  path: "docs/servers/index.html",
  title: "MCP servers",
  description: "Keep one list of MCP servers, give it to every connected tool, and keep their tokens in the vault rather than in each tool's config.",
  kind: "docs",
  toc: [
    { label: "Why a library", id: "why" },
    { label: "Adding a server", id: "adding" },
    { label: "Secrets", id: "secrets" },
    { label: "Installing", id: "installing" },
    { label: "What each state means", id: "states" },
    { label: "Adopting what you already have", id: "adopting" },
    { label: "Finding servers", id: "finding" },
    { label: "Limits", id: "limits" },
  ],
  body: `
    <h2 id="why">Why a library</h2>
    <p>Every coding tool keeps its own list of MCP servers in its own format, so the same server ends up configured three times, and its token sits in plaintext in three files. Synapse keeps one list instead. Each server is a small TOML file in the Synapse data directory, and its secrets are held as references into the vault rather than as values.</p>
    <p>What a tool is given is not the server itself. It is <code>synapse mcp run &lt;name&gt;</code>, written through that tool's own <code>mcp add</code>. When the tool starts it, Synapse reads the library, fetches the secrets from the vault, and replaces itself with the real server. Nothing is proxied: the tool talks to the server directly and keeps everything its own client does, and Synapse is gone from the path the moment the server starts.</p>
    ${note("Editing a server needs no reinstall. Tools only ever hold the <code>run</code> line, so the next session that starts the server picks up the change.")}

    <h2 id="adding">Adding a server</h2>
    <p>Describe it once. Everything after the bare <code>--</code> is the command, so its own flags are never read as Synapse's:</p>
    ${code("shell", `synapse mcp add github -- docker run -i --rm -e GITHUB_PERSONAL_ACCESS_TOKEN ghcr.io/github/github-mcp-server
synapse mcp show github
synapse mcp edit github`)}
    <p>The file it writes has a <code>command</code>, its <code>args</code>, plain settings under <code>[env]</code>, and secrets under <code>[secrets]</code>. A value can only be written under <code>[env]</code>, which is what keeps a token out of it: the <code>[secrets]</code> table holds nothing but references.</p>
    ${code("toml", `command = "docker"
args = ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"]

[secrets]
GITHUB_PERSONAL_ACCESS_TOKEN = "mcp-github.github-personal-access-token"`)}

    <h2 id="secrets">Secrets</h2>
    <p>A secret is entered at a prompt or piped in, never passed as an argument, the same as every other secret Synapse holds:</p>
    ${code("shell", `synapse mcp secret github GITHUB_PERSONAL_ACCESS_TOKEN`)}
    <p>Each server gets a vault of its own, named <code>mcp-&lt;server&gt;</code>, because a vault holds one secret per environment variable and two servers wanting <code>GITHUB_TOKEN</code> is ordinary. In the app, a server that still needs a secret shows a <strong>Set</strong> button beside it, and the value goes from the field straight into the vault.</p>
    <p>A server is not started while anything is missing — a secret with no value, a setting left empty, or an argument still written as a <code>&lt;placeholder&gt;</code>. It is refused with a line saying what to fill in, rather than starting half-configured and failing somewhere less obvious.</p>

    <h2 id="installing">Installing</h2>
    ${code("shell", `synapse mcp install            # every server, every tool that can hold one
synapse mcp install github --tool codex
synapse mcp status
synapse mcp uninstall github`)}
    <p>Claude Code, Codex, and Ainz can each be given library servers. pi has no MCP client of its own, so it gets no row. A tool somebody describes can take them too by adding a <code>[servers]</code> section to its descriptor with the <code>add</code>, <code>remove</code>, and <code>at</code> templates.</p>

    <h2 id="states">What each state means</h2>
    <table>
      <thead><tr><th>State</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td>installed</td><td>The tool starts this server through Synapse, from this binary.</td></tr>
        <tr><td>update available</td><td>Through Synapse, but from a binary that has moved. <code>install</code> rewrites it.</td></tr>
        <tr><td>not ours</td><td>The tool already has its own server by this name. It is left alone unless you say to replace it.</td></tr>
        <tr><td>not installed</td><td>The tool does not have it.</td></tr>
      </tbody>
    </table>
    <p>Whether an entry is Synapse's is read from the entry itself — <code>synapse mcp run &lt;its own name&gt;</code> — so there is no record to drift from what the file says. <code>synapse disconnect</code> and <code>synapse uninstall</code> take every such entry back out; a reset does not, because fixing a connection is no reason to lose a tool's servers.</p>

    <h2 id="adopting">Adopting what you already have</h2>
    <p>A server a tool already has can move into the library. Every value its entry carries goes into the vault — all of them, not only the ones whose names look secret, because a value misjudged as harmless would sit in plaintext in the library.</p>
    ${code("shell", `synapse mcp adopt github --tool claude
synapse mcp install github --tool claude --replace`)}
    <p>Adopting does not touch the tool. Its own entry still holds the token until you install over it with <code>--replace</code>, which swaps it for the <code>run</code> line and takes the plaintext out. In the app, that is the <strong>Swap</strong> button, offered only for the tool the server was adopted from.</p>

    <h2 id="finding">Finding servers</h2>
    <p><code>synapse mcp search</code> asks the official <a href="https://registry.modelcontextprotocol.io">MCP Registry</a>, and <code>synapse mcp get</code> turns an entry into a library server: an npm package becomes <code>npx</code>, a PyPI one <code>uvx</code>, a container image <code>docker run</code>. Whatever the entry needs and gives no value for is left for you to fill in.</p>
    ${code("shell", `synapse mcp search postgres
synapse mcp get io.github.owner/server-name
synapse mcp install server-name`)}
    <p>The official registry is one source among any you add: an organisation's own registry, or a GitHub repository holding <code>server.json</code> files. See <a href="../sources/">Sources</a>.</p>
    ${note("The registry is slow — fifteen to forty-five seconds a search when measured — and sorts by name, with nothing like a download count to rank by. Synapse puts what it can run first and waits a full minute before giving up.")}

    <h2 id="limits">Limits</h2>
    <ul>
      <li>Remote servers, which a tool reaches over HTTP rather than starting, are not kept here. Add those in each tool directly.</li>
      <li>A server's secrets are handed to it whenever a tool starts it, from any folder. The mapping you set up is the permission; there is no per-project scope for library servers.</li>
      <li>Deleting a server with <code>synapse mcp delete &lt;name&gt; --confirm</code> takes it out of every tool Synapse gave it to and forgets its vault. A tool's own entry by the same name is left alone.</li>
    </ul>
  `,
};
