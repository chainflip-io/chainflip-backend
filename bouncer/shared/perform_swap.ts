import { InternalAsset as Asset } from '@chainflip/cli';
import { Keyring } from '@polkadot/api';
import { encodeAddress } from '../polkadot/util-crypto';
import { DcaParams, newSwap, FillOrKillParamsX128 } from './new_swap';
import { send, sendViaCfTester } from './send';
import { getBalance } from './get_balance';
import {
  observeBalanceIncrease,
  observeCcmReceived,
  shortChainFromAsset,
  observeSwapEvents,
  observeBroadcastSuccess,
  getEncodedSolAddress,
  observeFetch,
  chainFromAsset,
  observeSwapRequested,
  SwapRequestType,
  evmChains,
  createEvmWalletAndFund,
  getSolWhaleKeyPair,
  decodeSolAddress,
  VaultSwapParams,
  TransactionOriginId,
  TransactionOrigin,
  defaultAssetAmounts,
  newAddress,
  getContractAddress,
  WhaleKeyManager,
} from '../shared/utils';
import { CcmDepositMetadata } from '../shared/new_swap';
import { SwapContext, SwapStatus } from './swap_context';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { executeEvmVaultSwap } from './evm_vault_swap';
import { executeSolVaultSwap } from './sol_vault_swap';
import { buildAndSendBtcVaultSwap } from './btc_vault_swap';

function encodeDestinationAddress(address: string, destAsset: Asset): string {
  let destAddress = address;

  if (destAddress && destAsset === 'Dot') {
    destAddress = encodeAddress(destAddress);
  } else if (shortChainFromAsset(destAsset) === 'Sol') {
    destAddress = getEncodedSolAddress(destAddress);
  }

  return destAddress;
}

export type SwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  depositAddress: string;
  destAddress: string;
  channelId: number;
};

export async function requestNewSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  tag = '',
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps?: number,
  log = true,
  boostFeeBps = 0,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<SwapParams> {
  const addressPromise = observeEvent('swapping:SwapDepositAddressReady', {
    test: (event) => {
      // Find deposit address for the right swap by looking at destination address:
      const destAddressEvent = encodeDestinationAddress(
        event.data.destinationAddress[shortChainFromAsset(destAsset)],
        destAsset,
      );
      if (!destAddressEvent) return false;

      const destAssetMatches = event.data.destinationAsset === destAsset;
      const sourceAssetMatches = event.data.sourceAsset === sourceAsset;
      const destAddressMatches =
        destAddressEvent.toLowerCase() ===
        encodeDestinationAddress(destAddress, destAsset).toLowerCase();

      const ccmMetadataMatches = messageMetadata
        ? event.data.channelMetadata !== null &&
        event.data.channelMetadata.message ===
        (messageMetadata.message === '0x' ? '' : messageMetadata.message) &&
        event.data.channelMetadata.gasBudget.replace(/,/g, '') === messageMetadata.gasBudget &&
        event.data.channelMetadata.ccmAdditionalData ===
        (messageMetadata.ccmAdditionalData === '0x' ? '' : messageMetadata.ccmAdditionalData)
        : event.data.channelMetadata === null;

      return destAddressMatches && destAssetMatches && sourceAssetMatches && ccmMetadataMatches;
    },
  }).event;

  await newSwap(
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    brokerCommissionBps,
    boostFeeBps,
    fillOrKillParams,
    dcaParams,
  );

  const res = (await addressPromise).data;

  const depositAddress = res.depositAddress[shortChainFromAsset(sourceAsset)];
  const channelDestAddress = res.destinationAddress[shortChainFromAsset(destAsset)];
  const channelId = Number(res.channelId.replaceAll(',', ''));

  if (log) {
    console.log(`${tag} Deposit address: ${depositAddress}`);
    console.log(`${tag} Destination address is: ${channelDestAddress} Channel ID is: ${channelId}`);
  }

  return {
    sourceAsset,
    destAsset,
    depositAddress,
    destAddress,
    channelId,
  };
}

export enum SenderType {
  Address,
  Vault,
}

