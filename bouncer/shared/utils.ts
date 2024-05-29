import * as crypto from 'crypto';
import { setTimeout as sleep } from 'timers/promises';
import Client from 'bitcoin-core';
import { ApiPromise, WsProvider, Keyring } from '@polkadot/api';
// eslint-disable-next-line no-restricted-imports
import { KeyringPair } from '@polkadot/keyring/types';
import { Mutex } from 'async-mutex';
import {
  Chain as SDKChain,
  InternalAsset as SDKAsset,
  InternalAssets as Assets,
  assetConstants,
  chainConstants,
} from '@chainflip/cli';
import Web3 from 'web3';
import { Connection, Keypair } from '@solana/web3.js';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import BigNumber from 'bignumber.js';
import { base58Decode, base58Encode } from '../polkadot/util-crypto';
import { newDotAddress } from './new_dot_address';
import { BtcAddressType, newBtcAddress } from './new_btc_address';
import { getBalance } from './get_balance';
import { newEvmAddress } from './new_evm_address';
import { CcmDepositMetadata } from './new_swap';
import { getCFTesterAbi } from './contract_interfaces';
import { SwapParams } from './perform_swap';
import { newSolAddress } from './new_sol_address';

const cfTesterAbi = await getCFTesterAbi();

export const lpMutex = new Mutex();
export const ethNonceMutex = new Mutex();
export const arbNonceMutex = new Mutex();
export const btcClientMutex = new Mutex();
export const brokerMutex = new Mutex();
export const snowWhiteMutex = new Mutex();

export const ccmSupportedChains = ['Ethereum', 'Arbitrum', 'Solana'] as Chain[];

export type Asset = SDKAsset | 'Sol' | 'SolUsdc';
export type Chain = SDKChain | 'Solana';

const isSDKAsset = (asset: Asset): asset is SDKAsset => asset in assetConstants;
const isSDKChain = (chain: Chain): chain is SDKChain => chain in chainConstants;

