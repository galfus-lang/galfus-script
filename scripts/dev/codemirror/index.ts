import { join } from "path";
import { buildPlayground } from "../../playground/build";

export async function devCodemirror() {
  const publicDir = join(import.meta.dir, "public");
  const wasmDir = join(publicDir, "wasm");

  console.log("Building Galfus WASM Playground...");
  await buildPlayground({
    target: "web",
    outDir: wasmDir,
  });

  console.log(
    "Starting local dev server for CodeMirror on http://localhost:3000",
  );

  Bun.serve({
    port: 3000,
    async fetch(req) {
      const url = new URL(req.url);
      let path = url.pathname;
      if (path === "/") path = "/index.html";

      const filePath = join(publicDir, path);
      const file = Bun.file(filePath);

      if (await file.exists()) {
        return new Response(file);
      } else {
        return new Response("Not found", { status: 404 });
      }
    },
  });
}
