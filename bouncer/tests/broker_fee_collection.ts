import assert from 'assert';
import { randomBytes } from 'crypto';
import { InternalAsset as Asset } from '@chainflip/cli';

import Keyring from 'polkadot/keyring';
import {
  cfMutex,
  decodeDotAddressForContract,
  handleSubstrateError,
  observeBalanceIncrease,
  shortChainFromAsset,
  hexStringToBytesArray,
  amountToFineAmountBigInt,
  SwapRequestType,
  observeSwapRequested,
  TransactionOrigin,
  defaultAssetAmounts,
  newAssetAddress,
  getFreeBalance,
  Assets,
} from 'shared/utils';
import { getBalance } from 'shared/get_balance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { send } from 'shared/send';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { swappingSwapExecuted } from 'generated/events/swapping/swapExecuted';

const commissionBps = 1000; // 10%

const brokerUri = '//BROKER_FEE_TEST';
const broker = new Keyring({ type: 'sr25519' }).createFromUri(brokerUri);

export async function submitBrokerWithdrawal(
  asset: Asset,
  addressObject: { [chain: string]: string },
) {
  await using chainflip = await getChainflipApi();
  // Only allow one withdrawal at a time to stop nonce issues
  return cfMutex.runExclusive(brokerUri, async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    return chainflip.tx.swapping
      .withdraw(asset, addressObject)
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });
}

const feeAsset = Assets.Usdc;

export async function getEarnedBrokerFees(logger: Logger, address: string): Promise<bigint> {
  logger.debug(`Getting earned broker fees for address: ${address}`);
  // NOTE: All broker fees are collected in USDC
  return getFreeBalance(address, Assets.Usdc);
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees<A = []>(
  cf: ChainflipIO<A>,
  inputAsset: Asset,
  seed?: string,
): Promise<void> {
  await using chainflip = await getChainflipApi();
  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(cf.logger, broker.address);
  cf.debug(`${inputAsset} earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const destAsset = inputAsset === feeAsset ? Assets.Flip : feeAsset;
  const destinationAddress = await newAssetAddress(
    destAsset,
    seed ?? randomBytes(32).toString('hex'),
  );
  const observeDestinationAddress =
    inputAsset === Assets.Dot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress;
  cf.debug(`${inputAsset} destinationAddress:`, destinationAddress);

  cf.debug(`Running swap ${inputAsset} -> ${destAsset}`);

  const rawDepositForSwapAmount = defaultAssetAmounts(inputAsset);

  // we need to manually create the swap channel and observe the relative event
  // because we want to use a separate broker to not interfere with other tests
  const addressPromise = observeEvent(cf.logger, 'swapping:SwapDepositAddressReady', {
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

  const refundParams = {
    refundAddress: chainflip.createType('EncodedAddress', {
      Eth: hexStringToBytesArray(await newAssetAddress(inputAsset, 'DEFAULT_REFUND')),
    }),
    minPrice: '0',
    retryDuration: 0,
  };

  await cfMutex.runExclusive(brokerUri, async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .requestSwapDepositAddress(
        inputAsset,
        destAsset,
        encodedEthAddr,
        commissionBps,
        null,
        0,
        refundParams,
      )
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });

  const res = (await addressPromise.event).data;

  const depositAddress = res.depositAddress[shortChainFromAsset(inputAsset)];
  const channelId = Number(res.channelId);

  await send(cf.logger, inputAsset, depositAddress, rawDepositForSwapAmount);
  const swapRequestedEvent = await observeSwapRequested(
    cf,
    inputAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );

  // Get values from the swap event
  const requestId = swapRequestedEvent.swapRequestId;

  const swapExecutedEvent = await cf.stepUntilEvent(
    'Swapping.SwapExecuted',
    swappingSwapExecuted.refine((event) => event.swapRequestId === requestId),
  );

  cf.debug('brokerFee:', swapExecutedEvent.brokerFee);

  // Check that the deposit amount is correct after deducting the deposit fee
  const depositAmountAfterIngressFee = swapRequestedEvent.inputAmount;
  const rawDepositForSwapAmountBigInt = amountToFineAmountBigInt(
    rawDepositForSwapAmount,
    inputAsset,
  );
  cf.debug('depositAmount:', depositAmountAfterIngressFee);
  assert(
    depositAmountAfterIngressFee >= 0 &&
      depositAmountAfterIngressFee <= rawDepositForSwapAmountBigInt,
    `Unexpected ${inputAsset} deposit amount ${depositAmountAfterIngressFee},
    }`,
  );

  // Check that the detected increase in earned broker fees matches the swap event values and it is equal to the expected amount (after the deposit fee is accounted for)
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(cf.logger, broker.address);
  cf.debug(`${inputAsset} earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);

  assert(earnedBrokerFeesAfter > earnedBrokerFeesBefore, 'No increase in earned broker fees');

  // Withdraw the broker fees
  const withdrawalAddress = await newAssetAddress(
    feeAsset,
    seed ?? randomBytes(32).toString('hex'),
  );
  const chain = shortChainFromAsset(feeAsset);
  cf.debug(`${chain} withdrawalAddress:`, withdrawalAddress);
  const balanceBeforeWithdrawal = await getBalance(feeAsset, withdrawalAddress);
  cf.debug(
    `Withdrawing broker fees to ${withdrawalAddress}, balance before: ${balanceBeforeWithdrawal}`,
  );
  const observeWithdrawalRequested = observeEvent(cf.logger, 'swapping:WithdrawalRequested', {
    test: (event) =>
      event.data.destinationAddress[chain]?.toLowerCase() === withdrawalAddress.toLowerCase(),
  });

  await submitBrokerWithdrawal(feeAsset, {
    [chain]: withdrawalAddress,
  });
  cf.debug(`Submitted withdrawal for ${feeAsset}`);

  const withdrawalRequestedEvent = await observeWithdrawalRequested.event;

  cf.debug(`Withdrawal requested, egressId: ${withdrawalRequestedEvent.data.egressId}`);

  await observeBalanceIncrease(cf.logger, feeAsset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(feeAsset, withdrawalAddress);
  cf.debug(`${inputAsset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = amountToFineAmountBigInt(balanceAfterWithdrawal, feeAsset);
  const balanceBeforeWithdrawalBigInt = amountToFineAmountBigInt(balanceBeforeWithdrawal, feeAsset);
  assert(
    balanceAfterWithdrawalBigInt > balanceBeforeWithdrawalBigInt,
    `Balance after withdrawal is less than balance before withdrawal.`,
  );
}

export async function testBrokerFeeCollection(testContext: TestContext): Promise<void> {
  const cf = await newChainflipIO(testContext.logger, []);
  await using chainflip = await getChainflipApi();

  // Check account role
  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker.address),
  ).replace(/"/g, '');
  cf.debug('Broker address:', broker.address);
  assert.strictEqual(role, 'Broker', `Broker has unexpected role: ${role}`);

  await testBrokerFees(cf, Assets.Flip, randomBytes(32).toString('hex'));
}
