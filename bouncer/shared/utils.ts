import { execSync } from 'child_process';
import * as crypto from 'crypto';
import { HDNodeWallet, Wallet, getDefaultProvider } from 'ethers';
import { setTimeout as sleep } from 'timers/promises';
import Client from 'bitcoin-core';
import { ApiPromise, Keyring } from '@polkadot/api';
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
import { Connection, Keypair, PublicKey } from '@solana/web3.js';
import { hexToU8a, u8aToHex, BN } from '@polkadot/util';
import { Vector, bool, Struct, Enum, Bytes as TsBytes } from 'scale-ts';
import BigNumber from 'bignumber.js';
import { EventParser, BorshCoder } from '@coral-xyz/anchor';
import { ISubmittableResult } from '@polkadot/types/types';
import { base58Decode, base58Encode, randomAsHex } from 'polkadot/util-crypto';
import { newDotAddress } from 'shared/new_dot_address';
import { BtcAddressType, newBtcAddress } from 'shared/new_btc_address';
import { getBalance } from 'shared/get_balance';
import { newEvmAddress } from 'shared/new_evm_address';
import { CcmDepositMetadata } from 'shared/new_swap';
import { getCFTesterAbi, getCfTesterIdl } from 'shared/contract_interfaces';
import { SwapParams } from 'shared/perform_swap';
import { newSolAddress } from 'shared/new_sol_address';
import { getChainflipApi, observeBadEvent, observeEvent } from 'shared/utils/substrate';
import { execWithLog } from 'shared/utils/exec_with_log';
import { send } from 'shared/send';
import { TestContext } from 'shared/utils/test_context';
import { Logger, loggerError, throwError } from 'shared/utils/logger';

const cfTesterAbi = await getCFTesterAbi();
const cfTesterIdl = await getCfTesterIdl();

export const lpMutex = new Mutex();
export const ethNonceMutex = new Mutex();
export const arbNonceMutex = new Mutex();
export const btcClientMutex = new Mutex();
export const brokerMutex = new Mutex();
export const snowWhiteMutex = new Mutex();

export const ccmSupportedChains = ['Ethereum', 'Arbitrum', 'Solana'] as Chain[];
export const vaultSwapSupportedChains = ['Ethereum', 'Arbitrum', 'Solana', 'Bitcoin'] as Chain[];
export const evmChains = ['Ethereum', 'Arbitrum'] as Chain[];

export const testInfoFile = '/tmp/chainflip/test_info.csv';

export type Asset = SDKAsset;
export type Chain = SDKChain;

export type VaultSwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  destAddress: string;
  transactionId: TransactionOriginId;
};

const isSDKAsset = (asset: Asset): asset is SDKAsset => asset in assetConstants;
const isSDKChain = (chain: Chain): chain is SDKChain => chain in chainConstants;

export type HubAsset = 'HubUsdc' | 'HubUsdt';

export function isPolkadotAsset(asset: string): boolean {
  return asset === 'Dot' || asset === 'HubDot' || asset === 'HubUsdc' || asset === 'HubUsdt';
}

export function getHubAssetId(asset: HubAsset) {
  switch (asset) {
    case 'HubUsdc':
      return 1337;
    case 'HubUsdt':
      return 1984;
    default:
      throw new Error(`Unsupported Assethub asset: ${asset}`);
  }
}

// Nonces deployed in two stages
export const solanaNumberOfNonces: number = 10;
export const solanaNumberOfAdditionalNonces: number = 40;

const solCcmAccountsCodec = Struct({
  cf_receiver: Struct({
    pubkey: TsBytes(32),
    is_writable: bool,
  }),
  additional_accounts: Vector(
    Struct({
      pubkey: TsBytes(32),
      is_writable: bool,
    }),
  ),
  fallback_address: TsBytes(32),
});

const solCcmAltAccountsCodec = Struct({
  ccm_accounts: solCcmAccountsCodec,
  alts: Vector(TsBytes(32)),
});

