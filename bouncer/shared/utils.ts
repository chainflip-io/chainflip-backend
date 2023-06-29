import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Module from "node:module";

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Mutex } from 'async-mutex';
import { Chain, Asset, assetChains } from '@chainflip-io/cli';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEthAddress } from './new_eth_address';
import { CcmDepositMetadata } from './new_swap';
import Web3 from 'web3';
import cfReceiverMockAbi from '../../eth-contract-abis/perseverance-rc17/CFReceiverMock.json';


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

// TODO: Convert parameters into the real uints for srcAddrs, dstToken...
export async function observeCcmReceived(dstCcy: string, address: string, messageMetadata: CcmDepositMetadata): Promise<void> {
  // await observeEVMEvent(cfReceiverMockAbi, address, "ReceivedxSwapAndCall", ['3','0x76a914000000000000000000000000000000000000000088ac','0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001342544320746f2045544820772f2043434d212100000000000000000000000000','0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE','399919033011484135','399919033011484135'])
  // await observeEVMEvent(cfReceiverMockAbi, address, "ReceivedxSwapAndCall", ['*','*',messageMetadata.message,'*','*','*'])
  await observeEVMEvent(cfReceiverMockAbi, address, "ReceivedxSwapAndCall", ['*','*','3','*','*','*'])


  // const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  // const contract = new web3.eth.Contract(cfReceiverMockAbi, address);


  // const initialBlock = await web3.eth.getBlockNumber();
  // let eventWitnessed = false;

  // for (let i = 0; i < 120 && !eventWitnessed; i++) {
  //   const events = await contract.getPastEvents('ReceivedxSwapAndCall', {fromBlock: initialBlock, toBlock: 'latest'});
  //   for (let i = 0; i < events.length; i++) {
  //     // For now we only check that the message matches
  //     if (events[i].returnValues.message === messageMetadata.message) {
  //       eventWitnessed = true;
  //       break;
  //     }
  //   }
  //   await sleep(1000);
  // }

  // if (eventWitnessed) {
  //   return Promise.resolve();
  // } else {
  //   return Promise.reject(new Error("Failed to observe the CCM Received event"));
  // }
}

export async function observeEVMEvent(contractAbi: any, address: string, eventName: string, eventParametersExpected: string[]): Promise<void> {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  const contract = new web3.eth.Contract(contractAbi, address);

  // This gets all the event parameters as an array (e.g. ['srcChain','srcAddress','message','token','amount','nativeReceived'])
  const eventAbi = cfReceiverMockAbi.find((item) => item.type === 'event' && item.name === eventName);

  // Get the parameter names of the event
  const parameterNames = eventAbi.inputs.map((input) => input.name);

  const initialBlock = await web3.eth.getBlockNumber();
  let eventWitnessed = false;
  
  for (let i = 0; i < 120 && !eventWitnessed; i++) {
    const events = await contract.getPastEvents(eventName, {fromBlock: initialBlock, toBlock: 'latest'});
    for (let i = 0; i < events.length; i++) {
      console.log(events[i].returnValues)
      console.log(eventParametersExpected)
      for (let i = 0; i < parameterNames.length; i++) {
        console.log(events[i].returnValues[parameterNames[i]])
        console.log(eventParametersExpected[i])
        // Allow for wildcard matching
        if (events[i].returnValues[parameterNames[i]] != eventParametersExpected[i] && eventParametersExpected[i] != '*') {
          break;
        }
      eventWitnessed = true;
      break;
      }
    }
    await sleep(1000);
  }

  if (eventWitnessed) {
    return Promise.resolve();
  } else {
    return Promise.reject(new Error("Failed to observe the CCM Received event"));
  }

}

