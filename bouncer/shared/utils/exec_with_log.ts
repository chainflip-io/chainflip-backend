import { spawn } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs/promises';
import { sleep } from 'shared/utils';

export const DEFAULT_LOG_ROOT = 'chainflip/logs/';

export async function mkTmpDir(dir: string): Promise<string> {
  const tmpDir = path.join(os.tmpdir(), dir);
  await fs.mkdir(tmpDir, { recursive: true });
  return tmpDir;
}

// Execute a command, logging stdout and stderr to a file.
// The file will be initialised in the default log directory.
export async function execWithLog(
  command: string,
  args: string[],
  commandAlias: string,
  additionalEnv: Record<string, string> = {},
): Promise<boolean> {
  try {
    // --- prepare log file ---
    const log = path.join(await mkTmpDir(DEFAULT_LOG_ROOT), `${commandAlias}.log`);
    await using file = await fs.open(log, 'w');

    // --- spawn process and register callbacks on events ---
    let running = true;
    let exitCode: number = 0;
    const ls = spawn(command, args, {
      env: { ...process.env, ...additionalEnv },
    });

    ls.stdout.on('data', async (data) => {
      await file.write(data.toString());
      console.log(data.toString());
    });

    ls.stderr.on('data', async (data) => {
      await file.write(data.toString());
      console.log(data.toString());
    });

    ls.on('exit', (code) => {
      running = false;
      exitCode = code ?? 0;
      console.log('child process exited with code ' + (code?.toString() ?? 'null'));
    });

    // --- wait for process to exit ---
    while (running) {
      await sleep(1000);
    }

    if (exitCode !== 0) {
      console.error(`${commandAlias} failed (exit code: ${exitCode})`);
      return false;
    }

    console.debug(`${commandAlias} succeeded`);
    return true;
  } catch (e) {
    console.error(`${commandAlias} failed: ${e}`);
    return false;
  }
}

export async function execWithRustLog(
  command: string,
  args: string[],
  logFileName: string,
  logLevel: string | undefined = 'info',
): Promise<boolean> {
  return execWithLog(command, args, logFileName, { RUST_LOG: logLevel });
}
