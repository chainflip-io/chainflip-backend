import { exec } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs/promises';
import util from 'util';
// Import the types from child_process

export const DEFAULT_LOG_ROOT = 'chainflip/logs/';

export async function mkTmpDir(dir: string): Promise<string> {
  const tmpDir = path.join(os.tmpdir(), dir);
  await fs.mkdir(tmpDir, { recursive: true });
  return tmpDir;
}

const execAsync = util.promisify(exec);

// Execute a command, logging stdout and stderr to a file.
// The file will be initialised in the default log directory.
export async function execWithLog(
  command: string,
  commandAlias: string,
  additionalEnv: Record<string, string> = {},
): Promise<boolean> {
  try {
    const log = path.join(await mkTmpDir(DEFAULT_LOG_ROOT), `${commandAlias}.log`);
    await using file = await fs.open(log, 'w');
    const { stdout, stderr } = await execAsync(command, {
      env: { ...process.env, ...additionalEnv },
    });
    await file.write(stdout);
    await file.write(stderr);
    console.debug(`${commandAlias} succeeded`);
    return true;
  } catch (e) {
    console.error(`${commandAlias} failed: ${e}`);
    return false;
  }
}

export async function execWithRustLog(
  command: string,
  logFileName: string,
  logLevel: string | undefined = 'info',
): Promise<boolean> {
  return execWithLog(command, logFileName, { RUST_LOG: logLevel });
}
