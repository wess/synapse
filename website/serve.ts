import { extname, join, normalize } from "node:path";

const root = join(import.meta.dir, "..", "site");
const types: Record<string, string> = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".txt": "text/plain; charset=utf-8",
  ".xml": "application/xml; charset=utf-8",
};

const filepath = (request: Request) => {
  const url = new URL(request.url);
  let path = decodeURIComponent(url.pathname).replace(/^\/synaps\/?/, "");
  if (!path || path.endsWith("/")) path += "index.html";
  const safe = normalize(path).replace(/^(\.\.\/)+/, "");
  return join(root, safe);
};

const server = Bun.serve({
  port: Number(Bun.env.PORT ?? 4173),
  async fetch(request) {
    const path = filepath(request);
    const file = Bun.file(path);
    if (!(await file.exists())) return new Response("Not found", { status: 404 });
    return new Response(file, {
      headers: { "content-type": types[extname(path)] ?? "application/octet-stream" },
    });
  },
});

console.log(`Synaps site: http://127.0.0.1:${server.port}/synaps/`);
