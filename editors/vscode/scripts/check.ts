import { existsSync } from "fs";
import { resolve } from "path";

type JsonObject = Record<string, unknown>;

type LanguageContribution = {
  id?: string;
  extensions?: string[];
  configuration?: string;
};

type GrammarContribution = {
  language?: string;
  scopeName?: string;
  path?: string;
};

type SnippetContribution = {
  language?: string;
  path?: string;
};

type ExtensionManifest = {
  name?: string;
  displayName?: string;
  description?: string;
  version?: string;
  publisher?: string;
  engines?: {
    vscode?: string;
  };
  contributes?: {
    languages?: LanguageContribution[];
    grammars?: GrammarContribution[];
    snippets?: SnippetContribution[];
  };
};

const rootDirectory = resolve(import.meta.dir, "..");

function fail(message: string): never {
  console.error(`Error: ${message}`);
  process.exit(1);
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    fail(message);
  }
}

function resolveProjectPath(relativePath: string): string {
  return resolve(rootDirectory, relativePath.replace(/^\.\//, ""));
}

async function readJson<T extends JsonObject>(
  relativePath: string,
): Promise<T> {
  const absolutePath = resolveProjectPath(relativePath);

  assert(existsSync(absolutePath), `Missing file: ${relativePath}`);

  try {
    const value = await Bun.file(absolutePath).json();

    assert(
      typeof value === "object" && value !== null && !Array.isArray(value),
      `Expected a JSON object: ${relativePath}`,
    );

    console.log(`Valid JSON: ${relativePath}`);

    return value as T;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    fail(`Invalid JSON in ${relativePath}: ${reason}`);
  }
}

function validateManifest(manifest: ExtensionManifest): void {
  const requiredStringFields = [
    ["name", manifest.name],
    ["displayName", manifest.displayName],
    ["description", manifest.description],
    ["version", manifest.version],
    ["publisher", manifest.publisher],
    ["engines.vscode", manifest.engines?.vscode],
  ] as const;

  for (const [field, value] of requiredStringFields) {
    assert(
      typeof value === "string" && value.length > 0,
      `Missing or invalid manifest field: ${field}`,
    );
  }

  assert(
    /^\d+\.\d+\.\d+$/.test(manifest.version ?? ""),
    `Invalid semantic version: ${manifest.version}`,
  );

  const languages = manifest.contributes?.languages ?? [];
  const grammars = manifest.contributes?.grammars ?? [];
  const snippets = manifest.contributes?.snippets ?? [];

  assert(languages.length > 0, "No language contributions found");
  assert(grammars.length > 0, "No grammar contributions found");

  const languageIds = new Set<string>();

  for (const language of languages) {
    assert(language.id, "Language contribution is missing an id");
    assert(
      !languageIds.has(language.id),
      `Duplicate language id: ${language.id}`,
    );

    languageIds.add(language.id);

    assert(
      Array.isArray(language.extensions) && language.extensions.length > 0,
      `Language ${language.id} has no extensions`,
    );

    for (const extension of language.extensions) {
      assert(
        extension.startsWith("."),
        `Invalid extension "${extension}" for ${language.id}`,
      );
    }

    assert(
      typeof language.configuration === "string",
      `Language ${language.id} has no configuration file`,
    );

    assert(
      existsSync(resolveProjectPath(language.configuration)),
      `Missing language configuration: ${language.configuration}`,
    );
  }

  for (const grammar of grammars) {
    assert(grammar.language, "Grammar is missing a language id");
    assert(
      languageIds.has(grammar.language),
      `Grammar references unknown language: ${grammar.language}`,
    );

    assert(grammar.scopeName, "Grammar is missing scopeName");
    assert(grammar.path, "Grammar is missing a file path");

    assert(
      existsSync(resolveProjectPath(grammar.path)),
      `Missing grammar file: ${grammar.path}`,
    );
  }

  for (const snippet of snippets) {
    assert(snippet.language, "Snippet is missing a language id");
    assert(
      languageIds.has(snippet.language),
      `Snippet references unknown language: ${snippet.language}`,
    );

    assert(snippet.path, "Snippet is missing a file path");
    assert(
      existsSync(resolveProjectPath(snippet.path)),
      `Missing snippet file: ${snippet.path}`,
    );
  }
}

async function validateGrammar(manifest: ExtensionManifest): Promise<void> {
  for (const contribution of manifest.contributes?.grammars ?? []) {
    assert(contribution.path, "Grammar is missing a path");
    assert(contribution.scopeName, "Grammar is missing scopeName");

    const grammar = await readJson<JsonObject>(contribution.path);

    assert(
      grammar.scopeName === contribution.scopeName,
      `Grammar scope mismatch in ${contribution.path}: expected ` +
        `"${contribution.scopeName}", received "${String(grammar.scopeName)}"`,
    );

    assert(
      Array.isArray(grammar.patterns),
      `Grammar has no patterns array: ${contribution.path}`,
    );

    assert(
      typeof grammar.repository === "object" && grammar.repository !== null,
      `Grammar has no repository object: ${contribution.path}`,
    );
  }
}

async function validateSnippets(manifest: ExtensionManifest): Promise<void> {
  for (const contribution of manifest.contributes?.snippets ?? []) {
    assert(contribution.path, "Snippet contribution is missing a path");

    const snippets = await readJson<JsonObject>(contribution.path);

    for (const [name, value] of Object.entries(snippets)) {
      assert(
        typeof value === "object" && value !== null && !Array.isArray(value),
        `Invalid snippet definition: ${name}`,
      );

      const snippet = value as JsonObject;

      assert(
        typeof snippet.prefix === "string" ||
          (Array.isArray(snippet.prefix) && snippet.prefix.length > 0),
        `Snippet has no valid prefix: ${name}`,
      );

      assert(
        typeof snippet.body === "string" ||
          (Array.isArray(snippet.body) && snippet.body.length > 0),
        `Snippet has no valid body: ${name}`,
      );
    }
  }
}

const manifest = await readJson<ExtensionManifest>("package.json");

validateManifest(manifest);

for (const language of manifest.contributes?.languages ?? []) {
  assert(language.configuration, "Language configuration is missing");
  await readJson<JsonObject>(language.configuration);
}

await validateGrammar(manifest);
await validateSnippets(manifest);

console.log("Extension validation passed");
