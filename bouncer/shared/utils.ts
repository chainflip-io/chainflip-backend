import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Module from "node:module";

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Mutex } from 'async-mutex';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEthAddress } from './new_eth_address';
import { Chain, Asset, assetChains } from '@chainflip-io/cli';

export const runWithTimeout = <T>(promise: Promise<T>, millis: number): Promise<T> =>
  Promise.race([
    promise,
    sleep(millis).then(() => {
      throw new Error(`Timed out after ${millis} ms.`);
    }),
  ]);

export const sha256 = (data: string): Buffer => crypto.createHash('sha256').update(data).digest();

export { sleep };

// It is important to cache WS connections because nodes seem to have a
// limit on how many can be opened at the same time (from the same IP presumably)
function getCachedSubstrateApi(defaultEndpoint: string) {
  let api: ApiPromise | undefined;

  return async (providedEndpoint?: string): Promise<ApiPromise> => {
    if (api) return api;

    const endpoint = providedEndpoint ?? defaultEndpoint;

    api = await ApiPromise.create({
      provider: new WsProvider(endpoint),
      noInitWarn: true,
    });

    return api;
  };
};

export const getChainflipApi = getCachedSubstrateApi('ws://127.0.0.1:9944');
export const getPolkadotApi = getCachedSubstrateApi('ws://127.0.0.1:9945');

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

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type EventQuery = (data: any) => boolean;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function observeEvent(eventName: string, chainflip: ApiPromise, eventQuery?: EventQuery): Promise<any> {
  let result;
  let waiting = true;

  const query = eventQuery ?? (() => true);

  const [expectedSection, expectedMethod] = eventName.split(':');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;

      if (event.section === expectedSection && event.method === expectedMethod) {

        const data = event.data.toJSON();

        if (query(data)) {
          result = event.data;
          waiting = false;
          unsubscribe();
        }

      }

    });
  });
  while (waiting) {
    await sleep(1000);
  }
  return result;
}

export async function getAddress(asset: Asset, seed: string, type?: BtcAddressType): Promise<string> {
  let rawAddress;

  switch (asset) {
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

export function chainFromAsset(asset: Asset): Chain {
  if (asset in assetChains) {
    return assetChains[asset];
  }

  throw new Error('unexpected asset');
}

export async function observeBalanceIncrease(dstCcy: string, address: string, oldBalance: number): Promise<number> {

  for (let i = 0; i < 60; i++) {
    const newBalance = await getBalance(dstCcy as Asset, address);

    if (newBalance > oldBalance) {
      return Number(newBalance);
    }

    await sleep(1000);
  }

  return Promise.reject(new Error("Failed to observe balance increase"));
}