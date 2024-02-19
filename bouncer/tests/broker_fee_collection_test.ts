#!/usr/bin/env -S pnpm tsx
import assert from 'assert';
import { randomBytes } from 'crypto';
import Keyring from '@polkadot/keyring';
import { Asset, Assets, assetDecimals } from '@chainflip-io/cli';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import {
  EgressId,
  amountToFineAmount,
  brokerMutex,
  chainShortNameFromAsset,
  decodeDotAddressForContract,
  getChainflipApi,
  handleSubstrateError,
  newAddress,
  observeBalanceIncrease,
  observeEvent,
  runWithTimeout,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { performSwap } from '../shared/perform_swap';

const swapAssetAmount = {
  [Assets.ETH]: 1,
  [Assets.DOT]: 1000,
  [Assets.FLIP]: 1000,
  [Assets.BTC]: 0.1,
  [Assets.USDC]: 1000,
};
const commissionBps = 1000; // 10%

// Maximum expected deposit and withdrawal fees.
// Values obtained from running this test on 1 node localnet.
const maxDepositFee = {
  [Assets.ETH]: BigInt(350000),
  [Assets.DOT]: BigInt(197300000),
  [Assets.FLIP]: BigInt(300000000),
  [Assets.BTC]: BigInt(190),
  [Assets.USDC]: BigInt(0), // Fee is too low for localnet, it rounds to 0
};
const maxWithdrawalFee = {
  [Assets.ETH]: BigInt(490000),
  [Assets.DOT]: BigInt(197450000),
  [Assets.FLIP]: BigInt(300000000),
  [Assets.BTC]: BigInt(100),
  [Assets.USDC]: BigInt(0),
};
const chainflip = await getChainflipApi();

const keyring = new Keyring({ type: 'sr25519' });
const broker1 = keyring.createFromUri('//BROKER_1');

export async function submitBrokerWithdrawal(
  asset: Asset,
  addressObject: { [chain: string]: string },
) {
  // Only allow one withdrawal at a time to stop nonce issues
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.swapping
      .withdraw(asset, addressObject)
      .signAndSend(broker1, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees(asset: Asset, seed?: string): Promise<void> {
  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker1.address, asset)).toString(),
  );
  console.log(`${asset} earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const swapAsset = asset === Assets.USDC ? Assets.FLIP : Assets.USDC;
  const destinationAddress = await newAddress(swapAsset, seed ?? randomBytes(32).toString('hex'));
  const observeDestinationAddress =
    asset === Assets.DOT ? decodeDotAddressForContract(destinationAddress) : destinationAddress;
  const destinationChain = chainShortNameFromAsset(swapAsset); // "ETH" -> "Eth"
  console.log(`${asset} destinationAddress:`, destinationAddress);
  const observeSwapScheduledEvent = observeEvent(
    ':SwapScheduled',
    chainflip,
    (event) =>
      event.data.destinationAddress[destinationChain]?.toLowerCase() ===
      observeDestinationAddress.toLowerCase(),
  );
  console.log(`Running swap ${asset} -> ${swapAsset}`);
  await performSwap(
    asset,
    swapAsset,
    destinationAddress,
    undefined,
    undefined,
    undefined,
    swapAssetAmount[asset].toString(),
    commissionBps,
    false,
  );

  // Get values from the swap event
  const swapScheduledEvent = await observeSwapScheduledEvent;
  const brokerCommission = BigInt(swapScheduledEvent.data.brokerCommission.replaceAll(',', ''));
  console.log('brokerCommission:', brokerCommission);

  // Check that the deposit amount is correct after deducting the deposit fee
  const depositAmount = BigInt(swapScheduledEvent.data.depositAmount.replaceAll(',', ''));
  const testSwapAmount = BigInt(
    amountToFineAmount(swapAssetAmount[asset].toString(), assetDecimals[asset]),
  );
  const minExpectedDepositAmount = testSwapAmount - maxDepositFee[asset];
  console.log('depositAmount:', depositAmount);
  assert(
    depositAmount >= minExpectedDepositAmount && depositAmount <= testSwapAmount,
    `Unexpected ${asset} deposit amount ${depositAmount}, expected >=${minExpectedDepositAmount}, did gas fees change? detectedGasFee: ${
      testSwapAmount - depositAmount
    }`,
  );

  // Check that the detected increase in earned broker fees matches the swap event values and it is equal to the expected amount (after the deposit fee is accounted for)
  const earnedBrokerFeesAfter = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker1.address, asset)).toString(),
  );
  console.log(`${asset} earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);
  const increase = earnedBrokerFeesAfter - earnedBrokerFeesBefore;
  console.log('increase:', increase);
  assert.strictEqual(
    increase,
    brokerCommission,
    `Mismatch between brokerCommission from the swap event and the detected increase. Did some other ${asset} swap happen at the same time as this test?`,
  );

  // Calculating the fee. Using some strange math here because the SC rounds down on 0.5 instead of up.
  const divisor = BigInt(10000 / commissionBps);
  const expectedIncrease =
    depositAmount / divisor + (depositAmount % divisor > divisor / 2n ? 1n : 0n);
  assert.strictEqual(
    increase,
    expectedIncrease,
    `Unexpected increase in the ${asset} earned broker fees. Did the broker commission change?`,
  );

  // Withdraw the broker fees
  const withdrawalAddress = await newAddress(asset, seed ?? randomBytes(32).toString('hex'));
  const observeWithdrawalAddress =
    asset === Assets.DOT ? decodeDotAddressForContract(withdrawalAddress) : withdrawalAddress;
  const chain = chainShortNameFromAsset(asset);
  console.log(`${chain} withdrawalAddress:`, withdrawalAddress);
  const balanceBeforeWithdrawal = await getBalance(asset, withdrawalAddress);
  console.log(
    `Withdrawing broker fees to ${observeWithdrawalAddress}, balance before: ${balanceBeforeWithdrawal}`,
  );
  const observeWithdrawalRequested = observeEvent(
    'swapping:WithdrawalRequested',
    chainflip,
    (event) =>
      event.data.destinationAddress[chain]?.toLowerCase() ===
      observeWithdrawalAddress.toLowerCase(),
  );

  await submitBrokerWithdrawal(asset, {
    [chain]: observeWithdrawalAddress,
  });
  console.log(`Submitted withdrawal for ${asset}`);
  const withdrawalRequestedEvent = await observeWithdrawalRequested;
  console.log(`Withdrawal requested, egressId: ${withdrawalRequestedEvent.data.egressId}`);
  const BatchBroadcastRequestedEvent = await observeEvent(
    ':BatchBroadcastRequested',
    chainflip,
    (event) =>
      event.data.egressIds.some(
        (egressId: EgressId) =>
          egressId[0] === withdrawalRequestedEvent.data.egressId[0] &&
          egressId[1] === withdrawalRequestedEvent.data.egressId[1],
      ),
  );
  console.log(
    `Batch broadcast requested, broadcastId: ${BatchBroadcastRequestedEvent.data.broadcastId}`,
  );

  await observeBalanceIncrease(asset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(asset, withdrawalAddress);
  console.log(`${asset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = BigInt(
    amountToFineAmount(balanceAfterWithdrawal, assetDecimals[asset]),
  );
  const balanceBeforeWithdrawalBigInt = BigInt(
    amountToFineAmount(balanceBeforeWithdrawal, assetDecimals[asset]),
  );
  const detectWithdrawalGasFee =
    balanceBeforeWithdrawalBigInt + earnedBrokerFeesAfter - balanceAfterWithdrawalBigInt;
  // Log the chain state for Ethereum assets to help debugging.
  if (['FLIP', 'ETH', 'USDC'].includes(asset.toString())) {
    const chainState = JSON.stringify(
      await chainflip.query.ethereumChainTracking.currentChainState(),
    );
    console.log('Ethereum chain tracking state:', chainState);
  }
  assert(
    detectWithdrawalGasFee <= maxWithdrawalFee[asset] &&
      balanceAfterWithdrawalBigInt <= balanceBeforeWithdrawalBigInt + earnedBrokerFeesAfter,
    `Unexpected ${asset} balance after withdrawal, amount ${balanceAfterWithdrawalBigInt}, did gas fees change? Max expected gas fee is ${maxWithdrawalFee[asset]}, detected gas fee: ${detectWithdrawalGasFee}`,
  );
}

async function main(): Promise<void> {
  console.log('\x1b[36m%s\x1b[0m', '=== Running broker fee collection test ===');
  await cryptoWaitReady();

  // Check account role
  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker1.address),
  ).replaceAll('"', '');
  console.log('Broker role:', role);
  console.log('Broker address:', broker1.address);
  assert.strictEqual(role, 'Broker', `Broker has unexpected role: ${role}`);

  // Run the test for all assets at the same time (with different seeds so the eth addresses are different)
  await Promise.all([
    testBrokerFees(Assets.FLIP, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.ETH, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.DOT, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.BTC, randomBytes(32).toString('hex')),
    testBrokerFees(Assets.USDC, randomBytes(32).toString('hex')),
  ]);

  console.log('\x1b[32m%s\x1b[0m', '=== Broker fee collection test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 1200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
