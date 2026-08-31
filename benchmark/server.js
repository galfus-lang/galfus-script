const port = Number.parseInt(process.argv[2] ?? "18080", 10);

Bun.serve({
  port,
  fetch() {
    return new Response(null, { status: 200 });
  },
});