export function getContractAddress(chain: Chain, contract: string): string {
  switch (chain) {
    case 'Ethereum':
      switch (contract) {
        case 'VAULT':
          return '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968';
        case 'KEY_MANAGER':
          return '0xa16E02E87b7454126E5E10d957A927A7F5B5d2be';
        case 'Eth':
          return '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE';
        case 'Flip':
          return process.env.ETH_FLIP_ADDRESS ?? '0x10C6E9530F1C1AF873a391030a1D9E8ed0630D26';
        case 'Usdc':
          return process.env.ETH_USDC_ADDRESS ?? '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0';
        case 'Usdt':
          return process.env.ETH_USDT_ADDRESS ?? '0x0DCd1Bf9A1b36cE34237eEaFef220932846BCD82';
        case 'CFTESTER':
          return '0xA51c1fc2f0D1a1b8494Ed1FE312d7C3a78Ed91C0';
        case 'GATEWAY':
          return process.env.ETH_GATEWAY_ADDRESS ?? '0xeEBe00Ac0756308ac4AaBfD76c05c4F3088B8883';
        default:
          throw new Error(`Unsupported contract: ${contract}`);
      }
    case 'Arbitrum':
      switch (contract) {
        case 'VAULT':
          return '0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512';
        case 'KEY_MANAGER':
          return '0x5FbDB2315678afecb367f032d93F642f64180aa3';
        case 'ADDRESS_CHECKER':
          return '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0';
        case 'ArbEth':
          return '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE';
        case 'ArbUsdc':
          return process.env.ARB_USDC_ADDRESS ?? '0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9';
        case 'CFTESTER':
          return '0x0DCd1Bf9A1b36cE34237eEaFef220932846BCD82';
        default:
          throw new Error(`Unsupported contract: ${contract}`);
      }
    case 'Solana':
      switch (contract) {
        case 'VAULT':
          return '8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf';
        case 'TOKEN_VAULT_PDA':
          return '9j17hjg8wR2uFxJAJDAFahwsgTCNx35sc5qXSxDmuuF6';
        case 'DATA_ACCOUNT':
          return '623nEsyGYWKYggY1yHxQFJiBarL9jdWdrMr7ASiCKP6a';
        case 'UPGRADE_MANAGER':
          return '274BzCz5RPHJZsxdcSGySahz4qAWqwSDcmz1YEKkGaZC';
        case 'UPGRADE_MANAGER_SIGNER':
          return '2SAhe89c1umM2JvCnmqCEnY8UCQtNPEKGe7UXA8KSQqH';
        case 'SolUsdc':
          return process.env.ARB_USDC_ADDRESS ?? '24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p';
        case 'CFTESTER':
          return '8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH';
        default:
          throw new Error(`Unsupported contract: ${contract}`);
      }
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

export function shortChainFromAsset(asset: Asset): string {
  switch (asset) {
    case 'Dot':
      return 'Dot';
    case 'Eth':
    case 'Flip':
    case 'Usdc':
    case 'Usdt':
      return 'Eth';
    case 'Btc':
      return 'Btc';
    case 'ArbUsdc':
    case 'ArbEth':
      return 'Arb';
    case 'Sol':
    case 'SolUsdc':
      return 'Sol';
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }
}

export function amountToFineAmount(amount: string, decimals: number | string): string {
  return new BigNumber(amount).shiftedBy(Number(decimals)).toFixed();
}

export function fineAmountToAmount(fineAmount: string, decimals: number | string): string {
  return new BigNumber(fineAmount).shiftedBy(-Number(decimals)).toFixed();
}

export function defaultAssetAmounts(asset: Asset): string {
  switch (asset) {
    case 'Btc':
      return '0.05';
    case 'Eth':
    case 'ArbEth':
      return '5';
    case 'Dot':
      return '50';
    case 'Usdc':
    case 'Usdt':
    case 'ArbUsdc':
    case 'Flip':
    case 'SolUsdc':
      return '500';
    case 'Sol':
      return '100';
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }
}

export function assetContractId(asset: Asset): number {
  if (isSDKAsset(asset)) return assetConstants[asset].contractId;
  if (asset === 'Sol') return 9;
  if (asset === 'SolUsdc') return 10;
  throw new Error(`Unsupported asset: ${asset}`);
}

export function assetDecimals(asset: Asset): number {
  if (isSDKAsset(asset)) return assetConstants[asset].decimals;
  if (asset === 'Sol') return 9;
  if (asset === 'SolUsdc') return 6;
  throw new Error(`Unsupported asset: ${asset}`);
}

export function chainContractId(chain: Chain): number {
  if (isSDKChain(chain)) return chainConstants[chain].contractId;
  if (chain === 'Solana') return 5;
  throw new Error(`Unsupported chain: ${chain}`);
}

export function chainGasAsset(chain: Chain): Asset {
  switch (chain) {
    case 'Ethereum':
      return Assets.Eth;
    case 'Bitcoin':
      return Assets.Btc;
    case 'Polkadot':
      return Assets.Dot;
    case 'Arbitrum':
      return Assets.ArbEth;
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

export function amountToFineAmountBigInt(amount: number | string, asset: Asset): bigint {
  const stringAmount = typeof amount === 'number' ? amount.toString() : amount;
  return BigInt(amountToFineAmount(stringAmount, assetDecimals(asset)));
}

// State Chain uses non-unique string identifiers for assets.
export function stateChainAssetFromAsset(asset: Asset): string {
  if (isSDKAsset(asset)) {
    return assetConstants[asset].asset;
  }
  throw new Error(`Unsupported asset: ${asset}`);
}

export const runWithTimeout = async <T>(promise: Promise<T>, millis: number): Promise<T> =>
  Promise.race([
    promise,
    sleep(millis, new Error(`Timed out after ${millis} ms.`), { ref: false }).then((error) => {
      throw error;
    }),
  ]);

export const sha256 = (data: string): Buffer => crypto.createHash('sha256').update(data).digest();

export const deferredPromise = <T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: Error) => void;
} => {
  let resolve: (value: T) => void;
  let reject: (error: Error) => void;

  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });

  return { promise, resolve: resolve!, reject: reject! };
};

export { sleep };

// @ts-expect-error polyfilling
Symbol.asyncDispose ??= Symbol('asyncDispose');
// @ts-expect-error polyfilling
Symbol.dispose ??= Symbol('dispose');

type DisposableApiPromise = ApiPromise & { [Symbol.asyncDispose](): Promise<void> };

