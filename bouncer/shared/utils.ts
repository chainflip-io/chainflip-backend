import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import { execSync } from "child_process";

import { ApiPromise, WsProvider } from '@polkadot/api';
import { ChainId } from '@chainflip-io/cli/.';

export const runWithTimeout = <T>(promise: Promise<T>, millis: number): Promise<T> =>
  Promise.race([
    promise,
    sleep(millis).then(() => {
      throw new Error(`Timed out after ${millis} ms.`);
    }),
  ]);

export const sha256 = (data: string): Buffer => crypto.createHash('sha256').update(data).digest();

export { sleep };

export async function chainflipApi(endpoint?: string): Promise<ApiPromise> {
  const cfNodeEndpoint = endpoint ?? 'ws://127.0.0.1:9944';
  return ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function observeEvent(eventName: string, chainflip: ApiPromise): Promise<any> {
  let result;
  let waiting = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1]) {
        result = event.data;
        waiting = false;
        unsubscribe();
      }
    });
  });
  while (waiting) {
    await sleep(1000);
  }
  return result;
}

export type Token = 'USDC' | 'ETH' | 'DOT' | 'FLIP' | 'BTC';

export function getAddress(token: Token, seed: string, type?: string): string {
  const rawAddress = (() => {

    switch (token) {
      case 'ETH':
      case 'USDC':
      case 'FLIP':
        return execSync(`pnpm tsx ./commands/new_eth_address.ts ${seed}`);
      case 'DOT':
        return execSync(`pnpm tsx ./commands/new_dot_address.ts ${seed}`);
      case 'BTC':
        return execSync(`pnpm tsx ./commands/new_btc_address.ts ${seed} ${type}`);
      default:
        throw new Error("unexpected token");
    }
  })();

  return String(rawAddress).trim();

}

export function chainFromToken(token: Token): ChainId {
  if (['FLIP', 'USDC', 'ETH'].includes(token)) {
    return ChainId.Ethereum;
  }
  if (token === 'DOT') {
    return ChainId.Polkadot;
  }
  if (token === 'BTC') {
    return ChainId.Bitcoin;
  }
  throw new Error("unsupported token");
};

// TODO: import JS function instead
export function getBalanceSync(dstCcy: string, address: string): number {
  return Number(execSync(`pnpm tsx ./commands/get_balance.ts ${dstCcy} ${address}`));
}

export async function observeBalanceIncrease(dstCcy: string, address: string, oldBalance: number): Promise<number> {

  for (let i = 0; i < 60; i++) {
    const newBalance = getBalanceSync(dstCcy, address);

    if (newBalance > oldBalance) {
      return Number(newBalance);
    }

    await sleep(1000);

  }

  return Promise.reject(new Error("Failed to observe balance increase"));
}