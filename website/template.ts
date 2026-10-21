import type { Page } from "./types";
import { repositoryurl, siteurl, version } from "./deploy";
import { route, sections, sourcepath } from "./navigation";
import { escapehtml } from "./markup";

const rootfor = (path: string) => "../".repeat(path.split("/").length - 1);
const current = (page: Page, kind: Page["kind"]) => page.kind === kind ? ' aria-current="page"' : "";

const topnav = (page: Page, root: string) => `
  <a class="skip" href="#content">Skip to content</a>
  <div class="windowbar"><a href="${root}">synapse / ${page.kind === "home" ? "local memory" : page.kind === "docs" ? "manual" : "tutorials"}</a><span>v${version} · on your machine</span></div>
  <header class="topbar">
    <a class="wordmark" href="${root}" aria-label="Synapse home"><img src="${root}icon.svg" width="28" height="28" alt=""><span>synapse</span></a>
    <button class="navtoggle" type="button" aria-expanded="false" aria-controls="topnav">Menu</button>
    <nav id="topnav" aria-label="Primary">
      <a href="${root}docs/"${current(page, "docs")}>Docs</a>
      <a href="${root}tutorials/"${current(page, "tutorial")}>Tutorials</a>
      <a href="${repositoryurl}">Source</a>
      <a class="navdownload" href="${root}docs/install/">Install</a>
    </nav>
  </header>`;

const links = (items: Page[], page: Page, root: string) => items.map((item) =>
  `<li><a href="${root}${route(item)}"${item.path === page.path ? ' aria-current="page"' : ""}>${item.title}</a></li>`,
).join("");

const rail = (page: Page, pages: Page[], root: string) => `
  <aside class="docsrail" aria-label="Documentation navigation">
    <div class="searchbox">
      <label for="docssearch">Search docs &amp; tutorials</label>
      <input id="docssearch" type="search" placeholder="Memory, vaults, restore…" autocomplete="off" aria-controls="searchresults">
      <div id="searchresults" class="searchresults" data-results hidden></div>
      <span class="searchstatus" data-searchstatus role="status" aria-live="polite"></span>
    </div>
    <details class="railnav" open>
      <summary>Browse all guides</summary>
      <nav aria-label="All guides">
        ${sections.map((section) => `<h2>${section.title}</h2><ul>${links(section.paths.flatMap((path) => pages.filter((item) => route(item) === path)), page, root)}</ul>`).join("")}
        <h2>Tutorials</h2><ul>${links(pages.filter((item) => item.kind === "tutorial"), page, root)}</ul>
      </nav>
    </details>
  </aside>`;

const ontoc = (page: Page) => page.toc?.length ? `
  <aside class="ontoc" aria-label="On this page"><span>Contents</span><ol>${page.toc.map((item) => `<li><a href="#${item.id}">${item.label}</a></li>`).join("")}</ol></aside>` : "";

const footer = (root: string) => `
  <footer class="footer">
    <div class="footerupper"><div><a class="wordmark" href="${root}"><img src="${root}icon.svg" width="26" height="26" alt="">synapse</a><p>A shared memory. A little less starting over.</p></div>
      <nav aria-label="Footer"><a href="${root}docs/">Manual</a><a href="${root}tutorials/">Tutorials</a><a href="${repositoryurl}">Source</a><a href="${repositoryurl}/releases">Releases</a><a href="https://wess.io">By Wess</a></nav></div>
    <p class="footerfine"><span>macOS &amp; Windows desktop · Linux CLI &amp; MCP</span><span>v${version} · MIT · No account required</span></p>
  </footer>`;

const articlefoot = (page: Page, pages: Page[], root: string) => {
  const siblings = page.kind === "docs"
    ? sections.flatMap((section) => section.paths.flatMap((path) => pages.filter((item) => route(item) === path)))
    : pages.filter((item) => item.kind === page.kind);
  const index = siblings.indexOf(page);
  const previous = siblings[index - 1];
  const next = siblings[index + 1];
  const neighbor = (item: Page | undefined, label: string) => item ? `<a href="${root}${route(item)}"><small>${label}</small><strong>${item.title}</strong></a>` : "<span></span>";
  return `<footer class="articlefoot"><div class="articlemeta"><a href="${repositoryurl}/blob/main/${sourcepath(page)}">View page source</a><span>Synapse ${version}</span></div><nav class="pagenav" aria-label="Guide pages">${neighbor(previous, "Previous")}${neighbor(next, "Next")}</nav></footer>`;
};

const article = (page: Page, pages: Page[], root: string) => `
  <div class="doclayout">${rail(page, pages, root)}
    <main class="article" id="content">
      <header class="articlehead"><nav class="breadcrumb" aria-label="Breadcrumb"><a href="${root}">Synapse</a><span aria-hidden="true">/</span><a href="${root}${page.kind === "docs" ? "docs/" : "tutorials/"}">${page.kind === "docs" ? "Manual" : "Tutorials"}</a></nav><h1>${page.title}</h1><p>${page.description}</p></header>
      ${page.body}${articlefoot(page, pages, root)}
    </main>${ontoc(page)}
  </div>`;

export const render = (page: Page, pages: Page[]) => {
  const root = rootfor(page.path);
  const canonical = `${siteurl}${route(page)}`;
  const title = escapehtml(page.kind === "home" ? "Synapse — local memory for your tools" : `${page.title} — Synapse`);
  const description = escapehtml(page.description);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${title}</title>
  <meta name="description" content="${description}">
  <meta name="theme-color" content="#352879">
  <meta property="og:title" content="${title}">
  <meta property="og:description" content="${description}">
  <meta property="og:type" content="website">
  <meta property="og:url" content="${canonical}">
  <link rel="canonical" href="${canonical}">
  <link rel="icon" href="${root}icon.svg" type="image/svg+xml">
  <link rel="preload" href="${root}pixel.ttf" as="font" type="font/ttf" crossorigin>
  <link rel="stylesheet" href="${root}style.css">
  <script type="module" src="${root}site.js"></script>
</head>
<body class="${page.kind}" data-root="${root}">
  <div class="shell">${topnav(page, root)}${page.kind === "home" ? page.body : article(page, pages, root)}${footer(root)}</div>
</body>
</html>`;
};
