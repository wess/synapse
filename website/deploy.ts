export const sitepath = "/synapse/";
export const siteurl = `https://wess.io${sitepath}`;
export const repositoryurl = "https://github.com/wess/synapse";
export const releaseurl = `${repositoryurl}/releases/latest`;
export const downloadurl = `${repositoryurl}/releases/latest/download/synapse.zip`;

const manifest = await Bun.file(new URL("../crates/synapse/Cargo.toml", import.meta.url)).text();
export const version = manifest.match(/^version = "([^"]+)"/m)?.[1] ?? "development";