// It is important to cache WS connections because nodes seem to have a
// limit on how many can be opened at the same time (from the same IP presumably)
function getCachedSubstrateApi(defaultEndpoint: string) {
  let api: DisposableApiPromise | undefined;
  let connections = 0;

  return async (providedEndpoint?: string): Promise<DisposableApiPromise> => {
    if (!api) {
      const endpoint = providedEndpoint ?? defaultEndpoint;

      const apiPromise = await ApiPromise.create({
        provider: new WsProvider(endpoint),
        noInitWarn: true,
        types: {
          EncodedAddress: {
            _enum: {
              Eth: '[u8; 20]',
              Arb: '[u8; 20]',
              Dot: '[u8; 32]',
              Btc: 'Vec<u8>',
            },
          },
        },
      });

      api = new Proxy(apiPromise as unknown as DisposableApiPromise, {
        get(target, prop, receiver) {
          if (prop === Symbol.asyncDispose) {
            return async () => {
              connections -= 1;
              if (connections === 0) {
                setTimeout(() => {
                  if (connections === 0) {
                    api = undefined;
                    Reflect.get(target, 'disconnect', receiver).call(target);
                  }
                }, 5_000).unref();
              }
            };
          }
          if (prop === 'disconnect') {
            return async () => {
              // noop
            };
          }

          return Reflect.get(target, prop, receiver);
        },
      });
    }

    connections += 1;

    return api;
  };
}

export const getChainflipApi = getCachedSubstrateApi(
  process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944',
);
export const getPolkadotApi = getCachedSubstrateApi(
  process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9947',
);

export const polkadotSigningMutex = new Mutex();

const toLowerCase = <const T extends string>(string: T) => string.toLowerCase() as Lowercase<T>;

export function ingressEgressPalletForChain(chain: Chain) {
  switch (chain) {
    case 'Ethereum':
    case 'Bitcoin':
    case 'Polkadot':
    case 'Arbitrum':
      return `${toLowerCase(chain)}IngressEgress` as const;
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

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
export type Event = { name: any; data: any; block: number; event_index: number };
export async function observeEvent(
  eventName: string,
  api: ApiPromise,
  eventQuery?: EventQuery,
  stopObserveEvent?: () => boolean,
  finalized = false,
): Promise<Event> {
  let result: Event | undefined;
  let eventFound = false;

  const query = eventQuery ?? (() => true);
  const stopObserve = stopObserveEvent ?? (() => false);

  const [expectedSection, expectedMethod] = eventName.split(':');

  const subscribeMethod = finalized
    ? api.rpc.chain.subscribeFinalizedHeads
    : api.rpc.chain.subscribeNewHeads;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await subscribeMethod(async (header) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const events: any[] = await api.query.system.events.at(header.hash);
    events.forEach((record, index) => {
      const { event } = record;
      if (
        !eventFound &&
        event.section.includes(expectedSection) &&
        event.method.includes(expectedMethod)
      ) {
        const expectedEvent = {
          name: { section: event.section, method: event.method },
          data: event.toHuman().data,
          block: header.number.toNumber(),
          event_index: index,
        };
        if (query(expectedEvent)) {
          result = expectedEvent;
          eventFound = true;
          unsubscribe();
        }
      }
    });
  });
  while (!eventFound && !stopObserve()) {
    await sleep(1000);
  }
  return result as Event;
}

export type EgressId = [Chain, number];
type BroadcastId = [Chain, number];
// Observe multiple events related to the same swap that could be emitted in the same block
export async function observeSwapEvents(
  { sourceAsset, destAsset, depositAddress, channelId }: SwapParams,
  api: ApiPromise,
  tag?: string,
  swapType?: SwapType,
  finalized = false,
): Promise<BroadcastId | undefined> {
  let eventFound = false;
  const subscribeMethod = finalized
    ? api.rpc.chain.subscribeFinalizedHeads
    : api.rpc.chain.subscribeNewHeads;

  const swapScheduledEvent = 'SwapScheduled';
  const swapExecutedEvent = 'SwapExecuted';
  const swapEgressScheduled = 'SwapEgressScheduled';
  const batchBroadcastRequested = 'BatchBroadcastRequested';
  let expectedMethod = swapScheduledEvent;

  let swapId = 0;
  let egressId: EgressId;
  let broadcastId;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await subscribeMethod(async (header) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const events: any[] = await api.query.system.events.at(header.hash);
    events.forEach((record) => {
      const { event } = record;
      if (!eventFound && event.method.includes(expectedMethod)) {
        const expectedEvent = {
          data: event.toHuman().data,
        };

        switch (expectedMethod) {
          case swapScheduledEvent:
            if ('DepositChannel' in expectedEvent.data.origin) {
              if (
                Number(expectedEvent.data.origin.DepositChannel.channelId) === channelId &&
                sourceAsset === (expectedEvent.data.sourceAsset as Asset) &&
                destAsset === (expectedEvent.data.destinationAsset as Asset) &&
                swapType
                  ? expectedEvent.data.swapType[swapType] !== undefined
                  : true &&
                    depositAddress ===
                      (Object.values(
                        expectedEvent.data.origin.DepositChannel.depositAddress,
                      )[0] as string)
              ) {
                expectedMethod = swapExecutedEvent;
                swapId = expectedEvent.data.swapId;
                console.log(`${tag} swap scheduled with swapId: ${swapId}`);
              }
            }
            break;
          case swapExecutedEvent:
            if (expectedEvent.data.swapId === swapId) {
              expectedMethod = swapEgressScheduled;
              console.log(`${tag} swap executed, with id: ${swapId}`);
            }
            break;
          case swapEgressScheduled:
            if (expectedEvent.data.swapId === swapId) {
              expectedMethod = batchBroadcastRequested;
              egressId = expectedEvent.data.egressId as EgressId;
              console.log(`${tag} swap egress scheduled with id: (${egressId[0]}, ${egressId[1]})`);
            }
            break;
          case batchBroadcastRequested:
            expectedEvent.data.egressIds.forEach((eventEgressId: EgressId) => {
              if (egressId[0] === eventEgressId[0] && egressId[1] === eventEgressId[1]) {
                broadcastId = [egressId[0], Number(expectedEvent.data.broadcastId)] as BroadcastId;
                console.log(`${tag} broadcast requested, with id: (${broadcastId})`);
                eventFound = true;
                unsubscribe();
              }
            });
            break;
          default:
            break;
        }
      }
    });
  });
  while (!eventFound) {
    await sleep(1000);
  }
  return broadcastId;
}

