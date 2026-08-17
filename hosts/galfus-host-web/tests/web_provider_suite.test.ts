import { expect, test } from 'bun:test';

import init, { start } from '../../../build/galfus-host-web-release/galfus_host_web.js';

const fixture = Bun.file('target/host-web-provider-suite/providers.bin');
const hostWasm = Bun.file('build/galfus-host-web-release/galfus_host_web_bg.wasm');

test('executes the provider fixture through the web host', async () => {
  expect(await fixture.exists()).toBe(true);
  expect(await hostWasm.exists()).toBe(true);

  await init(await hostWasm.arrayBuffer());

  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const output: string[] = [];
  const stdin = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoder.encode('input'));
      controller.close();
    },
  });
  const stdout = new WritableStream<Uint8Array>({
    write(chunk) {
      output.push(decoder.decode(chunk));
    },
  });

  const exitCode = await start({
    blob: new Uint8Array(await fixture.arrayBuffer()),
    envs: { 'suite.value': 'web' },
    stdin,
    stdout,
  });

  expect(exitCode).toBe(0);
  expect(output.join('')).toBe('web\n');
});
