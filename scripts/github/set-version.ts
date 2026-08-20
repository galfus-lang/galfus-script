import { join } from "path";
import { readFile, writeFile } from "fs/promises";

const repositoryRoot = join(import.meta.dir, "..", "..", "..");

type SetVersionOptions = {
  component?: string;
};

export async function setVersion(options: SetVersionOptions): Promise<void> {
  const ref = process.env.GITHUB_REF_NAME;
  if (!ref) {
    throw new Error("GITHUB_REF_NAME environment variable is not set");
  }

  // Example refs:
  // stable/v1.0.0 -> 1.0.0
  // alpha/v1.0.0 -> 1.0.0-alpha
  // alpha/playground/v1.0.0 -> 1.0.0-alpha

  const parts = ref.split("/");
  const channel = parts[0];
  const versionPart = parts[parts.length - 1]!; // last part should be v1.0.0

  if (!versionPart.startsWith("v")) {
    throw new Error(`Invalid version format in branch name: ${ref}`);
  }

  const rawVersion = versionPart.slice(1);
  let finalVersion = rawVersion;

  if (channel !== "stable") {
    finalVersion = `${rawVersion}-${channel}`;
  }

  console.log(`Extracted version: ${finalVersion} from branch ${ref}`);

  const cargoTomlPath = join(repositoryRoot, "Cargo.toml");
  const cargoToml = await readFile(cargoTomlPath, "utf-8");

  // Replace version in [workspace.package] block
  const replaced = cargoToml.replace(
    /(\[workspace\.package\][\s\S]*?version\s*=\s*")[^"]+(")/,
    `$1${finalVersion}$2`,
  );

  if (cargoToml === replaced) {
    throw new Error("Failed to find [workspace.package] version in Cargo.toml");
  }

  await writeFile(cargoTomlPath, replaced);
  console.log(`Updated Cargo.toml with version ${finalVersion}`);

  const githubOutput = process.env.GITHUB_OUTPUT;
  if (githubOutput) {
    const outputs =
      [`version=${rawVersion}`, `tag=${channel}`].join("\n") + "\n";

    await writeFile(githubOutput, outputs, { flag: "a" });
    console.log(`Wrote version and tag to GITHUB_OUTPUT`);
  }
}
