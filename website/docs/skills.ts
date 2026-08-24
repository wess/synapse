import { code, note } from "../markup";
import type { Page } from "../types";

export const skills: Page = {
  path: "docs/skills/index.html",
  title: "Skills",
  description:
    "Keep your Agent Skills in one library, install them into every connected tool, and let a session write down a procedure it worked out \u2014 for you to approve before it reaches anything.",
  kind: "docs",
  toc: [
    { label: "Why a library", id: "why" },
    { label: "Where skills go", id: "locations" },
    { label: "The format", id: "format" },
    { label: "Installing", id: "installing" },
    { label: "What each state means", id: "states" },
    { label: "Adopting what you already have", id: "adopting" },
    { label: "Project skills", id: "projects" },
    { label: "Skills agents write", id: "learning" },
    { label: "Limits", id: "limits" },
  ],
  body: `
    <h2 id="why">Why a library</h2>
    <p>Claude Code, Codex, and pi all read the <a href="https://agentskills.io">Agent Skills</a> open format, and each reads it from its own folder. A skill you want in all of them gets copied by hand as many times, and the copies start drifting the first time you improve one of them.</p>
    <p>Synapse keeps one copy. You edit it in one place, install it into every connected tool, and the app tells you which copies have fallen behind.</p>

    <h2 id="locations">Where skills go</h2>
    <table>
      <thead><tr><th>Place</th><th>Path</th></tr></thead>
      <tbody>
        <tr><td>The Synapse library</td><td><code>&lt;data&gt;/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>&hellip; for one project</td><td><code>&lt;data&gt;/skills/@&lt;project&gt;/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>Claude Code</td><td><code>~/.claude/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>Codex</td><td><code>~/.agents/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>pi</td><td><code>~/.pi/agent/skills/&lt;name&gt;/SKILL.md</code></td></tr>
        <tr><td>A project's own</td><td><code>&lt;project&gt;/.claude/skills/</code>, <code>.agents/skills/</code>, <code>.pi/agent/skills/</code></td></tr>
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
        <tr><td>waiting for review</td><td>An agent wrote it and you have not looked at it yet.</td><td>Nothing, until you approve it.</td></tr>
      </tbody>
    </table>
    <p>Synapse knows which copies are its own because it records the digest it wrote and the library digest it came from. Without that record, "the library moved on" and "somebody wrote this by hand" look identical, and the safe response to both would be to do nothing.</p>

    <h2 id="adopting">Adopting what you already have</h2>
    <p>If a tool already has a skill you want managed, adopt it. That copies it into the library and records the tool it came from as already having it, so the original stops reading as somebody else's and starts staying in step.</p>
    ${code("shell", `synapse skill adopt humanize --tool claude
synapse skill install humanize`)}
    <p>The app lists these under <strong>Already in your tools</strong> on the Skills screen, with an Adopt button beside each.</p>

    <h2 id="projects">Project skills</h2>
    <p>A procedure you work out in one repository is usually about that repository. A library where every skill is global costs every session on the machine to hold one project's checklist, so skills have <strong>shelves</strong>: the library root for what is true everywhere, and one shelf per project beside it.</p>
    ${code("shell", `synapse skill create release --project        # this repository's own
synapse skill install release --project      # into this repository's .claude/skills
synapse skill list --global                  # or just the shared ones`)}
    <p>A project skill installs into that project's own skills folders rather than into your home, so it is loaded when you work there and nowhere else. It also travels with the repository once it is installed, which is the point.</p>
    <p>Every command that takes a bare name looks at this project's shelf before the global one, so <code>synapse skill show release</code> run inside a repository shows that repository's version. A global skill and a project skill can share a name without colliding; <code>--global</code> and <code>--project [folder]</code> say which you mean when it matters.</p>
    ${note("Not every tool has a place for these", "A project skill needs a project-local skills folder, and a tool described without one simply has nowhere to put it. Synapse leaves that pairing out of <code>skill status</code> rather than reporting it as something to fix.")}

    <h2 id="learning">Skills agents write</h2>
    <p>Switch this on and a session can write down a procedure it worked out, and correct one that turned out wrong:</p>
    ${code("shell", `synapse settings learn on`)}
    <p>That adds two tools, <code>teach</code> and <code>revise</code>, to every connected session, and the guidance explaining them arrives with them. It is off until you ask for it, because a tool definition costs context in every session that loads it.</p>
    <p>Synapse has no model and runs no loop, so it never reflects on a session. The session decides what it learned; Synapse decides where that lands and who has to agree to it.</p>
    <h3>Nothing reaches a tool until you say so</h3>
    <p>A skill an agent writes is <strong>proposed</strong>. It is in the library and in no tool, and it stays there until you approve it. The gate is on installing rather than on writing, because the library is Synapse's own folder while a skill's description is loaded into every session of every tool holding it — so writing one costs you a line in a list, and installing it is the decision.</p>
    ${code("shell", `synapse skill proposed                  # what is waiting, and who wrote it
synapse skill show cut-a-release        # read it
synapse skill approve cut-a-release     # install it where it belongs
synapse skill reject cut-a-release --confirm`)}
    <p><code>synapse skill install</code> with no name steps over anything waiting, so a proposal never reaches a tool because you installed everything. The app shows the same queue on the <strong>Skills</strong> screen with Approve and Turn down beside each, and the terminal dashboard does it with <code>a</code> and <code>d</code>.</p>
    <p>You will also see it said once at the start of a session — <code>Synapse connected · 1 skill to review</code> — and nowhere else. Waiting is what a proposal is for; a queue that interrupts is not a queue.</p>
    <h3>Corrections do reach the copies</h3>
    <p><code>revise</code> is the deliberate exception. You already agreed to that skill being loaded, and a correction that never arrives leaves every session running the version that was wrong — so a revision goes out to the copies Synapse installed. It reaches only copies Synapse wrote that nobody has edited since; one you changed by hand is yours and is left where it is.</p>
    <p>Nothing is lost doing it. What the skill said before is kept, and putting it back is one command:</p>
    ${code("shell", `synapse skill history cut-a-release     # what it used to say, and what was wrong
synapse skill revert cut-a-release      # put the last version back`)}
    <p>Reverting is itself a revision, so a revert can be reverted. The newest twenty versions of each skill are kept.</p>

    <h2 id="limits">Limits</h2>
    <ul>
      <li>Installing copies the directory rather than linking it, so a copy stays put if the library moves. The cost is that an edit in the library has to be installed again to reach the tools, which is what <code>status</code> is for.</li>
      <li>Deleting a skill from the library does not remove the copies already installed. Use <code>synapse skill remove</code> for that, before or after.</li>
      <li>A skill whose <code>SKILL.md</code> does not parse is reported and skipped rather than copied anywhere.</li>
      <li>A skill an agent writes is only ever a skill. It cannot install itself, cannot reach a tool, and cannot write anywhere except the library.</li>
    </ul>
  `,
};
