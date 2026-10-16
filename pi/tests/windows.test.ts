import { expect, test } from "bun:test";
import { copyFileSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { binary } from "../extensions/synapse/binary.ts";
import { connect, type Client } from "../extensions/synapse/client.ts";

const command = process.env.SYNAPSE_TEST_COMMAND;

test.skipIf(process.platform !== "win32" || !command)(
  "Windows pi discovers the CLI and exchanges memory over MCP",
  async () => {
    const root = mkdtempSync(join(tmpdir(), "synapsepi"));
    const installed = join(root, "Programs", "Synapse", "bin", "synapse.exe");
    const preferred = join(root, "tools", "synapse.exe");
    for (const path of [installed, preferred]) {
      mkdirSync(dirname(path), { recursive: true });
      copyFileSync(command!, path);
    }
    const names = ["LOCALAPPDATA", "PATH", "SYNAPSE_COMMAND", "SYNAPSE_DATA", "SYNAPSE_HOME", "SYNAPSE_PI_PACKAGE", "PI_CODING_AGENT_DIR"];
    const previous = new Map(names.map(name => [name, process.env[name]]));
    let client: Client | undefined;
    try {
      process.env.LOCALAPPDATA = root;
      process.env.PATH = "";
      delete process.env.SYNAPSE_COMMAND;
      delete process.env.SYNAPSE_PI_PACKAGE;
      delete process.env.PI_CODING_AGENT_DIR;
      process.env.SYNAPSE_DATA = join(root, "data");
      process.env.SYNAPSE_HOME = join(root, "home");
      expect(binary()).toBe(installed);
      process.env.PATH = dirname(preferred);
      expect(binary()).toBe(preferred);
      process.env.SYNAPSE_COMMAND = installed;
      expect(binary()).toBe(installed);
      delete process.env.SYNAPSE_COMMAND;
      const bundled = join(dirname(dirname(installed)), "pi");
      mkdirSync(bundled, { recursive: true });
      writeFileSync(join(bundled, "package.json"), JSON.stringify({ name: "synapse-pi" }));
      writeFileSync(join(root, "tools", "pi.cmd"), "@echo off\r\necho pi test\r\n");
      const settings = join(root, "home", ".pi", "agent", "settings.json");
      mkdirSync(dirname(settings), { recursive: true });
      writeFileSync(settings, JSON.stringify({ packages: [bundled] }));
      const report = () => {
        const result = spawnSync(installed, ["doctor", "--json"], { encoding: "utf8" });
        expect(result.status).toBe(0);
        return JSON.parse(result.stdout).tools.find((tool: { name: string }) => tool.name === "pi");
      };
      expect(report().connected).toBe(true);
      process.env.SYNAPSE_PI_PACKAGE = "npm:custom-pi";
      expect(report().connected).toBe(false);
      delete process.env.SYNAPSE_PI_PACKAGE;
      client = await connect(binary()!, root);
      expect(client.tools.some(tool => tool.name === "remember")).toBe(true);
      const saved = await client.call("remember", {
        content: "Windows pi bridge works", scope: "global", source: "windows test",
      });
      expect(saved.failed).toBe(false);
      const recalled = await client.call("recall", { query: "Windows pi bridge", limit: 1 });
      expect(recalled.failed).toBe(false);
      expect(recalled.text).toContain("Windows pi bridge works");
    } finally {
      client?.close();
      for (const [name, value] of previous) {
        if (value === undefined) delete process.env[name];
        else process.env[name] = value;
      }
    }
  },
  20_000,
);
