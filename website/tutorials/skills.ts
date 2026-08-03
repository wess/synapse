import { code, note } from "../markup";
import type { Page } from "../types";

export const skills: Page = {
  path: "tutorials/skills/index.html",
  title: "Keep one skill library across every tool",
  description:
    "Write an Agent Skill once, install it into Codex and Claude Code together, and watch Synapse tell the difference between a library that moved on and a copy somebody edited by hand.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "See the library", id: "library" },
    { label: "Write a skill", id: "write" },
    { label: "Install it", id: "install" },
    { label: "Let it drift", id: "drift" },
    { label: "Edit a copy in place", id: "inplace" },
    { label: "Adopt what you already have", id: "adopt" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will write one Agent Skill, install it into every connected tool from a single source, and then deliberately create both kinds of drift so you can see how Synapse tells them apart. By the end you will know exactly when Synapse will overwrite a file and when it will refuse.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and on <code>PATH</code>.</li>
      <li>Codex, Claude Code, or both installed. They do not need to be connected for this tutorial — skills are copied into each tool's own folder, not through MCP.</li>
      <li>About fifteen minutes. Nothing here touches your memory store.</li>
    </ul>
    <p>If you have not connected a tool yet, start with <a href="../connect/">Install and connect your first tools</a>.</p>

    <ol class="steps">
      <li>
        <h3 id="library">See what is already there</h3>
        ${code("shell", `synapse skill list`)}
        <p>Synapse ships one skill, so a fresh library is not empty:</p>
        ${code("text", `synapse-mesh	1	Run a team of coding agents on the Synapse mesh. Use when a job is large enough to split a…`)}
        <p>The columns are the skill name, how many tools have it installed, and the start of its description. That description is the part an agent reads to decide whether the skill is relevant, so it matters more than the body: a skill with a vague description never gets loaded.</p>
        ${note("Why one library at all", "Claude Code reads personal skills from <code>~/.claude/skills</code> and Codex reads them from <code>~/.agents/skills</code>. A skill you want in both gets copied twice, and the two copies start drifting the first time you improve one of them. Synapse keeps the original and installs from it.")}
      </li>

      <li>
        <h3 id="write">Write a skill</h3>
        ${code("shell", `synapse skill create release-checklist`)}
        <p>The command reports where it landed and what to do next:</p>
        ${code("text", `Created ~/Library/Application Support/synapse/skills/release-checklist/SKILL.md
Edit it, then run \`synapse skill install release-checklist\`.`)}
        <p>Open it with <code>synapse skill edit release-checklist</code>, which uses <code>$EDITOR</code>, or edit the file directly. Replace the template with something real:</p>
        ${code("markdown", `---
name: release-checklist
description: Walk the release checklist for this project. Use when cutting a release, tagging a version, or when the user asks what still needs doing before shipping.
---

## Before you tag

1. Run the full test suite and paste the failure count, not a summary.
2. Confirm the version in the manifest matches the tag you are about to push.
3. Check that generated output was rebuilt from source rather than hand-edited.

## After the build

Report the published artifact and where it can be downloaded. If notarization
was skipped, say so explicitly rather than reporting success.`)}
        <p>Two rules the frontmatter has to follow: <code>name</code> must match the directory name, and a <code>description</code> containing a colon followed by a space has to be quoted or YAML reads it as a nested key. Synapse reports either problem by name rather than passing the raw parser error through.</p>
        ${note("A skill is a directory, not a file", "<code>scripts/</code>, <code>references/</code>, and <code>assets/</code> beside the <code>SKILL.md</code> travel with it and are copied whole. Agents load only the name and description until a task calls for the skill, so a long reference file costs nothing until it is actually needed.")}
      </li>

      <li>
        <h3 id="install">Install it into every tool at once</h3>
        <p>Check where things stand first. Every skill is listed against every detected tool:</p>
        ${code("shell", `synapse skill status`)}
        ${code("text", `release-checklist	Codex	not installed
synapse-mesh	Codex	not installed
release-checklist	Claude Code	not installed
synapse-mesh	Claude Code	not installed`)}
        <p>Now install everything, into everything:</p>
        ${code("shell", `synapse skill install`)}
        ${code("text", `release-checklist → Codex
synapse-mesh → Codex
release-checklist → Claude Code
synapse-mesh → Claude Code`)}
        <p>Run <code>synapse skill status</code> again and all four rows read <code>installed</code>. Confirm with your own eyes that two real copies exist:</p>
        ${code("shell", `ls ~/.claude/skills/release-checklist/
ls ~/.agents/skills/release-checklist/`)}
        <p>You can narrow either axis. <code>synapse skill install release-checklist</code> does one skill into every tool; <code>synapse skill install --tool claude</code> does every skill into one tool.</p>
        ${note("Codex does not read <code>~/.codex/skills</code> for this", "That folder holds the set Codex ships with. Personal skills live in the shared <code>~/.agents/skills</code> location, and that is the only place Synapse writes.")}
      </li>

      <li>
        <h3 id="drift">Let the library move on</h3>
        <p>This is the ordinary case: you improve the skill and the installed copies fall behind. Add a line to the library copy:</p>
        ${code("shell", `synapse skill edit release-checklist    # add anything to the body
synapse skill status`)}
        ${code("text", `release-checklist	Codex	update available
synapse-mesh	Codex	installed
release-checklist	Claude Code	update available
synapse-mesh	Claude Code	installed`)}
        <p><code>update available</code> means Synapse installed this copy and the library has changed since. Installing again brings them back into step, with no prompt and no flag:</p>
        ${code("shell", `synapse skill install release-checklist
synapse skill status`)}
        <p>All rows read <code>installed</code> again. Nothing was at risk here, because the copy being overwritten was one Synapse wrote and had not been touched since.</p>
      </li>

      <li>
        <h3 id="inplace">Now edit an installed copy by hand</h3>
        <p>This is the case that matters. Edit the copy inside Claude Code's folder rather than the library:</p>
        ${code("shell", `echo "" >> ~/.claude/skills/release-checklist/SKILL.md
echo "## A note I added directly in the tool" >> ~/.claude/skills/release-checklist/SKILL.md
synapse skill status`)}
        ${code("text", `release-checklist	Codex	installed
release-checklist	Claude Code	changed in place`)}
        <p>Ask Synapse to install over it and it refuses, naming the skill and the tool:</p>
        ${code("shell", `synapse skill install release-checklist --tool claude`)}
        ${code("text", `warning: release-checklist → Claude Code: \`release-checklist\` in Claude Code was changed in place — pass the replace option to overwrite it
Error: 1 skill install(s) did not happen`)}
        <p>That refusal is the whole point of the feature. Synapse records the digest it wrote and the library digest it came from; without that record, "the library moved on" and "somebody wrote this by hand" look identical, and the only safe response to both would be to do nothing. With it, the first case is silent and the second stops and asks.</p>
        <p>When you have decided the local edit is expendable:</p>
        ${code("shell", `synapse skill install release-checklist --tool claude --replace`)}
        ${note("The refusal is per skill, not per run", "A run that installs ten skills and hits one changed copy installs the other nine and reports the one it did not. The exit status is non-zero so a script notices, but nothing is left half-applied.")}
      </li>

      <li>
        <h3 id="adopt">Adopt a skill you already wrote</h3>
        <p>A skill Synapse did not install is never written over and never deleted — but it is also never kept in step, because Synapse has no claim on it. Adopting one changes that. It copies the skill into the library and records the tool it came from as already having it, so the original stops reading as somebody else's:</p>
        ${code("shell", `synapse skill status                          # look for "not in the library"
synapse skill adopt my-existing-skill --tool claude
synapse skill list                            # it is yours now
synapse skill install my-existing-skill       # and now it reaches the other tool too`)}
        <p>Adopting copies. It does not move or delete the original, so a mistake here costs you nothing.</p>
      </li>
    </ol>

    <h2 id="cleanup">Clean up</h2>
    ${code("shell", `synapse skill remove release-checklist        # take it out of every tool
synapse skill delete release-checklist --confirm  # and out of the library`)}
    <p>The order matters. Deleting from the library does not remove copies already installed — Synapse would then be deleting files it can no longer prove it wrote. Remove first, delete second, and nothing is left behind.</p>
    <p>Leave <code>synapse-mesh</code> where it is if you plan to continue; the <a href="../mesh/">mesh tutorial</a> uses it.</p>

    <h2>What you can rely on</h2>
    <ul>
      <li>A skill Synapse did not install is never overwritten or deleted, in either direction.</li>
      <li>Only personal skills are managed. A repository's own <code>.claude/skills</code> or <code>.agents/skills</code> belongs to that repository and is left alone.</li>
      <li>A <code>SKILL.md</code> that does not parse is reported and skipped rather than copied anywhere in a broken state.</li>
      <li><code>synapse disconnect</code> and <code>synapse uninstall</code> take back only skills with an install record. Everything you wrote survives both.</li>
    </ul>

    <h2>Next step</h2>
    <p>Continue to <a href="../launch/">Start a tool with everything in place</a>, which is the first tutorial in the team operator track, or go back to the <a href="../">tutorial index</a> to pick a different level.</p>
  `,
};
