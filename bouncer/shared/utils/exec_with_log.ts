import { execSync } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs';

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

function withFileStreamTo(fileName: string, cb: (file: number) => void): fs.WriteStream {
  const fileStream = fs.createWriteStream(fileName);
  return fileStream.on('open', (fileDescriptor) => {
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
  callback?: (success: boolean) => void,
) {
  let success: boolean | undefined;
  withFileStreamTo(initLogFile(`${commandAlias}.log`), (file) => {
    try {
      execSync(`${command}`, {
        env: { ...process.env, ...additionalEnv },
        stdio: [0, file, file],
      });
      console.debug(`${commandAlias} succeeded`);
      success = true;
    } catch (e) {
      console.error(`${commandAlias} failed: ${e}`);
      success = false;
      callback?.(false);
    }
  }).on('close', () => {
    callback?.(success!);
  });
}

export function execWithRustLog(
  command: string,
  logFileName: string,
  logLevel: string | undefined = 'info',
  callback?: (success: boolean) => void,
) {
  execWithLog(command, logFileName, { RUST_LOG: logLevel }, callback);
}
