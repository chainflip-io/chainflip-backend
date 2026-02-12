import { InternalAsset as Asset } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import {
  Assets,
  newAssetAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { send } from 'shared/send';
import { observeEvents } from 'shared/utils/substrate';
import { getBalance } from 'shared/get_balance';
import { executeVaultSwap, requestNewSwap } from 'shared/perform_swap';
import { DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';

async function testDCASwap<A = []>(
  parentCf: ChainflipIO<A>,
  inputAsset: Asset,
  amount: number,
  numberOfChunks: number,
  chunkIntervalBlocks: number,
  swapViaVault = false,
) {
  assert(numberOfChunks > 0, 'Number of chunks must be greater than 0');
  const cf = parentCf.withChildLogger(
    `DCA_test_${inputAsset}_${numberOfChunks}_chunks_at_${chunkIntervalBlocks}_interval`,
  );

  const dcaParams: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks,
  };
  const fillOrKillParams: FillOrKillParamsX128 = {
    refundAddress: await newAssetAddress(inputAsset, randomBytes(32).toString('hex')),
    minPriceX128: '1',
    retryDurationBlocks: 100,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const destAddress = await newAssetAddress(destAsset, randomBytes(32).toString('hex'));

  const destBalanceBefore = await getBalance(destAsset, destAddress);
  cf.debug(`DCA destination address: ${destAddress}`);

  let swapRequestedHandle;

  if (!swapViaVault) {
    const swapRequest = await requestNewSwap(
      cf,
      inputAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      fillOrKillParams,
      dcaParams,
    );

    const depositChannelId = swapRequest.channelId;
    swapRequestedHandle = observeSwapRequested(
      cf,
      inputAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: depositChannelId },
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(cf.logger, inputAsset, swapRequest.depositAddress, amount.toString());
    cf.debug(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);
  } else {
    const subcf = cf.with({ account: fullAccountFromUri('//BROKER_1', 'Broker') });
    const { transactionId } = await executeVaultSwap(
      subcf,
      inputAsset,
      destAsset,
      destAddress,
      undefined,
      amount.toString(),
      undefined,
      fillOrKillParams,
      dcaParams,
    );

    cf.debug(`Vault swap executed, tx id: ${transactionId}`);

    // Look after Swap Requested of data.origin.Vault.tx_hash
    swapRequestedHandle = observeSwapRequested(
      cf,
      inputAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestId = (await swapRequestedHandle).swapRequestId;
  cf.debug(
    `${inputAsset} swap ${swapViaVault ? 'via vault' : ''}, swapRequestId: ${swapRequestId}`,
  );

  // Wait for the swap to complete
  await cf.stepUntilEvent(
    `Swapping.SwapRequestCompleted`,
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapRequestId),
  );

  // Find the `SwapExecuted` events for this swap.
  const historicalCheckBlocks = numberOfChunks * chunkIntervalBlocks + 10;
  const observeSwapExecutedEvents = await observeEvents(cf.logger, `swapping:SwapExecuted`, {
    test: (event) => BigInt(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks,
    stopAfter: { blocks: historicalCheckBlocks },
  }).events;

  // Check that there were the correct number of SwapExecuted events, one for each chunk.
  assert.strictEqual(
    observeSwapExecutedEvents.length,
    numberOfChunks,
    `Unexpected number of SwapExecuted events: expected ${numberOfChunks}, found ${observeSwapExecutedEvents.length}`,
  );

  // Check the chunk interval of all chunks
  for (let i = 1; i < numberOfChunks; i++) {
    const interval = observeSwapExecutedEvents[i].block - observeSwapExecutedEvents[i - 1].block;
    assert.strictEqual(
      interval,
      chunkIntervalBlocks,
      `Unexpected chunk interval between chunk ${i - 1} & ${i}`,
    );
  }

  cf.debug(`Chunk interval of ${chunkIntervalBlocks} verified for all ${numberOfChunks} chunks`);

  await observeBalanceIncrease(cf.logger, destAsset, destAddress, destBalanceBefore);
}

export async function testDCASwaps(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await cf.all([
    (subcf) => testDCASwap(subcf, Assets.Eth, 1, 2, 2),
    (subcf) => testDCASwap(subcf, Assets.ArbEth, 1, 4, 1),
    (subcf) => testDCASwap(subcf, Assets.Sol, 1, 2, 3, true),
    (subcf) => testDCASwap(subcf, Assets.SolUsdc, 1, 2, 1, true),
  ]);
}
