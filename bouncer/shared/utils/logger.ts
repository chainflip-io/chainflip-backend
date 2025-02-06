import pino from 'pino';

export type Logger = pino.Logger;

const logFileDestination = pino.destination({
  dest: process.env.BOUNCER_LOG_PATH ?? '/tmp/chainflip/bouncer.log',
  sync: false,
});
const prettyConsoleTransport = pino.transport({
  target: 'pino-pretty',
  options: {
    colorize: true,
    ignore: 'time,pid,hostname',
  },
});

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

export const logger: Logger = pino(
  {
    hooks: { logMethod },
    level: 'trace',
    timestamp: pino.stdTimeFunctions.isoTime,
    // Log the level as a string ("info") instead of a number (30)
    formatters: {
      level: (label) => ({ level: label }),
    },
  },
  pino.multistream([
    { stream: prettyConsoleTransport, level: 'info' },
    { stream: logFileDestination, level: 'trace' },
  ]),
);

process.on('uncaughtException', (err) => {
  logger.error(err);
});
process.on('unhandledRejection', (reason, promise) => {
  logger.error({ reason, promise });
});

// Creates a child logger and appends the module name to any existing module names
export function loggerChild(parentLogger: Logger, name: string): Logger {
  const existingModule = parentLogger.bindings().module as string | undefined;
  const newModule = existingModule !== undefined ? `${existingModule}::${name}` : name;
  return parentLogger.child({ module: newModule });
}
