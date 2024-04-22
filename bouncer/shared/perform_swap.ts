import { encodeAddress } from '@polkadot/util-crypto';
import { InternalAsset as Asset } from '@chainflip/cli';
import { newSwap } from './new_swap';
import { send, sendViaCfTester } from './send';
import { getBalance } from './get_balance';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  observeCcmReceived,
  shortChainFromAsset,
  observeSwapScheduled,
  observeSwapEvents,
  observeBroadcastSuccess,
} from '../shared/utils';
import { CcmDepositMetadata } from '../shared/new_swap';

function encodeDestinationAddress(address: string, destAsset: Asset): string {
  let destAddress = address;

  if (destAddress && destAsset === 'Dot') {
    destAddress = encodeAddress(destAddress);
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
): Promise<SwapParams> {
  const chainflipApi = await getChainflipApi();

  const addressPromise = observeEvent(
    'swapping:SwapDepositAddressReady',
    chainflipApi,

    (event) => {
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

      // CF Parameters is always set to '' by the SDK for now
      const ccmMetadataMatches = messageMetadata
        ? event.data.channelMetadata !== null &&
          event.data.channelMetadata.message === messageMetadata.message &&
          Number(event.data.channelMetadata.gasBudget.replace(/,/g, '')) ===
            messageMetadata.gasBudget
        : event.data.channelMetadata === null;

      return destAddressMatches && destAssetMatches && sourceAssetMatches && ccmMetadataMatches;
    },
  );
  await newSwap(sourceAsset, destAsset, destAddress, messageMetadata, brokerCommissionBps);

  const res = (await addressPromise).data;

  const depositAddress = res.depositAddress[shortChainFromAsset(sourceAsset)];
  const channelDestAddress = res.destinationAddress[shortChainFromAsset(destAsset)];
  const channelId = Number(res.channelId);

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
  Contract,
}

export async function doPerformSwap(
  { sourceAsset, destAsset, destAddress, depositAddress, channelId }: SwapParams,
  tag = '',
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  log = true,
) {
  const oldBalance = await getBalance(destAsset, destAddress);

  if (log) console.log(`${tag} Old balance: ${oldBalance}`);

  const swapScheduledHandle = observeSwapScheduled(sourceAsset, destAsset, channelId);

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  await (senderType === SenderType.Address
    ? send(sourceAsset, depositAddress, amount, log)
    : sendViaCfTester(sourceAsset, depositAddress));

  if (log) console.log(`${tag} Funded the address`);

  await swapScheduledHandle;

  if (log) console.log(`${tag} Waiting for balance to update`);

  try {
    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);

    if (log) console.log(`${tag} Swap success! New balance: ${newBalance}!`);
  } catch (err) {
    throw new Error(`${tag} ${err}`);
  }
}

export async function performSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag?: string,
  messageMetadata?: CcmDepositMetadata,
  senderType = SenderType.Address,
  amount?: string,
  brokerCommissionBps?: number,
  log = true,
) {
  const tag = swapTag ?? '';

  if (log)
    console.log(
      `${tag} The args are: ${sourceAsset} ${destAsset} ${destAddress} ${
        messageMetadata
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
  await doPerformSwap(swapParams, tag, messageMetadata, senderType, amount, log);

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
  const chainflipApi = await getChainflipApi();

  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  await send(sourceAsset, swapParams.depositAddress, amount);
  console.log(`${tag} fund sent, waiting for the deposit to be witnessed..`);

  // SwapScheduled, SwapExecuted, SwapEgressScheduled, BatchBroadcastRequested
  const broadcastId = await observeSwapEvents(swapParams, chainflipApi, tag);

  if (broadcastId) await observeBroadcastSuccess(broadcastId);
  else throw new Error('Failed to retrieve broadcastId!');
  console.log(`${tag} broadcast executed succesfully, swap is complete!`);
}
