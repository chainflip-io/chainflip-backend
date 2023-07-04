import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Module from "node:module";

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Mutex } from 'async-mutex';
import { Chain, Asset, assetChains } from '@chainflip-io/cli';
import Web3 from 'web3';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEthAddress } from './new_eth_address';
import { CcmDepositMetadata } from './new_swap';
import cfReceiverMockAbi from '../../eth-contract-abis/perseverance-rc17/CFReceiverMock.json';

// TODO: Import this from the chainflip-io/cli package once it's exported in future versions.
export function assetToChain(asset: Asset): number {
  switch (asset) {
    case 'ETH':
    case 'FLIP': 
    case 'USDC': 
      return 1; // Ethereum
    case 'DOT': 
      return 2; // Polkadot
    case 'BTC':
      return 3; // Bitcoin
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }
}

// TODO: Import this from the chainflip-io/cli package once it's exported in future versions.
export function getEthContractAddress(contract: string): string {
  switch (contract) {
    case 'ETH':
      return '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE';
    case 'FLIP': 
      return '0x10C6E9530F1C1AF873a391030a1D9E8ed0630D26'; 
    case 'USDC': 
      return '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0';
    case 'CFRECEIVER': 
      return '0xA51c1fc2f0D1a1b8494Ed1FE312d7C3a78Ed91C0';
    default:
      throw new Error(`Unsupported contract: ${contract}`);
  }
}

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

export const getChainflipApi = getCachedSubstrateApi(
  process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944',
);
export const getPolkadotApi = getCachedSubstrateApi(
  process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9945'
  );

export const polkadotSigningMutex = new Mutex();

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

  for (let i = 0; i < 120; i++) {
    const newBalance = await getBalance(dstCcy as Asset, address);

    if (newBalance > oldBalance) {
      return Number(newBalance);
    }

    await sleep(1000);
  }

  return Promise.reject(new Error("Failed to observe balance increase"));
}

export async function observeCcmReceived(sourceToken: Asset, destToken: Asset, address: string, messageMetadata: CcmDepositMetadata): Promise<void> {
  await observeEVMEvent(cfReceiverMockAbi, address, "ReceivedxSwapAndCall", [assetToChain(sourceToken).toString(),'*',messageMetadata.message,getEthContractAddress(destToken.toString()),'*','*'])
}

export async function observeEVMEvent(contractAbi: any, address: string, eventName: string, eventParametersExpected: string[], initialBlockNumber?:number): Promise<void> {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  const contract = new web3.eth.Contract(contractAbi, address);
  let initBlockNumber = initialBlockNumber ?? await web3.eth.getBlockNumber();

  // Gets all the event parameter as an array
  const eventAbi = cfReceiverMockAbi.find((item) => item.type === 'event' && item.name === eventName);

  // Get the parameter names of the event
  const parameterNames = eventAbi.inputs.map((input) => input.name);

  let eventWitnessed = false;
  
  for (let i = 0; i < 120 && !eventWitnessed; i++) {
    const currentBlockNumber = await web3.eth.getBlockNumber();
    if (currentBlockNumber > initBlockNumber) {
      const events = await contract.getPastEvents(eventName, {fromBlock: initBlockNumber, toBlock: currentBlockNumber});
      for (let j = 0; j < events.length && !eventWitnessed; j++) {
        for (let k = 0; k < parameterNames.length; k++) {
          // Allow for wildcard matching
          if (events[j].returnValues[k] != eventParametersExpected[k] && eventParametersExpected[k] != '*') {
            break;
          } else if (k === parameterNames.length - 1) {
            eventWitnessed = true;
            break;
          }
        }
      }    
      initBlockNumber = currentBlockNumber + 1;
    }
    await sleep(2500);
  }

  if (eventWitnessed) {
    return Promise.resolve();
  } 
    return Promise.reject(new Error(`Failed to observe the ${eventName} event`));
  

}

// Converts s hex string into a bytes array. Support hex strings start with and without 0x
export function hexStringToBytesArray(hex: string) {
  return Array.from(Buffer.from(hex.replace(/^0x/, ''), 'hex'));
};

export function asciiStringToBytesArray(str: string) {
  return Array.from(Buffer.from(str.replace(/^0x/, '')));
}
