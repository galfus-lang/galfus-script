import { join } from "node:path";

const FORBIDDEN_DEPENDENCIES = {
  "galfus-runtime": ["galfus-compiler", "galfus-frontend", "galfus-workspace"],
  "galfus-loader": ["galfus-compiler", "galfus-frontend", "galfus-workspace"],
  "galfus-vm": ["galfus-workspace"],
  "galfus-builtins": ["galfus-runtime"],
} as const;

interface CargoPackage {
  name: string;
  dependencies: Array<{ name: string }>;
}

interface CargoMetadata {
  packages: CargoPackage[];
}

export async function checkCrateDependencies(): Promise<void> {
  const cargo = Bun.spawn(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    {
      cwd: join(import.meta.dir, ".."),
      stdout: "pipe",
      stderr: "inherit",
    },
  );
  const output = await new Response(cargo.stdout).text();

  if ((await cargo.exited) !== 0) {
    throw new Error("failed to read Cargo metadata");
  }

  const packages = new Map(
    (JSON.parse(output) as CargoMetadata).packages.map((pkg) => [
      pkg.name,
      pkg,
    ]),
  );
  const violations: string[] = [];

  for (const [packageName, forbiddenDependencies] of Object.entries(
    FORBIDDEN_DEPENDENCIES,
  )) {
    const pkg = packages.get(packageName);
    if (!pkg) {
      continue;
    }
    const dependencies = new Set(
      pkg.dependencies.map((dependency) => dependency.name),
    );
    for (const dependency of forbiddenDependencies) {
      if (dependencies.has(dependency)) {
        violations.push(`${packageName} must not depend on ${dependency}`);
      }
    }
  }

  if (violations.length > 0) {
    throw new Error(
      `Forbidden crate dependencies detected:\n- ${violations.join("\n- ")}`,
    );
  }
}
