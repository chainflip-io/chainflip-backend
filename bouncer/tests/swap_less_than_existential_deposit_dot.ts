#!/usr/bin/env -S pnpm tsx
import { getBalance } from '../shared/get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';
import { SwapParams, requestNewSwap } from '../shared/perform_swap';
import { sendDot } from '../shared/send_dot';
import { sendErc20 } from '../shared/send_erc20';
import {
  newAddress,
  getChainflipApi,
  observeEvent,
  observeSwapScheduled,
  observeCcmReceived,
  observeBalanceIncrease,
  getEthContractAddress,
  observeBadEvents,
  runWithTimeout,
} from '../shared/utils';

// This code is duplicated to allow us to specify a specific amount we want to swap
// and to wait for some specific events
export async function doPerformSwap(
  { sourceAsset, destAsset, destAddress, depositAddress, channelId }: SwapParams,
  amount: string,
  balanceIncrease: boolean,
  tag = '',
  messageMetadata?: CcmDepositMetadata,
) {
  const oldBalance = await getBalance(destAsset, destAddress);

  console.log(`${tag} Old balance: ${oldBalance}`);

  const swapScheduledHandle = observeSwapScheduled(sourceAsset, destAsset, channelId);

  const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
    : Promise.resolve();

  const contractAddress = getEthContractAddress('USDC');
  await sendErc20(depositAddress, contractAddress, amount);

  console.log(`${tag} Funded the address`);

  await swapScheduledHandle;

  console.log(`${tag} Waiting for balance to update`);

  if (!balanceIncrease) {
    const api = await getChainflipApi();
    await observeEvent('polkadotBroadcaster:BroadcastSuccess', api);

    const newBalance = await getBalance(destAsset, destAddress);

    console.log(`${tag} Swap success! Balance (Same as before): ${newBalance}!`);
  } else {
    try {
      const [newBalance] = await Promise.all([
        observeBalanceIncrease(destAsset, destAddress, oldBalance),
        ccmEventEmitted,
      ]);

      console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    } catch (err) {
      throw new Error(`${tag} ${err}`);
    }
  }
}

export async function swapLessThanED() {
  console.log('=== Testing USDC -> DOT swaps obtaining less than ED ===');

  let stopObserving = false;
  const observingBadEvents = observeBadEvents(':BroadcastAborted', () => stopObserving);

  // the initial price is 10USDC = 1DOT
  // we will swap only 5 USDC and check that the swap is completed succesfully
  const tag = `USDC -> DOT (less than ED)`;
  const address = await newAddress('DOT', 'random seed');

  console.log('Generated DOT address: ' + address);
  const swapParams = await requestNewSwap('USDC', 'DOT', address, tag);
  await doPerformSwap(swapParams, '5', false, tag);

  await sendDot(address, '50');
  console.log('Account funded, new balance: ' + (await getBalance('DOT', address)));

  // We will then send some dot to the address and perform another swap with less than ED
  const tag2 = `USDC -> DOT (to active account)`;
  const swapParams2 = await requestNewSwap('USDC', 'DOT', address, tag2);
  await doPerformSwap(swapParams2, '5', true, tag2);

  stopObserving = true;
  await observingBadEvents;

  console.log('=== Test complete ===');
}

runWithTimeout(swapLessThanED(), 300000)
  .then(() => {
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
