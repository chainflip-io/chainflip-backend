import { Chain } from '@/shared/enums';
import logger from './logger';
import env from '../config/env';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyFunction = (...args: any[]) => any;

export const memoize = <T extends AnyFunction>(fn: T, ttl?: number): T => {
  let initialized = false;
  let value: ReturnType<T> | undefined;
  let setAt = 0;

  return ((...args) => {
    if (
      !initialized ||
      (ttl && Date.now() - setAt > ttl) ||
      env.NODE_ENV === 'test' // TODO: remove this when we have a better solution for testing
    ) {
      initialized = true;
      value = fn(...args);
      setAt = Date.now();
    }
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

const blockTimeMap: Record<Chain, number> = {
  Bitcoin: 600,
  Ethereum: 15,
  Polkadot: 6,
};

export function calculateExpiryTime(args: {
  chainInfo?: {
    chain: Chain;
    height: bigint;
    blockTrackedAt: Date;
  } | null;
  expiryBlock?: bigint | null;
}) {
  const { chainInfo, expiryBlock } = args;

  if (chainInfo == null || expiryBlock == null) {
    return null;
  }

  const remainingBlocks = Number(expiryBlock - chainInfo.height); // If it is negative, it means the channel has already expired and will return the time from the past

  return new Date(
    chainInfo.blockTrackedAt.getTime() +
      remainingBlocks * blockTimeMap[chainInfo.chain] * 1000,
  );
}
