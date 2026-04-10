import { spawn } from 'child_process';
import path from 'path';
import fs from 'fs/promises';
import { sleep } from 'shared/utils';
import { globalLogger, type Logger } from 'shared/utils/logger';

export const DEFAULT_LOG_ROOT = 'chainflip/logs/';
export const DEFAULT_TMP_ROOT = '/tmp/chainflip';

export async function mkTmpDir(dir: string): Promise<string> {
  const tmpDir = path.join(DEFAULT_TMP_ROOT, dir);
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
  logger: Logger = globalLogger,
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
      logger.info(data.toString());
    });

    ls.stderr.on('data', async (data) => {
      await file.write(data.toString());
      logger.info(data.toString());
    });

    // Use 'close' rather than 'exit': on macOS the 'exit' event can fire before
    // all buffered stdio data events are delivered, causing writes to an already-disposed
    // file handle. 'close' fires only after all stdio streams are fully flushed.
    ls.on('close', (code) => {
      running = false;
      exitCode = code ?? 0;
      logger.info('child process exited with code ' + (code?.toString() ?? 'null'));
    });

    // --- wait for process to exit ---
    while (running) {
      await sleep(1000);
    }

    if (exitCode !== 0) {
      logger.error(`${commandAlias} failed (exit code: ${exitCode})`);
      return false;
    }

    logger.debug(`${commandAlias} succeeded`);
    return true;
  } catch (e) {
    logger.error(`${commandAlias} failed: ${e}`);
    return false;
  }
}

export async function execWithRustLog(
  command: string,
  args: string[],
  logFileName: string,
  logLevel: string | undefined = 'info',
  logger: Logger = globalLogger,
): Promise<boolean> {
  return execWithLog(command, args, logFileName, { RUST_LOG: logLevel }, logger);
}
