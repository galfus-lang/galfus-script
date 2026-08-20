import { rm, mkdir } from "fs/promises";
import { resolve } from "path";

type ExtensionManifest = {
  name?: string;
  version?: string;
};

const rootDirectory = resolve(import.meta.dir, "..");
const manifestPath = resolve(rootDirectory, "package.json");
const vscePath = resolve(
  rootDirectory,
  "node_modules",
  "@vscode",
  "vsce",
  "vsce",
);

const manifest = (await Bun.file(manifestPath).json()) as ExtensionManifest;

if (!manifest.name) {
  throw new Error("Missing package name");
}

if (!manifest.version) {
  throw new Error("Missing package version");
}

const nodeVersionProcess = Bun.spawnSync(["node", "--version"]);

if (nodeVersionProcess.exitCode !== 0) {
  throw new Error("Node.js is not available");
}

const nodeVersion = new TextDecoder().decode(nodeVersionProcess.stdout).trim();

const nodeMajorVersion = Number.parseInt(
  nodeVersion.replace(/^v/, "").split(".")[0] ?? "",
  10,
);

if (!Number.isFinite(nodeMajorVersion) || nodeMajorVersion < 22) {
  throw new Error(`Node.js 22 or newer is required, received ${nodeVersion}`);
}

const outputDirectory = resolve(rootDirectory, "dist");
const outputPath = resolve(
  outputDirectory,
  `${manifest.name}-${manifest.version}.vsix`,
);

await mkdir(outputDirectory, { recursive: true });
await rm(outputPath, { force: true });

const process = Bun.spawn(
  ["node", vscePath, "package", "--no-dependencies", "--out", outputPath],
  {
    cwd: rootDirectory,
    stdout: "inherit",
    stderr: "inherit",
  },
);

const exitCode = await process.exited;

if (exitCode !== 0) {
  throw new Error(`VSIX packaging failed with exit code ${exitCode}`);
}

console.log(`VSIX generated: ${outputPath}`);
