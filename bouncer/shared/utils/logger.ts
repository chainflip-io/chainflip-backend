import { toUpperCase } from '@chainflip/utils/string';
import pino from 'pino';

export type Logger = pino.Logger;

const logFile = process.env.BOUNCER_LOG_PATH ?? '/tmp/chainflip/bouncer.log';

const logFileDestination = pino.destination({
  dest: logFile,
  sync: false,
  mkdir: true,
});
const prettyConsoleTransport = pino.transport({
  target: 'pino-pretty',
  options: {
    colorize: true,
    // Note: we are ignoring the common bindings to keep the cli log clean.
    ignore: 'test,module,tag,logStorage',
  },
});

// Log the given value without having to include %s in the message. Just like console.log
function logMethod(
  this: pino.Logger,
  args: Parameters<pino.LogFn>,
  method: pino.LogFn,
  level: number,
) {
  const newArgs = args;
  if (args.length === 2 && !args[0].includes('%s')) {
    newArgs[0] = `${args[0]} %s`;
  }
  method.apply(this, newArgs);

  // we use a custom attribute called `logStorage` to store logs in memory.
  // In the `createTestFunction()` in `vitest.ts` we use this to extract
  // only the logs of the logger whose test failed, and attach them to the
  // error message.
  let currentLogs = this.bindings().logStorage as string | undefined;
  if (!currentLogs) {
    currentLogs = '';
  }

  // Getting the time using the time function of pino, there might be a better way to do this.
  const { time } = JSON.parse(`{"noop": "nothing"${pino.stdTimeFunctions.isoTime()}}`);

  // We manually reconstruct the same format that the pino messages are in.
  // There doesn't seem a way to use the `method: pino.LogFn` formatter.
  currentLogs += `[${time}] ${toUpperCase(this.levels.labels[level])}: ${newArgs}\n`;
  this.setBindings({ logStorage: currentLogs });
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
