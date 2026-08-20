import { join } from "path";
import { mkdir, copyFile } from "fs/promises";

const repositoryRoot = join(import.meta.dir, "..", "..", "..");

const ALIAS_TO_TARGET: Record<string, string> = {
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "windows-x64": "x86_64-pc-windows-msvc",
  "windows-arm64": "aarch64-pc-windows-msvc",
  "macos-x64": "x86_64-apple-darwin",
  "macos-arm64": "aarch64-apple-darwin",
};

// Map from the Rust target back to our clean build names
const TARGET_MAP: Record<string, string> = {
  "x86_64-unknown-linux-gnu": "linux-x64",
  "aarch64-unknown-linux-gnu": "linux-arm64",
  "x86_64-pc-windows-msvc": "windows-x64",
  "aarch64-pc-windows-msvc": "windows-arm64",
  "x86_64-apple-darwin": "macos-x64",
  "aarch64-apple-darwin": "macos-arm64",
};

type BuildHostOptions = {
  target?: string;
  profile: string;
};

export async function buildHostPackages(
  options: BuildHostOptions,
): Promise<void> {
  const rawTarget = options.target || (await getNativeTarget());
  const rustTarget = ALIAS_TO_TARGET[rawTarget] || rawTarget;
  const buildName = TARGET_MAP[rustTarget] || rustTarget;

  if (rustTarget === "web") {
    await buildWebHostPackage(options);
    return;
  }

  console.log(`Building host package for target: ${rustTarget} (${buildName})`);

  const cargoArgs = [
    "build",
    "-p",
    "galfus-host-native",
    "--target",
    rustTarget,
    "--locked",
  ];

  let cargoProfile = "dev";
  if (options.profile === "fastest") {
    cargoProfile = "release";
  } else if (options.profile === "minimal") {
    cargoProfile = "release-min";
  }

  if (cargoProfile !== "dev") {
    cargoArgs.push("--profile", cargoProfile);
  }

  await run("cargo", cargoArgs);

  const ext = rustTarget.includes("windows") ? ".exe" : "";
  const profileDir = cargoProfile === "dev" ? "debug" : cargoProfile;
  const sourceBinary = join(
    repositoryRoot,
    "target",
    rustTarget,
    profileDir,
    `galfus-host-native${ext}`,
  );

  const buildDir = join(repositoryRoot, "build");
  await mkdir(buildDir, { recursive: true });

  const destBinary = join(
    buildDir,
    `galfus-${buildName}-${options.profile}${ext}`,
  );

  await copyFile(sourceBinary, destBinary);
  console.log(`Built ExecutionHostPackage: ${destBinary}`);
}

async function getNativeTarget(): Promise<string> {
  const process = Bun.spawn(["rustc", "-vV"], {
    stdout: "pipe",
  });
  const output = await new Response(process.stdout).text();
  const match = output.match(/host:\s+([^\s]+)/);
  if (!match) {
    throw new Error("Failed to determine native rust target");
  }
  return match[1] ?? "";
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

async function buildWebHostPackage(options: BuildHostOptions): Promise<void> {
  const profile = options.profile || "release";
  console.log(`Building host package for target: web (${profile})`);

  let cargoProfile = "dev";
  if (profile === "fastest") {
    cargoProfile = "release";
  } else if (profile === "minimal") {
    cargoProfile = "release-min";
  } else if (profile === "release") {
    cargoProfile = "release";
  }

  const cargoArgs = [
    "build",
    "-p",
    "galfus-host-web",
    "--target",
    "wasm32-unknown-unknown",
    "--locked",
  ];

  if (cargoProfile !== "dev") {
    cargoArgs.push("--profile", cargoProfile);
  }

  await run("cargo", cargoArgs);

  const wasmBindgenVersion = "0.2.122";
  await run("cargo", [
    "install",
    "wasm-bindgen-cli",
    "--version",
    wasmBindgenVersion,
    "--locked",
  ]);

  const profileDir = cargoProfile === "dev" ? "debug" : cargoProfile;
  const sourceWasm = join(
    repositoryRoot,
    "target",
    "wasm32-unknown-unknown",
    profileDir,
    "galfus_host_web.wasm",
  );
  const outDir = join(repositoryRoot, "build", `galfus-host-web-${profile}`);

  await run("wasm-bindgen", [
    "--target",
    "web",
    "--out-dir",
    outDir,
    "--out-name",
    "galfus_host_web",
    sourceWasm,
  ]);

  console.log(`Built ExecutionHostPackage (Web): ${outDir}`);
}
