import { execSync, spawnSync } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs';
import { assert } from 'console';

export const DEFAULT_LOG_ROOT = 'chainflip/logs/';

export function createTmpDirIfNotExists(dir: string): string {
  const tmpDir = path.join(os.tmpdir(), dir);
  try {
    if (!fs.existsSync(tmpDir)) {
      fs.mkdirSync(tmpDir, { recursive: true });
    }
  } catch (err) {
    console.error(`Unable to create temporary directory at ${tmpDir}: ${err}`);
  }

  return tmpDir;
}

// Resolve the path to the log file, creating the path if it does not exist.
export function initLogFile(fileName: string, logRoot: string = DEFAULT_LOG_ROOT): string {
  return path.join(createTmpDirIfNotExists(logRoot), fileName);
}

export function withFileStreamTo(fileName: string, cb: (file: number) => void) {
  const fileStream = fs.createWriteStream(fileName);
  fileStream.on('open', (fileDescriptor) => {
    cb(fileDescriptor);
    fileStream.close();
  });
}

// Execute a command, logging stdout and stderr to a file.
// The file will be initialised in the default log directory.
export function execWithLog(
  command: string,
  commandAlias: string,
  additionalEnv: Record<string, string> = {},
): { success: boolean } {
  let success = false;
  withFileStreamTo(initLogFile(`${commandAlias}.log`), (file) => {
    const result = spawnSync(command, {
      env: { ...process.env, ...additionalEnv },
      stdio: [0, file, file],
    });
    if (result.error) {
      console.error(`${commandAlias} failed: ${result.error}`);
    } else {
      console.debug(`${commandAlias} succeeded`);
      success = true;
    }
  });
  return { success };
}

export function execWithRustLog(
  command: string,
  logFileName: string,
  logLevel: string | undefined = 'info',
): { success: boolean } {
  return execWithLog(command, logFileName, { RUST_LOG: logLevel });
}