export const solVersionedCcmAdditionalDataCodec = Enum({
  V0: solCcmAccountsCodec,
  V1: solCcmAltAccountsCodec,
});

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
        case 'PRICE_FEED_BTC':
          return '0x322813Fd9A801c5507c9de605d63CEA4f2CE6c44';
        case 'PRICE_FEED_ETH':
          return '0xa85233C63b9Ee964Add6F2cffe00Fd84eb32338f';
        case 'PRICE_FEED_SOL':
          return '0x4A679253410272dd5232B3Ff7cF5dbB88f295319';
        default:
          throw new Error(`Unsupported contract: ${contract}`);
      }
    case 'Arbitrum':
      switch (contract) {
        case 'VAULT':
          return '0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512';
        case 'KEY_MANAGER':
          return '0x5FbDB2315678afecb367f032d93F642f64180aa3';
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
          return '7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ';
        case 'TOKEN_VAULT_ATA':
          return '9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9';
        case 'DATA_ACCOUNT':
          return 'BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG';
        case 'SolUsdc':
          return process.env.SOL_USDC_ADDRESS ?? '24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p';
        case 'SolUsdcTokenSupport':
          return PublicKey.findProgramAddressSync(
            [
              Buffer.from('supported_token'),
              new PublicKey(getContractAddress('Solana', 'SolUsdc')).toBuffer(),
            ],
            new PublicKey(getContractAddress('Solana', 'VAULT')),
          )[0].toBase58();
        case 'CFTESTER':
          return '8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH';
        case 'SWAP_ENDPOINT':
          return '35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT';
        case 'SWAP_ENDPOINT_DATA_ACCOUNT':
          return '2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9';
        case 'SWAP_ENDPOINT_NATIVE_VAULT_ACCOUNT':
          return 'EWaGcrFXhf9Zq8yxSdpAa75kZmDXkRxaP17sYiL6UpZN';
        case 'USER_ADDRESS_LOOKUP_TABLE':
          return '9J7p5fmNjRjdYRyutwssUBRLYmJEmK7fwBpYxhtnxYTG';
        case 'PRICE_FEED_MOCK':
          return 'DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X';
        case 'PRICE_FEED_BTC':
          return 'HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG';
        case 'PRICE_FEED_ETH':
          return '8U3c4SqXaXKPQiarNH3xHXiVoBLYbkqkzusthyJJjGrE';
        case 'PRICE_FEED_SOL':
          return 'CrjmdLxTkmd5bxTQjE82FNgiuxeoY3G4EzzhDJ4RH9Wx';
        default:
          throw new Error(`Unsupported contract: ${contract}`);
      }
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

export function shortChainFromAsset(asset: Asset) {
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
    case 'HubDot':
    case 'HubUsdc':
    case 'HubUsdt':
      return 'Hub';
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
    case 'HubDot':
      return '50';
    case 'Usdc':
    case 'Usdt':
    case 'ArbUsdc':
    case 'Flip':
    case 'SolUsdc':
    case 'HubUsdc':
    case 'HubUsdt':
      return '500';
    case 'Sol':
      return '100';
    default:
      throw new Error(`Unsupported asset: ${asset}`);
  }
}

export function assetContractId(asset: Asset): number {
  if (isSDKAsset(asset)) return assetConstants[asset].contractId;
  throw new Error(`Unsupported asset: ${asset}`);
}

export function assetDecimals(asset: Asset): number {
  if (isSDKAsset(asset)) return assetConstants[asset].decimals;
  throw new Error(`Unsupported asset: ${asset}`);
}

