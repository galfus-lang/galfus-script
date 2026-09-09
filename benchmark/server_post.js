const port = Number.parseInt(process.argv[2] ?? "18080", 10);

Bun.serve({
  port,
  fetch(req) {
    return new Response(req.body, { status: 200 });
  },
});
