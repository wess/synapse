import type { Page } from "./types";

export const sections = [
  { title: "Start here", paths: ["docs/", "docs/install/", "docs/harnesses/", "docs/app/"] },
  { title: "Reference", paths: ["docs/memory/", "docs/mcp/", "docs/mesh/", "docs/skills/", "docs/servers/", "docs/sources/"] },
  { title: "Your machine", paths: ["docs/vault/", "docs/data/", "docs/config/", "docs/security/", "docs/sync/", "docs/troubleshoot/"] },
  { title: "Build with it", paths: ["docs/cli/", "docs/developing/", "docs/synapsecore/", "docs/embedding/", "docs/retrieval/", "docs/map/", "docs/startup/", "docs/windows/", "docs/federation/"] },
];

export const route = (page: Page): string => page.path.replace(/index\.html$/, "");

export const sourcepath = (page: Page): string => {
  if (page.source) return page.source;
  const path = page.path.replace(/\/index\.html$/, "");
  return `website/${path === "docs" || path === "tutorials" ? `${path}/index` : path}.ts`;
};
