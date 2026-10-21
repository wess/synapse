import { readdir } from "node:fs/promises";
import { basename, join, posix } from "node:path";
import type { Page } from "./types";
import { repositoryurl } from "./deploy";

const descriptions: Record<string, string> = {
  developing: "Build, test, document, and publish each Synapse surface from its own workspace.",
  synapsecore: "The shipped core library, its public modules, storage boundary, and independent build.",
  embedding: "Choose MCP, CLI automation, or the Rust library, and preserve scope and response budgets.",
  retrieval: "Ranking, drift, exact UTF-8 paging, relevance checks, and reproducible performance fixtures.",
  map: "How the memory map lays out projects, supersession, and shared vocabulary.",
  startup: "A measured startup snapshot and the fixtures needed to make a fresh comparison.",
  windows: "Build the Windows x64 installer and verify its desktop, CLI, and storage behavior.",
  sync: "The optional encrypted replication library and opaque log server, with current setup limitations.",
  federation: "A proposal for advisory peer knowledge. These interfaces are not implemented.",
};

const folder = join(import.meta.dir, "..", "docs");
const files = (await readdir(folder)).filter((file) => file.endsWith(".md")).sort();
const slugs = new Set(files.map((file) => basename(file, ".md")));

export const technical: Page[] = await Promise.all(files.map(async (file): Promise<Page> => {
  const slug = basename(file, ".md");
  const markdown = await Bun.file(join(folder, file)).text();
  const title = markdown.match(/^# (.+)$/m)?.[1] ?? slug;
  let body = Bun.markdown.html(markdown.replace(/^# .+\n/, ""), {
    headings: { ids: true },
    tagFilter: true,
  });
  body = body.replace(/href="([^"]+)"/g, (attribute, href: string) => {
    if (/^(?:#|https?:|mailto:)/.test(href)) return attribute;
    const [path, hash] = href.split("#");
    const target = basename(path, ".md");
    const local = slugs.has(target) && path.endsWith(".md") && !path.includes("..");
    const url = local
      ? `../${target}/${hash ? `#${hash}` : ""}`
      : `${repositoryurl}/blob/main/${posix.normalize(posix.join("docs", path))}${hash ? `#${hash}` : ""}`;
    return `href="${url}"`;
  });
  body = body.replace(/<pre><code(?: class="language-([^"]*)")?>([\s\S]*?)<\/code><\/pre>/g,
    (_, language, contents) => `<div class="codeblock"><div class="codebar"><span>${language || "text"}</span><button type="button" data-copy aria-live="polite">Copy</button></div><pre><code>${contents}</code></pre></div>`,
  );
  const toc = [...body.matchAll(/<h2 id="([^"]+)">([\s\S]*?)<\/h2>/g)].map((match) => ({
    id: match[1],
    label: match[2].replace(/<[^>]+>/g, ""),
  }));
  return {
    path: `docs/${slug}/index.html`,
    title,
    description: descriptions[slug] ?? `Technical documentation for ${title}.`,
    kind: "docs",
    source: `docs/${file}`,
    body,
    toc,
  };
}));
