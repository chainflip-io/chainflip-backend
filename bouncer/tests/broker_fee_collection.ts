import assert from 'assert';
import { randomBytes } from 'crypto';

import {
  decodeDotAddressForContract,
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
  chainFromAsset,
  Chains,
  Asset,
} from 'shared/utils';
import { getBalance } from 'shared/get_balance';
import { getChainflipApi } from 'shared/utils/substrate';
import { send } from 'shared/send';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import {
  ChainflipIO,
  fullAccountFromUri,
  newChainflipIO,
  WithBrokerAccount,
} from 'shared/utils/chainflip_io';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { swappingWithdrawalRequested } from 'generated/events/swapping/withdrawalRequested';
import { swappingSwapDepositAddressReady } from 'generated/events/swapping/swapDepositAddressReady';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';

const commissionBps = 1000; // 10%
// All broker fees are swapped to USDC
const feeAsset = Assets.Usdc;

export async function submitBrokerWithdrawal<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  asset: Asset,
  addressObject: { [chain: string]: string },
) {
  const broker = cf.requirements.account.keypair;

  cf.debug(`Submitted withdrawal for ${asset} broker: ${broker.address}`);

  const withdrawalRequestedEvent = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.swapping.withdraw(asset, addressObject),
    expectedEvent: {
      name: 'Swapping.WithdrawalRequested',
      schema: swappingWithdrawalRequested.refine(
        (event) => event.accountId === broker.address && event.egressAsset === asset,
      ),
    },
  });

  cf.debug(
    `Withdrawal request successful for ${asset} broker: ${withdrawalRequestedEvent.accountId} egressId: ${withdrawalRequestedEvent.egressId}`,
  );
}

/// Gets the USDC free balance for the broker.
/// The broker fee is taken from the output asset and swap to USDC,
/// so you will have to wait for the fee swap to complete before it will show up in this free balance.
export async function getEarnedBrokerFees(logger: Logger, address: string): Promise<bigint> {
  logger.debug(`Getting earned broker fees for address: ${address}`);
  return getFreeBalance(address, feeAsset);
}

/// Runs a swap, checks that the broker fees are collected,
/// then withdraws the broker fees, making sure the balance is correct after the withdrawal.
async function testBrokerFees<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  seed?: string,
): Promise<void> {
  const broker = cf.requirements.account.keypair;
  await using chainflip = await getChainflipApi();

  // Check the broker fees before the swap
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(cf.logger, broker.address);
  cf.debug(`earnedBrokerFeesBefore:`, earnedBrokerFeesBefore);

  // Run a swap
  const inputAsset = Assets.Flip;
  // Using an asset that is not USDC as the output asset so that the broker fees are swapped after the swap completes.
  const destAsset = Assets.Eth;
  const destinationAddress = await newAssetAddress(
    destAsset,
    seed ?? randomBytes(32).toString('hex'),
  );
  const observeDestinationAddress =
    chainFromAsset(inputAsset) === Chains.Assethub
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress;
  cf.debug(`destinationAddress:`, destinationAddress);

  cf.debug(`Running swap ${inputAsset} -> ${destAsset}`);

  const rawDepositForSwapAmount = defaultAssetAmounts(inputAsset);

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

  // we need to manually create the swap channel and observe the relative event
  // because we want to use a separate broker to not interfere with other tests
  const swapDepositAddressReadyEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.swapping.requestSwapDepositAddress(
        inputAsset,
        destAsset,
        encodedEthAddr,
        commissionBps,
        null,
        0,
        refundParams,
      ),
    expectedEvent: {
      name: 'Swapping.SwapDepositAddressReady',
      schema: swappingSwapDepositAddressReady.refine(
        (event) =>
          event.destinationAddress.chain === chainFromAsset(destAsset) &&
          event.destinationAddress.address === observeDestinationAddress.toLowerCase() &&
          event.destinationAsset === destAsset &&
          event.sourceAsset === inputAsset,
      ),
    },
  });

  const depositAddress = swapDepositAddressReadyEvent.depositAddress.address;
  const channelId = Number(swapDepositAddressReadyEvent.channelId);

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

  // Wait for the swap request to complete
  const swapRequestCompletedEvent = await cf.stepUntilEvent(
    'Swapping.SwapRequestCompleted',
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === requestId),
  );

  // Check that the broker fees where swapped to USDC
  const [feeSwapBroker, feeSwapRequestId] = swapRequestCompletedEvent.brokerFeeSwaps[0];
  if (!feeSwapRequestId) {
    throw new Error(
      `No broker fee swap found in swap request completed event ${JSON.stringify(swapRequestCompletedEvent)}`,
    );
  }
  assert.strictEqual(feeSwapBroker, broker.address, `Unexpected broker fee swap account`);

  // Wait for the broker fee swap to complete
  await cf.stepUntilEvent(
    'Swapping.SwapRequestCompleted',
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === feeSwapRequestId),
  );

  // Check that the earned broker fees increased
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(cf.logger, broker.address);
  cf.debug(`earnedBrokerFeesAfter:`, earnedBrokerFeesAfter);

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

  await submitBrokerWithdrawal(cf, feeAsset, {
    [chain]: withdrawalAddress,
  });

  await observeBalanceIncrease(cf.logger, feeAsset, withdrawalAddress, balanceBeforeWithdrawal);

  // Check that the balance after withdrawal is correct after deducting withdrawal fee
  const balanceAfterWithdrawal = await getBalance(feeAsset, withdrawalAddress);
  cf.debug(`${feeAsset} Balance after withdrawal:`, balanceAfterWithdrawal);
  const balanceAfterWithdrawalBigInt = amountToFineAmountBigInt(balanceAfterWithdrawal, feeAsset);
  const balanceBeforeWithdrawalBigInt = amountToFineAmountBigInt(balanceBeforeWithdrawal, feeAsset);
  assert(
    balanceAfterWithdrawalBigInt > balanceBeforeWithdrawalBigInt,
    `Balance after withdrawal is less than balance before withdrawal.`,
  );
}

export async function testBrokerFeeCollection(testContext: TestContext): Promise<void> {
  const parentCf = await newChainflipIO(testContext.logger, []);

  const brokerUri = '//BROKER_FEE_TEST';

  await setupAccount(parentCf, brokerUri, AccountRole.Broker);

  const cf = parentCf.with({ account: fullAccountFromUri(brokerUri, 'Broker') });
  await testBrokerFees(cf, randomBytes(32).toString('hex'));
}
