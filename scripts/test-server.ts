import { connect } from "node:net";

const SERVER_URL = "http://127.0.0.1:8080";
const SERVER_COMMAND = [
  "./target/debug/galfus-cli",
  "run",
  "examples/server_auto.gfs",
];
const READY_TIMEOUT_MS = 10_000;
const WEBSOCKET_TIMEOUT_MS = 5_000;

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

async function mirrorOutput(
  stream: ReadableStream<Uint8Array>,
  output: { value: string },
  destination: typeof process.stdout,
): Promise<void> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    const text = decoder.decode(value, { stream: true });
    output.value += text;
    destination.write(text);
  }
  const trailing = decoder.decode();
  output.value += trailing;
  destination.write(trailing);
}

async function waitFor(
  condition: () => boolean,
  timeoutMs: number,
  message: string,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (condition()) return;
    await Bun.sleep(25);
  }
  throw new Error(message);
}

async function waitForServer(serverProcess: Bun.Subprocess): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS;
  let lastError: unknown;

  while (Date.now() < deadline) {
    if (serverProcess.exitCode !== null) {
      throw new Error(
        `O servidor encerrou antes de ficar pronto (exit code ${serverProcess.exitCode}).`,
      );
    }

    try {
      const response = await fetch(`${SERVER_URL}/api/data`, {
        signal: AbortSignal.timeout(500),
      });
      await response.body?.cancel();
      return;
    } catch (error) {
      lastError = error;
      await Bun.sleep(100);
    }
  }

  throw new Error(
    `O servidor não ficou pronto em ${READY_TIMEOUT_MS} ms: ${String(lastError)}`,
  );
}

