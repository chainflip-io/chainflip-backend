import { existsSync, mkdirSync } from 'fs';
import { dirname } from 'path';
import pino from 'pino';

export type Logger = pino.Logger;

const logFile = process.env.BOUNCER_LOG_PATH ?? '/tmp/chainflip/bouncer.log';

const logFileDestination = pino.destination({
  dest: logFile,
  sync: false,
});
const prettyConsoleTransport = pino.transport({
  target: 'pino-pretty',
  options: {
    colorize: true,
    // Note: we are ignoring the common bindings to keep the cli log clean.
    ignore: 'test,module,tag',
  },
});

// Create the logging folder if it doesn't exist
const logFolder = dirname(logFile);
if (!existsSync(logFolder)) {
  mkdirSync(logFolder, { recursive: true });
}

// Log the given value without having to include %s in the message. Just like console.log
function logMethod(
  this: pino.Logger,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  args: any[],
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  method: (this: pino.Logger, ...args: any[]) => void,
) {
  const newArgs = args;
  if (args.length === 2 && !args[0].includes('%s')) {
    newArgs[0] = `${args[0]} %s`;
  }
  method.apply(this, newArgs);
}

export const globalLogger: Logger = pino(
  {
    hooks: { logMethod },
    level: 'trace',
    // We don't want to log the hostname or pid
    base: undefined,
    timestamp: pino.stdTimeFunctions.isoTime,
    // Log the level as a string ("info") instead of a number (30)
    formatters: {
      level: (label) => ({ level: label }),
    },
  },
  pino.multistream([
    { stream: prettyConsoleTransport, level: process.env.BOUNCER_LOG_LEVEL ?? 'info' },
    { stream: logFileDestination, level: 'trace' },
  ]),
);

process.on('uncaughtException', (err) => {
  globalLogger.error(err);
});
process.on('unhandledRejection', (reason, promise) => {
  globalLogger.error({ reason, promise });
});

// Creates a child logger and appends the module name to any existing module names on the logger
export function loggerChild(parentLogger: Logger, name: string): Logger {
  const existingModule = parentLogger.bindings().module as string | undefined;
  const newModule = existingModule !== undefined ? `${existingModule}::${name}` : name;
  return parentLogger.child({ module: newModule });
}

// Takes all of the contextual information attached to the logger and appends it to the error message
export function loggerError(parentLogger: Logger, error: Error): Error {
  const bindings = parentLogger.bindings();
  const newError = error;
  for (const [key, value] of Object.entries(bindings)) {
    newError.message += `\n   ${key}: ${value}`;
  }
  return newError;
}

// Takes all of the contextual information attached to the logger and appends it to the error message before throwing it
export function throwError(parentLogger: Logger, error: Error): never {
  throw loggerError(parentLogger, error);
}
