#!/usr/bin/env -S pnpm tsx
import assert from 'assert';
import { randomBytes } from 'crypto';
import { KeyringPair } from '@polkadot/keyring/types';
import Keyring from '@polkadot/keyring';
import { Asset, Assets, assetDecimals } from '@chainflip-io/cli';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import {
  EgressId,
  amountToFineAmount,
  chainShortNameFromAsset,
  decodeDotAddressForContract,
  getChainflipApi,
  newAddress,
  observeBalanceIncrease,
  observeEvent,
  runWithTimeout,
  sleep,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { performSwap } from '../shared/perform_swap';

const testSwapAssetAmount = {
  [Assets.ETH]: 1,
  [Assets.DOT]: 1000,
  [Assets.FLIP]: 1000,
  [Assets.BTC]: 0.1,
  [Assets.USDC]: 1000,
};
const testCommissionBps = 1000; // 10%

// Maximum expected deposit and withdrawal fees
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

let withdrawLockout = false;
let broker1: KeyringPair | undefined;
export async function broker1KeyringPair() {
  if (!broker1) {
    await cryptoWaitReady();
    const keyring = new Keyring({ type: 'sr25519' });
    broker1 = keyring.createFromUri('//BROKER_1');
  }
  return broker1;
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees(asset: Asset, seed?: string): Promise<void> {
  const broker = await broker1KeyringPair();

  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker.address, asset)).toString(),
  );
  console.log(`${asset} earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const swapAsset = asset === Assets.USDC ? Assets.FLIP : Assets.USDC;
  const destinationAddress = await newAddress(swapAsset, seed ?? randomBytes(32).toString('hex'));
  const useableDestinationAddress =
    asset === Assets.DOT ? decodeDotAddressForContract(destinationAddress) : destinationAddress;
  const destinationChain = chainShortNameFromAsset(swapAsset); // "ETH" -> "Eth"
  console.log(`${asset} destinationAddress:`, destinationAddress);
  const observeSwapScheduledEvent = observeEvent(
    ':SwapScheduled',
    chainflip,
    (event) =>
      event.data.destinationAddress[destinationChain]?.toLowerCase() ===
      useableDestinationAddress.toLowerCase(),
  );
  console.log(`Running swap ${asset} -> ${swapAsset}`);
  await performSwap(
    asset,
    swapAsset,
    destinationAddress,
    undefined,
    undefined,
    undefined,
    testSwapAssetAmount[asset].toString(),
    testCommissionBps,
    false,
  );

  // Check the broker fees after the swap
  const earnedBrokerFeesAfter = BigInt(
    (await chainflip.query.swapping.earnedBrokerFees(broker.address, asset)).toString(),
  );
  console.log(`${asset} earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);

  // Get values from the swap event
  const swapScheduledEvent = await observeSwapScheduledEvent;
  const brokerCommission = BigInt(
    JSON.stringify(swapScheduledEvent.data.brokerCommission)
      .replaceAll('"', '')
      .replaceAll(',', ''),
  );
  console.log('brokerCommission:', brokerCommission);

  // Check that the deposit amount is correct after deducting the deposit fee
  const depositAmount = BigInt(
    JSON.stringify(swapScheduledEvent.data.depositAmount).replaceAll('"', '').replaceAll(',', ''),
  );
  const testSwapAmount = BigInt(
    amountToFineAmount(testSwapAssetAmount[asset].toString(), assetDecimals[asset]),
  );
  const expectedDepositAmount = testSwapAmount - maxDepositFee[asset];
  console.log('depositAmount:', depositAmount);
  assert(
    depositAmount >= expectedDepositAmount && depositAmount <= testSwapAmount,
    `Unexpected ${asset} deposit amount ${depositAmount}, expected >=${expectedDepositAmount}, did gas fees change? detectedGasFee: ${
      testSwapAmount - depositAmount
    }`,
  );

  // Check that the detected increase matches the swap event values and it is equal to the expected amount (after the deposit fee is accounted for)
  const increase = earnedBrokerFeesAfter - earnedBrokerFeesBefore;
  console.log('increase:', increase);
  assert.strictEqual(
    increase,
    brokerCommission,
    `Mismatch between brokerCommission from the swap event and the detected increase. Did some other ${asset} swap happen at the same time as this test?`,
  );

  // Calculating the fee. Using some strange math here because the SC rounds down on 0.5 instead of up.
  const divisor = BigInt(1 / (testCommissionBps / 10000));
  const expectedIncrease =
    depositAmount / divisor + (depositAmount % divisor > divisor / 2n ? 1n : 0n);
  assert.strictEqual(
    increase,
    expectedIncrease,
    `Unexpected increase in the ${asset} earned broker fees. Did the broker commission change?`,
  );

  // Withdraw the broker fees
  const withdrawalAddress = await newAddress(asset, seed ?? randomBytes(32).toString('hex'));
  const useableWithdrawalAddress =
    asset === Assets.DOT ? decodeDotAddressForContract(withdrawalAddress) : withdrawalAddress;
  const chain = chainShortNameFromAsset(asset);
  console.log(`${chain} withdrawalAddress:`, withdrawalAddress);
  const balanceBeforeWithdrawal = await getBalance(asset, withdrawalAddress);
  console.log(
    `Withdrawing broker fees to ${useableWithdrawalAddress}, balance before: ${balanceBeforeWithdrawal}`,
  );
  const observeWithdrawalRequested = observeEvent(
    'swapping:WithdrawalRequested',
    chainflip,
    (event) =>
      event.data.destinationAddress[chain]?.toLowerCase() ===
      useableWithdrawalAddress.toLowerCase(),
  );

  // Only allow one withdrawal at a time to stop nonce issues
  while (withdrawLockout) {
    await sleep(100);
  }
  withdrawLockout = true;
  const withdrawal = await chainflip.tx.swapping
    .withdraw(asset, {
      [chain]: useableWithdrawalAddress,
    })
    .signAndSend(broker);

  console.log('Submitted Withdrawal:', JSON.stringify(withdrawal));
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

  withdrawLockout = false;

  await observeBalanceIncrease(asset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(asset, withdrawalAddress);
  console.log(`${asset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = BigInt(
    amountToFineAmount(balanceAfterWithdrawal, assetDecimals[asset]),
  );
  const expectedBalanceAfterWithdrawal =
    BigInt(amountToFineAmount(balanceBeforeWithdrawal, assetDecimals[asset])) +
    earnedBrokerFeesAfter -
    maxWithdrawalFee[asset];
  const detectWithdrawalGasFee = -(
    balanceAfterWithdrawalBigInt -
    BigInt(amountToFineAmount(balanceBeforeWithdrawal, assetDecimals[asset])) -
    earnedBrokerFeesAfter
  );

  assert(
    balanceAfterWithdrawalBigInt >= expectedBalanceAfterWithdrawal &&
      balanceAfterWithdrawalBigInt <=
        BigInt(amountToFineAmount(balanceBeforeWithdrawal, assetDecimals[asset])) +
          earnedBrokerFeesAfter,
    `Unexpected ${asset} balance after withdrawal, amount ${balanceAfterWithdrawalBigInt}, expected >=${expectedBalanceAfterWithdrawal}, did gas fees change? detected gas fee: ${detectWithdrawalGasFee}`,
  );
}

async function main(): Promise<void> {
  console.log('\x1b[36m%s\x1b[0m', '=== Running broker fee collection test ===');

  // Check account role
  const brokerAddress = (await broker1KeyringPair()).address;
  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(brokerAddress),
  ).replaceAll('"', '');
  console.log('role:', role);
  console.log('broker1.address:', brokerAddress);
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
