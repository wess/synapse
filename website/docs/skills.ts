import { code, note } from "../markup";
import type { Page } from "../types";

export const skills: Page = {
  path: "docs/skills/index.html",
  title: "Skills",
  description:
    "Keep your Agent Skills in one library and install them into every connected tool, instead of copying folders by hand and watching the copies drift.",
  kind: "docs",
  toc: [
    { label: "Why a library", id: "why" },
    { label: "Where skills go", id: "locations" },
    { label: "The format", id: "format" },
    { label: "Installing", id: "installing" },
    { label: "What each state means", id: "states" },
    { label: "Adopting what you already have", id: "adopting" },
    { label: "Limits", id: "limits" },
  ],
  body: `
    <h2 id="why">Why a library</h2>
    <p>Claude Code and Codex both read the <a href="https://agentskills.io">Agent Skills</a> open format, and both read it from their own folder. A skill you want in both gets copied twice, and the two copies start drifting the first time you improve one of them.</p>
    <p>Synapse keeps one copy. You edit it in one place, install it into every connected tool, and the app tells you which copies have fallen behind.</p>

    <h2 id="locations">Where skills go</h2>
    <table>
      <thead><tr><th>Place</th><th>Path</th></tr></thead>
      <tbody>
        <tr><td>The Synapse library</td><td><code>&lt;data&gt;/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>Claude Code</td><td><code>~/.claude/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>Codex</td><td><code>~/.agents/skills/&lt;name&gt;/SKILL.md</code></td></tr>
      </tbody>
    </table>
    ${note("Codex reads personal skills from the shared <code>~/.agents/skills</code> folder rather than from its own home. <code>~/.codex/skills</code> holds the set Codex ships with, and Synapse does not write there.")}

    <h2 id="format">The format</h2>
    <p>A skill is a directory with a <code>SKILL.md</code> at its root. The frontmatter needs a <code>name</code> that matches the directory and a <code>description</code> saying what the skill does and when to reach for it. Everything after the frontmatter is the instructions.</p>
    ${code("markdown", `---
name: release-checklist
description: Walk the release checklist for this project. Use when cutting a release, tagging a version, or when the user asks what still needs doing before shipping.
---

## Steps

1. Run the full test suite.
2. Bump the version.
3. Write the release notes.`)}
    <p>A skill can carry more than its <code>SKILL.md</code> — <code>scripts/</code>, <code>references/</code>, and <code>assets/</code> travel with it, and Synapse copies the whole directory. Agents load only the name and description until a task calls for the skill, so a long reference file costs nothing until it is needed.</p>
    ${note("A description containing a colon followed by a space has to be quoted, or YAML reads it as a nested key. Synapse says so by name rather than passing the raw parser error through.")}
    <p>Synapse ships one skill, <code>synapse-mesh</code>, covering how to actually run a team of agents. It lands in your library on first use and is an ordinary editable skill from then on.</p>

    <h2 id="installing">Installing</h2>
    ${code("shell", `synapse skill list                     # what is in the library
synapse skill create my-workflow       # start one from a template
synapse skill edit my-workflow         # open it in $EDITOR
synapse skill install                  # copy everything into every tool
synapse skill install my-workflow      # or just one
synapse skill install --tool claude    # or into just one tool
synapse skill status                   # where each one is
synapse skill remove my-workflow       # take it back out`)}
    <p>Editing a skill in the library leaves the installed copies behind, and <code>status</code> says so. Running <code>install</code> again brings them back in step. In the app, the <strong>Skills</strong> screen does the same thing with a button per skill and one that installs everything.</p>

    <h2 id="states">What each state means</h2>
    <table>
      <thead><tr><th>State</th><th>What it means</th><th>What Synapse will do</th></tr></thead>
      <tbody>
        <tr><td>not installed</td><td>The tool does not have this skill.</td><td>Install it.</td></tr>
        <tr><td>installed</td><td>The copy matches the library.</td><td>Nothing needed.</td></tr>
        <tr><td>update available</td><td>Synapse installed it, and the library has changed since.</td><td>Install again to sync.</td></tr>
        <tr><td>changed in place</td><td>Synapse installed it, and it has been edited inside the tool.</td><td>Refuse, unless you pass <code>--replace</code>.</td></tr>
        <tr><td>not ours</td><td>A skill of the same name that Synapse never wrote.</td><td>Refuse. It is yours, not Synapse's.</td></tr>
        <tr><td>not in the library</td><td>The tool has a skill Synapse does not know about.</td><td>Nothing, until you adopt it.</td></tr>
      </tbody>
    </table>
    <p>Synapse knows which copies are its own because it records the digest it wrote and the library digest it came from. Without that record, "the library moved on" and "somebody wrote this by hand" look identical, and the safe response to both would be to do nothing.</p>

    <h2 id="adopting">Adopting what you already have</h2>
    <p>If a tool already has a skill you want managed, adopt it. That copies it into the library and records the tool it came from as already having it, so the original stops reading as somebody else's and starts staying in step.</p>
    ${code("shell", `synapse skill adopt humanize --tool claude
synapse skill install humanize`)}
    <p>The app lists these under <strong>Already in your tools</strong> on the Skills screen, with an Adopt button beside each.</p>

    <h2 id="limits">Limits</h2>
    <ul>
      <li>Personal skills only. Project skills, in a repository's own <code>.claude/skills</code> or <code>.agents/skills</code>, belong to that repository and Synapse leaves them alone.</li>
      <li>Installing copies the directory rather than linking it, so a copy stays put if the library moves. The cost is that an edit in the library has to be installed again to reach the tools, which is what <code>status</code> is for.</li>
      <li>Deleting a skill from the library does not remove the copies already installed. Use <code>synapse skill remove</code> for that, before or after.</li>
      <li>A skill whose <code>SKILL.md</code> does not parse is reported and skipped rather than copied anywhere.</li>
    </ul>
  `,
};
