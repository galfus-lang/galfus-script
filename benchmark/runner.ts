import { spawn } from 'bun';

async function runCommand(cmd: string[]): Promise<{ time: number; result: number } | null> {
  try {
    const proc = spawn(cmd);
    const output = await new Response(proc.stdout).text();
    await proc.exited;

    const resultMatch = output.match(/RESULT=(\d+)/);
    const timeMatch = output.match(/TIME_MS=(\d+)/);

    if (resultMatch && timeMatch) {
      return {
        result: parseInt(resultMatch[1], 10),
        time: parseInt(timeMatch[1], 10),
      };
    }
    return null;
  } catch (e) {
    return null;
  }
}

async function main() {
  console.log('Compiling Galfus Engine...');
  const buildProc = spawn(['cargo', 'build', '--profile', 'release']);
  await buildProc.exited;

  console.log('Running benchmarks (Fibonacci 35)...\n');

  const targets = [
    { name: 'Galfus Script', cmd: ['./target/release/galfus-cli', 'run', 'benchmark/fib.gfs'] },
    {
      name: 'JavaScript (Bun)',
      cmd: [
        process.env.BUN_INSTALL ? `${process.env.BUN_INSTALL}/bin/bun` : 'bun',
        'benchmark/fib.js',
      ],
    },
    { name: 'Python 3', cmd: ['python3', 'benchmark/fib.py'] },
    { name: 'Lua 5.4', cmd: ['lua', 'benchmark/fib.lua'] },
    { name: 'Lua JIT', cmd: ['luajit', 'benchmark/fib.lua'] },
  ];

  const results = [];

  for (const target of targets) {
    process.stdout.write(`Testing ${target.name}... `);
    const data = await runCommand(target.cmd);
    if (data) {
      console.log(`${data.time}ms`);
      results.push({
        Language: target.name,
        'Time (ms)': data.time,
        Result: data.result,
      });
    } else {
      console.log('Failed or Skipped');
    }
  }

  console.log('\n--- Benchmark Results ---');
  // Sort by Time
  results.sort((a, b) => a['Time (ms)'] - b['Time (ms)']);
  console.table(results);
}

main().catch(console.error);
