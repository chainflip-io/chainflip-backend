import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Module from "node:module";

import { ApiPromise, WsProvider } from '@polkadot/api';
import { ChainId } from '@chainflip-io/cli';
import { Mutex } from 'async-mutex';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEthAddress } from './new_eth_address';

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

export async function polkadotApi(endpoint?: string): Promise<ApiPromise> {
  const polkadotEndpoint = endpoint ?? 'ws://127.0.0.1:9945';
  return ApiPromise.create({
    provider: new WsProvider(polkadotEndpoint),
    noInitWarn: true,
  });
}

export const polkadotSigningMutex = new Mutex();
export const ethereumSigningMutex = new Mutex();

export function getBtcClient(btcEndpoint?: string): any {

  const require = Module.createRequire(import.meta.url);

  const BTC_ENDPOINT = btcEndpoint || 'http://127.0.0.1:8332';

  const Client = require('bitcoin-core');

  return new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'watch',
  });
}

type EventQuery = (event: any) => boolean;

export async function observeEventWithQuery(query: EventQuery, chainflip: ApiPromise): Promise<any> {
  let result;
  let waiting = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;

      if (query(event)) {
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

export async function observeEventWithNameAndQuery(eventName: string, query: EventQuery, chainflip: ApiPromise): Promise<any> {
  return observeEventWithQuery(
    event => {
      const nameMatches = event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1];
      return nameMatches && query(event)
    },
    chainflip);
}

export async function observeEvent(eventName: string, chainflip: ApiPromise): Promise<any> {
  return observeEventWithNameAndQuery(eventName,
    _ => true,
    chainflip);
}

export type Token = 'USDC' | 'ETH' | 'DOT' | 'FLIP' | 'BTC';

export async function getAddress(token: Token, seed: string, type?: BtcAddressType): Promise<string> {
  let rawAddress;

  switch (token) {
    case 'ETH':
    case 'USDC':
    case 'FLIP':
      rawAddress = newEthAddress(seed);
      break;
    case 'DOT':
      rawAddress = await newDotAddress(seed);
      break;
    case 'BTC':
      rawAddress = await newBtcAddress(seed, type ?? 'P2PKH')
      break;
    default:
      throw new Error("unexpected token");
  }

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

export async function observeBalanceIncrease(dstCcy: string, address: string, oldBalance: number): Promise<number> {

  for (let i = 0; i < 60; i++) {
    const newBalance = await getBalance(dstCcy as Token, address);

    if (newBalance > oldBalance) {
      return Number(newBalance);
    }

    await sleep(1000);

  }

  return Promise.reject(new Error("Failed to observe balance increase"));
}