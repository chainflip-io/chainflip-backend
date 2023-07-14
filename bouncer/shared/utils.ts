import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Client from 'bitcoin-core';
import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
import { Mutex } from 'async-mutex';
import { Chain, Asset, assetChains, chainContractIds } from '@chainflip-io/cli';
import Web3 from 'web3';
import { u8aToHex } from '@polkadot/util';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEthAddress } from './new_eth_address';
import { CcmDepositMetadata } from './new_swap';
import cfReceiverMockAbi from '../../eth-contract-abis/perseverance-rc17/CFReceiverMock.json';

export const lpMutex = new Mutex();
export const ethNonceMutex = new Mutex();
export const btcClientMutex = new Mutex();
export const brokerMutex = new Mutex();
export const snowWhiteMutex = new Mutex();

export function getEthContractAddress(contract: string): string {
  switch (contract) {
    case 'VAULT':
      return '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968';
    case 'ETH':
      return '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE';
    case 'FLIP':
      return process.env.ETH_FLIP_ADDRESS ?? '0x10C6E9530F1C1AF873a391030a1D9E8ed0630D26';
    case 'USDC':
      return process.env.ETH_USDC_ADDRESS ?? '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0';
    case 'CFRECEIVER':
      return '0xA51c1fc2f0D1a1b8494Ed1FE312d7C3a78Ed91C0';
    case 'GATEWAY':
      return process.env.ETH_GATEWAY_ADDRESS ?? '0xeEBe00Ac0756308ac4AaBfD76c05c4F3088B8883';
    default:
      throw new Error(`Unsupported contract: ${contract}`);
  }
}

export function assetToChain(asset: Asset): string {
  switch (asset) {
    case 'DOT':
      return 'Dot';
    case 'ETH':
    case 'FLIP':
    case 'USDC':
      return 'Eth';
    case 'BTC':
      return 'Btc';
    default:
      return '';
  }
}

export function amountToFineAmount(amount: string, decimals: number): string {
  let fineAmount = '';
  if (amount.indexOf('.') === -1) {
    fineAmount = amount + '0'.repeat(decimals);
  } else {
    const amountParts = amount.split('.');
    fineAmount = amountParts[0] + amountParts[1].padEnd(decimals, '0').substr(0, decimals);
  }
  return fineAmount;
}

export function fineAmountToAmount(fineAmount: string, decimals: number): string {
  let balance = '';
  if (fineAmount.length > decimals) {
    const decimalLocation = fineAmount.length - decimals;
    balance = fineAmount.slice(0, decimalLocation) + '.' + fineAmount.slice(decimalLocation);
  } else {
    balance = '0.' + fineAmount.padStart(decimals, '0');
  }
  return balance;
}

export function defaultAssetAmounts(asset: Asset): string {
  switch (asset) {
    case 'BTC':
      return '0.05';
    case 'ETH':
      return '5';
    case 'DOT':
      return '50';
    case 'USDC':
    case 'FLIP':
      return '500';
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }
}

export const runWithTimeout = async <T>(promise: Promise<T>, millis: number): Promise<T> => {
  const controller = new AbortController();
  const result = await Promise.race([
    promise,
    sleep(millis, { signal: AbortController }).then(() => {
      throw new Error(`Timed out after ${millis} ms.`);
    }),
  ]);
  controller.abort();
  return result;
};

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
}

export const getChainflipApi = getCachedSubstrateApi(
  process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944',
);
export const getPolkadotApi = getCachedSubstrateApi(
  process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9945',
);

export const polkadotSigningMutex = new Mutex();

export function getBtcClient(): Client {
  const endpoint = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';

  return new Client({
    host: endpoint.split(':')[1].slice(2),
    port: Number(endpoint.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'watch',
  });
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type EventQuery = (data: any) => boolean;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Event = { data: any; block: number; event_index: number };
export async function observeEvent(
  eventName: string,
  api: ApiPromise,
  eventQuery?: EventQuery,
): Promise<Event> {
  let result: Event | undefined;
  let waiting = true;

  const query = eventQuery ?? (() => true);

  const [expectedSection, expectedMethod] = eventName.split(':');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await api.rpc.chain.subscribeNewHeads(async (header) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const events: any[] = await api.query.system.events.at(header.hash);
    events.forEach((record, index) => {
      const { event } = record;
      if (waiting && event.section === expectedSection && event.method === expectedMethod) {
        result = {
          data: event.toHuman().data,
          block: header.number.toNumber(),
          event_index: index,
        };
        if (query(result)) {
          waiting = false;
          unsubscribe();
        }
      }
    });
  });
  while (waiting) {
    await sleep(1000);
  }
  return result as Event;
}

export async function getAddress(
  asset: Asset,
  seed: string,
  type?: BtcAddressType,
): Promise<string> {
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
      rawAddress = await newBtcAddress(seed, type ?? 'P2PKH');
      break;
    default:
      throw new Error('unexpected asset');
  }

  return String(rawAddress).trim();
}

