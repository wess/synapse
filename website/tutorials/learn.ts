import { code, note } from "../markup";
import type { Page } from "../types";

export const learn: Page = {
  path: "tutorials/learn/index.html",
  title: "Let your agents write skills",
  description:
    "Turn on self-improvement, watch a session write down a procedure it worked out, approve it deliberately, then correct it and put the correction back.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Turn it on", id: "enable" },
    { label: "Let a session teach", id: "teach" },
    { label: "Read the queue", id: "queue" },
    { label: "Prove the gate", id: "gate" },
    { label: "Approve one", id: "approve" },
    { label: "Correct it", id: "revise" },
    { label: "Take it back", id: "revert" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will let a session write a skill, prove for yourself that it reaches no tool until you say so, approve it, correct it, and undo the correction. By the end you will know exactly what an agent can and cannot do to your skill library.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and on <code>PATH</code>.</li>
      <li>Claude Code, Codex, or pi connected. <a href="../connect/">Install and connect your first tools</a> if not.</li>
      <li>About twenty minutes. Nothing here touches your memory store.</li>
    </ul>
    <p>Read <a href="../skills/">Keep one skill library across every tool</a> first if the library itself is new to you — this tutorial assumes you know what installing a skill does.</p>

    <ol class="steps">
      <li>
        <h2 id="enable">Turn it on</h2>
        ${code("shell", `synapse settings learn on
synapse settings show | grep learn`)}
        <p>Two tools, <code>teach</code> and <code>revise</code>, appear in every connected session the next time it starts, along with the guidance explaining them. Both arrive and leave together — a tool a session cannot explain is one it will use wrongly.</p>
        ${note("Why it is off by default", "Two more tool definitions cost context in every session that loads them, whether or not they are used. Somebody who does not want agents editing a library should never pay for the option.")}
      </li>

      <li>
        <h2 id="teach">Let a session teach you something</h2>
        <p>Start a connected tool and give it a job with a procedure buried in it — something with steps you would otherwise re-derive. A release, a tricky deploy, a debugging path that took three wrong turns.</p>
        <p>When it finishes, ask it plainly:</p>
        ${code("text", `That took a while to work out. Write it down as a skill so the next session starts from it.`)}
        <p>It calls <code>teach</code> with a name, a one-line description, and the steps as it would give them to somebody doing this for the first time. Synapse writes the frontmatter itself — a model never gets to invent YAML keys or a name that disagrees with its own directory.</p>
        <p>You can also just let it happen. A session about to be compacted is asked to write down both what it learned and what it worked out, which is the moment where not having done so costs immediately.</p>
      </li>

      <li>
        <h2 id="queue">Read the queue</h2>
        ${code("shell", `synapse skill proposed`)}
        <p>One row per skill nobody has looked at, oldest first, with the tool that wrote it and the line it left saying why. Read the thing itself before deciding:</p>
        ${code("shell", `synapse skill show cut-a-release`)}
        <p>Start a new session in the same project and you will be told the count once, at the boundary, and never again — <code>Synapse connected · 1 skill to review</code>. Waiting is what a proposal is for; a queue that interrupts is not a queue.</p>
      </li>

      <li>
        <h2 id="gate">Prove the gate for yourself</h2>
        <p>Do not take it on faith. Look where the skill would be if it had been installed:</p>
        ${code("shell", `ls ~/.claude/skills/
synapse skill status cut-a-release`)}
        <p>It is not there, and the status says <code>waiting for review</code> rather than <code>not installed</code> — a different fact with a different fix. Now try the blunt instrument:</p>
        ${code("shell", `synapse skill install
ls ~/.claude/skills/`)}
        <p>Still not there. Installing everything means everything <em>approved</em>; a proposal reaching a tool because somebody ran a bulk install is the one way this gate could leak, so both the CLI and the app filter it explicitly.</p>
        ${note("Why the gate is on installing, not writing", "The library is Synapse's own folder, so writing there costs nobody anything. A skill's description is loaded into every session of every tool holding it — that is the bill you have to agree to. So teaching is free and installing is the decision, rather than the other way around.")}
      </li>

      <li>
        <h2 id="approve">Approve one</h2>
        ${code("shell", `synapse skill approve cut-a-release
synapse skill status cut-a-release`)}
        <p>Now it is installed, and the queue is empty. If the agent scoped it to the project, it went into that repository's own skills folder rather than your home — a procedure about one checkout belongs to it.</p>
        <p>Turning one down removes it and its history:</p>
        ${code("shell", `synapse skill reject some-other-skill --confirm`)}
        <p>Only something still waiting can be rejected. Once approved it is an ordinary skill, and <code>skill delete</code> is what removes it — a command that says what it does.</p>
      </li>

      <li>
        <h2 id="revise">Let it correct itself</h2>
        <p>Use the skill in a session and let it turn out wrong — a missing step, a stale path. Tell the session:</p>
        ${code("text", `Step 3 is wrong now. Fix the skill.`)}
        <p>It calls <code>revise</code>, and this one <em>does</em> reach the copies Synapse installed:</p>
        ${code("shell", `grep -c "the corrected step" ~/.claude/skills/cut-a-release/SKILL.md`)}
        <p>That is the deliberate exception. You already agreed to this skill being loaded, and a correction that never arrives leaves every session running the version that was wrong. It reaches only copies Synapse wrote and nobody has edited — one you changed by hand is yours, and is left where it is.</p>
      </li>

      <li>
        <h2 id="revert">Take a correction back</h2>
        ${code("shell", `synapse skill history cut-a-release
synapse skill revert cut-a-release`)}
        <p>The history is every version the skill has had, newest first, with the line saying what was wrong with each. Reverting puts one back in the library and in every tool holding a Synapse copy — and is itself recorded as a revision, so a revert can be reverted. The newest twenty are kept.</p>
        <p>This is the same bargain a corrected memory makes: nothing is hidden without a way back.</p>
      </li>

      <li>
        <h2 id="cleanup">Clean up</h2>
        ${code("shell", `synapse skill remove cut-a-release
synapse skill delete cut-a-release --confirm
synapse settings learn off`)}
        <p>Turning it off leaves everything already in the library exactly where it is. What changes is that no new session gets the tools.</p>
      </li>
    </ol>

    <h2>What to read next</h2>
    <p><a href="../curate/">Curate and optimize memory</a> is the same idea for the other half of what a session learns — facts rather than procedures. <a href="../overseer/">Hand a job to one agent</a> gives you a session long enough to work out a procedure worth keeping.</p>
  `,
};