export async function doPerformSwap(
  { sourceAsset, destAsset, destAddress, depositAddress, channelId }: SwapParams,
  tag = '',
  // only used for EVM chains at the moment
  privateKey?: string,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  log = true,
  swapContext?: SwapContext,
) {
  const oldBalance = await getBalance(destAsset, destAddress);

  if (log) console.log(`${tag} Old balance: ${oldBalance}`);

  const swapRequestedHandle = observeSwapRequested(
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  if (senderType === SenderType.Address) {
    await send(sourceAsset, depositAddress, amount, log, privateKey);
  } else {
    if (!privateKey) {
      throw new Error('No private key provided');
    }
    await sendViaCfTester(sourceAsset, depositAddress, privateKey, amount);
  }

  if (log) console.log(`${tag} Funded the address`);

  swapContext?.updateStatus(tag, SwapStatus.Funded);

  await swapRequestedHandle;

  swapContext?.updateStatus(tag, SwapStatus.SwapScheduled);

  if (log) console.log(`${tag} Waiting for balance to update`);

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);

    const chain = chainFromAsset(sourceAsset);
    if (chain !== 'Bitcoin' && chain !== 'Polkadot') {
      if (log) console.log(`${tag} Waiting deposit fetch ${depositAddress}`);
      await observeFetch(sourceAsset, depositAddress);
    }

    if (log) console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    swapContext?.updateStatus(tag, SwapStatus.Success);
  } catch (err) {
    swapContext?.updateStatus(tag, SwapStatus.Failure);
    throw new Error(`${tag} ${err}`);
  }
}

export async function performSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  // Only EVM uses this private key at the moment
  privateKey?: string,
  swapTag?: string,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  brokerCommissionBps?: number,
  log = true,
  swapContext?: SwapContext,
) {
  const tag = swapTag ?? '';

  if (log)
    console.log(
      `${tag} The args are: ${sourceAsset} ${destAsset} ${destAddress} ${messageMetadata
        ? messageMetadata.message.substring(0, 6) +
        '...' +
        messageMetadata.message.substring(messageMetadata.message.length - 4)
        : ''
      }`,
    );

  const swapParams = await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
    brokerCommissionBps,
    log,
  );

  await doPerformSwap(swapParams, tag, privateKey, messageMetadata, senderType, amount, log, swapContext);

  return swapParams;
}

// function to create a swap and track it until we detect the corresponding broadcast success
export async function performAndTrackSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  amount?: string,
  tag?: string,
) {
  await using chainflipApi = await getChainflipApi();

  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  const chain = chainFromAsset(sourceAsset);
  let privateKey = undefined;
  if (chain === 'Ethereum' || chain === 'Arbitrum') {
    privateKey = await WhaleKeyManager.getNextKey();
  }
  await send(sourceAsset, swapParams.depositAddress, amount, undefined, privateKey);
  console.log(`${tag} fund sent, waiting for the deposit to be witnessed..`);

  // SwapScheduled, SwapExecuted, SwapEgressScheduled, BatchBroadcastRequested
  const broadcastId = await observeSwapEvents(swapParams, chainflipApi, tag);

  if (broadcastId) await observeBroadcastSuccess(broadcastId, tag);
  else throw new Error('Failed to retrieve broadcastId!');
  console.log(`${tag} broadcast executed successfully, swap is complete!`);
}