export function chainFromAsset(asset: Asset): Chain {
  if (asset in assetChains) {
    return assetChains[asset];
  }

  throw new Error('unexpected asset');
}

export async function observeBalanceIncrease(
  dstCcy: string,
  address: string,
  oldBalance: string,
): Promise<number> {
  for (let i = 0; i < 120; i++) {
    const newBalance = Number(await getBalance(dstCcy as Asset, address));
    if (newBalance > Number(oldBalance)) {
      return newBalance;
    }

    await sleep(1000);
  }

  return Promise.reject(new Error('Failed to observe balance increase'));
}

export async function observeEVMEvent(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  contractAbi: any,
  address: string,
  eventName: string,
  eventParametersExpected: string[],
  initialBlockNumber?: number,
): Promise<void> {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  const contract = new web3.eth.Contract(contractAbi, address);
  let initBlockNumber = initialBlockNumber ?? (await web3.eth.getBlockNumber());

  // Gets all the event parameter as an array
  const eventAbi = cfReceiverMockAbi.find(
    (item) => item.type === 'event' && item.name === eventName,
  )!;

  // Get the parameter names of the event
  const parameterNames = eventAbi.inputs.map((input) => input.name);

  let eventWitnessed = false;

  for (let i = 0; i < 120 && !eventWitnessed; i++) {
    const currentBlockNumber = await web3.eth.getBlockNumber();
    if (currentBlockNumber >= initBlockNumber) {
      const events = await contract.getPastEvents(eventName, {
        fromBlock: initBlockNumber,
        toBlock: currentBlockNumber,
      });
      for (let j = 0; j < events.length && !eventWitnessed; j++) {
        for (let k = 0; k < parameterNames.length; k++) {
          // Allow for wildcard matching
          if (
            events[j].returnValues[k] !== eventParametersExpected[k] &&
            eventParametersExpected[k] !== '*'
          ) {
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

export async function observeCcmReceived(
  sourceAsset: Asset,
  destAsset: Asset,
  address: string,
  messageMetadata: CcmDepositMetadata,
): Promise<void> {
  await observeEVMEvent(cfReceiverMockAbi, address, 'ReceivedxSwapAndCall', [
    chainContractIds[assetChains[sourceAsset]].toString(),
    '*',
    messageMetadata.message,
    getEthContractAddress(destAsset.toString()),
    '*',
    '*',
  ]);
}

// Converts a hex string into a bytes array. Support hex strings start with and without 0x
export function hexStringToBytesArray(hex: string) {
  return Array.from(Buffer.from(hex.replace(/^0x/, ''), 'hex'));
}

export function asciiStringToBytesArray(str: string) {
  return Array.from(Buffer.from(str.replace(/^0x/, '')));
}

export function encodeBtcAddressForContract(address: string) {
  const addressHex = address.replace(/^0x/, '');
  return Buffer.from(addressHex, 'hex').toString();
}

export function decodeDotAddressForContract(address: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  return u8aToHex(keyring.decodeAddress(address));
}

export function decodeFlipAddressForContract(address: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  return u8aToHex(keyring.decodeAddress(address));
}

export function hexPubkeyToFlipAddress(hexPubkey: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  return keyring.encodeAddress(hexPubkey);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function handleSubstrateError(api: any) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (arg: any) => {
    const { dispatchError } = arg;
    if (dispatchError) {
      let error;
      if (dispatchError.isModule) {
        const { docs, name, section } = api.registry.findMetaError(dispatchError.asModule);
        error = section + '.' + name + ' ' + docs;
      } else {
        error = dispatchError.toString();
      }
      console.log('Extrinsic failed: ' + error);
      process.exit(1);
    }
  };
}
