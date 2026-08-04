import { mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { pages } from "./content";
import { siteurl } from "./deploy";
import { render } from "./template";

const project = join(import.meta.dir, "..");
const output = join(project, "site");

if (!output.endsWith("/site")) {
  throw new Error(`refusing to replace unexpected output path: ${output}`);
}

await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

for (const page of pages) {
  const target = join(output, page.path);
  await mkdir(dirname(target), { recursive: true });
  await Bun.write(target, render(page, pages));
}

const styles = await Promise.all(
  ["base.css", "landing.css", "docs.css", "responsive.css"].map((asset) =>
    Bun.file(join(import.meta.dir, asset)).text(),
  ),
);
await Bun.write(join(output, "style.css"), `${styles.join("\n")}\n`);

for (const asset of ["site.js", "grain.jpg"]) {
  await Bun.write(join(output, asset), Bun.file(join(import.meta.dir, asset)));
}

await Bun.write(
  join(output, "icon.svg"),
  Bun.file(join(project, "crates", "synapse", "assets", "icon.svg")),
);
await Bun.write(join(output, ".nojekyll"), "");

const search = pages
  .filter((page) => page.kind !== "home")
  .map((page) => ({
    title: page.title,
    description: page.description,
    path: page.path.replace(/index\.html$/, ""),
    text: page.body.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim(),
  }));

await Bun.write(join(output, "search.json"), JSON.stringify(search));
await Bun.write(
  join(output, "robots.txt"),
  `User-agent: *\nAllow: /\nSitemap: ${siteurl}sitemap.xml\n`,
);
await Bun.write(
  join(output, "sitemap.xml"),
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${pages
    .map(
      (page) =>
        `\n  <url><loc>${siteurl}${page.path.replace(/index\.html$/, "")}</loc></url>`,
    )
    .join("")}\n</urlset>\n`,
);

console.log(`Built ${pages.length} pages in ${output}`);
