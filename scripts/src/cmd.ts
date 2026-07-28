import { Command } from 'commander';

import { checkCrateDependencies } from './dependencies';
import { buildPlayground } from './playground/build';
import { setupExtension } from './setup/extension';

const program = new Command();
program
  .name('galfus-scripts')
  .description('Galfus repository automation commands');

const setup = program.command('setup').description('Local development setup commands');
const playground = program
  .command('playground')
  .description('Playground development and distribution commands');
const check = program.command('check').description('Repository validation commands');


setup
  .command('extension')
  .description('Install the local editor extension')
  .option('-v, --vscode', 'Install to VS Code and VS Code Insiders')
  .option('-a, --antigravity', 'Install to Antigravity IDE')
  .option('--all', 'Install to all editors (default)')
  .action(setupExtension);

playground
  .command('build')
  .description('Build the playground WebAssembly module and generate bindings')
  .option('-t, --target <target>', 'wasm-bindgen target (web, bundler, nodejs, etc)', 'web')
  .option('-o, --out-dir <path>', 'Output directory relative to the repository root')
  .action(buildPlayground);

check
  .command('dependencies')
  .description('Reject forbidden crate dependencies')
  .action(checkCrateDependencies);

program.parseAsync(process.argv).catch((error) => {
  console.error('[galfus-scripts] Failed:', error);
  process.exitCode = 1;
});
