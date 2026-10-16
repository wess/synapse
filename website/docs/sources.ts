import { code, note } from "../markup";
import type { Page } from "../types";

export const sources: Page = {
  path: "docs/sources/index.html",
  title: "Sources",
  description: "Choose where skill and MCP searches look: public indexes, MCP registries, and your own or your team's GitHub repositories.",
  kind: "docs",
  toc: [
    { label: "What a source is", id: "kinds" },
    { label: "Adding one", id: "adding" },
    { label: "Private repositories", id: "private" },
    { label: "A project's sources", id: "project" },
    { label: "How a repository is read", id: "repositories" },
  ],
  body: `
    <h2 id="kinds">What a source is</h2>
    <p><code>synapse skill search</code> and <code>synapse mcp search</code>, and the Search tabs in the app, ask every source at once and label each result with the one it came from. A source is one of three kinds:</p>
    <table>
      <thead><tr><th>Kind</th><th>Holds</th><th>What it is</th></tr></thead>
      <tbody>
        <tr><td>skills index</td><td>Skills</td><td>skills.sh, or anything answering its <code>/api/search</code> and <code>/api/download</code>.</td></tr>
        <tr><td>MCP registry</td><td>MCP servers</td><td>The official MCP Registry, or any registry implementing its <code>/v0/servers</code> API — including one an organisation runs for itself.</td></tr>
        <tr><td>GitHub repository</td><td>Both</td><td>Every <code>SKILL.md</code> folder and every <code>server.json</code> in a repository, or in one folder of it.</td></tr>
      </tbody>
    </table>
    <p>skills.sh and the official MCP Registry are built in. Once you change the list it is yours, so a built-in can be removed like any other.</p>

    <h2 id="adding">Adding one</h2>
    ${code("shell", `synapse source add team acme/agent-skills
synapse source add team acme/agents@main/skills      # a branch and a folder
synapse source add internal https://mcp.example.com   # a registry
synapse source list
synapse source remove skills.sh`)}
    <p>Synapse works out the kind. <code>owner/repo</code> or a github.com address is a repository; an address is asked whether it answers as an MCP registry or as a skills index, so a typo is caught when the source is added rather than on every search. <code>--kind</code> settles it when asking cannot.</p>
    <p>Results are written <code>source:id</code>, which is what <code>get</code> takes. A bare id still means the first index, so <code>synapse skill get owner/repo/skill</code> works as it always has.</p>
    ${code("shell", `synapse skill search release notes
synapse skill get team:skills/release-notes
synapse mcp get internal:com.example/billing`)}
    <p>In the app, the bottom of each Search tab lists the sources that tab asks, with a field to add one and Remove beside each.</p>

    <h2 id="private">Private repositories</h2>
    <p>A private repository reads its token from the vault, and the sources file holds only the reference:</p>
    ${code("shell", `synapse secret set work github GITHUB_TOKEN      # prompted
synapse source add team acme/agent-skills --token work.github`)}

    <h2 id="project">A project's sources</h2>
    <p>A repository can carry <code>.synapse/sources.toml</code> so everyone working in it searches the team's own sources:</p>
    ${code("toml", `[[source]]
name = "team"
kind = "github"
repo = "acme/agent-skills"
path = "skills"`)}
    <p>It is used only once somebody has approved its exact contents, the same bargain <code>.synapse.yaml</code> makes. A file somebody else committed decides where your searches go and whose instructions come back, so it is listed and not searched until you say otherwise:</p>
    ${code("shell", `synapse source allow`)}
    <p>Editing the file takes the approval away until it is allowed again. A project source whose name is already in your own list is skipped and reported, so <code>team:release</code> always means one thing.</p>
    ${note("Approving a source is not approving what it holds. A skill from any source still arrives waiting for review, and a server still reaches no tool until you install it.")}

    <h2 id="repositories">How a repository is read</h2>
    <p>One request to GitHub's trees API lists the whole repository, and the files come from raw.githubusercontent.com, which does not count against GitHub's allowance of sixty requests an hour without a token. The catalogue — every skill's name and description, every <code>server.json</code> — is cached in the data folder for an hour, and a refresh whose tree has not changed reads nothing else. <code>synapse source refresh</code> reads it again now.</p>
    <p>A <code>server.json</code> straight out of a repository is often the template a release fills in, with <code>\${VERSION}</code> where the version goes. Synapse cannot fill it, so the server runs unpinned.</p>
  `,
};
