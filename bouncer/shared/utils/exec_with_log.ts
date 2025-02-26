import { exec } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs';
import util from 'util';
// Import the types from child_process
import type { ExecOptions } from 'child_process';

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

// Define the type for the exec result
interface ExecResult {
  stdout: string;
  stderr: string;
}

// Update the execAsync function to use promisify with proper typing
const execAsync = util.promisify(exec) as (
  command: string, 
  options?: ExecOptions
) => Promise<ExecResult>;

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
export async function execWithLog(
  command: string,
  commandAlias: string,
  additionalEnv: Record<string, string> = {},
): Promise<boolean> {
  return new Promise((resolve) => {
    withFileStreamTo(initLogFile(`${commandAlias}.log`), async (file) => {
      try {
        const { stdout, stderr } = await execAsync(`${command}`, {
          env: { ...process.env, ...additionalEnv },
        });
        // Write stdout and stderr to the file
        fs.writeSync(file, stdout);
        fs.writeSync(file, stderr);
        console.debug(`${commandAlias} succeeded`);
        resolve(true);
      } catch (e) {
        console.error(`${commandAlias} failed: ${e}`);
        resolve(false);
      }
    }).on('close', () => {
      resolve(true);
    });
  });
}

export async function execWithRustLog(
  command: string,
  logFileName: string,
  logLevel: string | undefined = 'info',
): Promise<boolean> {
  return execWithLog(command, logFileName, { RUST_LOG: logLevel });
}
