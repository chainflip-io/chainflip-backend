import logger from './logger';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyFunction = (...args: any[]) => any;

export const memoize = <T extends AnyFunction>(fn: T): T => {
  let initialized = false;
  let value: ReturnType<T> | undefined;

  return ((...args) => {
    if (initialized) return value as ReturnType<T>;
    initialized = true;
    value = fn(...args);
    return value;
  }) as T;
};

const exitHandlers = new Set<AnyFunction>();

['SIGINT', 'SIGTERM'].forEach((signal) => {
  process.on(signal, () => {
    logger.info(`handling "${signal}" signal`);
    Promise.allSettled([...exitHandlers].map((handler) => handler()));
  });
});

export const handleExit = (cb: AnyFunction) => {
  exitHandlers.add(cb);

  return () => {
    exitHandlers.delete(cb);
  };
};