async function curl(
  args: string[],
): Promise<{ stdout: string; stderr: string }> {
  const process = Bun.spawn(["curl", ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);

  if (exitCode !== 0) {
    throw new Error(`curl falhou (exit code ${exitCode}): ${stderr.trim()}`);
  }

  return { stdout, stderr };
}

function assertHttpResponse(
  output: string,
  protocol: "HTTP/1.1" | "HTTP/2",
): void {
  const separator = output.indexOf("\r\n\r\n");
  assert(
    separator >= 0,
    `curl não retornou headers HTTP completos:\n${output}`,
  );

  const headers = output.slice(0, separator);
  const body = output.slice(separator + 4);
  assert(
    new RegExp(`^${protocol.replace(".", "\\.")} 200(?:\\s|$)`).test(headers),
    `Esperava ${protocol} 200, recebido:\n${headers}`,
  );
  assert(
    /^content-type:\s*application\/json\b/im.test(headers),
    `Content-Type inválido:\n${headers}`,
  );
  let payload: unknown;
  try {
    payload = JSON.parse(body);
  } catch {
    throw new Error(`Resposta declarada como JSON, mas inválida: ${body}`);
  }
  assert(
    typeof payload === "object" &&
      payload !== null &&
      "status" in payload &&
      payload.status === "ok",
    `Payload inesperado: ${body}`,
  );
}

function assertStatusResponse(output: string, status: number): void {
  const separator = output.indexOf("\r\n\r\n");
  assert(
    separator >= 0,
    `curl não retornou headers HTTP completos:\n${output}`,
  );
  assert(
    new RegExp(`^HTTP/1\\.1 ${status}(?:\\s|$)`).test(
      output.slice(0, separator),
    ),
    `Esperava HTTP/1.1 ${status}, recebido:\n${output.slice(0, separator)}`,
  );
}

function splitHttpResponse(output: string): { headers: string; body: string } {
  const separator = output.indexOf("\r\n\r\n");
  assert(
    separator >= 0,
    `curl não retornou headers HTTP completos:\n${output}`,
  );
  return {
    headers: output.slice(0, separator),
    body: output.slice(separator + 4),
  };
}

async function testRequestContract(): Promise<void> {
  const url = await curl([
    "--http1.1",
    "--silent",
    "--show-error",
    "--dump-header",
    "-",
    "--output",
    "-",
    `${SERVER_URL}/url?check=1`,
  ]);
  const urlResponse = splitHttpResponse(url.stdout);
  assert(
    /^HTTP\/1\.1 200(?:\s|$)/.test(urlResponse.headers),
    `Query string não chegou ao Request:\n${urlResponse.headers}`,
  );
  assert(
    urlResponse.body === "url-ok",
    `Corpo inesperado para URL: ${urlResponse.body}`,
  );

  const body = "echo body \u{1F680}";
  const echo = await curl([
    "--http1.1",
    "--silent",
    "--show-error",
    "--request",
    "POST",
    "--header",
    "Content-Type: text/plain",
    "--data-binary",
    body,
    "--dump-header",
    "-",
    "--output",
    "-",
    `${SERVER_URL}/echo`,
  ]);
  const echoResponse = splitHttpResponse(echo.stdout);
  assert(
    /^HTTP\/1\.1 201(?:\s|$)/.test(echoResponse.headers),
    `Método POST não chegou ao Request:\n${echoResponse.headers}`,
  );
  assert(
    /^content-type:\s*application\/octet-stream\b/im.test(echoResponse.headers),
    `Header Content-Type ausente:\n${echoResponse.headers}`,
  );
  assert(
    /^x-server-test:\s*echo\b/im.test(echoResponse.headers),
    `Segundo header de resposta ausente:\n${echoResponse.headers}`,
  );
  assert(
    echoResponse.body === body,
    `Corpo do Request não foi preservado: ${echoResponse.body}`,
  );

  const requestHeader = await curl([
    "--http1.1",
    "--silent",
    "--show-error",
    "--header",
    "X-Request-Test: present",
    "--dump-header",
    "-",
    "--output",
    "-",
    `${SERVER_URL}/request-header`,
  ]);
  const requestHeaderResponse = splitHttpResponse(requestHeader.stdout);
  assert(
    /^HTTP\/1\.1 200(?:\s|$)/.test(requestHeaderResponse.headers),
    `Header não chegou ao Request:\n${requestHeaderResponse.headers}`,
  );
  assert(
    requestHeaderResponse.body === "header-ok",
    `Header do Request não foi preservado: ${requestHeaderResponse.body}`,
  );

  const method = await curl([
    "--http1.1",
    "--silent",
    "--show-error",
    "--request",
    "GET",
    "--dump-header",
    "-",
    "--output",
    "-",
    `${SERVER_URL}/echo`,
  ]);
  assertStatusResponse(method.stdout, 405);
}

async function testConcurrentRequests(): Promise<void> {
  const responses = await Promise.all(
    Array.from({ length: 8 }, () => fetch(`${SERVER_URL}/api/data`)),
  );
  for (const response of responses) {
    assert(
      response.status === 200,
      `Resposta concorrente inválida: ${response.status}`,
    );
    await response.body?.cancel();
  }
}

async function webSocketMessageToText(data: unknown): Promise<string> {
  if (typeof data === "string") return data;
  if (data instanceof Blob) return data.text();
  if (data instanceof ArrayBuffer || ArrayBuffer.isView(data))
    return new TextDecoder().decode(data);
  throw new Error(
    `Tipo de mensagem WebSocket inesperado: ${Object.prototype.toString.call(data)}`,
  );
}

async function testWebSocket(message: string | Uint8Array): Promise<void> {
  const expected =
    typeof message === "string" ? message : new TextDecoder().decode(message);

  await new Promise<void>((resolve, reject) => {
    const ws = new WebSocket("ws://127.0.0.1:8080/ws");
    const timeout = setTimeout(() => {
      ws.close();
      reject(
        new Error(
          `Timeout de ${WEBSOCKET_TIMEOUT_MS} ms esperando o echo WebSocket.`,
        ),
      );
    }, WEBSOCKET_TIMEOUT_MS);

    ws.onopen = () => ws.send(message);
    ws.onmessage = async (event) => {
      try {
        const received = await webSocketMessageToText(event.data);
        assert(
          received === expected,
          `Echo WebSocket divergente: esperado ${expected}, recebido ${received}`,
        );
        clearTimeout(timeout);
        ws.close(1000, "test complete");
        ws.onclose = () => resolve();
      } catch (error) {
        clearTimeout(timeout);
        ws.close();
        reject(error);
      }
    };
    ws.onerror = () => {
      clearTimeout(timeout);
      reject(new Error("O cliente WebSocket reportou um erro."));
    };
  });
}

async function testWebSocketError(serverOutput: {
  value: string;
}): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const socket = connect(8080, "127.0.0.1");
    let handshakeComplete = false;
    let response = "";
    const timeout = setTimeout(() => {
      socket.destroy();
      reject(
        new Error(
          "Timeout esperando o servidor rejeitar o frame WebSocket inválido.",
        ),
      );
    }, WEBSOCKET_TIMEOUT_MS);

    socket.on("connect", () => {
      socket.write(
        "GET /ws HTTP/1.1\r\n" +
          "Host: 127.0.0.1:8080\r\n" +
          "Connection: Upgrade\r\n" +
          "Upgrade: websocket\r\n" +
          "Sec-WebSocket-Version: 13\r\n" +
          "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
      );
    });
    socket.on("data", (chunk) => {
      response += chunk.toString();
      if (!handshakeComplete && response.includes("\r\n\r\n")) {
        handshakeComplete = true;
        // FIN + RSV1 + masked empty text frame. RSV1 without an extension is invalid.
        socket.write(Buffer.from([0xc1, 0x80, 0, 0, 0, 0]));
      }
    });
    socket.on("close", () => {
      clearTimeout(timeout);
      if (handshakeComplete) resolve();
      else
        reject(new Error("O handshake WebSocket inválido não foi concluído."));
    });
    socket.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });

  await waitFor(
    () =>
      serverOutput.value.includes("WebSocket error with status") &&
      serverOutput.value.includes("-1"),
    WEBSOCKET_TIMEOUT_MS,
    "onError não foi chamado após o frame WebSocket inválido.",
  );
}

