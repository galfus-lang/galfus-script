import { expect, test } from 'bun:test';

import init, { Playground } from '../../../build/galfus-playground-web-release/galfus_playground_web.js';

const wasm = Bun.file('build/galfus-playground-web-release/galfus_playground_web_bg.wasm');
const mainSource = Bun.file('hosts/galfus-playground-web/tests/fixtures/main.gfs');
const replacementSource = Bun.file('hosts/galfus-playground-web/tests/fixtures/replacement.gfs');
const invalidTypeSource = Bun.file('hosts/galfus-playground-web/tests/fixtures/invalid_type.gfs');

type Result = {
  ok: boolean;
  error?: string;
};

type CheckResult = {
  is_valid: boolean;
  diagnostics: string;
};

let initialization: Promise<void> | undefined;

async function initialize(): Promise<void> {
  initialization ??= (async () => {
    expect(await wasm.exists()).toBe(true);
    await init(await wasm.arrayBuffer());
  })();
  await initialization;
}

function result(value: string): Result {
  return JSON.parse(value) as Result;
}

function checkResult(value: string): CheckResult {
  return JSON.parse(value) as CheckResult;
}

test('runs the default playground source with arguments and stdout', async () => {
  await initialize();
  const playground = new Playground();
  const output: string[] = [];
  const decoder = new TextDecoder();
  const stdout = new WritableStream<Uint8Array>({
    write(chunk) {
      output.push(decoder.decode(chunk));
    },
  });

  expect(result(playground.setSource('src/main.gfs', await mainSource.text()))).toEqual({ ok: true });
  expect(checkResult(playground.check())).toEqual({ is_valid: true, diagnostics: '[]' });
  expect(result(playground.compile())).toEqual({ ok: true });

  expect(await playground.start({ args: ['first', 'second'], stdout })).toBe(17);
  expect(output.join('')).toBe('playground\n');
});

test('uses configured entries and recompiles replaced source', async () => {
  await initialize();
  const playground = new Playground();
  const config = '[module]\nname = "custom-playground"\ntarget = "app"\n[entry]\npath = "examples/main.gfs"\n';

  expect(result(playground.setConfig(config))).toEqual({ ok: true });
  expect(result(playground.setSource('examples/main.gfs', await mainSource.text()))).toEqual({ ok: true });
  expect(result(playground.compile())).toEqual({ ok: true });
  expect(await playground.start({ args: ['first', 'second'] })).toBe(17);

  expect(result(playground.setSource('examples/main.gfs', await replacementSource.text()))).toEqual({ ok: true });
  expect(checkResult(playground.check())).toEqual({ is_valid: true, diagnostics: '[]' });
  expect(result(playground.compile())).toEqual({ ok: true });
  expect(await playground.start({})).toBe(23);
});

test('returns semantic diagnostics and prevents compilation of invalid source', async () => {
  await initialize();
  const playground = new Playground();

  expect(result(playground.setSource('src/main.gfs', await invalidTypeSource.text()))).toEqual({ ok: true });

  const check = checkResult(playground.check());
  expect(check.is_valid).toBe(false);
  expect(check.diagnostics).toContain('T0001');

  const compilation = result(playground.compile());
  expect(compilation.ok).toBe(false);
  expect(compilation.error).toContain('playground compilation failed');
});
