import { readdir } from "node:fs/promises";
import { extname, join, relative } from "node:path";
import { pages } from "./content";
import { route, sections, sourcepath } from "./navigation";
import { downloadurl, repositoryurl, siteurl, version } from "./deploy";

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
for (const target of ["docs/install/", "docs/memory/", "docs/skills/", "docs/mesh/", "docs/security/", "tutorials/continuity/"]) {
  if (!home.includes(`href="${target}"`)) fail(`landing page is missing entry point: ${target}`);
}
if (!home.includes(downloadurl)) fail("landing page is missing its release download");

const documents = await Promise.all(htmlfiles.map((file) => Bun.file(file).text()));
const corpus = documents.join("\n");
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
if (!corpus.includes(downloadurl)) {
  fail("generated pages are missing the current release download");
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
  "Save to the vault",
  "Enable shell hook",
  "Repair hook",
  "Remove hook",
  "Open data folder",
]) {
  if (!app.includes(phrase)) fail(`desktop app reference is missing: ${phrase}`);
}

// every command family in `HELP` (src/cli/command.rs). When a command is added
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

// the subcommands that do something a reader cannot guess from the family name.
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
  "synapse memory eval",
  "synapse memory supersede",
  "synapse memory restore",
  "synapse memory undo",
  "synapse memory wipe",
  "synapse guidance adopt",
  "synapse data export",
  "synapse data restore",
  "synapse settings mesh",
  "synapse settings optimize",
  "synapse settings learn",
  "synapse learn report",
  "synapse learn check",
  "synapse learn proposals",
  "synapse learn proposal",
  "synapse learn approve",
  "synapse learn cases",
  "synapse learn review",
  "synapse learn eval",
  "synapse learn resolve",
  "synapse learn restore",
  "synapse settings workers",
  "synapse skill proposed",
  "synapse skill approve",
  "synapse skill reject",
  "synapse skill history",
  "synapse skill revert",
  "synapse skill search",
  "synapse skill get",
  "synapse mcp add",
  "synapse mcp install",
  "synapse mcp adopt",
  "synapse mcp secret",
  "synapse mcp search",
  "synapse mcp run",
  "synapse source add",
  "synapse source allow",
  "synapse vault backend",
  "synapse vault migrate",
  "synapse secret copy",
]) {
  if (!corpus.includes(command)) fail(`documentation is missing subcommand: ${command}`);
}

// The four always-present tools, the four self-improvement adds, then the
// sixteen the mesh adds. A tool that ships without documentation is one an
// agent can call and a reader cannot look up.
for (const tool of [
  "remember",
  "recall",
  "readmemory",
  "vaultstatus",
  "teach",
  "revise",
  "feedback",
  "resolvedrift",
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

const search = await Bun.file(join(output, "search.json")).json();
const routes = new Set(pages.map(route));
const navigation = new Set(sections.flatMap((section) => section.paths));
for (const page of pages) {
  const html = await Bun.file(join(output, page.path)).text();
  if ((html.match(/<h1(?:\s[^>]*)?>/g) ?? []).length !== 1) fail(`${page.path}: expected one page heading`);
  const allids = [...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]);
  if (new Set(allids).size !== allids.length) fail(`${page.path}: duplicate document ids`);
  if (!html.includes(`v${version}`)) fail(`${page.path}: missing current release version`);
  if (page.kind === "home") continue;
  if (!(await Bun.file(join(project, sourcepath(page))).exists())) fail(`${page.path}: missing page source`);
  if (!search.some((entry: { path: string }) => entry.path === route(page))) fail(`${page.path}: absent from search index`);
  if (page.kind === "docs" && !navigation.has(route(page))) fail(`${page.path}: absent from manual navigation`);
}
for (const path of navigation) if (!routes.has(path)) fail(`navigation points to missing route: ${path}`);
for (const name of await readdir(join(project, "docs"))) {
  if (name.endsWith(".md") && !pages.some((page) => page.source === `docs/${name}`)) fail(`technical doc is not published: ${name}`);
}
const help = await Bun.file(join(project, "crates/synapsecore/src/cli/command.rs")).text();
const commands = help.match(/const HELP: &str = "([\s\S]*?)";/)?.[1] ?? "";
if (!commands) fail("could not read CLI help for command coverage");
for (const match of commands.matchAll(/^  ([a-z]+)(?: ([a-z]+))?/gm)) {
  const command = `synapse ${match[1]}${match[2] ? ` ${match[2]}` : ""}`;
  if (!corpus.includes(command)) fail(`released CLI command is undocumented: ${command}`);
}
if (/fonts\.googleapis|fonts\.gstatic/.test(corpus)) fail("site fonts must be self-hosted");
if (!relativefiles.has("pixel.ttf") || !relativefiles.has("fontlicense.txt")) fail("missing font or license");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(
  `Checked ${htmlfiles.length} HTML pages, ${relativefiles.size} files, and complete app/CLI/MCP coverage`,
);
