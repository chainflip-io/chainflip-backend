#!/usr/bin/env -S pnpm tsx
import assert from 'assert';
import { randomBytes } from 'crypto';
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import Keyring from '../polkadot/keyring';
import {
  brokerMutex,
  decodeDotAddressForContract,
  handleSubstrateError,
  newAddress,
  observeBalanceIncrease,
  shortChainFromAsset,
  hexStringToBytesArray,
  calculateFeeWithBps,
  amountToFineAmountBigInt,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { doPerformSwap } from '../shared/perform_swap';
import { getChainflipApi, observeEvent } from './utils/substrate';

const swapAssetAmount = {
  [Assets.Eth]: 1,
  [Assets.Dot]: 1000,
  [Assets.Flip]: 1000,
  [Assets.Btc]: 0.1,
  [Assets.Usdc]: 1000,
  [Assets.Usdt]: 1000,
  [Assets.ArbEth]: 1,
  [Assets.ArbUsdc]: 1000,
};
const commissionBps = 1000; // 10%

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_FEE_TEST');

export async function submitBrokerWithdrawal(
  asset: Asset,
  addressObject: { [chain: string]: string },
) {
  await using chainflip = await getChainflipApi();
  // Only allow one withdrawal at a time to stop nonce issues
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.swapping
      .withdraw(asset, addressObject)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees(inputAsset: Asset, seed?: string): Promise<void> {
  await using chainflip = await getChainflipApi();
  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker.address, inputAsset)).toString(),
  );
  console.log(`${inputAsset} earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;
  const destinationAddress = await newAddress(destAsset, seed ?? randomBytes(32).toString('hex'));
  const observeDestinationAddress =
    inputAsset === Assets.Dot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress;
  const destinationChain = shortChainFromAsset(destAsset);
  console.log(`${inputAsset} destinationAddress:`, destinationAddress);
  const observeSwapScheduledEvent = observeEvent(':SwapScheduled', {
    test: (event) =>
      event.data.destinationAddress[destinationChain]?.toLowerCase() ===
      observeDestinationAddress.toLowerCase(),
  });

  console.log(`Running swap ${inputAsset} -> ${destAsset}`);

  const rawDepositForSwapAmount = swapAssetAmount[inputAsset].toString();

  // we need to manually create the swap channel and observe the relative event
  // because we want to use a separate broker to not interfere with other tests
  const addressPromise = observeEvent('swapping:SwapDepositAddressReady', {
    test: (event) => {
      // Find deposit address for the right swap by looking at destination address:
      const destAddressEvent = event.data.destinationAddress[shortChainFromAsset(destAsset)];
      if (!destAddressEvent) return false;

      const destAssetMatches = event.data.destinationAsset === destAsset;
      const sourceAssetMatches = event.data.sourceAsset === inputAsset;
      const destAddressMatches =
        destAddressEvent.toLowerCase() === observeDestinationAddress.toLowerCase();

      return destAddressMatches && destAssetMatches && sourceAssetMatches;
    },
  });

  const encodedEthAddr = chainflip.createType('EncodedAddress', {
    Eth: hexStringToBytesArray(destinationAddress),
  });
  await brokerMutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .requestSwapDepositAddress(inputAsset, destAsset, encodedEthAddr, commissionBps, null, 0)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  const res = (await addressPromise.event).data;

  const depositAddress = res.depositAddress[shortChainFromAsset(inputAsset)];
  const channelId = Number(res.channelId);
  await doPerformSwap(
    {
      sourceAsset: inputAsset,
      destAsset,
      destAddress: destinationAddress,
      depositAddress,
      channelId,
    },
    `${inputAsset}->${destAsset} BrokerFee`,
    undefined,
    undefined,
    rawDepositForSwapAmount,
  );

  // Get values from the swap event
  const swapScheduledEvent = await observeSwapScheduledEvent.event;
  const brokerCommission = BigInt(swapScheduledEvent.data.brokerCommission.replace(/,/g, ''));
  console.log('brokerCommission:', brokerCommission);

  // Check that the deposit amount is correct after deducting the deposit fee
  const depositAmountAfterIngressFee = BigInt(
    swapScheduledEvent.data.depositAmount.replaceAll(',', ''),
  );
  const rawDepositForSwapAmountBigInt = amountToFineAmountBigInt(
    rawDepositForSwapAmount,
    inputAsset,
  );
  console.log('depositAmount:', depositAmountAfterIngressFee);
  assert(
    depositAmountAfterIngressFee >= 0 &&
      depositAmountAfterIngressFee <= rawDepositForSwapAmountBigInt,
    `Unexpected ${inputAsset} deposit amount ${depositAmountAfterIngressFee},
    }`,
  );

  // Check that the detected increase in earned broker fees matches the swap event values and it is equal to the expected amount (after the deposit fee is accounted for)
  const earnedBrokerFeesAfter = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker.address, inputAsset)).toString(),
  );
  console.log(`${inputAsset} earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);
  const increase = earnedBrokerFeesAfter - earnedBrokerFeesBefore;
  console.log('increase:', increase);
  assert.strictEqual(
    increase,
    brokerCommission,
    `Mismatch between brokerCommission from the swap event and the detected increase. Did some other ${inputAsset} swap happen at the same time as this test?`,
  );

  const expectedIncrease = calculateFeeWithBps(depositAmountAfterIngressFee, commissionBps);
  assert.strictEqual(
    increase,
    expectedIncrease,
    `Unexpected increase in the ${inputAsset} earned broker fees. Did the broker commission change?`,
  );

  // Withdraw the broker fees
  const withdrawalAddress = await newAddress(inputAsset, seed ?? randomBytes(32).toString('hex'));
  const observeWithdrawalAddress =
    inputAsset === Assets.Dot ? decodeDotAddressForContract(withdrawalAddress) : withdrawalAddress;
  const chain = shortChainFromAsset(inputAsset);
  console.log(`${chain} withdrawalAddress:`, withdrawalAddress);
  const balanceBeforeWithdrawal = await getBalance(inputAsset, withdrawalAddress);
  console.log(
    `Withdrawing broker fees to ${observeWithdrawalAddress}, balance before: ${balanceBeforeWithdrawal}`,
  );
  const observeWithdrawalRequested = observeEvent('swapping:WithdrawalRequested', {
    test: (event) =>
      event.data.destinationAddress[chain]?.toLowerCase() ===
      observeWithdrawalAddress.toLowerCase(),
  });

  await submitBrokerWithdrawal(inputAsset, {
    [chain]: observeWithdrawalAddress,
  });
  console.log(`Submitted withdrawal for ${inputAsset}`);

  const withdrawalRequestedEvent = await observeWithdrawalRequested.event;

  console.log(`Withdrawal requested, egressId: ${withdrawalRequestedEvent.data.egressId}`);

  await observeBalanceIncrease(inputAsset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(inputAsset, withdrawalAddress);
  console.log(`${inputAsset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = amountToFineAmountBigInt(balanceAfterWithdrawal, inputAsset);
  const balanceBeforeWithdrawalBigInt = amountToFineAmountBigInt(
    balanceBeforeWithdrawal,
    inputAsset,
  );
  // Log the chain state for Ethereum assets to help debugging.
  if (['Flip', 'Eth', 'Usdc'].includes(inputAsset.toString())) {
    const chainState = JSON.stringify(
      await chainflip.query.ethereumChainTracking.currentChainState(),
    );
    console.log('Ethereum chain tracking state:', chainState);
  }
  assert(
    balanceAfterWithdrawalBigInt > balanceBeforeWithdrawalBigInt,
    `Balance after withdrawal is less than balance before withdrawal.`,
  );
}

export async function testBrokerFeeCollection(): Promise<void> {
  console.log('\x1b[36m%s\x1b[0m', '=== Running broker fee collection test ===');
  await using chainflip = await getChainflipApi();

  // Check account role
  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker.address),
  ).replace(/"/g, '');
  console.log('Broker role:', role);
  console.log('Broker address:', broker.address);
  assert.strictEqual(role, 'Broker', `Broker has unexpected role: ${role}`);

  // Run the test for all assets at the same time (with different seeds so the eth addresses are different)
  await Promise.all([
    testBrokerFees(Assets.Flip, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.Eth, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.Dot, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.Btc, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.Usdc, randomBytes(32).toString('hex')),
  ]);

  console.log('\x1b[32m%s\x1b[0m', '=== Broker fee collection test complete ===');
}