export function chainContractId(chain: Chain): number {
  if (isSDKChain(chain)) return chainConstants[chain].contractId;
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
    case 'Solana':
      return Assets.Sol;
    case 'Assethub':
      return Assets.HubDot;
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

export async function runWithTimeout<T>(
  promise: Promise<T>,
  seconds: number,
  logger?: Logger,
  taskDescription?: string,
): Promise<T> {
  // Add the task description to the error message if provided
  let error = new Error(
    `Timed out after ${seconds}s.` + (taskDescription ? ` Waiting on: ${taskDescription}` : ''),
  );
  if (logger) {
    // Add the logger info to the error message if a logger is provided
    error = loggerError(logger, error);
  }
  return Promise.race([
    promise,
    sleep(seconds * 1000, error, { ref: false }).then((e) => {
      throw e;
    }),
  ]);
}

/// Runs the given promise with a timeout and handles exiting the process. Used for running commands.
export async function runWithTimeoutAndExit<T>(
  promise: Promise<T>,
  seconds: number,
): Promise<void> {
  const start = Date.now();
  await runWithTimeout(promise, seconds).catch((error) => {
    console.error(error);
    process.exit(-1);
  });
  const executionTime = (Date.now() - start) / 1000;

  if (executionTime > seconds * 0.9) {
    console.warn(`Warning: Execution time was close to the timeout: ${executionTime}/${seconds}s`);
  } else {
    console.log(`Execution time: ${executionTime}/${seconds}s`);
  }
  process.exit(0);
}

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

export const polkadotSigningMutex = new Mutex();
export const assethubSigningMutex = new Mutex();

const toLowerCase = <const T extends string>(string: T) => string.toLowerCase() as Lowercase<T>;

export function ingressEgressPalletForChain(chain: Chain) {
  switch (chain) {
    case 'Ethereum':
    case 'Bitcoin':
    case 'Polkadot':
    case 'Arbitrum':
    case 'Assethub':
    case 'Solana':
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
export type Event = { name: any; data: any; block: number; event_index: number };

export type EgressId = [Chain, number];
type BroadcastChainAndId = [Chain, number];
// Observe multiple events related to the same swap that could be emitted in the same block
export async function observeSwapEvents(
  logger: Logger,
  { sourceAsset, destAsset, depositAddress, channelId }: SwapParams,
  api: ApiPromise,
  finalized = false,
): Promise<BroadcastChainAndId | undefined> {
  let broadcastEventFound = false;
  const subscribeMethod = finalized
    ? api.rpc.chain.subscribeFinalizedHeads
    : api.rpc.chain.subscribeNewHeads;

  const swapRequestedEvent = 'SwapRequested';
  const swapScheduledEvent = 'SwapScheduled';
  const swapExecutedEvent = 'SwapExecuted';
  const swapEgressScheduled = 'SwapEgressScheduled';
  const batchBroadcastRequested = 'BatchBroadcastRequested';
  let expectedEvent = swapRequestedEvent;

  let swapId: number | undefined;
  let swapRequestId: number | undefined;
  let egressId: EgressId;
  let broadcastId;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await subscribeMethod(async (header) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const events: any[] = await api.query.system.events.at(header.hash);

    for (const record of events) {
      const { event } = record;
      if (broadcastEventFound || !event.method.includes(expectedEvent)) {
        // eslint-disable-next-line no-continue
        continue;
      }

      const data = event.toHuman().data;

      switch (expectedEvent) {
        case swapRequestedEvent: {
          const channel = data.origin.DepositChannel;

          if (
            channel &&
            Number(channel.channelId) === channelId &&
            Object.values(channel.depositAddress)[0] === depositAddress &&
            sourceAsset === (data.inputAsset as Asset) &&
            destAsset === (data.outputAsset as Asset)
          ) {
            swapRequestId = data.swapRequestId;
            expectedEvent = swapScheduledEvent;
          }

          break;
        }
        case swapScheduledEvent:
          if (data.swapRequestId === swapRequestId) {
            swapId = data.swapId;
            expectedEvent = swapExecutedEvent;
          }

          break;
        case swapExecutedEvent:
          if (data.swapId === swapId) {
            expectedEvent = swapEgressScheduled;
            logger.trace(`Swap executed, with id: ${swapId}`);
          }
          break;
        case swapEgressScheduled:
          if (data.swapRequestId === swapRequestId) {
            expectedEvent = batchBroadcastRequested;
            egressId = data.egressId as EgressId;
            logger.trace(`Swap egress scheduled with id: (${egressId[0]}, ${egressId[1]})`);
          }
          break;
        case batchBroadcastRequested:
          for (const eventEgressId of data.egressIds) {
            if (egressId[0] === eventEgressId[0] && egressId[1] === eventEgressId[1]) {
              broadcastId = [egressId[0], Number(data.broadcastId)] as BroadcastChainAndId;
              logger.trace(`Broadcast requested, with id: (${broadcastId})`);
              broadcastEventFound = true;
              unsubscribe();
              break;
            }
          }
          break;
        default:
          break;
      }
    }
  });
  while (!broadcastEventFound) {
    if (!api.isConnected) {
      throw new Error('API is not connected');
    }
    await sleep(1000);
  }
  return broadcastId;
}

export enum SwapType {
  Swap = 'Swap',
  NetworkFee = 'NetworkFee',
  IngressEgressFee = 'IngressEgressFee',
}

export enum SwapRequestType {
  Regular = 'Regular',
  NetworkFee = 'NetworkFee',
  IngressEgressFee = 'IngressEgressFee',
}

export enum TransactionOrigin {
  DepositChannel = 'DepositChannel',
  VaultSwapEvm = 'VaultSwapEvm',
  VaultSwapSolana = 'VaultSwapSolana',
  VaultSwapBitcoin = 'VaultSwapBitcoin',
  OnChainAccount = 'OnChainAccount',
}

export type TransactionOriginId =
  | { type: TransactionOrigin.DepositChannel; channelId: number }
  | { type: TransactionOrigin.VaultSwapEvm; txHash: string }
  | { type: TransactionOrigin.VaultSwapSolana; addressAndSlot: [string, number] }
  | { type: TransactionOrigin.VaultSwapBitcoin; txId: string }
  | { type: TransactionOrigin.OnChainAccount; accountId: string };

function checkRequestTypeMatches(actual: object | string, expected: SwapRequestType) {
  if (typeof actual === 'object') {
    return expected in actual;
  }
  return expected === actual;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function checkTransactionInMatches(actual: any, expected: TransactionOriginId): boolean {
  if ('DepositChannel' in actual) {
    return (
      expected.type === TransactionOrigin.DepositChannel &&
      Number(actual.DepositChannel.channelId.replaceAll(',', '')) === expected.channelId
    );
  }
  if ('Vault' in actual) {
    return (
      ('Evm' in actual.Vault.txId &&
        expected.type === TransactionOrigin.VaultSwapEvm &&
        actual.Vault.txId.Evm === expected.txHash) ||
      ('Solana' in actual.Vault.txId &&
        expected.type === TransactionOrigin.VaultSwapSolana &&
        actual.Vault.txId.Solana[1].replaceAll(',', '') === expected.addressAndSlot[1].toString() &&
        actual.Vault.txId.Solana[0].toString() === expected.addressAndSlot[0].toString()) ||
      ('Bitcoin' in actual.Vault.txId &&
        expected.type === TransactionOrigin.VaultSwapBitcoin &&
        actual.Vault.txId.Bitcoin ===
          // Reverse byte order of BTC transactions
          '0x' +
            // eslint-disable-next-line @typescript-eslint/no-use-before-define
            [...new Uint8Array(hexStringToBytesArray(expected.txId).reverse())]
              .map((x) => x.toString(16).padStart(2, '0'))
              .join(''))
    );
  }
  if ('OnChainAccount' in actual) {
    return (
      expected.type === TransactionOrigin.OnChainAccount &&
      actual.OnChainAccount.accountId === expected.accountId
    );
  }
  throw new Error(`Unsupported transaction origin type ${actual}`);
}

export async function observeSwapRequested(
  logger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  id: TransactionOriginId,
  swapRequestType: SwapRequestType,
) {
  // need to await this to prevent the chainflip api from being disposed prematurely
  return observeEvent(logger, 'swapping:SwapRequested', {
    test: (event) => {
      const data = event.data;

      if (typeof data.origin === 'object') {
        const channelMatches = checkTransactionInMatches(data.origin, id);
        const sourceAssetMatches = sourceAsset === (data.inputAsset as Asset);
        const destAssetMatches = destAsset === (data.outputAsset as Asset);
        const requestTypeMatches = checkRequestTypeMatches(data.requestType, swapRequestType);

        return channelMatches && sourceAssetMatches && destAssetMatches && requestTypeMatches;
      }
      // Otherwise it was a swap scheduled by interacting with the Eth smart contract
      return false;
    },
    // We assume that a swaprequest is uniquely identifiable by the `id: TransactionOriginId`.
    // To reduce potential race conditions we always check the last 30 blocks.
    historicalCheckBlocks: 30,
  }).event;
}

export async function observeBroadcastSuccess(logger: Logger, broadcastId: BroadcastChainAndId) {
  const broadcaster = broadcastId[0].toLowerCase() + 'Broadcaster';
  const broadcastIdNumber = broadcastId[1];

  const observeBroadcastFailure = observeBadEvent(logger, `${broadcaster}:BroadcastAborted`, {
    test: (event) => broadcastIdNumber === Number(event.data.broadcastId),
  });

  await observeEvent(logger, `${broadcaster}:BroadcastSuccess`, {
    test: (event) => broadcastIdNumber === Number(event.data.broadcastId),
  }).event;

  await observeBroadcastFailure.stop();
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
    case Assets.HubDot:
    case Assets.HubUsdc:
    case Assets.HubUsdt:
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

// Returns an address that can hold an asset and can be used as a destination
// address of a swap or a refund address. If it's a CCM swap or refund, the
// returned address is a valid CCM receiver.
export async function newAssetAddress(
  asset: Asset,
  seed?: string,
  type?: BtcAddressType,
  isCcm = false,
): Promise<string> {
  const chain = chainFromAsset(asset);
  // For CCM swaps the destination address should be the CF Tester.
  // Solana CCM are egressed to a random destination address
  if (isCcm && chain !== 'Solana') {
    if (!ccmSupportedChains.includes(chain)) {
      throw new Error(`Unsupported chain for CCM: ${chain}`);
    }
    return getContractAddress(chain, 'CFTESTER');
  }
  return newAddress(asset, seed ?? randomAsHex(32), type);
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
  return new Connection(process.env.SOL_HTTP_ENDPOINT ?? 'http://0.0.0.0:8899', {
    commitment: 'confirmed',
    wsEndpoint: `${process.env.SOL_WS_ENDPOINT ?? 'ws://0.0.0.0:8900'}`,
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
  logger: Logger,
  dstCcy: Asset,
  address: string,
  oldBalance: string,
): Promise<number> {
  logger.debug(`Observing balance increase of ${dstCcy} at ${address}`);
  for (let i = 0; i < 2400; i++) {
    const newBalance = Number(await getBalance(dstCcy, address));
    if (newBalance > Number(oldBalance)) {
      return newBalance;
    }

    await sleep(3000);
  }

  return throwError(logger, new Error('Failed to observe balance increase'));
}

export async function observeFetch(asset: Asset, address: string): Promise<void> {
  for (let i = 0; i < 360; i++) {
    const balance = Number(await getBalance(asset, address));
    if (balance === 0) {
      const chain = chainFromAsset(asset);
      if (chain === 'Ethereum' || chain === 'Arbitrum') {
        const web3 = new Web3(getEvmEndpoint(chain));
        if ((await web3.eth.getCode(address)) === '0x') {
          throw new Error('EVM address has no bytecode');
        }
      }
      return;
    }
    await sleep(3000);
  }

  throw new Error('Failed to observe the fetch');
}

type ContractEvent = {
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
  destAddress: string,
  eventName: string,
  eventParametersExpected: (string | null)[],
  stopObserveEvent?: () => boolean,
  initialBlockNumber?: number,
): Promise<ContractEvent | undefined> {
  const web3 = new Web3(getEvmEndpoint(chain));
  const contract = new web3.eth.Contract(contractAbi, destAddress);
  let initBlockNumber = initialBlockNumber ?? (await web3.eth.getBlockNumber());
  const stopObserve = stopObserveEvent ?? (() => false);

  // Gets all the event parameter as an array
  const eventAbi = contractAbi.find(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (item: any) => item.type === 'event' && item.name === eventName,
  )!;

  // Get the parameter names of the event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const parameterNames = eventAbi.inputs.map((input: any) => input.name);

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

export async function observeSolanaCcmEvent(
  eventName: string,
  sourceChain: string,
  sourceAddress: string | null,
  messageMetadata: CcmDepositMetadata,
): Promise<ContractEvent> {
  const connection = getSolConnection();
  const idl = cfTesterIdl;
  const cfTesterAddress = new PublicKey(getContractAddress('Solana', 'CFTESTER'));

  for (let i = 0; i < 300; i++) {
    const txSignatures = await connection.getSignaturesForAddress(cfTesterAddress);
    for (const txSignature of txSignatures) {
      const tx = await connection.getTransaction(txSignature.signature, {
        maxSupportedTransactionVersion: 0,
      });
      if (tx) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const eventParser = new EventParser(cfTesterAddress, new BorshCoder(idl as any));
        const events = eventParser.parseLogs(tx.meta?.logMessages ?? []);
        for (const event of events) {
          const matchEventName = event.name === eventName;
          const matchSourceChain = event.data.source_chain.toString() === sourceChain;

          const hexMessage = '0x' + (event.data.message as Buffer).toString('hex');
          const matchMessage = hexMessage === messageMetadata.message;

          // The message is being used as the main discriminator
          if (matchEventName && matchSourceChain && matchMessage) {
            const decodedCcmAdditionalData = solVersionedCcmAdditionalDataCodec.dec(
              messageMetadata.ccmAdditionalData!,
            );
            const expectedAdditionalAccounts =
              decodedCcmAdditionalData.tag === 'V0'
                ? decodedCcmAdditionalData.value.additional_accounts
                : decodedCcmAdditionalData.value.ccm_accounts.additional_accounts;

            if (
              expectedAdditionalAccounts.length !== event.data.remaining_is_writable.length ||
              expectedAdditionalAccounts.length !== event.data.remaining_pubkeys.length
            ) {
              throw new Error(
                `Unexpected additional accounts length: ${expectedAdditionalAccounts.length}, expecting ${event.data.remaining_is_writable.length}, ${event.data.remaining_pubkeys.length}`,
              );
            }

            for (let index = 0; index < expectedAdditionalAccounts.length; index++) {
              if (
                expectedAdditionalAccounts[index].is_writable.toString() !==
                event.data.remaining_is_writable[index].toString()
              ) {
                throw new Error(
                  `Unexpected additional account is_writable: ${event.data.remaining_is_writable[index]}, expecting ${expectedAdditionalAccounts[index].is_writable}`,
                );
              }
              const expectedPubkey = new PublicKey(
                expectedAdditionalAccounts[index].pubkey,
              ).toString();
              if (expectedPubkey !== event.data.remaining_pubkeys[index].toString()) {
                throw new Error(
                  `Unexpected additional account pubkey: ${event.data.remaining_pubkeys[index].toString()}, expecting ${expectedPubkey}`,
                );
              }
            }

            if (event.data.remaining_is_signer.some((value: boolean) => value === true)) {
              throw new Error(`Expected all additional accounts to be read-only`);
            }

            // Expect always empty bytes as source address. This will change when we have versioned transactions.
            if (event.data.source_address.toString() !== Buffer.from([]).toString()) {
              throw new Error(
                `Unexpected source address: ${event.data.source_address}, expecting empty ${Buffer.from([0])}`,
              );
            }
            return {
              name: event.name,
              address: cfTesterAddress.toString(),
              txHash: txSignature.signature,
              returnValues: event.data,
            };
          }
        }
      }
    }
    await sleep(10000);
  }
  throw new Error(`Failed to observe Solana's ${eventName} event`);
}

export async function observeCcmReceived(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata: CcmDepositMetadata,
  sourceAddress?: string,
  stopObserveEvent?: () => boolean,
): Promise<ContractEvent | undefined> {
  const destChain = chainFromAsset(destAsset);
  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      return observeEVMEvent(
        destChain,
        cfTesterAbi,
        destAddress,
        'ReceivedxSwapAndCall',
        [
          chainContractId(chainFromAsset(sourceAsset)).toString(),
          sourceAddress ?? null,
          messageMetadata.message,
          getContractAddress(destChain, destAsset.toString()),
          '*',
          '*',
          '*',
        ],
        stopObserveEvent,
      );
    case 'Solana':
      return observeSolanaCcmEvent(
        'ReceivedCcm',
        chainContractId(chainFromAsset(sourceAsset)).toString(),
        sourceAddress ?? null,
        messageMetadata,
      );
    case 'Assethub':
      // In Assethub XCM it is not clear what destination chain the XCM call should be observed
      // Instead, we check the success manually in the appropriate test.
      return Promise.resolve(undefined);
    default:
      throw new Error(`Unsupported chain: ${destChain}`);
  }
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

export function handleSubstrateError(api: ApiPromise, exit = true) {
  return (arg: ISubmittableResult) => {
    const { dispatchError } = arg;
    if (dispatchError) {
      let error;
      if (dispatchError.isModule) {
        const { docs, name, section } = api.registry.findMetaError(dispatchError.asModule);
        error = section + '.' + name + ': ' + docs;
      } else {
        error = dispatchError.toString();
      }
      if (exit) {
        console.log('Dispatch error:' + error);
        process.exit(-1);
      } else {
        throw new Error('Dispatch error: ' + error);
      }
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

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let extrinsicResult: any;
  const nonce = await chainflipApi.rpc.system.accountNextIndex(account.address);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await extrinsic.signAndSend(account, { nonce }, (arg: any) => {
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

// Throws error if unsuccessful.
export async function tryUntilSuccess(
  closure: () => Promise<boolean>,
  pollTime: number,
  maxAttempts: number,
  logTag?: string,
) {
  for (let i = 0; i < maxAttempts; i++) {
    if (await closure()) {
      return;
    }
    await sleep(pollTime);
  }
  throw new Error('tryUntilSuccess failed: ' + logTag);
}

export async function getNodesInfo(numberOfNodes: 1 | 3) {
  const SELECTED_NODES = numberOfNodes === 1 ? 'bashful' : 'bashful doc dopey';
  const nodeCount = numberOfNodes + '-node';
  return { SELECTED_NODES, nodeCount };
}

export async function killEngines() {
  execSync(`kill $(ps aux | grep engine-runner | grep -v grep | awk '{print $2}')`);
}

export async function startEngines(
  localnetInitPath: string,
  binaryPath: string,
  numberOfNodes: 1 | 3,
) {
  console.log('Starting all the engines');

  const { SELECTED_NODES, nodeCount } = await getNodesInfo(numberOfNodes);
  await execWithLog(
    `${localnetInitPath}/scripts/start-all-engines.sh`,
    'start-all-engines-pre-upgrade',
    {
      INIT_RUN: 'false',
      LOG_SUFFIX: '-pre-upgrade',
      NODE_COUNT: nodeCount,
      SELECTED_NODES,
      LOCALNET_INIT_DIR: localnetInitPath,
      BINARY_ROOT_PATH: binaryPath,
    },
  );

  await sleep(7000);

  console.log('Engines started');
}

// Check that all Solana Nonces are available
export async function checkAvailabilityAllSolanaNonces(testContext: TestContext) {
  testContext.info('Checking Solana Nonce Availability');

  // Check that all Solana nonces are available
  await using chainflip = await getChainflipApi();
  const maxRetries = 10; // 60 seconds
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    const availableNonces = (await chainflip.query.environment.solanaAvailableNonceAccounts())
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .toJSON() as any[];
    if (availableNonces.length === solanaNumberOfNonces + solanaNumberOfAdditionalNonces) {
      break;
    } else if (attempt === maxRetries - 1) {
      throw new Error(
        `Unexpected number of available nonces: ${availableNonces.length}, expected ${solanaNumberOfNonces + solanaNumberOfAdditionalNonces}`,
      );
    } else {
      await sleep(6000);
    }
  }
}

export function createStateChainKeypair(uri: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  return keyring.createFromUri(uri);
}

/// Takes the user friendly price of an "asset per asset" and converts it to the internal price format.
export function assetPriceToInternalAssetPrice(
  baseAsset: Asset,
  quoteAsset: Asset,
  price: number,
): string {
  return BigInt(
    Math.round((price / 10 ** (assetDecimals(baseAsset) - assetDecimals(quoteAsset))) * 2 ** 128),
  ).toString();
}

// Get the current time in the format HH:MM:SS
export function getTimeStamp(): string {
  const now = new Date();
  const hours = now.getHours().toString().padStart(2, '0');
  const minutes = now.getMinutes().toString().padStart(2, '0');
  const seconds = now.getSeconds().toString().padStart(2, '0');
  return `${hours}:${minutes}:${seconds}`;
}

export async function createEvmWalletAndFund(logger: Logger, asset: Asset): Promise<HDNodeWallet> {
  const chain = chainFromAsset(asset);

  const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
  if (mnemonic === '') {
    throw new Error('Failed to create random mnemonic');
  }
  const wallet = Wallet.fromPhrase(mnemonic).connect(getDefaultProvider(getEvmEndpoint(chain)));
  await send(logger, chainGasAsset(chain) as SDKAsset, wallet.address, undefined);
  await send(logger, asset, wallet.address, undefined);
  return wallet;
}

/**
 * Executes an RPC call with automatic retries and timeout handling.
 *
 * This function attempts to execute the provided RPC call function, and if it fails,
 * will retry up to the specified maximum number of attempts. Each attempt is also
 * subject to a timeout, after which the attempt is considered failed.
 *
 * @param rpcCall - A function that returns a Promise with the RPC call result
 * @param options - Configuration options:
 *   - maxAttempts: Maximum number of retry attempts
 *   - timeoutMs: Timeout in milliseconds for each attempt
 *   - operation: Description of the operation for logging purposes
 * @returns A Promise that resolves with the result of the RPC call
 * @throws Error if all retry attempts fail or timeout
 */
export async function retryRpcCall<T>(
  rpcCall: () => Promise<T>,
  options: { maxAttempts: number; timeoutMs: number; operation: string },
): Promise<T> {
  const { maxAttempts, timeoutMs, operation } = options;
  let attempt = 0;

  while (attempt < maxAttempts) {
    try {
      // Use Promise.race to handle timeout
      return await Promise.race([
        rpcCall(),
        new Promise<T>((_, reject) => {
          setTimeout(() => reject(new Error(`Timeout after ${timeoutMs}ms`)), timeoutMs);
        }),
      ]);
    } catch (error) {
      attempt++;
      console.warn(`Attempt ${attempt} failed for ${operation}: ${error}`);
      if (attempt >= maxAttempts) {
        throw new Error(`Failed to complete ${operation} after ${maxAttempts} attempts`);
      }
    }
  }

  throw new Error(`Failed to complete ${operation} after ${maxAttempts} attempts`);
}