async function testBindConflict(): Promise<void> {
  const process = Bun.spawn(SERVER_COMMAND, {
    stdout: "ignore",
    stderr: "ignore",
  });
  const timeout = Symbol("timeout");
  const result = await Promise.race([
    process.exited,
    Bun.sleep(3_000).then(() => timeout),
  ]);
  if (result === timeout) {
    process.kill();
    await process.exited;
    throw new Error(
      "O segundo servidor não falhou ao tentar usar uma porta ocupada.",
    );
  }
  assert(result !== 0, "O segundo servidor deveria encerrar com erro de bind.");
}

async function main(): Promise<void> {
  console.log("Construindo o Galfus CLI...");
  const build = Bun.spawn(["cargo", "build", "-p", "galfus-cli"], {
    stdout: "inherit",
    stderr: "inherit",
  });
  assert((await build.exited) === 0, "Falha ao construir o Galfus CLI.");

  console.log("Iniciando o servidor Galfus nativo...");
  const serverProcess = Bun.spawn(SERVER_COMMAND, {
    stdout: "pipe",
    stderr: "pipe",
  });
  const serverStdout = { value: "" };
  const serverStderr = { value: "" };
  const stdoutTask = mirrorOutput(
    serverProcess.stdout,
    serverStdout,
    process.stdout,
  );
  const stderrTask = mirrorOutput(
    serverProcess.stderr,
    serverStderr,
    process.stderr,
  );

  try {
    await waitForServer(serverProcess);

    console.log("[1/8] Testando HTTP/1.1...");
    const http1 = await curl([
      "--http1.1",
      "--silent",
      "--show-error",
      "--dump-header",
      "-",
      "--output",
      "-",
      `${SERVER_URL}/api/data`,
    ]);
    assertHttpResponse(http1.stdout, "HTTP/1.1");

    console.log("[2/8] Testando HTTP/2 h2c...");
    const http2 = await curl([
      "--http2-prior-knowledge",
      "--silent",
      "--show-error",
      "--dump-header",
      "-",
      "--output",
      "-",
      `${SERVER_URL}/api/data`,
    ]);
    assertHttpResponse(http2.stdout, "HTTP/2");

    console.log("[3/8] Testando Response com campos padrão...");
    const notFound = await curl([
      "--http1.1",
      "--silent",
      "--show-error",
      "--dump-header",
      "-",
      "--output",
      "-",
      `${SERVER_URL}/missing`,
    ]);
    assertStatusResponse(notFound.stdout, 404);

    console.log("[4/8] Testando Request, URL e headers...");
    await testRequestContract();

    console.log("[5/8] Testando requisições concorrentes...");
    await testConcurrentRequests();

    console.log("[6/8] Testando WebSocket texto e binário...");
    await testWebSocket("Hello WebSocket in Galfus!");
    await testWebSocket(new TextEncoder().encode("binary WebSocket payload"));

    console.log("[7/8] Testando onError do WebSocket...");
    await testWebSocketError(serverStdout);

    console.log("[8/8] Testando conflito de porta...");
    await testBindConflict();
    console.log("Todos os testes passaram.");
  } finally {
    console.log("Encerrando o servidor...");
    if (serverProcess.exitCode === null) serverProcess.kill();
    await serverProcess.exited;
    await Promise.all([stdoutTask, stderrTask]);
  }
}

main().catch((error: unknown) => {
  console.error("Teste do servidor falhou:", error);
  process.exitCode = 1;
});
