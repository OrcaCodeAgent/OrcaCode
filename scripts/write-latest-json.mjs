import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const repo = "OrcaCodeAgent/OrcaCode";
const version = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const tag = process.env.GITHUB_REF_NAME ?? `v${version}`;
if (tag !== `v${version}`) {
  console.error(`Tag ${tag} must match the app version v${version} in src-tauri/tauri.conf.json.`);
  process.exit(1);
}

const directory = "src-tauri/target/release/bundle/macos";
const archive = readdirSync(directory).find((name) => name.endsWith(".app.tar.gz"));
if (!archive) {
  console.error(`No updater archive in ${directory}.`);
  process.exit(1);
}

const arch = process.arch === "arm64" ? "aarch64" : process.arch === "x64" ? "x86_64" : process.arch;
const signature = readFileSync(`${directory}/${archive}.sig`, "utf8").trim();
const url = `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(archive)}`;
const manifest = {
  version,
  notes: `Orca Code ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    [`darwin-${arch}`]: { signature, url },
  },
};

writeFileSync("latest.json", `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote latest.json for darwin-${arch} ${version}`);
