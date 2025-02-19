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
  amountToFineAmountBigInt,
  SwapRequestType,
  observeSwapRequested,
  TransactionOrigin,
  defaultAssetAmounts,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { send } from '../shared/send';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

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

const feeAsset = Assets.Usdc;

export async function getEarnedBrokerFees(address: string): Promise<bigint> {
  await using chainflip = await getChainflipApi();
  // NOTE: All broker fees are collected in USDC now:
  const feeStr = (
    await chainflip.query.assetBalances.freeBalances(address, Assets.Usdc)
  ).toString();
  return BigInt(feeStr);
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees(logger: Logger, inputAsset: Asset, seed?: string): Promise<void> {
  await using chainflip = await getChainflipApi();
  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker.address);
  logger.debug(`${inputAsset} earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const destAsset = inputAsset === feeAsset ? Assets.Flip : feeAsset;
  const destinationAddress = await newAddress(destAsset, seed ?? randomBytes(32).toString('hex'));
  const observeDestinationAddress =
    inputAsset === Assets.Dot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress;
  logger.debug(`${inputAsset} destinationAddress:`, destinationAddress);

  logger.debug(`Running swap ${inputAsset} -> ${destAsset}`);

  const rawDepositForSwapAmount = defaultAssetAmounts(inputAsset);

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

  const swapRequestedHandle = observeSwapRequested(
    inputAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );

  await send(inputAsset, depositAddress, rawDepositForSwapAmount, true /* log */);

  const swapRequestedEvent = (await swapRequestedHandle).data;

  // Get values from the swap event
  const requestId = swapRequestedEvent.swapRequestId;

  const swapExecutedEvent = await observeEvent('swapping:SwapExecuted', {
    test: (event) => event.data.swapRequestId === requestId,
  }).event;

  const brokerFee = BigInt(swapExecutedEvent.data.brokerFee.replace(/,/g, ''));
  logger.debug('brokerFee:', brokerFee);

  // Check that the deposit amount is correct after deducting the deposit fee
  const depositAmountAfterIngressFee = BigInt(swapRequestedEvent.inputAmount.replaceAll(',', ''));
  const rawDepositForSwapAmountBigInt = amountToFineAmountBigInt(
    rawDepositForSwapAmount,
    inputAsset,
  );
  logger.debug('depositAmount:', depositAmountAfterIngressFee);
  assert(
    depositAmountAfterIngressFee >= 0 &&
      depositAmountAfterIngressFee <= rawDepositForSwapAmountBigInt,
    `Unexpected ${inputAsset} deposit amount ${depositAmountAfterIngressFee},
    }`,
  );

  // Check that the detected increase in earned broker fees matches the swap event values and it is equal to the expected amount (after the deposit fee is accounted for)
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(broker.address);
  logger.debug(`${inputAsset} earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);

  assert(earnedBrokerFeesAfter > earnedBrokerFeesBefore, 'No increase in earned broker fees');

  // Withdraw the broker fees
  const withdrawalAddress = await newAddress(feeAsset, seed ?? randomBytes(32).toString('hex'));
  const chain = shortChainFromAsset(feeAsset);
  logger.debug(`${chain} withdrawalAddress:`, withdrawalAddress);
  const balanceBeforeWithdrawal = await getBalance(feeAsset, withdrawalAddress);
  logger.debug(
    `Withdrawing broker fees to ${withdrawalAddress}, balance before: ${balanceBeforeWithdrawal}`,
  );
  const observeWithdrawalRequested = observeEvent('swapping:WithdrawalRequested', {
    test: (event) =>
      event.data.destinationAddress[chain]?.toLowerCase() === withdrawalAddress.toLowerCase(),
  });

  await submitBrokerWithdrawal(feeAsset, {
    [chain]: withdrawalAddress,
  });
  logger.debug(`Submitted withdrawal for ${feeAsset}`);

  const withdrawalRequestedEvent = await observeWithdrawalRequested.event;

  logger.debug(`Withdrawal requested, egressId: ${withdrawalRequestedEvent.data.egressId}`);

  await observeBalanceIncrease(feeAsset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(feeAsset, withdrawalAddress);
  logger.debug(`${inputAsset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = amountToFineAmountBigInt(balanceAfterWithdrawal, feeAsset);
  const balanceBeforeWithdrawalBigInt = amountToFineAmountBigInt(balanceBeforeWithdrawal, feeAsset);
  assert(
    balanceAfterWithdrawalBigInt > balanceBeforeWithdrawalBigInt,
    `Balance after withdrawal is less than balance before withdrawal.`,
  );
}

export async function testBrokerFeeCollection(testContext: TestContext): Promise<void> {
  await using chainflip = await getChainflipApi();

  // Check account role
  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker.address),
  ).replace(/"/g, '');
  testContext.debug('Broker address:', broker.address);
  assert.strictEqual(role, 'Broker', `Broker has unexpected role: ${role}`);

  await testBrokerFees(testContext.logger, Assets.Flip, randomBytes(32).toString('hex'));
}