// TODO: To import from the SDK once it's exported
export enum SwapType {
  Swap = 'Swap',
  CcmPrincipal = 'CcmPrincipal',
  CcmGas = 'CcmGas',
}

export async function observeSwapScheduled(
  sourceAsset: Asset,
  destAsset: Asset,
  channelId: number,
  swapType?: SwapType,
) {
  await using chainflipApi = await getChainflipApi();

  // need to await this to prevent the chainflip api from being disposed prematurely
  const result = await observeEvent('swapping:SwapScheduled', chainflipApi, (event) => {
    if ('DepositChannel' in event.data.origin) {
      const channelMatches = Number(event.data.origin.DepositChannel.channelId) === channelId;
      const sourceAssetMatches = sourceAsset === (event.data.sourceAsset as Asset);
      const destAssetMatches = destAsset === (event.data.destinationAsset as Asset);
      const swapTypeMatches = swapType ? event.data.swapType[swapType] !== undefined : true;
      return channelMatches && sourceAssetMatches && destAssetMatches && swapTypeMatches;
    }
    // Otherwise it was a swap scheduled by interacting with the Eth smart contract
    return false;
  });

  return result;
}

// Make sure the stopObserveEvent returns true before the end of the test
export async function observeBadEvents(
  eventName: string,
  stopObserveEvent: () => boolean,
  eventQuery?: EventQuery,
  testTag?: string,
) {
  await using chainflipApi = await getChainflipApi();
  const event = await observeEvent(eventName, chainflipApi, eventQuery, stopObserveEvent);
  if (event) {
    const testMessage = testTag ? `Test: ${testTag}: ` : '';
    throw new Error(
      `${testMessage}Unexpected event emitted ${event.name.section}: ${event.name.method} in block ${event.block} `,
    );
  }
}

