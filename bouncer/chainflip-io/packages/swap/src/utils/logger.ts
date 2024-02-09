import 'dotenv/config';
import {
  createLogger,
  format,
  LeveledLogMethod,
  transports,
  type Logger,
} from 'winston';
import { isProduction, isTest } from './consts';

type CommonAlertCode = 'DbReadError' | 'DbWriteError';

type SwapAlertCode = CommonAlertCode | 'EventHandlerError' | 'UnknownError';

interface CustomMetadata {
  alertCode: SwapAlertCode;
  heapUsed: number | string;
  performance: string;
}

interface CustomLoggerMethod {
  (message: string): Logger;
  (message: string, customMeta?: Partial<CustomMetadata>): Logger;
  (
    message: string,
    customMeta: Partial<CustomMetadata>,
    meta?: unknown,
  ): Logger;
}

interface CustomLogger extends Logger {
  customError: CustomLoggerMethod;
  customInfo: CustomLoggerMethod;
  customWarn: CustomLoggerMethod;
}

const transformedLogger = (
  loggerFn: LeveledLogMethod,
  message: string,
  customMeta?: Partial<CustomMetadata>,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  meta?: any,
) => {
  if (meta instanceof Error) {
    const error = {
      name: meta.name,
      message: meta.message,
      stack: meta.stack,
    };
    return loggerFn(message, { error, metadata: customMeta });
  }
  if (
    meta !== null &&
    typeof meta === 'object' &&
    'error' in meta &&
    meta.error instanceof Error
  ) {
    const error = {
      name: meta.error.name,
      message: meta.error.message,
      stack: meta.error.stack,
    };
    return loggerFn(message, { metadata: customMeta, ...meta, error });
  }
  return loggerFn(message, { metadata: customMeta, ...meta });
};

const printFormat = () => {
  if (isProduction) return format.json();

  return format.printf((info) => {
    const { timestamp, level, component, message, error, metadata } = info;
    return `${timestamp} ${level} [${component}]: ${message} ${
      error ? `${error.name} ${error.message} ${error.stack ?? ''}` : ''
    } ${metadata ? JSON.stringify({ metadata }) : ''}`;

    // If we want to enable multiple params for logger
    // const rest: string[] = info[Symbol.for('splat') as unknown as string];
  });
};

const createLoggerFunc = (label: string) => {
  const logger = createLogger({
    format: format.combine(
      format.timestamp({
        format: 'YY-MM-DD HH:mm:ss',
      }),
      printFormat(),
    ),
    defaultMeta: { component: label.toUpperCase() },
    transports: [
      new transports.Console({
        format: isProduction
          ? format.json()
          : format.combine(format.colorize({ all: true })),
        silent: isTest,
      }),
    ],
  }) as CustomLogger;

  logger.customInfo = (
    message: string,
    customMeta?: Partial<CustomMetadata>,
    meta?: unknown,
  ) => transformedLogger(logger.info, message, customMeta, meta);
  logger.customWarn = (
    message: string,
    customMeta?: Partial<CustomMetadata>,
    meta?: unknown,
  ) => transformedLogger(logger.warn, message, customMeta, meta);
  logger.customError = (
    message: string,
    customMeta?: Partial<CustomMetadata>,
    meta?: unknown,
  ) => transformedLogger(logger.error, message, customMeta, meta);

  return logger;
};

const logger = createLoggerFunc('swap');

export default logger;
