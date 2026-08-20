import { join, resolve } from "path";

const repositoryRoot = join(import.meta.dir, "..", "..", "..");
const wasmBindgenVersion = "0.2.122";

type BuildPlaygroundOptions = {
  outDir?: string;
  target?: string;
  profile?: string;
};

export async function buildPlayground(
  options: BuildPlaygroundOptions,
): Promise<void> {
  await ensureWasmBindgen();
  await run("cargo", [
    "build",
    "-p",
    "galfus-playground-web",
    "--target",
    "wasm32-unknown-unknown",
    "--features",
    "wasm",
    "--release",
    "--locked",
  ]);

  const target = options.target ?? "web";
  const profile = options.profile ?? "release";
  const outDir = options.outDir
    ? resolve(options.outDir)
    : join(repositoryRoot, "build", `galfus-playground-web-${profile}`);

  await run("wasm-bindgen", [
    "--target",
    target,
    "--out-dir",
    outDir,
    "--out-name",
    "galfus_playground_web",
    join(
      repositoryRoot,
      "target",
      "wasm32-unknown-unknown",
      "release",
      "galfus_playground_web.wasm",
    ),
  ]);
}

async function ensureWasmBindgen(): Promise<void> {
  const installedVersion = await getWasmBindgenVersion();
  if (installedVersion === wasmBindgenVersion) {
    return;
  }

  if (installedVersion) {
    console.log(
      `Updating wasm-bindgen-cli from ${installedVersion} to ${wasmBindgenVersion} for compatibility.`,
    );
  } else {
    console.log(`Installing wasm-bindgen-cli ${wasmBindgenVersion}.`);
  }

  await run("cargo", [
    "install",
    "wasm-bindgen-cli",
    "--version",
    wasmBindgenVersion,
    "--locked",
  ]);
}

async function getWasmBindgenVersion(): Promise<string | undefined> {
  const wasmBindgenPath = Bun.which("wasm-bindgen");
  if (!wasmBindgenPath) {
    return undefined;
  }

  const process = Bun.spawn([wasmBindgenPath, "--version"], {
    cwd: repositoryRoot,
    stderr: "ignore",
    stdout: "pipe",
  });

  if ((await process.exited) !== 0) {
    return undefined;
  }

  const output = (await new Response(process.stdout).text()).trim();
  return output.split(" ").at(-1);
}

async function run(command: string, args: string[]): Promise<void> {
  const process = Bun.spawn([command, ...args], {
    cwd: repositoryRoot,
    stderr: "inherit",
    stdout: "inherit",
  });
  const exitCode = await process.exited;

  if (exitCode !== 0) {
    throw new Error(`${command} exited with code ${exitCode}.`);
  }
}