export async function observeBroadcastSuccess(broadcastId: BroadcastId, testTag?: string) {
  await using chainflipApi = await getChainflipApi();
  const broadcaster = broadcastId[0].toLowerCase() + 'Broadcaster';
  const broadcastIdNumber = broadcastId[1];

  let stopObserving = false;
  const observeBroadcastFailure = observeBadEvents(
    broadcaster + ':BroadcastAborted',
    () => stopObserving,
    (event) => {
      if (broadcastIdNumber === Number(event.data.broadcastId)) return true;
      return false;
    },
    testTag ? `observe BroadcastSuccess test tag: ${testTag}` : 'observe BroadcastSuccess',
  );

  await observeEvent(broadcaster + ':BroadcastSuccess', chainflipApi, (event) => {
    if (broadcastIdNumber === Number(event.data.broadcastId)) return true;
    return false;
  });

  stopObserving = true;
  await observeBroadcastFailure;
}

export async function newAddress(
  asset: Asset,
  seed: string,
  type?: BtcAddressType,
): Promise<string> {
  let rawAddress;

  switch (asset) {
    case Assets.Flip:
    case Assets.Eth:
    case Assets.Usdc:
    case Assets.Usdt:
    case Assets.ArbEth:
    case Assets.ArbUsdc:
      rawAddress = newEvmAddress(seed);
      break;
    case Assets.Dot:
      rawAddress = await newDotAddress(seed);
      break;
    case Assets.Btc:
      rawAddress = await newBtcAddress(seed, type ?? 'P2PKH');
      break;
    case 'Sol':
    case 'SolUsdc':
      rawAddress = newSolAddress(seed);
      break;
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }

  return String(rawAddress).trim();
}

export function chainFromAsset(asset: Asset): Chain {
  if (isSDKAsset(asset)) return assetConstants[asset].chain;
  if (asset === 'Sol' || asset === 'SolUsdc') return 'Solana';
  throw new Error(`Unsupported asset: ${asset}`);
}

export function getEvmEndpoint(chain: Chain): string {
  switch (chain) {
    case 'Ethereum':
      return process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
    case 'Arbitrum':
      return process.env.ARB_ENDPOINT ?? 'http://127.0.0.1:8547';
    default:
      throw new Error(`${chain} is not a supported EVM chain`);
  }
}

export function getSolConnection(): Connection {
  return new Connection(process.env.SOL_ENDPOINT ?? 'http://0.0.0.0:8899', {
    commitment: 'confirmed',
    wsEndpoint: 'ws://0.0.0.0:8900/',
  });
}

export function getWhaleMnemonic(chain: Chain): string {
  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
      return (
        process.env.ETH_USDC_WHALE_MNEMONIC ??
        'test test test test test test test test test test test junk'
      );
    default:
      throw new Error(`${chain} does not have a whale mnemonic`);
  }
}
export function getSolWhaleKeyPair(): Keypair {
  const secretKey = [
    6, 151, 150, 20, 145, 210, 176, 113, 98, 200, 192, 80, 73, 63, 133, 232, 208, 124, 81, 213, 117,
    199, 196, 243, 219, 33, 79, 217, 157, 69, 205, 140, 247, 157, 94, 2, 111, 18, 237, 198, 68, 58,
    83, 75, 44, 221, 80, 114, 35, 57, 137, 180, 21, 215, 89, 101, 115, 231, 67, 243, 229, 179, 134,
    251,
  ];
  return Keypair.fromSecretKey(new Uint8Array(secretKey));
}

export function getWhaleKey(chain: Chain): string {
  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
      return (
        process.env.ETH_USDC_WHALE ??
        '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
      );
    default:
      throw new Error(`${chain} does not have a whale key`);
  }
}

export async function observeBalanceIncrease(
  dstCcy: Asset,
  address: string,
  oldBalance: string,
): Promise<number> {
  for (let i = 0; i < 1200; i++) {
    const newBalance = Number(await getBalance(dstCcy, address));
    if (newBalance > Number(oldBalance)) {
      return newBalance;
    }

    await sleep(1000);
  }

  return Promise.reject(new Error('Failed to observe balance increase'));
}

