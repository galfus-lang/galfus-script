import { spawn } from 'child_process';
import { join } from 'path';

export async function devVscode() {
  const rootDir = join(import.meta.dir, '..', '..', '..');
  
  console.log('Building Galfus CLI locally...');
  const build = spawn('cargo', ['build', '-p', 'galfus-cli'], {
    cwd: rootDir,
    stdio: 'inherit',
  });

  build.on('close', (code) => {
    if (code !== 0) {
      console.error(`Cargo build failed with code ${code}`);
      process.exit(1);
    }

    const cliPath = join(rootDir, 'target', 'debug', 'galfus-cli');
    const extensionPath = join(rootDir, 'editors', 'vscode');

    console.log('Installing VS Code extension dependencies...');
    const npmInstall = spawn('bun', ['install'], {
      cwd: extensionPath,
      stdio: 'inherit'
    });

    npmInstall.on('close', (npmCode) => {
      if (npmCode !== 0) {
        console.error(`Failed to install extension dependencies`);
        process.exit(1);
      }

      console.log('Compiling extension...');
      const compile = spawn('bun', ['run', 'compile'], {
        cwd: extensionPath,
        stdio: 'inherit'
      });

      compile.on('close', (compileCode) => {
        if (compileCode !== 0) {
          console.error(`Failed to compile extension`);
          process.exit(1);
        }

        console.log('Configuring dev environment...');
        const devEnv = join(extensionPath, '.galfus-dev-env');
        require('fs').writeFileSync(devEnv, JSON.stringify({
          GALFUS_DEV_MODE: '1',
          GALFUS_CLI_PATH: cliPath
        }));

        console.log('Launching VS Code Extension Development Host...');
        spawn('code', ['--extensionDevelopmentPath=' + extensionPath], {
          env: process.env,
          stdio: 'inherit',
        });
      });
    });
  });
}
