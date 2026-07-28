import type { Page } from "./types";
import { repositoryurl, siteurl } from "./deploy";

const depth = (path: string) => path.split("/").length - 1;
const rootfor = (path: string) => "../".repeat(depth(path));
const route = (page: Page) => page.path.replace(/index\.html$/, "");

const current = (page: Page, kind: Page["kind"]) =>
  page.kind === kind ? ' aria-current="page"' : "";

const topnav = (page: Page, root: string) => `
  <a class="skip" href="#content">Skip to content</a>
  <header class="topbar">
    <a class="wordmark" href="${root}" aria-label="Synaps home">
      <img src="${root}icon.svg" width="30" height="30" alt="">
      <span>Synaps</span>
    </a>
    <button class="navtoggle" type="button" aria-expanded="false" aria-controls="topnav">Menu</button>
    <nav id="topnav" aria-label="Primary">
      <a href="${root}docs/"${current(page, "docs")}>Docs</a>
      <a href="${root}tutorials/"${current(page, "tutorial")}>Tutorials</a>
      <a href="${repositoryurl}">GitHub <span aria-hidden="true">↗</span></a>
      <a class="navdownload" href="${repositoryurl}/releases/latest">Download</a>
    </nav>
  </header>`;

const group = (pages: Page[], kind: Page["kind"], page: Page, root: string) =>
  pages
    .filter((item) => item.kind === kind)
    .map(
      (item) => `
        <li><a href="${root}${route(item)}"${
          item.path === page.path ? ' aria-current="page"' : ""
        }>${item.title}</a></li>`,
    )
    .join("");

const rail = (page: Page, pages: Page[], root: string) => `
  <aside class="docsrail" aria-label="Documentation navigation">
    <div class="searchbox">
      <label for="search">Search the guide</label>
      <input id="search" type="search" placeholder="Memory, vaults, restore…" autocomplete="off">
      <div class="searchresults" data-results hidden></div>
      <span class="searchstatus" data-searchstatus role="status" aria-live="polite"></span>
    </div>
    <details class="railnav" open>
      <summary>Browse all guides</summary>
      <nav aria-label="All guides">
        <h2>Documentation</h2>
        <ul>${group(pages, "docs", page, root)}</ul>
        <h2>Tutorials</h2>
        <ul>${group(pages, "tutorial", page, root)}</ul>
      </nav>
    </details>
  </aside>`;

const ontoc = (page: Page) =>
  page.toc?.length
    ? `<aside class="ontoc" aria-label="On this page">
        <span>On this page</span>
        <ol>${page.toc
          .map((item) => `<li><a href="#${item.id}">${item.label}</a></li>`)
          .join("")}</ol>
      </aside>`
    : "";

const footer = (root: string) => `
  <footer class="footer">
    <div>
      <a class="wordmark" href="${root}"><img src="${root}icon.svg" width="26" height="26" alt="">Synaps</a>
      <p>Local memory and scoped credentials for developer tools.</p>
    </div>
    <nav aria-label="Footer">
      <a href="${root}docs/">Documentation</a>
      <a href="${root}tutorials/">Tutorials</a>
      <a href="${repositoryurl}">Source</a>
      <a href="${repositoryurl}/releases">Releases</a>
    </nav>
    <p class="footerfine">Local-first. No account required. macOS 13+ beta.</p>
  </footer>`;

const article = (page: Page, pages: Page[], root: string) => `
  <div class="doclayout">
    ${rail(page, pages, root)}
    <main class="article" id="content">
      <header class="articlehead">
        <h1>${page.title}</h1>
        <span>${page.description}</span>
      </header>
      ${page.body}
    </main>
    ${ontoc(page)}
  </div>`;

export const render = (page: Page, pages: Page[]) => {
  const root = rootfor(page.path);
  const canonical = `${siteurl}${route(page)}`;
  const contract = page.contract ? `${page.contract}\n` : "";
  return `${contract}<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${page.title === "Synaps" ? "Synaps — one memory, every tool" : `${page.title} — Synaps`}</title>
  <meta name="description" content="${page.description}">
  <meta name="theme-color" content="#0a43c8">
  <meta property="og:title" content="${page.title}">
  <meta property="og:description" content="${page.description}">
  <meta property="og:type" content="website">
  <meta property="og:url" content="${canonical}">
  <link rel="canonical" href="${canonical}">
  <link rel="icon" href="${root}icon.svg" type="image/svg+xml">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Barlow+Condensed:wght@500;600;700&family=Fragment+Mono&family=Manrope:wght@400;500;600;700&display=swap">
  <link rel="stylesheet" href="${root}style.css">
  <script type="module" src="${root}site.js"></script>
</head>
<body class="${page.kind}" data-root="${root}">
  ${topnav(page, root)}
  ${page.kind === "home" ? page.body : article(page, pages, root)}
  ${footer(root)}
</body>
</html>`;
};