export async function observeFetch(asset: Asset, address: string): Promise<void> {
  for (let i = 0; i < 120; i++) {
    const balance = Number(await getBalance(asset, address));
    if (balance === 0) {
      const chain = chainFromAsset(asset);
      if (chain === 'Ethereum' || chain === 'Arbitrum') {
        const web3 = new Web3(getEvmEndpoint(chain));
        if ((await web3.eth.getCode(address)) === '0x') {
          throw new Error('Eth address has no bytecode');
        }
      }
      return;
    }
    await sleep(1000);
  }

  throw new Error('Failed to observe the fetch');
}

type EVMEvent = {
  name: string;
  address: string;
  txHash: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  returnValues: any;
};
export async function observeEVMEvent(
  chain: Chain,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  contractAbi: any,
  address: string,
  eventName: string,
  eventParametersExpected: (string | null)[],
  stopObserveEvent?: () => boolean,
  initialBlockNumber?: number,
): Promise<EVMEvent | undefined> {
  const web3 = new Web3(getEvmEndpoint(chain));
  const contract = new web3.eth.Contract(contractAbi, address);
  let initBlockNumber = initialBlockNumber ?? (await web3.eth.getBlockNumber());
  const stopObserve = stopObserveEvent ?? (() => false);

  // Gets all the event parameter as an array
  const eventAbi = contractAbi.find((item) => item.type === 'event' && item.name === eventName)!;

  // Get the parameter names of the event
  const parameterNames = eventAbi.inputs.map((input) => input.name);

  for (let i = 0; i < 1200; i++) {
    if (stopObserve()) return undefined;
    const currentBlockNumber = await web3.eth.getBlockNumber();
    if (currentBlockNumber >= initBlockNumber) {
      const events = await contract.getPastEvents(eventName, {
        fromBlock: initBlockNumber,
        toBlock: currentBlockNumber,
      });
      for (let j = 0; j < events.length; j++) {
        if (Object.keys(events[j].returnValues).length / 2 !== parameterNames.length)
          throw new Error('Unexpected event length');
        for (let k = 0; k < parameterNames.length; k++) {
          // Allow for wildcard matching
          if (
            events[j].returnValues[k] !== eventParametersExpected[k] &&
            eventParametersExpected[k] !== '*'
          ) {
            break;
          } else if (k === parameterNames.length - 1) {
            return {
              name: events[j].event,
              address: events[j].address,
              txHash: events[j].transactionHash,
              returnValues: events[j].returnValues,
            };
          }
        }
      }
      initBlockNumber = currentBlockNumber + 1;
    }
    await sleep(2500);
  }

  throw new Error(`Failed to observe the ${eventName} event`);
}

export async function observeCcmReceived(
  sourceAsset: Asset,
  destAsset: Asset,
  address: string,
  messageMetadata: CcmDepositMetadata,
  sourceAddress?: string,
  stopObserveEvent?: () => boolean,
): Promise<EVMEvent | undefined> {
  return observeEVMEvent(
    chainFromAsset(destAsset),
    cfTesterAbi,
    address,
    'ReceivedxSwapAndCall',
    [
      chainContractId(chainFromAsset(sourceAsset)).toString(),
      sourceAddress ?? null,
      messageMetadata.message,
      getContractAddress(chainFromAsset(destAsset), destAsset.toString()),
      '*',
      '*',
      '*',
    ],
    stopObserveEvent,
  );
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

export function decodeSolAddress(address: string): string {
  return u8aToHex(base58Decode(address));
}

export function encodeSolAddress(address: string): string {
  return base58Encode(hexToU8a(address));
}

export function getEncodedSolAddress(address: string): string {
  return /^0x[a-fA-F0-9]+$/.test(address) ? encodeSolAddress(address) : address;
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
        error = section + '.' + name + ': ' + docs;
      } else {
        error = dispatchError.toString();
      }
      console.log('Dispatch error:' + error);
      console.log('Extrinsic failed: ' + error);
      process.exit(1);
    }
  };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function decodeModuleError(module: any, api: any): string {
  const errorIndex = {
    index: new BN(module.index),
    error: new Uint8Array(Buffer.from(module.error.slice(2), 'hex')),
  };
  const { docs, name, section } = api.registry.findMetaError(errorIndex);
  return `${section}.${name}: ${docs}`;
}

