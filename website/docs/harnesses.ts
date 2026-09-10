import { code } from "../markup";
import type { Page } from "../types";

export const harnesses: Page = {
  path: "docs/harnesses/index.html",
  title: "Supported and custom harnesses",
  description: "Connect a supported coding agent, describe your own with one TOML file, or contribute a connector.",
  kind: "docs",
  toc: [
    { label: "Supported harnesses", id: "supported" },
    { label: "Create a connector", id: "create" },
    { label: "Descriptor example", id: "example" },
    { label: "Verify and update", id: "verify" },
    { label: "Contribute a harness", id: "contribute" },
  ],
  body: `
    <h2 id="supported">Supported harnesses</h2>
    <p>A harness is the coding agent you run. Synapse ships connectors for Claude Code (<code>claude</code>), Codex (<code>codex</code>), pi (<code>pi</code>), and Ainz (<code>ainz</code>). Claude Code, Codex, and Ainz connect through MCP. pi uses the <code>synapse-pi</code> extension.</p>
    <p>Each connector declares shared guidance and skill locations, connection detection, and launch arguments. Claude Code and pi also have dedicated session integration. A custom descriptor does not acquire those hooks by declaring the same paths.</p>
    ${code("shell", `synapse tool list
synapse tool show codex
synapse connect codex`)}

    <h2 id="create">Create a connector</h2>
    <p>Install your harness and make its executable available on <code>PATH</code>. From your repository root, run:</p>
    ${code("shell", `synapse tool create mytool`)}
    <p>Synapse opens a commented template using <code>VISUAL</code>, then <code>EDITOR</code>, or <code>vi</code> if neither is set. Use an editor executable that waits until you finish editing. Replace the sample paths and arguments with your harness's actual settings. Synapse checks the descriptor before saving it to <code>.synapse/tools/mytool.toml</code>; an invalid draft does not replace the saved file. Validation checks the descriptor structure, not whether your harness accepts its commands.</p>
    <p>Use <code>synapse tool create mytool --user</code> to save under the <code>tools/</code> folder in Synapse's data directory. Run <code>synapse path</code> to locate that directory. A project descriptor overrides a user descriptor with the same name, and a user descriptor overrides a built-in. Use a lowercase name such as <code>mytool</code>; it becomes the filename and the name passed to connection commands.</p>
    <p>You can share a project descriptor in your repository. Run the commands here from that repository's root so creation, inspection, and connection use the same project layer. Harness descriptors live in <code>.synapse/tools/</code>; <code>.synapse.yaml</code> configures project credential scopes.</p>

    <h2 id="example">Descriptor example</h2>
    <p>This example assumes a fictional harness named <code>mytool</code> with an <code>mcp add</code> command and a JSON MCP registry. Adapt it to your harness before connecting.</p>
    ${code("toml", `name = "My Tool"
command = "mytool"

[home]
default = ".mytool"

[paths]
instructions = "{home}/AGENTS.md"
settings = "{home}/settings.json"
integration = "{home}/settings.json"
skills = "{home}/skills"
projectskills = ".mytool/skills"

[connect]
add = ["mcp", "add", "synapse", "--", "{server}", "mcp"]
remove = ["mcp", "remove", "synapse"]

[detect]
format = "json"
at = ["mcpServers", "synapse"]
args = ["mcp"]

[launch]
prompt = ["{prompt}"]`)}
    <ul>
      <li><code>command</code> names the executable. Connection arguments are separate array entries passed to that executable.</li>
      <li><code>home.default</code> is relative to the user's home. Use <code>{configdir}/mytool</code> for the platform configuration directory, or set <code>home.env</code> if the harness supports a home-directory environment override.</li>
      <li><code>{home}</code> in paths refers to that resolved harness directory. <code>projectskills</code> is relative to the project root.</li>
      <li><code>{server}</code> expands to the Synapse executable. The harness must support the registration and removal commands you supply.</li>
      <li><code>detect.format</code> accepts <code>json</code> or <code>toml</code>. <code>detect.at</code> names the key path to the Synapse server entry, whose command and arguments Synapse checks.</li>
      <li>Launch slots such as <code>prompt</code>, <code>headless</code>, <code>config</code>, and <code>model</code> are optional. Declare only flags your harness supports. A descriptor with an interactive prompt alone does not establish unattended mesh support.</li>
    </ul>
    <p>Use <code>synapse tool show &lt;name&gt;</code> to inspect a built-in example. A harness with a different transport, configuration schema, or lifecycle hook may need a code adapter; a descriptor cannot implement a protocol.</p>

    <h2 id="verify">Verify and update</h2>
    ${code("shell", `synapse tool show mytool
synapse connect mytool
synapse status
synapse launch mytool --print`)}
    <p>Restart the harness. Confirm it exposes Synapse's memory tools, then save and recall a harmless project convention. Inspect its instruction file for the shared guidance pointer. If it supports Agent Skills, test installing a skill with <code>synapse skill install &lt;skill&gt; --tool mytool</code>.</p>
    <p>To revise the descriptor, run <code>synapse tool edit mytool</code>, followed by <code>synapse connect mytool --refresh</code>. Add <code>--user</code> to the edit command for a personal descriptor. The show command reports the winning layer, which helps identify a project override.</p>
    <p>Test cleanup with <code>synapse disconnect mytool</code> and check that unrelated configuration remains intact. Deleting a descriptor with <code>synapse tool delete mytool</code> does not disconnect the harness; disconnect first if you are removing the integration.</p>

    <h2 id="contribute">Contribute a harness</h2>
    <p>PRs for additional harnesses are welcome at <a href="https://github.com/wess/synapse">wess/synapse</a>. You can use your custom connector while the PR is under review.</p>
    <ol>
      <li>Add the tested descriptor to <code>crates/synapsecore/assets/tools/&lt;name&gt;.toml</code> and add its embedded entry to <code>BUILTINS</code> in <code>crates/synapsecore/src/agent/tool.rs</code>.</li>
      <li>Include tests for the resolved paths, connection detection, registration and removal. Cover launch arguments and skill installation for the features you declare. The custom-harness integration test in <code>crates/synapsecore/tests/cli.rs</code> demonstrates the full connection lifecycle with a fake executable.</li>
      <li>State the harness version and operating systems tested. Check that setup and disconnect preserve unrelated settings and instructions.</li>
      <li>Update the README support table and this guide. Document limitations or any adapter code needed for special behavior.</li>
    </ol>
    ${code("shell", `cargo test --locked --manifest-path crates/synapsecore/Cargo.toml
bun run site
bun run sitecheck`)}
  `,
};
