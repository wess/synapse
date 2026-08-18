import { readdir } from "node:fs/promises";
import { extname, join, relative } from "node:path";
import { releasetag, repositoryurl, siteurl } from "./deploy";

const project = join(import.meta.dir, "..");
const output = join(project, "site");

const walk = async (folder: string): Promise<string[]> => {
  const entries = await readdir(folder, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map((entry) => {
      const path = join(folder, entry.name);
      return entry.isDirectory() ? walk(path) : [path];
    }),
  );
  return nested.flat();
};

const files = await walk(output);
const relativefiles = new Set(files.map((file) => relative(output, file)));
const htmlfiles = files.filter((file) => extname(file) === ".html");
const failures: string[] = [];

const fail = (message: string) => failures.push(message);
const localtarget = (source: string, reference: string) => {
  const url = new URL(reference, `https://example.test/${relative(output, source)}`);
  let path = url.pathname.slice(1);
  if (!path || path.endsWith("/")) path += "index.html";
  return { path, hash: url.hash.slice(1) };
};

for (const file of files) {
  const name = relative(output, file)
    .split("/")
    .at(-1)
    ?.replace(/^\./, "");
  if (name && /[A-Z_-]/.test(name)) fail(`forbidden generated filename: ${name}`);
}

for (const file of htmlfiles) {
  const path = relative(output, file);
  const html = await Bun.file(file).text();
  const route = path.replace(/index\.html$/, "");
  const canonical = `${siteurl}${route}`;
  if (!/<html lang="en">/.test(html)) fail(`${path}: missing document language`);
  if (!/<meta name="description"/.test(html)) fail(`${path}: missing description`);
  if (!/<main[^>]+id="content"/.test(html)) fail(`${path}: missing content landmark`);
  if (/\<img(?![^>]*\balt=)[^>]*\>/g.test(html)) fail(`${path}: image without alt`);
  if (!html.includes(`<meta property="og:url" content="${canonical}">`)) {
    fail(`${path}: missing canonical Open Graph URL`);
  }
  if (!html.includes(`<link rel="canonical" href="${canonical}">`)) {
    fail(`${path}: missing canonical link`);
  }

  const ids = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]));
  for (const reference of html.matchAll(/\b(?:href|src)="([^"]+)"/g)) {
    const value = reference[1];
    if (/^(?:https?:|mailto:|tel:|data:)/.test(value)) continue;
    if (value.startsWith("#")) {
      if (value.length > 1 && !ids.has(value.slice(1))) fail(`${path}: missing anchor ${value}`);
      continue;
    }
    const target = localtarget(file, value);
    if (!relativefiles.has(target.path)) {
      fail(`${path}: missing local target ${value} -> ${target.path}`);
      continue;
    }
    if (target.hash) {
      const targethtml = await Bun.file(join(output, target.path)).text();
      if (!targethtml.includes(`id="${target.hash}"`)) {
        fail(`${path}: missing target anchor ${value}`);
      }
    }
  }
}

const home = await Bun.file(join(output, "index.html")).text();
if (!home.startsWith("<!--\nTHESIS:")) fail("landing page is missing its direction contract");
for (const phrase of [
  "Your tools forget. Synapse remembers.",
  "Download macOS beta",
  "Remember decisions",
  "Scope credentials",
  "Stay in control",
]) {
  if (!home.includes(phrase)) fail(`landing page is missing: ${phrase}`);
}

const corpus = await Promise.all(htmlfiles.map((file) => Bun.file(file).text())).then((items) =>
  items.join("\n"),
);
const stylesheet = await Bun.file(join(output, "style.css")).text();
const robots = await Bun.file(join(output, "robots.txt")).text();
const sitemap = await Bun.file(join(output, "sitemap.xml")).text();
if (/\@import\s/.test(stylesheet)) fail("generated stylesheet contains a render-blocking import");
if (!robots.includes(`Sitemap: ${siteurl}sitemap.xml`)) {
  fail("robots.txt is missing the canonical sitemap URL");
}
for (const page of htmlfiles) {
  const route = relative(output, page).replace(/index\.html$/, "");
  if (!sitemap.includes(`<loc>${siteurl}${route}</loc>`)) {
    fail(`sitemap is missing canonical route: ${route}`);
  }
}
const formerspelling = ["syn", "aps"].join("");
if (new RegExp(`${formerspelling}(?!e)`, "i").test(corpus)) {
  fail("generated pages contain the former product spelling");
}
if (!corpus.includes(repositoryurl)) {
  fail("generated pages are missing the repository URL");
}
if (!corpus.includes(`/releases/download/${releasetag}/synapse.zip`)) {
  fail("generated pages are missing the current beta download");
}
if (!corpus.includes('data-searchstatus role="status" aria-live="polite"')) {
  fail("documentation search is missing an announced status");
}
if (!corpus.includes('<details class="railnav" open>')) {
  fail("documentation is missing compact guide navigation");
}
if (!corpus.includes('data-copy aria-live="polite"')) {
  fail("copy controls are missing announced feedback");
}