export async function executeVaultSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  // for evm chains
  privateKey?: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  brokerFees?: {
    account: string;
    commissionBps: number;
  },
  affiliateFees: {
    accountAddress: string;
    accountShortId: number;
    commissionBps: number;
  }[] = [],
) {
  let sourceAddress: string;
  let transactionId: TransactionOriginId;

  const srcChain = chainFromAsset(sourceAsset);

  const brokerFeesValue = brokerFees ?? {
    account: new Keyring({ type: 'sr25519' }).createFromUri('//BROKER_1').address,
    commissionBps: 1,
  };

  if (evmChains.includes(srcChain)) {
    if (!privateKey) {
      throw new Error('No private key provided for EVM vault swap');
    }
    // Generate a new wallet for each vault swap to prevent nonce issues when running in parallel
    // with other swaps via deposit channels.
    const wallet = await createEvmWalletAndFund(sourceAsset, privateKey);
    sourceAddress = wallet.address.toLowerCase();

    // To uniquely identify the VaultSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const txHash = await executeEvmVaultSwap(
      sourceAsset,
      destAsset,
      destAddress,
      privateKey,
      brokerFeesValue,
      messageMetadata,
      amount,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      wallet,
      affiliateFees.map((f) => ({ account: f.accountShortId, commissionBps: f.commissionBps })),
    );
    transactionId = { type: TransactionOrigin.VaultSwapEvm, txHash };
    sourceAddress = wallet.address.toLowerCase();
  } else if (srcChain === 'Bitcoin') {
    const txId = await buildAndSendBtcVaultSwap(
      Number(amount ?? defaultAssetAmounts(sourceAsset)),
      destAsset,
      destAddress,
      fillOrKillParams === undefined
        ? await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND')
        : fillOrKillParams.refundAddress,
      brokerFeesValue,
      affiliateFees.map((f) => ({ account: f.accountAddress, bps: f.commissionBps })),
    );
    transactionId = { type: TransactionOrigin.VaultSwapBitcoin, txId };
    // Unused for now
    sourceAddress = '';
  } else {
    const { slot, accountAddress } = await executeSolVaultSwap(
      sourceAsset,
      destAsset,
      destAddress,
      brokerFeesValue,
      messageMetadata,
      undefined,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      affiliateFees.map((f) => ({ account: f.accountAddress, bps: f.commissionBps })),
    );
    transactionId = {
      type: TransactionOrigin.VaultSwapSolana,
      addressAndSlot: [decodeSolAddress(accountAddress.toBase58()), slot],
    };
    sourceAddress = decodeSolAddress(getSolWhaleKeyPair().publicKey.toBase58());
  }

  return { transactionId, sourceAddress };
}

export async function performVaultSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  // for evm chains
  privateKey?: string,
  swapTag = '',
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  log = true,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  brokerFees?: {
    account: string;
    commissionBps: number;
  },
  affiliateFees: {
    accountAddress: string;
    accountShortId: number;
    commissionBps: number;
  }[] = [],
): Promise<VaultSwapParams> {
  const tag = swapTag ?? '';

  const oldBalance = await getBalance(destAsset, destAddress);
  if (log) {
    console.log(`${tag} Old balance: ${oldBalance}`);
    console.log(
      `${tag} Executing (${sourceAsset}) vault swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
    );
  }

  try {
    const { transactionId, sourceAddress } = await executeVaultSwap(
      sourceAsset,
      destAsset,
      destAddress,
      privateKey,
      messageMetadata,
      amount,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
      brokerFees,
      affiliateFees,
    );
    swapContext?.updateStatus(swapTag, SwapStatus.VaultSwapInitiated);

    await observeSwapRequested(sourceAsset, destAsset, transactionId, SwapRequestType.Regular);

    swapContext?.updateStatus(swapTag, SwapStatus.VaultSwapScheduled);

    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata, sourceAddress)
      : Promise.resolve();

    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);
    if (log) {
      console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    }
    if (sourceAsset === 'Sol') {
      // Native Vault swaps are fetched proactively. SPL-tokens don't need a fetch.
      const swapEndpointNativeVaultAddress = getContractAddress(
        'Solana',
        'SWAP_ENDPOINT_NATIVE_VAULT_ACCOUNT',
      );
      if (log)
        console.log(
          `${tag} Waiting for Swap Endpoint Native Vault Swap Fetch ${swapEndpointNativeVaultAddress}`,
        );
      await observeFetch(sourceAsset, swapEndpointNativeVaultAddress);
    }
    swapContext?.updateStatus(swapTag, SwapStatus.Success);
    return {
      sourceAsset,
      destAsset,
      destAddress,
      transactionId,
    };
  } catch (err) {
    console.error('err:', err);
    swapContext?.updateStatus(swapTag, SwapStatus.Failure);
    if (err instanceof Error) {
      console.log(err.stack);
    }
    throw new Error(`${tag} ${err}`);
  }
}
