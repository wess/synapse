import { readdir } from "node:fs/promises";
import { extname, join, relative } from "node:path";
import { siteurl } from "./template";

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
  "One memory. Every tool.",
  "Download macOS beta",
  "Local SQLite",
  "macOS Keychain",
  "Open MCP",
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
if (corpus.includes("https://wess.github.io/synaps/")) {
  fail("generated pages contain the redirecting GitHub Pages hostname");
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
if (docs.length < 11) fail(`expected at least 11 documentation pages, found ${docs.length}`);
if (tutorials.length < 6) fail(`expected at least 6 tutorial pages, found ${tutorials.length}`);

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

for (const command of [
  "synaps app",
  "synaps mcp",
  "synaps run",
  "synaps hook",
  "synaps allow",
  "synaps deny",
  "synaps export",
  "synaps status",
  "synaps vault",
  "synaps secret",
  "synaps scope",
  "synaps data",
  "synaps memory",
  "synaps settings",
  "synaps install",
  "synaps path",
  "synaps version",
]) {
  if (!corpus.includes(command)) fail(`documentation is missing command family: ${command}`);
}
for (const tool of ["remember", "recall", "vaultstatus"]) {
  if (!corpus.includes(tool)) fail(`documentation is missing MCP tool: ${tool}`);
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(
  `Checked ${htmlfiles.length} HTML pages, ${relativefiles.size} files, and complete app/CLI/MCP coverage`,
);