export function isValidHexHash(hash: string): boolean {
  const hexHashRegex = /^0x[0-9a-fA-F]{64}$/;
  return hexHashRegex.test(hash);
}

export function isValidEthAddress(address: string): boolean {
  const ethRegex = /^0x[a-fA-F0-9]{40}$/;
  return ethRegex.test(address);
}

export function isWithinOnePercent(value1: bigint, value2: bigint): boolean {
  if (value1 < value2) {
    return value2 - value1 <= value2 / BigInt(100);
  }
  if (value2 < value1) {
    return value1 - value2 <= value1 / BigInt(100);
  }
  return true;
}

// "v1 is greater than v2" -> "greater"
export function compareSemVer(version1: string, version2: string) {
  const v1 = version1.split('.').map(Number);
  const v2 = version2.split('.').map(Number);

  for (let i = 0; i < 3; i++) {
    if (v1[i] > v2[i]) return 'greater';
    if (v1[i] < v2[i]) return 'less';
  }

  return 'equal';
}

export function parseAssetString(input: string): Asset {
  const foundAsset = Object.values(Assets).find(
    (asset) => asset.toLowerCase() === input.toLowerCase(),
  );

  if (foundAsset) {
    return foundAsset as Asset;
  }
  throw new Error(`Unsupported asset: ${input}`);
}

type SwapRate = {
  intermediary: string;
  output: string;
};
export async function getSwapRate(from: Asset, to: Asset, fromAmount: string) {
  await using chainflipApi = await getChainflipApi();

  const fineFromAmount = amountToFineAmount(fromAmount, assetDecimals(from));
  const hexPrice = (await chainflipApi.rpc(
    'cf_swap_rate',
    {
      chain: chainFromAsset(from),
      asset: stateChainAssetFromAsset(from),
    },
    {
      chain: chainFromAsset(to),
      asset: stateChainAssetFromAsset(to),
    },
    Number(fineFromAmount).toString(16),
  )) as SwapRate;

  const finePriceOutput = parseInt(hexPrice.output);
  const outputPrice = fineAmountToAmount(finePriceOutput.toString(), assetDecimals(to));

  return outputPrice;
}

/// Submits an extrinsic and waits for it to be included in a block.
/// Returning the extrinsic result or throwing the dispatchError.
export async function submitChainflipExtrinsic(
  account: KeyringPair,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  extrinsic: any,
  errorOnFail = true,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): Promise<any> {
  await using chainflipApi = await getChainflipApi();

  let extrinsicResult;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await extrinsic.signAndSend(account, { nonce: -1 }, (arg: any) => {
    if (arg.blockNumber !== undefined || arg.dispatchError !== undefined) {
      extrinsicResult = arg;
    }
  });
  while (!extrinsicResult) {
    await sleep(100);
  }
  if (extrinsicResult.dispatchError && errorOnFail) {
    let error;
    if (extrinsicResult.dispatchError.isModule) {
      const { docs, name, section } = chainflipApi.registry.findMetaError(
        extrinsicResult.dispatchError.asModule,
      );
      error = section + '.' + name + ': ' + docs;
    } else {
      error = extrinsicResult.dispatchError.toString();
    }
    throw new Error(`Extrinsic failed: ${error}`);
  }
  return extrinsicResult;
}

export class ChainflipExtrinsicSubmitter {
  private keyringPair: KeyringPair;

  private mutex: Mutex;

  constructor(keyringPair: KeyringPair, mutex: Mutex = new Mutex()) {
    this.keyringPair = keyringPair;
    this.mutex = mutex;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async submit(extrinsic: any, errorOnFail: boolean = true) {
    let extrinsicResult;
    await this.mutex.runExclusive(async () => {
      extrinsicResult = await submitChainflipExtrinsic(this.keyringPair, extrinsic, errorOnFail);
    });
    return extrinsicResult;
  }
}

/// Calculate the fee using the given bps. Used for broker & boost fee calculation.
export function calculateFeeWithBps(fineAmount: bigint, bps: number): bigint {
  // Using some strange math here because the SC rounds down on 0.5 instead of up.
  const divisor = BigInt(10000 / bps);
  return fineAmount / divisor + (fineAmount % divisor > divisor / 2n ? 1n : 0n);
}