const docs = htmlfiles.filter((file) => relative(output, file).startsWith("docs/"));
const tutorials = htmlfiles.filter((file) => relative(output, file).startsWith("tutorials/"));
if (docs.length < 13) fail(`expected at least 13 documentation pages, found ${docs.length}`);
if (tutorials.length < 10) fail(`expected at least 10 tutorial pages, found ${tutorials.length}`);

const app = await Bun.file(join(output, "docs", "app", "index.html")).text();
for (const phrase of [
  "Desktop app reference",
  "Edit instructions",
  "Confirm wipe",
  "Save to Keychain",
  "Enable shell hook",
  "Repair hook",
  "Remove hook",
  "Open data folder",
]) {
  if (!app.includes(phrase)) fail(`desktop app reference is missing: ${phrase}`);
}

// Every command family in `HELP` (src/cli/command.rs). When a command is added
// there it has to appear here too, or the site can quietly ship without it —
// which is how `launch`, `mux`, `relay`, and `skill` were all documented but
// unverified for four releases.
for (const command of [
  "synapse app",
  "synapse mcp",
  "synapse launch",
  "synapse run",
  "synapse mux",
  "synapse hook",
  "synapse allow",
  "synapse deny",
  "synapse status",
  "synapse vault",
  "synapse secret",
  "synapse scope",
  "synapse data",
  "synapse memory",
  "synapse guidance",
  "synapse relay",
  "synapse skill",
  "synapse session",
  "synapse statusline",
  "synapse compact",
  "synapse doctor",
  "synapse settings",
  "synapse install",
  "synapse connect",
  "synapse disconnect",
  "synapse tool",
  "synapse uninstall",
  "synapse path",
  "synapse version",
]) {
  if (!corpus.includes(command)) fail(`documentation is missing command family: ${command}`);
}

// The subcommands that do something a reader cannot guess from the family name.
for (const command of [
  "synapse tool create",
  "synapse relay team",
  "synapse relay role",
  "synapse relay launch",
  "synapse relay ps",
  "synapse relay kill",
  "synapse relay feed",
  "synapse skill install",
  "synapse skill adopt",
  "synapse skill status",
  "synapse memory import",
  "synapse memory grep",
  "synapse memory supersede",
  "synapse memory restore",
  "synapse memory undo",
  "synapse memory wipe",
  "synapse guidance adopt",
  "synapse data export",
  "synapse data restore",
  "synapse settings mesh",
  "synapse settings optimize",
]) {
  if (!corpus.includes(command)) fail(`documentation is missing subcommand: ${command}`);
}

// The three always-present tools, then the sixteen the mesh adds. A mesh tool
// that ships without documentation is one an agent can call and a reader cannot
// look up.
for (const tool of [
  "remember",
  "recall",
  "vaultstatus",
  "register",
  "send",
  "post",
  "broadcast",
  "join",
  "leave",
  "wait",
  "inbox",
  "reportstatus",
  "waitstatus",
  "agents",
  "channels",
  "whoami",
  "spawn",
  "workers",
  "stopworker",
]) {
  if (!corpus.includes(tool)) fail(`documentation is missing MCP tool: ${tool}`);
}

// Every environment variable the binary reads. `grep -rho 'SYNAPSE_[A-Z_]*'
// src` is the list this has to keep up with.
for (const variable of [
  "SYNAPSE_DATA",
  "SYNAPSE_HOME",
  "SYNAPSE_BIN",
  "SYNAPSE_PROJECT_DIR",
  "SYNAPSE_PAGE",
  "SYNAPSE_DOCUMENT",
  "SYNAPSE_SHELL_ACTIVE",
  "SYNAPSE_SHELL_KEYS",
  "SYNAPSE_SHELL_COMMAND",
  "CODEX_HOME",
]) {
  if (!corpus.includes(variable)) fail(`documentation is missing environment variable: ${variable}`);
}

// Each tutorial level needs at least one page, so the ladder cannot lose a rung
// without the build saying so.
const tutorialindex = await Bun.file(join(output, "tutorials", "index.html")).text();
for (const level of ["Newcomer", "Daily driver", "Team operator", "Maintainer"]) {
  if (!tutorialindex.includes(level)) fail(`tutorial index is missing the ${level} level`);
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(
  `Checked ${htmlfiles.length} HTML pages, ${relativefiles.size} files, and complete app/CLI/MCP coverage`,
);
